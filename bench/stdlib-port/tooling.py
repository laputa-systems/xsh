#!/usr/bin/env python3
"""Paired, uninstrumented measurements of the three fixed xsht commands."""

import argparse
import base64
import json
import os
import statistics
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from run import sha256_file, summarize_rounds


HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../.."))
COMMANDS = {
    "xsht_api": ("api", "summary"),
    "xsht_check_core": ("check", "core/ls.xsh"),
    "xsht_lint_core": ("lint", "core/ls.xsh"),
}


def invoke(binary, args, linux=None, capture=True):
    command = [binary, *args]
    if linux is not None:
        command = [
            "docker", "run", "--rm", "--platform", "linux/arm64",
            "-v", f"{ROOT}:/work:ro", "-v", f"{binary}:/bench/xsht:ro",
            "-w", "/work", linux, "/bench/xsht", *args,
        ]
    start = time.perf_counter()
    result = subprocess.run(
        command,
        cwd=ROOT,
        stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    elapsed_ms = (time.perf_counter() - start) * 1000
    return result.returncode, result.stdout if capture else b"", result.stderr, elapsed_ms


def parity(binary_ref, binary_cand, args, linux):
    reference = invoke(binary_ref, args, linux)
    candidate = invoke(binary_cand, args, linux)
    observed = {}
    for side, result in [("reference", reference), ("candidate", candidate)]:
        observed[side] = {
            "status": result[0],
            "stdout_base64": base64.b64encode(result[1]).decode("ascii"),
            "stderr_base64": base64.b64encode(result[2]).decode("ascii"),
        }
    matches = {
        "status": reference[0] == candidate[0] == 0,
        "stdout": reference[1] == candidate[1],
        "stderr": reference[2] == candidate[2],
    }
    return {**observed, "matches": matches, "passed": all(matches.values())}


def host_round(reference, candidate, samples, round_index):
    rows = {}
    for name, args in COMMANDS.items():
        warmup_order = ["reference", "candidate"]
        if round_index % 2:
            warmup_order.reverse()
        for side in warmup_order:
            binary = reference if side == "reference" else candidate
            status, _, stderr, _ = invoke(binary, args, capture=False)
            if status != 0:
                raise RuntimeError(f"{name} {side} warmup exited {status}: {stderr[-400:]!r}")
        row = {
            "index": round_index,
            "warmup_order": warmup_order,
            "sample_order": [],
            "reference_samples_ms": [],
            "candidate_samples_ms": [],
        }
        for index in range(samples):
            order = ["reference", "candidate"]
            if (round_index + index) % 2:
                order.reverse()
            row["sample_order"].append(order)
            for side in order:
                binary = reference if side == "reference" else candidate
                status, _, stderr, elapsed_ms = invoke(binary, args, capture=False)
                if status != 0:
                    raise RuntimeError(f"{name} {side} exited {status}: {stderr[-400:]!r}")
                row[f"{side}_samples_ms"].append(elapsed_ms)
        row["reference_median_ms"] = statistics.median(row["reference_samples_ms"])
        row["candidate_median_ms"] = statistics.median(row["candidate_samples_ms"])
        rows[name] = row
    return rows


def linux_round(reference, candidate, runner_xsh, samples, round_index, image_id):
    command = [
        "docker", "run", "--rm", "--platform", "linux/arm64",
        "-v", f"{ROOT}:/work:ro",
        "-v", f"{reference}:/bench/reference/xsht:ro",
        "-v", f"{candidate}:/bench/candidate/xsht:ro",
        "-v", f"{runner_xsh}:/bench/candidate/xsh:ro",
        "-w", "/work", image_id,
        "/bench/candidate/xsh", "/work/bench/stdlib-port/tooling-runner.xsh", "--",
        "/bench/reference/xsht", "/bench/candidate/xsht",
        str(round_index), str(samples), ",".join(COMMANDS),
    ]
    result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError(f"Linux tooling round failed: {result.stderr[-1200:]!r}")
    rows = {}
    expected = []
    for name in COMMANDS:
        warmup_order = ["reference", "candidate"]
        if round_index % 2:
            warmup_order.reverse()
        row = {
            "index": round_index,
            "warmup_order": [],
            "sample_order": [[] for _ in range(samples)],
            "reference_samples_ms": [],
            "candidate_samples_ms": [],
        }
        rows[name] = row
        expected.extend((name, "warmup", 0, side) for side in warmup_order)
        for index in range(samples):
            order = ["reference", "candidate"]
            if (round_index + index) % 2:
                order.reverse()
            expected.extend((name, "sample", index, side) for side in order)
    actual = []
    for line in result.stdout.decode().splitlines():
        parts = line.split("\t")
        if len(parts) != 5:
            raise RuntimeError(f"bad Linux tooling row: {line!r}")
        name, phase, index_text, side, ns_text = parts
        try:
            index, ns = int(index_text), int(ns_text)
        except ValueError as error:
            raise RuntimeError(f"bad Linux tooling measurement: {line!r}") from error
        if name not in rows or side not in ("reference", "candidate") or ns < 0:
            raise RuntimeError(f"bad Linux tooling side or duration: {line!r}")
        row = rows[name]
        actual.append((name, phase, index, side))
        if phase == "warmup":
            row["warmup_order"].append(side)
        elif phase == "sample" and 0 <= index < samples:
            row["sample_order"][index].append(side)
            row[f"{side}_samples_ms"].append(ns / 1_000_000)
    if actual != expected:
        raise RuntimeError("Linux tooling emitted an incomplete or reordered sample sequence")
    for row in rows.values():
        row["reference_median_ms"] = statistics.median(row["reference_samples_ms"])
        row["candidate_median_ms"] = statistics.median(row["candidate_samples_ms"])
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True, help="absolute path to reference xsht")
    parser.add_argument("--candidate", required=True, help="absolute path to candidate xsht")
    parser.add_argument("--candidate-xsh", help="candidate Linux xsh, required with --linux")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--samples", type=int, default=30)
    parser.add_argument("--linux", action="store_true")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    if args.rounds < 1 or args.samples < 1:
        parser.error("--rounds and --samples must be positive")
    for path in [args.reference, args.candidate]:
        if not os.path.isabs(path) or not os.path.isfile(path):
            parser.error("xsht binaries must be existing absolute paths")
    if args.linux and (
        not args.candidate_xsh
        or not os.path.isabs(args.candidate_xsh)
        or not os.path.isfile(args.candidate_xsh)
    ):
        parser.error("--linux requires an existing absolute --candidate-xsh path")
    image = None
    if args.linux:
        image = os.environ.get("XSH_TEST_IMAGE", "xsh-test")
        image_id = subprocess.check_output(
            ["docker", "image", "inspect", "--format", "{{.Id}}", image], text=True
        ).strip()
    results = {
        "binaries": {
            "reference": {"path": args.reference, "sha256": sha256_file(args.reference)},
            "candidate": {"path": args.candidate, "sha256": sha256_file(args.candidate)},
        },
        "commands": {name: list(command) for name, command in COMMANDS.items()},
        "rounds": args.rounds,
        "samples_per_round": args.samples,
        "workloads": {},
    }
    if args.linux:
        results["linux_container"] = {
            "image": image,
            "image_id": image_id,
            "platform": "linux/arm64",
            "target": "aarch64-unknown-linux-musl",
            "dockerfile_sha256": sha256_file(os.path.join(ROOT, "Dockerfile.test")),
            "target_policy_sha256": sha256_file(os.path.join(ROOT, "dev/targets.xsh")),
            "runner_xsh_sha256": sha256_file(args.candidate_xsh),
        }
    for name, command in COMMANDS.items():
        results["workloads"][name] = {
            "round_data": [],
            "parity": parity(args.reference, args.candidate, command, image_id if args.linux else None),
        }
    for round_index in range(args.rounds):
        if args.linux:
            rows = linux_round(
                args.reference, args.candidate, args.candidate_xsh,
                args.samples, round_index, image_id,
            )
        else:
            rows = host_round(args.reference, args.candidate, args.samples, round_index)
        for name, row in rows.items():
            results["workloads"][name]["round_data"].append(row)
            print(f"round={round_index + 1} {name}: {row['reference_median_ms']:.3f} / {row['candidate_median_ms']:.3f} ms")
    for entry in results["workloads"].values():
        entry.update(summarize_rounds("end_to_end", entry["round_data"]))
        entry["performance_passed"] = entry["passed"]
        entry["passed"] = entry["performance_passed"] and entry["parity"]["passed"]
    results["failed"] = sorted(name for name, entry in results["workloads"].items() if not entry["passed"])
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(results, handle, indent=2, sort_keys=True)
    return 1 if results["failed"] else 0


def self_test():
    with tempfile.TemporaryDirectory(prefix="xsh-tooling-") as directory:
        binary = os.path.join(directory, "xsht")
        with open(binary, "w", encoding="ascii") as handle:
            handle.write("#!/bin/sh\nprintf '%s\\n' \"$*\"\n")
        os.chmod(binary, 0o755)
        assert parity(binary, binary, COMMANDS["xsht_api"], None)["passed"]
        row = host_round(binary, binary, 2, 0)
        assert list(row) == list(COMMANDS)
        assert all(len(item["reference_samples_ms"]) == 2 for item in row.values())
    print("tooling self-test: three paired commands and parity passed")


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        self_test()
    else:
        sys.exit(main())
