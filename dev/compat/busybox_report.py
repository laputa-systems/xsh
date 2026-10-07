#!/usr/bin/env python3
"""Convert BusyBox testsuite result lines into the compatibility report schema."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


RESULT = re.compile(r"^(PASS|FAIL|SKIPPED|UNTESTED):\s*(.*?)\s*$")


def parse_log(log: str, utility: str, *, exit_status: int) -> dict:
    counts = {"pass": 0, "fail": 0, "skip": 0, "excluded": 0}
    failing = []
    for line in log.splitlines():
        match = RESULT.match(line)
        if match is None:
            continue
        result, name = match.groups()
        if result == "PASS":
            counts["pass"] += 1
        elif result == "FAIL":
            counts["fail"] += 1
            failing.append(name)
        else:
            counts["skip"] += 1

    reported = sum(counts[name] for name in ("pass", "fail", "skip"))
    if reported == 0:
        if exit_status == 0:
            raise ValueError(f"BusyBox {utility} run did not report any cases")
        counts["fail"] = 1
        failing.append(f"runtest exited with status {exit_status} before reporting cases")
    elif exit_status != 0 and counts["fail"] == 0:
        counts["fail"] = 1
        failing.append(f"runtest exited with status {exit_status} before completion")

    counts["failing"] = failing
    counts["applet"] = True
    return counts


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--archive-sha256", required=True)
    parser.add_argument(
        "--run-manifest",
        type=Path,
        required=True,
        help="tab-separated utility, exit status, and log path rows",
    )
    args = parser.parse_args()

    utilities = {}
    for line in args.run_manifest.read_text().splitlines():
        utility, status, log_path = line.split("\t", 2)
        if utility in utilities:
            raise ValueError(f"duplicate BusyBox result for {utility}")
        log = Path(log_path).read_text(errors="replace")
        utilities[utility] = parse_log(log, utility, exit_status=int(status))

    report = {
        "suite": "busybox",
        "upstream": {"version": args.version, "archive_sha256": args.archive_sha256},
        "utilities": utilities,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    for utility, result in utilities.items():
        print(
            f"{utility}: {result['pass']} pass, {result['fail']} fail, "
            f"{result['skip']} skipped"
        )
    return 1 if any(result["fail"] for result in utilities.values()) else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"BusyBox report: {error}", file=sys.stderr)
        raise SystemExit(2)
