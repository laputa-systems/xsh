#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path


TIME_RE = re.compile(r"([0-9]+(?:\.[0-9]+)?)\s*(ns|µs|ms|s)")
COUNT_RE = re.compile(r"^\s*(?:│\s*)?([0-9]+(?:\.[0-9]+)?)\s+│")
BYTES_RE = re.compile(r"^\s*(?:│\s*)?([0-9]+(?:\.[0-9]+)?)\s+(B|KB|MB|GB)\s+│")
ALLOC_RE = re.compile(r"^\s*(?:│\s*)?alloc:\s+│")
BENCH_RE = re.compile(r"^[├╰]─\s+(\S+)")
RESET = "\033[0m"
RED = "\033[31m"
GREEN = "\033[32m"


def sysctl(name: str) -> str:
    return subprocess.check_output(["sysctl", "-n", name], text=True).strip()


def host_info() -> tuple[str, int]:
    cpus = os.cpu_count() or 1
    system = platform.system()
    if system == "Darwin":
        brand = sysctl("machdep.cpu.brand_string")
        model = next((f"m{i}" for i in (1, 2, 3, 4, 5) if f"M{i}" in brand), "intel")
        memory = int(sysctl("hw.memsize"))
        return f"mac-{model}-{cpus}-{memory // (1 << 30)}gb", cpus
    if system == "Linux":
        memory_kib = 0
        for line in Path("/proc/meminfo").read_text().splitlines():
            if line.startswith("MemTotal:"):
                memory_kib = int(line.split()[1])
                break
        memory_gb = (memory_kib * 1024 + (1 << 30) - 1) // (1 << 30)
        arch = re.sub(r"[^a-z0-9]+", "-", platform.machine().lower()).strip("-")
        return f"linux-{arch}-{cpus}-{memory_gb}gb", cpus
    raise SystemExit("bench-baseline.py supports macOS and Linux")


def time_ns(value: float, unit: str) -> float:
    return value * {"ns": 1, "µs": 1_000, "ms": 1_000_000, "s": 1_000_000_000}[unit]


def byte_count(value: float, unit: str) -> float:
    return value * {"B": 1, "KB": 1024, "MB": 1 << 20, "GB": 1 << 30}[unit]


def parse_report(output: str) -> dict[str, tuple[float, float, float]]:
    values: dict[str, tuple[float, float, float]] = {}
    current: str | None = None
    median: float | None = None
    allocation_count = 0.0
    allocation_bytes = 0.0
    allocation_state = 0

    def flush() -> None:
        if current is not None and median is not None:
            values[current] = (median, allocation_count, allocation_bytes)

    for line in output.splitlines():
        match = BENCH_RE.match(line)
        if match:
            flush()
            current = match.group(1)
            median = None
            allocation_count = 0.0
            allocation_bytes = 0.0
            allocation_state = 0
            pairs = TIME_RE.findall(line)
            if len(pairs) >= 3:
                median = time_ns(float(pairs[2][0]), pairs[2][1])
            continue
        if ALLOC_RE.match(line):
            allocation_state = 1
            continue
        if allocation_state == 1:
            match = COUNT_RE.match(line)
            if match:
                allocation_count = float(match.group(1))
            allocation_state = 2
            continue
        if allocation_state == 2:
            match = BYTES_RE.match(line)
            if match:
                allocation_bytes = byte_count(float(match.group(1)), match.group(2))
            allocation_state = 0
            continue
        if current is not None and median is None:
            pairs = TIME_RE.findall(line)
            if len(pairs) >= 3:
                median = time_ns(float(pairs[2][0]), pairs[2][1])
    flush()
    return values


def read_baseline(path: Path) -> dict[str, tuple[float, float, float]]:
    if not path.exists():
        return {}
    values = {}
    for line in path.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        name, median, counts, allocations = line.split("\t")
        values[name] = (float(median), float(counts), float(allocations))
    return values


def write_baseline(
    path: Path,
    host: str,
    values: dict[str, tuple[float, float, float]],
    warmup_runs: int,
    measured_runs: int,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as file:
            file.write("# benchmark baseline\n")
            file.write(f"# host: {host}\n")
            file.write(
                f"# aggregation: median of {measured_runs} measured runs "
                f"after {warmup_runs} warmup\n"
            )
            file.write("# columns: benchmark median_ns alloc_count alloc_bytes\n")
            for name in sorted(values):
                median, counts, allocations = values[name]
                file.write(f"{name}\t{median:.0f}\t{counts:.0f}\t{allocations:.0f}\n")
        os.replace(temporary, path)
    except BaseException:
        os.unlink(temporary)
        raise


def format_metric(value: float, unit: str) -> str:
    if unit == "ns":
        scales = ((1_000_000_000, "s"), (1_000_000, "ms"), (1_000, "µs"))
    else:
        scales = ((1 << 30, "GB"), (1 << 20, "MB"), (1 << 10, "KB"))
    for scale, label in scales:
        if value >= scale:
            return f"{value / scale:.2f} {label}/op"
    return f"{value:.0f} {unit}/op"


def delta(current: float, previous: float | None, colors: bool) -> str:
    if previous is None:
        return "new"
    if previous == 0:
        return "0.00%" if current == 0 else "new"
    change = (current - previous) / previous * 100
    text = f"{change:+.2f}%"
    if colors and change:
        color = GREEN if change < 0 else RED
        return f"{color}{text}{RESET}"
    return text


def print_table(
    current: dict[str, tuple[float, float, float]],
    previous: dict[str, tuple[float, float, float]],
    reports: list[dict[str, tuple[float, float, float]]],
) -> None:
    colors = sys.stdout.isatty() and "NO_COLOR" not in os.environ
    rows = []
    for name in sorted(current):
        median, counts, allocations = current[name]
        old = previous.get(name)
        old_median, old_counts, old_allocations = old or (None, None, None)
        run_times = [report[name][0] for report in reports]
        spread = (max(run_times) - min(run_times)) / median * 100 if median else 0.0
        rows.append(
            (
                name,
                f"{format_metric(median, 'ns')} ({delta(median, old_median, colors)})",
                f"{spread:.2f}%",
                f"{format_metric(allocations, 'B')} ({delta(allocations, old_allocations, colors)})",
                f"{counts:.0f} ({delta(counts, old_counts, colors)})",
            )
        )
    for name in sorted(set(previous) - set(current)):
        rows.append((name, "removed", "removed", "removed", "removed"))
    headers = ("benchmark", "time", "run spread", "memory", "allocs/op")
    widths = [
        max(len(row[index]) for row in rows + [headers])
        for index in range(len(headers))
    ]
    terminal = shutil.get_terminal_size((160, 24)).columns
    widths[0] = min(widths[0], max(20, terminal - sum(widths[1:]) - 13))
    print("  ".join(header.ljust(width) for header, width in zip(headers, widths)))
    for row in rows:
        print("  ".join(value.ljust(width) for value, width in zip(row, widths)))


def run_suite(root: Path) -> tuple[int, str, dict[str, tuple[float, float, float]]]:
    completed = subprocess.run(
        ["cargo", "bench", "-p", "xsh-multicall", "--bench", "bench"],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return completed.returncode, completed.stdout, parse_report(completed.stdout)


def aggregate_reports(
    reports: list[dict[str, tuple[float, float, float]]],
) -> dict[str, tuple[float, float, float]]:
    names = set(reports[0])
    for report in reports[1:]:
        if set(report) != names:
            missing = sorted(names - set(report))
            added = sorted(set(report) - names)
            details = []
            if missing:
                details.append("missing: " + ", ".join(missing))
            if added:
                details.append("added: " + ", ".join(added))
            raise ValueError("benchmark set changed between runs (" + "; ".join(details) + ")")
    return {
        name: tuple(
            statistics.median(report[name][metric] for report in reports)
            for metric in range(3)
        )
        for name in names
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--variant")
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--print-path", action="store_true")
    parser.add_argument("--warmup-runs", type=int, default=1)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    if args.warmup_runs < 0:
        parser.error("--warmup-runs must be nonnegative")
    if args.runs < 1:
        parser.error("--runs must be positive")
    root = Path(__file__).resolve().parents[1]
    host, _ = host_info()
    suffix = f"-{args.variant}" if args.variant else ""
    baseline = args.baseline or (
        root / "crates" / "xsh-multicall" / "benches" / f"{host}{suffix}-baseline.txt"
    )
    if args.print_path:
        print(baseline)
        return 0

    for run in range(args.warmup_runs):
        if not args.quiet:
            print(f"warmup {run + 1}/{args.warmup_runs}", file=sys.stderr)
        returncode, output, report = run_suite(root)
        if returncode:
            print(output, end="")
            return returncode
        if not report:
            print("benchmark: could not parse warmup Divan output", file=sys.stderr)
            print(output, end="", file=sys.stderr)
            return 1

    reports = []
    for run in range(args.runs):
        if not args.quiet:
            print(f"measured run {run + 1}/{args.runs}", file=sys.stderr)
        returncode, output, report = run_suite(root)
        if returncode:
            print(output, end="")
            return returncode
        if not report:
            print("benchmark: could not parse Divan output", file=sys.stderr)
            print(output, end="", file=sys.stderr)
            return 1
        reports.append(report)
    try:
        current = aggregate_reports(reports)
    except ValueError as error:
        print(f"benchmark: {error}", file=sys.stderr)
        return 1

    previous = read_baseline(baseline)
    if not args.quiet:
        print_table(current, previous, reports)
    write_baseline(baseline, host, current, args.warmup_runs, args.runs)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
