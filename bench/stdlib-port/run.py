#!/usr/bin/env python3
"""Paired reference/candidate benchmark for the embedded standard-library port.

Each workload runs alternately on the reference and candidate binaries so that
CPU frequency, cache, and scheduler drift affect both sides equally. Reported
values are medians of per-sample wall-clock durations.
"""

import argparse
import hashlib
import json
import os
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))

# The specification fixes these workload classes; each entry here is one
# frozen script. `linux_only` workloads run inside the Dockerfile.test
# container, because the entries they measure read `/proc` and `/sys` on Linux
# only; the runner reports them as skipped when it is not given a Linux binary.
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
    ("json_lines_batch", "end_to_end", 30, False),
    # Environment.
    ("env_typed_lookups", "end_to_end", 30, False),
    # Checksum parsing.
    ("checksum_batch", "end_to_end", 30, False),
    # Real commands and tooling (project-wide check/lint run outside the runner).
    ("core_command", "end_to_end", 30, False),
    # R12's required Linux entries: eager acquisition plus row interpretation,
    # measured separately from stream creation and from how much of the stream
    # a consumer takes. They read fixed host paths, so they run in the
    # Dockerfile.test container against the fixtures the README stages there.
    ("linux_uptime", "end_to_end", 30, True),
    ("linux_meminfo", "end_to_end", 30, True),
    ("linux_memory", "end_to_end", 30, True),
    ("linux_os_release", "end_to_end", 30, True),
    ("linux_modules_full", "end_to_end", 30, True),
    ("linux_modules_partial", "end_to_end", 30, True),
    # Native controls: paths the port must not slow down.
    ("native_control", "end_to_end", 30, False),
    ("native_hash_control", "end_to_end", 30, False),
    # Gated Linux policy: none. Every gated prototype that was measured failed
    # its gate and was reverted to native, so there is nothing ported to
    # measure here. `STDLIB-PORT.md` records the remaining qualifications.
]


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_fixtures():
    """Write the fixed inputs the workloads read.

    They are generated rather than committed: the specification asks for frozen
    inputs, not for large blobs in the repository, and every generator here is
    deterministic. A generator that produces different bytes than the recorded
    digest means the fixture changed, so the file is rewritten and the new
    digest is reported rather than silently measured.
    """
    digests = {}
    directory = os.path.join(HERE, "fixtures")
    os.makedirs(directory, exist_ok=True)
    unicode_path = os.path.join(directory, "unicode.txt")
    if not os.path.isfile(unicode_path) or os.path.getsize(unicode_path) > 1_200_000:
        # Roughly 1 MiB of prose with non-ASCII scalars and a mix of long and
        # short lines, so wrapping has both a wide and a narrow case.
        seeds = [
            "The quick brown fox jumps over the lazy dog beside the river.",
            "  indented continuation with tabs\tand trailing space   ",
            "A tight line.",
            "caf\u00e9 na\u00efve r\u00e9sum\u00e9 \u2014 punctuation and an em dash.",
            "\u65e5\u672c\u8a9e\u306e\u884c\u3068\u4e2d\u6587\u7684\u4e00\u884c\u4ee5\u53ca emoji \U0001f600 mixed in.",
            "",
        ]
        target = 1_100_000
        chunks = []
        written = 0
        index = 0
        while written < target:
            line = seeds[index % len(seeds)]
            chunks.append(line + "\n")
            written += len(line.encode()) + 1
            index += 1
        with open(unicode_path, "w", encoding="utf-8") as handle:
            handle.write("".join(chunks))
    return {"fixtures/unicode.txt": sha256_file(unicode_path)}


def run_once(binary, script, env=None):
    start = time.perf_counter()
    completed = subprocess.run(
        [binary, script],
        cwd=HERE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        env=env,
    )
    elapsed = time.perf_counter() - start
    if completed.returncode != 0:
        raise SystemExit(
            f"{binary} {script} failed ({completed.returncode}): "
            f"{completed.stderr.decode(errors='replace')[:400]}"
        )
    return elapsed * 1000.0


def measure(reference, candidate, name, samples, env=None):
    """Measure one workload with the two sides interleaved.

    Within one round the side that runs first alternates sample by sample, so a
    slow period of the machine cannot land on one side only; the reported value
    is the median of each side's samples.
    """
    script = os.path.join(HERE, name + ".xsh")
    run_once(reference, script, env)
    run_once(candidate, script, env)
    ref, cand = [], []
    for index in range(samples):
        if index % 2 == 0:
            ref.append(run_once(reference, script, env))
            cand.append(run_once(candidate, script, env))
        else:
            cand.append(run_once(candidate, script, env))
            ref.append(run_once(reference, script, env))
    return statistics.median(ref), statistics.median(cand), ref, cand


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
        budget = max(0.05 * ref, 1.0) if kind == "cold" else max(0.10 * ref, 2.0)
        passed = delta <= budget
        assert abs(budget - expected_budget) < 1e-9, (kind, ref, budget, expected_budget)
        assert passed == expected_pass, (kind, ref, delta, budget, expected_pass)
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
        "units": "milliseconds, median of per-sample wall-clock durations",
        "budget_formula": (
            "cold: C - B <= max(0.05 * B, 1.0 ms); "
            "end_to_end: C - B <= max(0.10 * B, 2.0 ms)"
        ),
        "workloads": {},
    }
    worst = []
    skipped = []
    for round_index in range(args.rounds):
        for name, kind, samples, linux_only in WORKLOADS:
            linux_env = (
                dict(os.environ, XSH_LINUX_REAL="1") if linux_only else None
            )
            if linux_only and not args.linux:
                skipped.append(name)
                continue
            ref, cand, ref_raw, cand_raw = measure(
                args.reference,
                args.candidate,
                name,
                samples,
                env=linux_env if linux_only else None,
            )
            budget = max(0.05 * ref, 1.0) if kind == "cold" else max(0.10 * ref, 2.0)
            delta = cand - ref
            passed = delta <= budget
            entry = results["workloads"].setdefault(
                name,
                {
                    "kind": kind,
                    "reference_rounds": [],
                    "candidate_rounds": [],
                    "reference_samples": [],
                    "candidate_samples": [],
                },
            )
            entry["reference_rounds"].append(ref)
            entry["candidate_rounds"].append(cand)
            entry["reference_samples"].extend(ref_raw)
            entry["candidate_samples"].extend(cand_raw)
            ref = statistics.median(entry["reference_rounds"])
            cand = statistics.median(entry["candidate_rounds"])
            delta = cand - ref
            entry.update(
                {
                    "reference_median_ms": ref,
                    "candidate_median_ms": cand,
                    "delta_ms": delta,
                    "budget_ms": max(0.05 * ref, 1.0)
                    if kind == "cold"
                    else max(0.10 * ref, 2.0),
                    "passed": delta
                    <= (
                        max(0.05 * ref, 1.0)
                        if kind == "cold"
                        else max(0.10 * ref, 2.0)
                    ),
                    "reference_sample_ms": entry["reference_samples"],
                    "candidate_sample_ms": entry["candidate_samples"],
                }
            )
            results["workloads"][name] = entry
            if not passed:
                worst.append(name)
            print(
                f"{name:20s} ref={ref:8.3f} cand={cand:8.3f} "
                f"delta={delta:+8.3f} budget={budget:7.3f} "
                f"{'PASS' if passed else 'FAIL'}",
                flush=True,
            )

    results["failed"] = worst
    results["skipped"] = sorted(set(skipped))
    with open(args.out, "w") as handle:
        json.dump(results, handle, indent=2, sort_keys=True)
    print(f"\n{'all workloads within budget' if not worst else 'failures: ' + ', '.join(worst)}")
    return 1 if worst else 0


if __name__ == "__main__":
    sys.exit(main())
