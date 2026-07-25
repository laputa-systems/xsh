#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
from pathlib import Path


RESET = "\033[0m"
RED = "\033[31m"
GREEN = "\033[32m"


def read_baseline(path: Path) -> dict[str, tuple[float, float, float]]:
    values = {}
    for line in path.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        name, median, counts, allocations = line.split("\t")
        values[name] = (float(median), float(counts), float(allocations))
    return values


def format_time(value: float) -> str:
    if value < 1_000:
        return f"{value:.0f} ns/op"
    if value < 1_000_000:
        return f"{value / 1_000:.2f} µs/op"
    if value < 1_000_000_000:
        return f"{value / 1_000_000:.2f} ms/op"
    return f"{value / 1_000_000_000:.2f} s/op"


def format_bytes(value: float) -> str:
    if value < 1024:
        return f"{value:.0f} B/op"
    if value < 1 << 20:
        return f"{value / 1024:.2f} KB/op"
    if value < 1 << 30:
        return f"{value / (1 << 20):.2f} MB/op"
    return f"{value / (1 << 30):.2f} GB/op"


def delta(candidate: float | None, baseline: float | None, colors: bool) -> str:
    if candidate is None:
        return "removed"
    if baseline is None:
        return "new"
    if baseline == 0:
        change = 0.0 if candidate == 0 else None
    else:
        change = (candidate - baseline) / baseline * 100
    if change is None:
        return "new"
    text = f"{change:+.2f}%"
    if colors and change:
        color = GREEN if change < 0 else RED
        return f"{color}{text}{RESET}"
    return text


def main() -> int:
    parser = argparse.ArgumentParser(description="Compare two benchmark baselines")
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    args = parser.parse_args()
    for path in (args.baseline, args.candidate):
        if not path.is_file():
            parser.error(f"baseline not found: {path}")
    baseline = read_baseline(args.baseline)
    candidate = read_baseline(args.candidate)
    colors = os.environ.get("NO_COLOR") is None and os.isatty(1)
    print(f"{args.baseline} → {args.candidate}")
    print("benchmark\ttime\tmemory\tallocs/op")
    for name in sorted(set(baseline) | set(candidate)):
        old = baseline.get(name)
        new = candidate.get(name)
        old_time, old_count, old_bytes = old or (None, None, None)
        new_time, new_count, new_bytes = new or (None, None, None)
        time = (
            f"{format_time(new_time)} ({delta(new_time, old_time, colors)})"
            if new_time is not None
            else "removed"
        )
        memory = (
            f"{format_bytes(new_bytes)} ({delta(new_bytes, old_bytes, colors)})"
            if new_bytes is not None
            else "removed"
        )
        count = (
            f"{new_count:.0f} ({delta(new_count, old_count, colors)})"
            if new_count is not None
            else "removed"
        )
        print(f"{name}\t{time}\t{memory}\t{count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
