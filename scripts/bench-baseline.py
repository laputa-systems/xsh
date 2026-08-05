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
import time
from pathlib import Path


TIME_RE = re.compile(r"([0-9]+(?:\.[0-9]+)?)\s*(ns|µs|ms|s)")
NUMBER_RE = re.compile(r"^[0-9]+(?:\.[0-9]+)?$")
BYTES_CELL_RE = re.compile(r"^([0-9]+(?:\.[0-9]+)?)\s+(B|KB|MB|GB)$")
MAX_ALLOC_RE = re.compile(r"^\s*(?:│\s*)?max alloc:\s*")
ALLOC_RE = re.compile(r"^\s*(?:│\s*)?alloc:\s*")
BENCH_RE = re.compile(r"^[├╰]─\s+(\S+)")
RESET = "\033[0m"
RED = "\033[31m"
GREEN = "\033[32m"

# (median_ns, alloc_count, alloc_bytes, max_alloc_count, max_alloc_bytes)
Metrics = tuple[float, float, float, float, float]
MEDIAN_COLUMN = 2


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


def cells(line: str) -> list[str]:
    return [part.strip() for part in line.split("│")]


def count_columns(line: str) -> list[float] | None:
    values = [float(cell) for cell in cells(line) if NUMBER_RE.match(cell)]
    return values or None


def byte_columns(line: str) -> list[float] | None:
    values = []
    for cell in cells(line):
        match = BYTES_CELL_RE.match(cell)
        if match:
            values.append(byte_count(float(match.group(1)), match.group(2)))
    return values or None


def column_value(values: list[float], index: int = MEDIAN_COLUMN) -> float:
    if len(values) > index:
        return values[index]
    return values[0]


def parse_report(output: str) -> dict[str, Metrics]:
    values: dict[str, Metrics] = {}
    current: str | None = None
    median: float | None = None
    allocation_count = 0.0
    allocation_bytes = 0.0
    max_allocation_count = 0.0
    max_allocation_bytes = 0.0
    # 0 idle, 1 expect max-alloc counts, 2 expect max-alloc bytes,
    # 3 expect alloc counts, 4 expect alloc bytes
    allocation_state = 0

    def flush() -> None:
        if current is not None and median is not None:
            values[current] = (
                median,
                allocation_count,
                allocation_bytes,
                max_allocation_count,
                max_allocation_bytes,
            )

    for line in output.splitlines():
        match = BENCH_RE.match(line)
        if match:
            flush()
            current = match.group(1)
            median = None
            allocation_count = 0.0
            allocation_bytes = 0.0
            max_allocation_count = 0.0
            max_allocation_bytes = 0.0
            allocation_state = 0
            pairs = TIME_RE.findall(line)
            if len(pairs) >= 3:
                median = time_ns(float(pairs[2][0]), pairs[2][1])
            continue
        if MAX_ALLOC_RE.match(line):
            allocation_state = 1
            continue
        if ALLOC_RE.match(line):
            allocation_state = 3
            continue
        if allocation_state == 1:
            counts = count_columns(line)
            if counts is not None:
                max_allocation_count = column_value(counts)
            allocation_state = 2
            continue
        if allocation_state == 2:
            sizes = byte_columns(line)
            if sizes is not None:
                max_allocation_bytes = column_value(sizes)
            allocation_state = 0
            continue
        if allocation_state == 3:
            counts = count_columns(line)
            if counts is not None:
                allocation_count = column_value(counts)
            allocation_state = 4
            continue
        if allocation_state == 4:
            sizes = byte_columns(line)
            if sizes is not None:
                allocation_bytes = column_value(sizes)
            allocation_state = 0
            continue
        if current is not None and median is None:
            pairs = TIME_RE.findall(line)
            if len(pairs) >= 3:
                median = time_ns(float(pairs[2][0]), pairs[2][1])
    flush()
    return values


def read_baseline(path: Path) -> dict[str, Metrics]:
    if not path.exists():
        return {}
    values: dict[str, Metrics] = {}
    for line in path.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) == 4:
            name, median, counts, allocations = parts
            values[name] = (
                float(median),
                float(counts),
                float(allocations),
                0.0,
                0.0,
            )
            continue
        if len(parts) != 6:
            raise ValueError(f"unsupported baseline row in {path}: {line}")
        name, median, counts, allocations, max_counts, max_allocations = parts
        values[name] = (
            float(median),
            float(counts),
            float(allocations),
            float(max_counts),
            float(max_allocations),
        )
    return values


def write_baseline(
    path: Path,
    host: str,
    values: dict[str, Metrics],
    warmup_runs: int,
    measured_runs: int,
    sample_count: int | None,
    sample_size: int | None,
    fast: bool,
    wall_s: float,
    measured_wall_s: float,
    warmup_wall_s: float,
    suite_wall_times_s: list[float],
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as file:
            file.write("# benchmark baseline\n")
            file.write(f"# host: {host}\n")
            file.write(f"# mode: {'fast' if fast else 'normal'}\n")
            file.write(
                f"# aggregation: median of {measured_runs} measured runs "
                f"after {warmup_runs} warmup\n"
            )
            if sample_count is not None:
                file.write(f"# divan_sample_count: {sample_count}\n")
            if sample_size is not None:
                file.write(f"# divan_sample_size: {sample_size}\n")
            file.write(f"# wall_s: {wall_s:.3f}\n")
            file.write(f"# measured_wall_s: {measured_wall_s:.3f}\n")
            file.write(f"# warmup_wall_s: {warmup_wall_s:.3f}\n")
            if suite_wall_times_s:
                file.write(
                    "# measured_suite_wall_s: "
                    + ",".join(f"{value:.3f}" for value in suite_wall_times_s)
                    + "\n"
                )
            file.write(
                "# columns: benchmark median_ns alloc_count alloc_bytes "
                "max_alloc_count max_alloc_bytes\n"
            )
            for name in sorted(values):
                median, counts, allocations, max_counts, max_allocations = values[name]
                file.write(
                    f"{name}\t{median:.0f}\t{counts:.0f}\t{allocations:.0f}\t"
                    f"{max_counts:.0f}\t{max_allocations:.0f}\n"
                )
        os.replace(temporary, path)
    except BaseException:
        os.unlink(temporary)
        raise


def format_metric(value: float, unit: str) -> str:
    if unit == "ns":
        scales = ((1_000_000_000, "s"), (1_000_000, "ms"), (1_000, "µs"))
        for factor, label in scales:
            if value >= factor:
                amount = value / factor
                text = f"{amount:.0f}" if amount >= 100 else f"{amount:.2f}"
                return f"{text} {label}"
        return f"{value:.0f} ns"
    scales = ((1 << 30, "GB"), (1 << 20, "MB"), (1 << 10, "KB"))
    for factor, label in scales:
        if value >= factor:
            amount = value / factor
            text = f"{amount:.0f}" if amount >= 100 else f"{amount:.2f}"
            return f"{text} {label}"
    return f"{value:.0f} B"


def format_wall(seconds: float) -> str:
    if seconds < 1:
        return f"{seconds * 1_000:.0f} ms"
    if seconds < 60:
        return f"{seconds:.2f} s"
    minutes, rem = divmod(seconds, 60)
    if minutes < 60:
        return f"{minutes:.0f}m {rem:04.1f}s"
    hours, minutes = divmod(minutes, 60)
    return f"{hours:.0f}h {minutes:.0f}m {rem:04.1f}s"


def delta(current: float | None, previous: float | None, colors: bool) -> str:
    if previous is None:
        return "new"
    if previous == 0:
        if current == 0:
            return "+0.00%"
        return "new"
    change = (current - previous) / previous * 100
    text = f"{change:+.2f}%"
    if colors and change:
        color = GREEN if change < 0 else RED
        return f"{color}{text}{RESET}"
    return text


def print_table(
    current: dict[str, Metrics],
    previous: dict[str, Metrics],
    reports: list[dict[str, Metrics]],
    *,
    memory_only: bool,
) -> None:
    colors = sys.stdout.isatty() and "NO_COLOR" not in os.environ
    rows = []
    for name in sorted(current):
        median, counts, allocations, max_counts, max_allocations = current[name]
        old = previous.get(name)
        if old is None:
            old_median = old_counts = old_allocations = old_max_counts = old_max_allocations = None
        else:
            old_median, old_counts, old_allocations, old_max_counts, old_max_allocations = old
        memory_cols = (
            f"{format_metric(allocations, 'B')} ({delta(allocations, old_allocations, colors)})",
            f"{counts:.0f} ({delta(counts, old_counts, colors)})",
            f"{format_metric(max_allocations, 'B')} ({delta(max_allocations, old_max_allocations, colors)})",
            f"{max_counts:.0f} ({delta(max_counts, old_max_counts, colors)})",
        )
        if memory_only:
            rows.append((name, *memory_cols))
            continue
        run_times = [report[name][0] for report in reports]
        spread = (max(run_times) - min(run_times)) / median * 100 if median else 0.0
        rows.append(
            (
                name,
                f"{format_metric(median, 'ns')} ({delta(median, old_median, colors)})",
                f"{spread:.2f}%",
                *memory_cols,
            )
        )
    removed = ("removed",) * (4 if memory_only else 6)
    for name in sorted(set(previous) - set(current)):
        rows.append((name, *removed))
    if memory_only:
        headers = (
            "benchmark",
            "memory",
            "allocs/op",
            "peak",
            "peak allocs",
        )
        gutter = 13
    else:
        headers = (
            "benchmark",
            "time",
            "run spread",
            "memory",
            "allocs/op",
            "peak",
            "peak allocs",
        )
        gutter = 19
    widths = [
        max(len(row[index]) for row in rows + [headers])
        for index in range(len(headers))
    ]
    terminal = shutil.get_terminal_size((160, 24)).columns
    widths[0] = min(widths[0], max(20, terminal - sum(widths[1:]) - gutter))
    print("  ".join(header.ljust(width) for header, width in zip(headers, widths)))
    for row in rows:
        print("  ".join(value.ljust(width) for value, width in zip(row, widths)))


def run_suite(
    root: Path,
    sample_count: int | None,
    sample_size: int | None,
) -> tuple[int, str, dict[str, Metrics], float]:
    command = [
        "cargo",
        "bench",
        "-p",
        "xsh-multicall",
        "--bench",
        "bench",
        "--features",
        "benchmark",
    ]
    divan_args: list[str] = []
    if sample_count is not None:
        divan_args.extend(["--sample-count", str(sample_count)])
    if sample_size is not None:
        divan_args.extend(["--sample-size", str(sample_size)])
    if divan_args:
        command.append("--")
        command.extend(divan_args)
    started = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    wall_s = time.perf_counter() - started
    return completed.returncode, completed.stdout, parse_report(completed.stdout), wall_s


def aggregate_reports(reports: list[dict[str, Metrics]]) -> dict[str, Metrics]:
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
            for metric in range(5)
        )
        for name in names
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--variant")
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--print-path", action="store_true")
    parser.add_argument(
        "--fast",
        action="store_true",
        help=(
            "memory-focused defaults: 0 warmup, 1 measured suite, "
            "Divan --sample-count 1 --sample-size 1, and a separate -fast baseline"
        ),
    )
    parser.add_argument("--warmup-runs", type=int)
    parser.add_argument("--runs", type=int)
    parser.add_argument("--sample-count", type=int)
    parser.add_argument("--sample-size", type=int)
    args = parser.parse_args()

    if args.fast:
        warmup_runs = 0 if args.warmup_runs is None else args.warmup_runs
        measured_runs = 1 if args.runs is None else args.runs
        sample_count = 1 if args.sample_count is None else args.sample_count
        sample_size = 1 if args.sample_size is None else args.sample_size
        variant = "fast" if args.variant is None else args.variant
    else:
        warmup_runs = 1 if args.warmup_runs is None else args.warmup_runs
        measured_runs = 3 if args.runs is None else args.runs
        sample_count = args.sample_count
        sample_size = args.sample_size
        variant = args.variant

    if warmup_runs < 0:
        parser.error("--warmup-runs must be nonnegative")
    if measured_runs < 1:
        parser.error("--runs must be positive")
    if sample_count is not None and sample_count < 1:
        parser.error("--sample-count must be positive")
    if sample_size is not None and sample_size < 1:
        parser.error("--sample-size must be positive")

    root = Path(__file__).resolve().parents[1]
    host, _ = host_info()
    suffix = f"-{variant}" if variant else ""
    baseline = args.baseline or (
        root / "crates" / "xsh-multicall" / "benches" / f"{host}{suffix}-baseline.txt"
    )
    if args.print_path:
        print(baseline)
        return 0

    total_started = time.perf_counter()
    warmup_wall_s = 0.0
    measured_wall_s = 0.0
    suite_wall_times_s: list[float] = []

    for run in range(warmup_runs):
        if not args.quiet:
            print(f"warmup {run + 1}/{warmup_runs}", file=sys.stderr)
        returncode, output, report, wall_s = run_suite(root, sample_count, sample_size)
        warmup_wall_s += wall_s
        if not args.quiet:
            print(f"warmup {run + 1}/{warmup_runs} wall {format_wall(wall_s)}", file=sys.stderr)
        if returncode:
            print(output, end="")
            return returncode
        if not report:
            print("benchmark: could not parse warmup Divan output", file=sys.stderr)
            print(output, end="", file=sys.stderr)
            return 1

    reports = []
    for run in range(measured_runs):
        if not args.quiet:
            print(f"measured run {run + 1}/{measured_runs}", file=sys.stderr)
        returncode, output, report, wall_s = run_suite(root, sample_count, sample_size)
        measured_wall_s += wall_s
        suite_wall_times_s.append(wall_s)
        if not args.quiet:
            print(
                f"measured run {run + 1}/{measured_runs} wall {format_wall(wall_s)}",
                file=sys.stderr,
            )
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

    wall_s = time.perf_counter() - total_started
    previous = read_baseline(baseline)
    if not args.quiet:
        print_table(current, previous, reports, memory_only=args.fast)
    # Always emit whole-suite wall timing so iteration cost is visible even in
    # --quiet mode used by PGO and automation.
    print(
        f"wall {format_wall(wall_s)} "
        f"(measured {format_wall(measured_wall_s)} over {measured_runs}; "
        f"warmup {format_wall(warmup_wall_s)} over {warmup_runs})",
        file=sys.stderr,
    )

    write_baseline(
        baseline,
        host,
        current,
        warmup_runs,
        measured_runs,
        sample_count,
        sample_size,
        args.fast,
        wall_s,
        measured_wall_s,
        warmup_wall_s,
        suite_wall_times_s,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
