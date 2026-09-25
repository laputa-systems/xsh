#!/usr/bin/env python3
"""Paired reference/candidate benchmark for the embedded standard-library port.

Each workload runs alternately on the reference and candidate binaries so that
CPU frequency, cache, and scheduler drift affect both sides equally. Results
retain independent rounds and report the median of their sample medians.
"""

import argparse
import hashlib
import json
import os
import statistics
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))

# Each workload class uses one frozen script. Linux-only workloads run in the
# pinned test container because their entries read `/proc` and `/sys`; the
# runner reports them as skipped when it is not given a Linux binary.
WORKLOADS = [
    # Cold startup: a fresh process through a fixed tiny script.
    ("cold_trivial", "cold", 60, False),
    ("cold_quote", "cold", 60, False),
    ("cold_pad", "cold", 60, False),
    ("cold_cli_parse", "cold", 60, False),
    ("cold_cli_usage", "cold", 60, False),
    ("cold_cli_error", "cold", 60, False),
    ("cold_dynamic_ref", "cold", 40, False),
    # CLI.
    ("cli_small_schema", "end_to_end", 30, False),
    ("cli_wide_schema", "end_to_end", 30, False),
    ("cli_repeated_parse", "end_to_end", 30, False),
    # Text and formatting.
    ("text_wrap_unicode", "end_to_end", 30, False),
    ("text_pad_batch", "end_to_end", 30, False),
    ("fmt_batch", "end_to_end", 30, False),
    # Quoting and tokenization.
    ("quote_batch", "end_to_end", 30, False),
    ("quote_edge_cases", "end_to_end", 30, False),
    # MIME and INI.
    ("mime_batch", "end_to_end", 30, False),
    ("ini_large_record", "end_to_end", 30, False),
    # JSON.
    ("json_path_ops", "end_to_end", 30, False),
    # This workload takes many seconds per process. Ten samples in each of
    # three independent rounds retain 30 observations without a single long round.
    ("json_lines_batch", "end_to_end", 10, False),
    # Environment.
    ("env_typed_lookups", "end_to_end", 30, False),
    # Checksum parsing.
    ("checksum_batch", "end_to_end", 30, False),
    # Real commands and tooling (project-wide check/lint run outside the runner).
    ("core_command", "end_to_end", 30, False),
    # Linux entries under test: eager acquisition plus row interpretation,
    # measured separately from stream creation and from how much of the stream
    # a consumer takes. They read fixed host paths, so they run in the
    # Dockerfile.test container against fixtures staged at those fixed paths.
    ("linux_uptime", "end_to_end", 30, True),
    ("linux_meminfo", "end_to_end", 30, True),
    ("linux_memory", "end_to_end", 30, True),
    ("linux_os_release", "end_to_end", 30, True),
    ("linux_modules_full", "end_to_end", 30, True),
    ("linux_modules_partial", "end_to_end", 30, True),
    # Native controls: paths the port must not slow down.
    ("native_control", "end_to_end", 30, False),
    ("native_hash_control", "end_to_end", 30, False),
    # No gated Linux prototypes remain enabled: every measured prototype failed
    # its gate and returned to native code, so there is nothing ported to measure.
]

# Every frozen workload currently exits zero, including the handled CLI error.
# A future expected nonzero case must be named here so timing still rejects
# unexpected statuses while parity compares the exact result.
EXPECTED_STATUSES = {}


def expected_status(name):
    return EXPECTED_STATUSES.get(name, 0)


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_unicode_fixture(path):
    """Restore the generated workload if its bytes differ from the fixed seed."""
    seeds = [
        "The quick brown fox jumps over the lazy dog beside the river.",
        "  indented continuation with tabs\tand trailing space   ",
        "A tight line.",
        "caf\u00e9 na\u00efve r\u00e9sum\u00e9 \u2014 punctuation and an em dash.",
        "\u65e5\u672c\u8a9e\u306e\u884c\u3068\u4e2d\u6587\u7684\u4e00\u884c\u4ee5\u53ca emoji \U0001f600 mixed in.",
        "",
    ]
    chunks = []
    written = 0
    index = 0
    while written < 1_100_000:
        line = seeds[index % len(seeds)]
        chunks.append(line + "\n")
        written += len(line.encode("utf-8")) + 1
        index += 1
    expected = "".join(chunks).encode("utf-8")
    digest = hashlib.sha256(expected).hexdigest()
    if not os.path.isfile(path) or sha256_file(path) != digest:
        with open(path, "wb") as handle:
            handle.write(expected)
    return digest


def ensure_fixtures():
    """Restore the generated input and identify every fixed workload fixture."""
    directory = os.path.join(HERE, "fixtures")
    os.makedirs(directory, exist_ok=True)
    unicode_digest = ensure_unicode_fixture(os.path.join(directory, "unicode.txt"))
    modules_path = os.path.join(directory, "modules-200.txt")
    with open(modules_path, encoding="ascii") as handle:
        if len(handle.readlines()) != 200:
            raise SystemExit("modules-200.txt must contain exactly 200 rows")
    for name in ["meminfo.txt", "uptime.txt"]:
        if not os.path.isfile(os.path.join(directory, name)):
            raise SystemExit(f"missing committed Linux fixture: {name}")
    return {
        "fixtures/unicode.txt": unicode_digest,
        "fixtures/modules-200.txt": sha256_file(modules_path),
        "fixtures/meminfo.txt": sha256_file(os.path.join(directory, "meminfo.txt")),
        "fixtures/uptime.txt": sha256_file(os.path.join(directory, "uptime.txt")),
    }


def run_once(binary, script, expected=0, env=None):
    start = time.perf_counter()
    completed = subprocess.run(
        [binary, script],
        cwd=HERE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        env=env,
    )
    elapsed = time.perf_counter() - start
    if completed.returncode != expected:
        raise SystemExit(
            f"{binary} {script} exited {completed.returncode}, expected {expected}: "
            f"{completed.stderr.decode(errors='replace')[:400]}"
        )
    return elapsed * 1000.0


def measure(reference, candidate, name, samples, round_index, env=None):
    """Measure one workload with the two sides interleaved.

    The first side alternates across samples and rounds. Preserve the order and
    raw samples so the median can be checked without re-running the workload.
    """
    script = os.path.join(HERE, name + ".xsh")
    expected = expected_status(name)
    warmup_order = ["reference", "candidate"]
    if round_index % 2:
        warmup_order.reverse()
    for side in warmup_order:
        run_once(reference if side == "reference" else candidate, script, expected, env)
    ref, cand = [], []
    sample_order = []
    for index in range(samples):
        if (round_index + index) % 2 == 0:
            sample_order.append(["reference", "candidate"])
            ref.append(run_once(reference, script, expected, env))
            cand.append(run_once(candidate, script, expected, env))
        else:
            sample_order.append(["candidate", "reference"])
            cand.append(run_once(candidate, script, expected, env))
            ref.append(run_once(reference, script, expected, env))
    return {
        "index": round_index,
        "warmup_order": warmup_order,
        "sample_order": sample_order,
        "reference_samples_ms": ref,
        "candidate_samples_ms": cand,
        "reference_median_ms": statistics.median(ref),
        "candidate_median_ms": statistics.median(cand),
    }


def measure_linux_round(reference, candidate, workloads, round_index, fixtures):
    """Time Linux children inside one pinned container, outside Docker startup."""
    root = os.path.abspath(os.path.join(HERE, "../.."))
    image = os.environ.get("XSH_TEST_IMAGE", "xsh-test")
    shell = (
        "mount --bind /work/bench/stdlib-port/fixtures/modules-200.txt /proc/modules "
        "&& mount --bind /work/bench/stdlib-port/fixtures/meminfo.txt /proc/meminfo "
        "&& mount --bind /work/bench/stdlib-port/fixtures/uptime.txt /proc/uptime "
        "&& test \"$(wc -l </proc/modules)\" -eq 200 "
        "&& sha256sum /proc/modules /proc/meminfo /proc/uptime "
        "&& exec \"$@\""
    )
    argv = [
        "docker", "run", "--rm", "--privileged", "--platform", "linux/arm64",
        "-v", f"{root}:/work:ro",
        "-v", f"{reference}:/bench/reference/xsh:ro",
        "-v", f"{candidate}:/bench/candidate/xsh:ro",
        "-w", "/work/bench/stdlib-port",
        "-e", "TARGET=aarch64-unknown-linux-musl",
        "-e", "XSH_LINUX_REAL=1",
        image, "sh", "-c", shell, "sh",
        "/bench/candidate/xsh", "/work/bench/stdlib-port/linux-runner.xsh", "--",
        "/bench/reference/xsh", "/bench/candidate/xsh", str(round_index),
    ]
    for name, _, samples, _ in workloads:
        argv.extend([name, str(samples), str(expected_status(name))])
    completed = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if completed.returncode:
        raise SystemExit(
            f"pinned Linux benchmark failed ({completed.returncode}):\n{completed.stderr[-2000:]}"
        )
    lines = completed.stdout.splitlines()
    expected_fixtures = [
        [fixtures[f"fixtures/{name}"], path]
        for name, path in [
            ("modules-200.txt", "/proc/modules"),
            ("meminfo.txt", "/proc/meminfo"),
            ("uptime.txt", "/proc/uptime"),
        ]
    ]
    if [line.split() for line in lines[:3]] != expected_fixtures:
        raise SystemExit("container /proc fixtures do not match the committed inputs")
    rows = {}
    expected = []
    for name, _, samples, _ in workloads:
        rows[name] = {
            "index": round_index,
            "warmup_order": [],
            "sample_order": [[] for _ in range(samples)],
            "reference_samples_ms": [],
            "candidate_samples_ms": [],
        }
        warmup = ["reference", "candidate"]
        if round_index % 2:
            warmup.reverse()
        expected.extend((name, "warmup", 0, side) for side in warmup)
        for index in range(samples):
            sides = ["reference", "candidate"]
            if (round_index + index) % 2:
                sides.reverse()
            expected.extend((name, "sample", index, side) for side in sides)
    actual = []
    for line in lines[3:]:
        parts = line.split("\t")
        if len(parts) != 5:
            raise SystemExit(f"invalid Linux benchmark row: {line!r}")
        name, phase, index_text, side, ns_text = parts
        try:
            index = int(index_text)
            elapsed_ms = int(ns_text) / 1_000_000.0
        except ValueError as error:
            raise SystemExit(f"invalid Linux benchmark measurement: {line!r}") from error
        if side not in ("reference", "candidate") or elapsed_ms < 0:
            raise SystemExit(f"invalid Linux benchmark side or duration: {line!r}")
        actual.append((name, phase, index, side))
        if name not in rows:
            raise SystemExit(f"unexpected Linux workload: {name}")
        row = rows[name]
        if phase == "warmup":
            row["warmup_order"].append(side)
        elif phase == "sample" and 0 <= index < len(row["sample_order"]):
            row["sample_order"][index].append(side)
            row[f"{side}_samples_ms"].append(elapsed_ms)
    if actual != expected:
        raise SystemExit("Linux benchmark emitted an incomplete or reordered sample sequence")
    for row in rows.values():
        row["reference_median_ms"] = statistics.median(row["reference_samples_ms"])
        row["candidate_median_ms"] = statistics.median(row["candidate_samples_ms"])
    return rows


def budget_for(kind, reference_ms):
    if kind == "cold":
        return max(0.05 * reference_ms, 1.0)
    if kind == "end_to_end":
        return max(0.10 * reference_ms, 2.0)
    raise ValueError(f"unknown workload kind: {kind}")


def summarize_rounds(kind, rounds):
    """Apply the fixed gate to the median of independent round medians."""
    reference_rounds = [round_["reference_median_ms"] for round_ in rounds]
    candidate_rounds = [round_["candidate_median_ms"] for round_ in rounds]
    reference = statistics.median(reference_rounds)
    candidate = statistics.median(candidate_rounds)
    budget = budget_for(kind, reference)
    reference_mad = (
        statistics.median(abs(value - reference) for value in reference_rounds)
        if len(rounds) > 1
        else None
    )
    candidate_mad = (
        statistics.median(abs(value - candidate) for value in candidate_rounds)
        if len(rounds) > 1
        else None
    )
    return {
        "reference_rounds": reference_rounds,
        "candidate_rounds": candidate_rounds,
        "reference_samples": [
            value for round_ in rounds for value in round_["reference_samples_ms"]
        ],
        "candidate_samples": [
            value for round_ in rounds for value in round_["candidate_samples_ms"]
        ],
        "reference_median_ms": reference,
        "candidate_median_ms": candidate,
        "reference_round_mad_ms": reference_mad,
        "candidate_round_mad_ms": candidate_mad,
        "delta_ms": candidate - reference,
        "budget_ms": budget,
        "passed": candidate - reference <= budget,
    }


def self_test():
    """Validate the budget arithmetic on a known synthetic set.

    The floors are the interesting part: a reference under 20 ms takes the
    absolute floor, and a reference above it takes the proportional allowance.
    """
    cases = [
        # (kind, reference_ms, delta_ms, expected_budget_ms, expected_pass)
        ("end_to_end", 94.0, 0.0, 9.4, True),
        ("end_to_end", 115.0, 0.0, 11.5, True),
        ("cold", 10.0, 1.0, 1.0, True),
        ("cold", 10.0, 1.001, 1.0, False),
        ("end_to_end", 15.0, 2.0, 2.0, True),
        ("end_to_end", 15.0, 2.001, 2.0, False),
        ("end_to_end", 25.0, 2.5, 2.5, True),
        ("end_to_end", 25.0, 2.501, 2.5, False),
    ]
    for kind, ref, delta, expected_budget, expected_pass in cases:
        budget = budget_for(kind, ref)
        passed = delta <= budget
        assert abs(budget - expected_budget) < 1e-9, (kind, ref, budget, expected_budget)
        assert passed == expected_pass, (kind, ref, delta, budget, expected_pass)
    rounds = [
        {
            "reference_median_ms": 10.0,
            "candidate_median_ms": 12.0,
            "reference_samples_ms": [10.0],
            "candidate_samples_ms": [12.0],
        },
        {
            "reference_median_ms": 12.0,
            "candidate_median_ms": 12.0,
            "reference_samples_ms": [12.0],
            "candidate_samples_ms": [12.0],
        },
        {
            "reference_median_ms": 14.0,
            "candidate_median_ms": 14.0,
            "reference_samples_ms": [14.0],
            "candidate_samples_ms": [14.0],
        },
    ]
    summary = summarize_rounds("end_to_end", rounds)
    assert summary["reference_median_ms"] == 12.0
    assert summary["candidate_median_ms"] == 12.0
    assert summary["reference_round_mad_ms"] == 2.0
    assert summary["candidate_round_mad_ms"] == 0.0
    assert summary["reference_samples"] == [10.0, 12.0, 14.0]
    assert summary["passed"]
    with tempfile.TemporaryDirectory(prefix="xsh-benchmark-status-") as directory:
        binary = os.path.join(directory, "exit-seven")
        with open(binary, "w", encoding="ascii") as handle:
            handle.write("#!/bin/sh\nexit 7\n")
        os.chmod(binary, 0o755)
        assert run_once(binary, os.path.join(HERE, "cold_trivial.xsh"), expected=7) >= 0
        try:
            run_once(binary, os.path.join(HERE, "cold_trivial.xsh"), expected=0)
        except SystemExit:
            pass
        else:
            raise AssertionError("unexpected exit status must fail timing")
        fixture = os.path.join(directory, "unicode.txt")
        digest = ensure_unicode_fixture(fixture)
        with open(fixture, "r+b") as handle:
            first = handle.read(1)
            handle.seek(0)
            handle.write(b"X" if first != b"X" else b"Y")
        assert sha256_file(fixture) != digest
        assert ensure_unicode_fixture(fixture) == digest
        assert sha256_file(fixture) == digest
    print(f"self-test: {len(cases)} budget cases match")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--out", default=os.path.join(HERE, "results.json"))
    parser.add_argument("--rounds", type=int, default=1)
    parser.add_argument(
        "--linux",
        action="store_true",
        help="the binaries are Linux builds in the Dockerfile.test container; run the Linux-only workloads",
    )
    args = parser.parse_args()
    if args.rounds < 1:
        parser.error("--rounds must be at least 1")
    if not os.path.isabs(args.reference) or not os.path.isabs(args.candidate):
        parser.error("--reference and --candidate must be absolute paths")

    fixtures = ensure_fixtures()
    # Each workload is one frozen script; recording its digest with the result
    # makes "the same workload" checkable after the fact rather than assumed.
    workload_scripts = {
        name: sha256_file(os.path.join(HERE, name + ".xsh"))
        for name, _, _, _ in WORKLOADS
        if os.path.isfile(os.path.join(HERE, name + ".xsh"))
    }
    results = {
        "reference": args.reference,
        "candidate": args.candidate,
        "reference_sha256": sha256_file(args.reference),
        "candidate_sha256": sha256_file(args.candidate),
        "fixtures": fixtures,
        "workload_scripts": workload_scripts,
        "rounds": args.rounds,
        "units": "milliseconds, median of independent round medians of wall-clock samples",
        "budget_formula": (
            "cold: C - B <= max(0.05 * B, 1.0 ms); "
            "end_to_end: C - B <= max(0.10 * B, 2.0 ms)"
        ),
        "workload_order_by_round": [],
        "workloads": {},
    }
    if args.linux:
        image = os.environ.get("XSH_TEST_IMAGE", "xsh-test")
        image_id = subprocess.check_output(
            ["docker", "image", "inspect", "--format", "{{.Id}}", image],
            text=True,
        ).strip()
        results["linux_container"] = {
            "image": image,
            "image_id": image_id,
            "platform": "linux/arm64",
            "target": "aarch64-unknown-linux-musl",
            "dockerfile_sha256": sha256_file(os.path.join(HERE, "../../Dockerfile.test")),
            "target_policy_sha256": sha256_file(os.path.join(HERE, "../../dev/targets.xsh")),
            "proc_modules_sha256": fixtures["fixtures/modules-200.txt"],
            "proc_modules_rows": 200,
            "proc_meminfo_sha256": fixtures["fixtures/meminfo.txt"],
            "proc_uptime_sha256": fixtures["fixtures/uptime.txt"],
        }
    sys.path.insert(0, HERE)
    sys.dont_write_bytecode = True
    import parity

    linux_parity = (
        (results["linux_container"]["image_id"], fixtures)
        if args.linux
        else None
    )
    for name, kind, samples, linux_only in WORKLOADS:
        if linux_only and not args.linux:
            continue
        results["workloads"][name] = {
            "kind": kind,
            "samples_per_round": samples,
            "round_data": [],
            "parity": parity.compare_workload(
                name, args.reference, args.candidate, expected_status(name), linux_parity
            )
        }
    skip_reasons = {}
    for round_index in range(args.rounds):
        workloads = WORKLOADS if round_index % 2 == 0 else list(reversed(WORKLOADS))
        selected = [
            workload for workload in workloads if args.linux or not workload[3]
        ]
        linux_round = (
            measure_linux_round(
                args.reference,
                args.candidate,
                selected,
                round_index,
                fixtures,
            )
            if args.linux
            else None
        )
        results["workload_order_by_round"].append([])
        for name, kind, samples, linux_only in workloads:
            if linux_only and not args.linux:
                skip_reasons[name] = (
                    "Linux-only workload; rerun in the pinned container with --linux"
                )
                continue
            results["workload_order_by_round"][-1].append(name)
            round_result = (
                linux_round[name]
                if linux_round is not None
                else measure(args.reference, args.candidate, name, samples, round_index)
            )
            entry = results["workloads"][name]
            entry["round_data"].append(round_result)
            ref = round_result["reference_median_ms"]
            cand = round_result["candidate_median_ms"]
            budget = budget_for(kind, ref)
            delta = cand - ref
            print(
                f"round={round_index + 1} {name:20s} ref={ref:8.3f} cand={cand:8.3f} "
                f"delta={delta:+8.3f} budget={budget:7.3f} "
                f"{'PASS' if delta <= budget else 'FAIL'}",
                flush=True,
            )

    for entry in results["workloads"].values():
        entry.update(summarize_rounds(entry["kind"], entry["round_data"]))
        entry["performance_passed"] = entry["passed"]
        entry["passed"] = entry["performance_passed"] and entry["parity"]["passed"]
    failed = sorted(
        name for name, entry in results["workloads"].items() if not entry["passed"]
    )
    results["failed"] = failed
    results["parity_failed"] = sorted(
        name for name, entry in results["workloads"].items() if not entry["parity"]["passed"]
    )
    results["skipped"] = sorted(skip_reasons)
    results["skip_reasons"] = skip_reasons
    with open(args.out, "w") as handle:
        json.dump(results, handle, indent=2, sort_keys=True)
    if results["parity_failed"]:
        print("parity failures: " + ", ".join(results["parity_failed"]))
    print(f"\n{'all workloads within budget' if not failed else 'failures: ' + ', '.join(failed)}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
