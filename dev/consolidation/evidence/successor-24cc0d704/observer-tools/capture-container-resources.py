#!/usr/bin/env python3
"""Run one command and retain its in-container cgroup and RSS evidence."""

from datetime import datetime, timezone
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


CGROUP_ROOT = Path("/sys/fs/cgroup")
PROC_ROOT = Path("/proc")
SAMPLE_INTERVAL_SECONDS = 0.1


def read_text(path: Path) -> str | None:
    try:
        return path.read_text(encoding="utf-8").strip()
    except OSError:
        return None


def parse_key_values(value: str | None) -> dict[str, int] | None:
    if value is None:
        return None
    result = {}
    for line in value.splitlines():
        fields = line.split()
        if len(fields) == 2:
            try:
                result[fields[0]] = int(fields[1])
            except ValueError:
                continue
    return result


def process_rss_bytes(proc_root: Path = PROC_ROOT) -> tuple[int, int]:
    page_size = os.sysconf("SC_PAGE_SIZE")
    total = 0
    process_count = 0
    try:
        entries = proc_root.iterdir()
        for entry in entries:
            if not entry.name.isdecimal():
                continue
            try:
                fields = (entry / "statm").read_text(encoding="ascii").split()
                resident_pages = int(fields[1])
            except (OSError, IndexError, ValueError):
                continue
            total += resident_pages * page_size
            process_count += 1
    except OSError:
        return 0, 0
    return total, process_count


def cgroup_snapshot(cgroup_root: Path = CGROUP_ROOT) -> dict[str, object]:
    events = parse_key_values(read_text(cgroup_root / "memory.events"))
    swap_events = parse_key_values(read_text(cgroup_root / "memory.swap.events"))
    return {
        "memory_current": read_text(cgroup_root / "memory.current"),
        "memory_peak": read_text(cgroup_root / "memory.peak"),
        "memory_max": read_text(cgroup_root / "memory.max"),
        "memory_swap_current": read_text(cgroup_root / "memory.swap.current"),
        "memory_swap_peak": read_text(cgroup_root / "memory.swap.peak"),
        "memory_swap_max": read_text(cgroup_root / "memory.swap.max"),
        "memory_events": events,
        "memory_swap_events": swap_events,
        "cpu_max": read_text(cgroup_root / "cpu.max"),
        "cpuset_cpus_effective": read_text(cgroup_root / "cpuset.cpus.effective"),
        "pids_current": read_text(cgroup_root / "pids.current"),
        "pids_peak": read_text(cgroup_root / "pids.peak"),
        "pids_max": read_text(cgroup_root / "pids.max"),
    }


def snapshot(cgroup_root: Path = CGROUP_ROOT, proc_root: Path = PROC_ROOT) -> dict[str, object]:
    rss_bytes, process_count = process_rss_bytes(proc_root)
    return {
        "sampled_utc": datetime.now(timezone.utc).isoformat(timespec="milliseconds"),
        "process_rss_sum_bytes": rss_bytes,
        "visible_process_count": process_count,
        "cgroup": cgroup_snapshot(cgroup_root),
    }


def atomic_json(path: Path, value: object) -> None:
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(temporary, path)


def run(output_path: Path, command: list[str], *, resources_only: bool = False) -> int:
    if not command:
        raise ValueError("a command is required after --")
    initial = snapshot()
    rss_peak = int(initial["process_rss_sum_bytes"])
    samples = 1
    child: subprocess.Popen[bytes] | None = None
    received_signal: int | None = None

    def forward_signal(signum: int, _frame: object) -> None:
        nonlocal received_signal
        received_signal = signum
        if child is not None and child.poll() is None:
            try:
                os.killpg(child.pid, signum)
            except ProcessLookupError:
                pass

    previous = {signum: signal.signal(signum, forward_signal) for signum in (signal.SIGINT, signal.SIGTERM)}
    started = None if resources_only else time.perf_counter_ns()
    status: int | None = None
    spawn_error: str | None = None
    try:
        child = subprocess.Popen(command, start_new_session=True)
        while True:
            try:
                status = child.wait(timeout=SAMPLE_INTERVAL_SECONDS)
                break
            except subprocess.TimeoutExpired:
                sample = snapshot()
                rss_peak = max(rss_peak, int(sample["process_rss_sum_bytes"]))
                samples += 1
    except OSError as error:
        spawn_error = str(error)
        status = 127
    finally:
        if child is not None and child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                status = child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                status = child.wait()
        final = snapshot()
        rss_peak = max(rss_peak, int(final["process_rss_sum_bytes"]))
        samples += 1
        for signum, handler in previous.items():
            signal.signal(signum, handler)

    elapsed = time.perf_counter_ns() - started if started is not None else None
    timing_accepted = not resources_only and status == 0 and received_signal is None and spawn_error is None
    cgroup = final["cgroup"]
    receipt = {
        "schema_version": 1,
        "execution_mode": "resources-only" if resources_only else "timed",
        "command": command,
        "status": status,
        "spawn_error": spawn_error,
        "received_signal": signal.Signals(received_signal).name if received_signal else None,
        "wall_ns": elapsed if timing_accepted else None,
        "timing_discarded": not timing_accepted,
        "resource_sample_interval_ms": int(SAMPLE_INTERVAL_SECONDS * 1000),
        "process_rss_peak_bytes": rss_peak,
        "process_rss_peak_method": "sum of resident pages for visible container processes, sampled every 100 ms",
        "resource_samples": samples,
        "initial": initial,
        "final": final,
        "cgroup_limits_and_peaks": cgroup,
        "oom_events": (cgroup.get("memory_events") or {}).get("oom"),
        "oom_kill_events": (cgroup.get("memory_events") or {}).get("oom_kill"),
        "swap_events": cgroup.get("memory_swap_events"),
    }
    atomic_json(output_path, receipt)
    return status if status is not None else 1


def main() -> int:
    arguments = sys.argv[1:]
    resources_only = arguments[:1] == ["--resources-only"]
    if resources_only:
        arguments = arguments[1:]
    if len(arguments) < 3 or arguments[1] != "--":
        print("usage: capture-container-resources.py [--resources-only] OUTPUT.json -- COMMAND...", file=sys.stderr)
        return 2
    try:
        return run(Path(arguments[0]), arguments[2:], resources_only=resources_only)
    except Exception as error:
        print(f"container resource observer failed: {error}", file=sys.stderr)
        return 125


if __name__ == "__main__":
    raise SystemExit(main())
