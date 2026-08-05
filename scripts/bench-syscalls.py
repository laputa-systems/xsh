#!/usr/bin/env python3

from __future__ import annotations

import json
import re
import shlex
import subprocess
from pathlib import Path


IMAGE = "xsh-benchmark-syscalls:local"
WORKDIR = "/workspace"
BEGIN_MARKER = 'prctl(PR_SET_NAME, "BENCH_BEGIN"'
END_MARKER = 'prctl(PR_SET_NAME, "BENCH_END"'
SYSCALL_RE = re.compile(
    r"^(?:\[pid\s+\d+\]\s+|\d+\s+)?"
    r"(?:([A-Za-z_][A-Za-z0-9_]*)\(|<\.\.\.\s+([A-Za-z_][A-Za-z0-9_]*)\s+resumed>)"
)


def run(command: list[str], *, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, check=True, text=True, capture_output=capture)


def docker_run(command: str, *, capture: bool = False) -> subprocess.CompletedProcess[str]:
    root = Path(__file__).resolve().parents[1]
    return run(
        [
            "docker",
            "run",
            "--rm",
            "--cap-add=SYS_PTRACE",
            "--security-opt",
            "seccomp=unconfined",
            "-v",
            f"{root}:{WORKDIR}",
            "-v",
            "xsh-benchmark-target:/workspace/target",
            "-v",
            "xsh-cargo-registry:/root/.cargo/registry",
            "-w",
            WORKDIR,
            IMAGE,
            "sh",
            "-lc",
            command,
        ],
        capture=capture,
    )


def build_image() -> None:
    root = Path(__file__).resolve().parents[1]
    run(
        [
            "docker",
            "build",
            "--quiet",
            "-t",
            IMAGE,
            "-f",
            "Dockerfile.test",
            str(root),
        ],
        capture=True,
    )


def benchmark_executable() -> str:
    result = docker_run(
        "cargo bench -p xsh-multicall --bench bench --features benchmark "
        "--no-run --message-format=json",
        capture=True,
    )
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        target = message.get("target", {})
        if message.get("reason") == "compiler-artifact" and target.get("name") == "bench":
            executable = message.get("executable")
            if executable:
                return executable
    raise RuntimeError("could not find Divan benchmark executable")


def benchmark_names(executable: str) -> list[str]:
    result = docker_run(f"{shlex.quote(executable)} --bench --list", capture=True)
    pattern = re.compile(r"^[├╰]─\s+(\S+)")
    return [
        match.group(1)
        for line in result.stdout.splitlines()
        if (match := pattern.match(line))
    ]


def trace_benchmark(executable: str, name: str) -> list[tuple[str, int, int]]:
    command = (
        f"set -eu; SYSCALL_TRACE=1 strace -f -qq -o /tmp/strace-events "
        f"{shlex.quote(executable)} --bench {shlex.quote(name)} "
        "--sample-count 1 --sample-size 1 >/dev/null 2>&1; "
        "cat /tmp/strace-events"
    )
    result = docker_run(command, capture=True)
    totals: dict[str, list[int]] = {}
    active = False
    for line in result.stdout.splitlines():
        if BEGIN_MARKER in line:
            active = True
            continue
        if END_MARKER in line:
            active = False
            continue
        if not active:
            continue
        match = SYSCALL_RE.search(line)
        if not match or " = " not in line:
            continue
        syscall = match.group(1) or match.group(2)
        tally = totals.setdefault(syscall, [0, 0])
        tally[0] += 1
        if re.search(r" = -1(?:\s|$)", line):
            tally[1] += 1
    return [(name, calls, errors) for name, (calls, errors) in totals.items()]


def print_report(results: dict[str, list[tuple[str, int, int]]]) -> None:
    for benchmark, syscalls in results.items():
        print(benchmark)
        print("syscall\tcalls\terrors")
        for syscall, calls, errors in sorted(syscalls, key=lambda row: (-row[1], row[0])):
            print(f"{syscall}\t{calls}\t{errors}")
        print()


def main() -> int:
    build_image()
    executable = benchmark_executable()
    results = {
        name: trace_benchmark(executable, name) for name in benchmark_names(executable)
    }
    print_report(results)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
