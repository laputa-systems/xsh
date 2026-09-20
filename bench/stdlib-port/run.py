#!/usr/bin/env python3
"""Paired reference/candidate benchmark for the embedded standard-library port.

Each workload runs alternately on the reference and candidate binaries so that
CPU frequency, cache, and scheduler drift affect both sides equally. Reported
values are medians of per-sample wall-clock durations.
"""

import argparse
import json
import os
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))

WORKLOADS = [
    ("cold_trivial", "cold", 60),
    ("cold_quote", "cold", 60),
    ("cold_pad", "cold", 60),
    ("cold_dynamic_ref", "cold", 40),
    ("text_pad_batch", "end_to_end", 30),
    ("quote_batch", "end_to_end", 30),
    ("fmt_batch", "end_to_end", 30),
    ("checksum_batch", "end_to_end", 30),
    ("native_control", "end_to_end", 30),
]


def run_once(binary, script):
    start = time.perf_counter()
    completed = subprocess.run(
        [binary, script],
        cwd=HERE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    elapsed = time.perf_counter() - start
    if completed.returncode != 0:
        raise SystemExit(
            f"{binary} {script} failed ({completed.returncode}): "
            f"{completed.stderr.decode(errors='replace')[:400]}"
        )
    return elapsed * 1000.0


def measure(reference, candidate, name, samples):
    script = os.path.join(HERE, name + ".xsh")
    run_once(reference, script)
    run_once(candidate, script)
    ref, cand = [], []
    for _ in range(samples):
        ref.append(run_once(reference, script))
        cand.append(run_once(candidate, script))
    return statistics.median(ref), statistics.median(cand), ref, cand


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--out", default=os.path.join(HERE, "results.json"))
    parser.add_argument("--rounds", type=int, default=1)
    args = parser.parse_args()

    results = {"reference": args.reference, "candidate": args.candidate, "workloads": {}}
    worst = []
    for round_index in range(args.rounds):
        for name, kind, samples in WORKLOADS:
            ref, cand, ref_raw, cand_raw = measure(
                args.reference, args.candidate, name, samples
            )
            budget = max(0.05 * ref, 1.0) if kind == "cold" else max(0.10 * ref, 2.0)
            delta = cand - ref
            passed = delta <= budget
            entry = {
                "kind": kind,
                "reference_median_ms": ref,
                "candidate_median_ms": cand,
                "delta_ms": delta,
                "budget_ms": budget,
                "passed": passed,
                "reference_samples": ref_raw,
                "candidate_samples": cand_raw,
            }
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
    with open(args.out, "w") as handle:
        json.dump(results, handle, indent=2, sort_keys=True)
    print(f"\n{'all workloads within budget' if not worst else 'failures: ' + ', '.join(worst)}")
    return 1 if worst else 0


if __name__ == "__main__":
    sys.exit(main())
