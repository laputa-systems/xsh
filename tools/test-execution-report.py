#!/usr/bin/env python3
"""Count executed and skipped tests from completed Rust and xsht test logs."""

import argparse
import json
import re
from collections import defaultdict
from datetime import date
from pathlib import Path


RUST_CASE = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored(?:, .*)?)$")
XSH_CASE = re.compile(r"^(\S+) \.\.\. (ok|FAILED|skipped(?:: .*)?) [\d.]+(?:ms|s)$")
RUST_SUMMARY = re.compile(
    r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;"
)
XSH_SUMMARY = re.compile(
    r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) skipped$"
)


def parse_log(path: Path, kind: str) -> dict:
    case_pattern = RUST_CASE if kind == "rust" else XSH_CASE
    summary_pattern = RUST_SUMMARY if kind == "rust" else XSH_SUMMARY
    counts = {"passed": 0, "failed": 0, "skipped": 0}
    skips = defaultdict(list)
    summaries = []
    for line in path.read_text().splitlines():
        summary = summary_pattern.match(line)
        if summary:
            summaries.append(tuple(map(int, summary.groups())))
        case = case_pattern.match(line)
        if not case:
            continue
        name, result = case.groups()
        if result == "ok":
            counts["passed"] += 1
        elif result == "FAILED":
            counts["failed"] += 1
        else:
            counts["skipped"] += 1
            reason = result.partition(", ")[2] if kind == "rust" else result.partition(": ")[2]
            skips[reason or "unspecified"].append(name)
    observed = (counts["passed"], counts["failed"], counts["skipped"])
    if len(summaries) != 1 or observed != summaries[0]:
        raise ValueError(f"{path}: case lines do not match one complete test summary")
    return {
        **counts,
        "executed": counts["passed"] + counts["failed"],
        "discovered": sum(counts.values()),
        "skip_reasons": [
            {"reason": reason, "count": len(names), "tests": sorted(names)}
            for reason, names in sorted(skips.items())
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--run", nargs=3, action="append", metavar=("PLATFORM", "KIND", "LOG"), required=True
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    runs = []
    seen = set()
    for platform, kind, path_text in args.run:
        if platform not in ("macos-arm64", "linux-arm64-musl") or kind not in (
            "rust-interactive", "native-all"
        ):
            parser.error(f"unsupported platform or gate: {platform} {kind}")
        if (platform, kind) in seen:
            parser.error(f"duplicate gate: {platform} {kind}")
        seen.add((platform, kind))
        log_kind = "rust" if kind == "rust-interactive" else "xsh"
        runs.append({"platform": platform, "gate": kind, **parse_log(Path(path_text), log_kind)})
    report = {
        "schema_version": 1,
        "snapshot_date": date.today().isoformat(),
        "gates": {
            "rust-interactive": "cargo test --test integration runtime::interactive -- --test-threads=1",
            "native-all": "xsht test --jobs 1",
        },
        "runs": runs,
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
