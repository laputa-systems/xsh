#!/usr/bin/env python3
"""Own and instrument the one Docker run used by the canonical gate batch."""

import json
import os
from pathlib import Path
import sys
from datetime import datetime, timezone


NATIVE_IMAGE_ID = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
IMAGE_REFERENCES = {NATIVE_IMAGE_ID, "xsh-test", "xsh-test:consolidation-native"}
CPU_LIMIT = 12
MEMORY_LIMIT = "24g"
RESOURCE_FLAGS = {
    "-c", "-m", "--cpus", "--cpu-period", "--cpu-quota", "--cpu-rt-period",
    "--cpu-rt-runtime", "--cpu-shares", "--cpuset-cpus", "--cpuset-mems",
    "--kernel-memory", "--memory", "--memory-reservation", "--memory-swap",
    "--memory-swappiness",
}
GATE_BATCH_TAIL = [
    "cargo", "run", "--quiet", "--target", "x86_64-unknown-linux-musl",
    "-p", "xsh", "--bin", "xsh", "--",
    "dev/main.xsh", "--", "internal", "gate-batch",
]


def has_resource_override(arguments: list[str]) -> bool:
    index = 0
    while index < len(arguments):
        value = arguments[index]
        if value in RESOURCE_FLAGS or any(value.startswith(flag + "=") for flag in RESOURCE_FLAGS if flag.startswith("--")) or value.startswith(("--cpu-", "--cpuset-", "--memory-")):
            return True
        if value.startswith(("-c", "-m")) and not value.startswith("--"):
            return True
        if value == "--env-file" or value.startswith("--env-file="):
            return True
        if value in ("-e", "--env"):
            index += 1
            if index >= len(arguments) or arguments[index].split("=", 1)[0] == "CARGO_BUILD_JOBS":
                return True
        elif value.startswith("--env="):
            if value[len("--env="):].split("=", 1)[0] == "CARGO_BUILD_JOBS":
                return True
        elif value.startswith("-e") and not value.startswith("--"):
            if value[2:].lstrip("=").split("=", 1)[0] == "CARGO_BUILD_JOBS":
                return True
        index += 1
    return False


def valid_cpuset(value: str) -> bool:
    fields = value.split(",")
    if len(fields) != CPU_LIMIT:
        return False
    try:
        cpus = [int(field) for field in fields]
    except ValueError:
        return False
    return len(set(cpus)) == CPU_LIMIT and all(cpu >= 0 and str(cpu) == field for cpu, field in zip(cpus, fields))


def image_index(arguments: list[str]) -> int | None:
    matches = [index for index, value in enumerate(arguments) if value in IMAGE_REFERENCES]
    return matches[0] if len(matches) == 1 else None


def instrument_batch_argv(
    arguments: list[str],
    *,
    cidfile: str,
    owner: str,
    container_observer: str,
    container_resource_file: str,
    cpuset_cpus: str,
) -> list[str] | None:
    if not arguments or arguments[0] != "run":
        return None
    image_at = image_index(arguments)
    if image_at is None or arguments[image_at + 1:] != GATE_BATCH_TAIL:
        return None
    if has_resource_override(arguments[1:image_at]) or not valid_cpuset(cpuset_cpus):
        return None
    ownership_options = [
        "--cidfile", cidfile,
        "--label", f"org.xsh.final-performance.owner={owner}",
        "--label", "org.xsh.final-performance.stage=canonical-gate-batch",
    ]
    resource_options = [
        "--cpuset-cpus", cpuset_cpus,
        "--cpus", str(CPU_LIMIT),
        "--memory", MEMORY_LIMIT,
        "--memory-swap", MEMORY_LIMIT,
        "-e", f"CARGO_BUILD_JOBS={CPU_LIMIT}",
    ]
    command = [
        "/usr/bin/python3", container_observer, container_resource_file, "--",
        "/root/.cargo/bin/cargo", *GATE_BATCH_TAIL[1:],
    ]
    return arguments[:1] + ownership_options + resource_options + arguments[1:image_at + 1] + command


def append_json_line(path: Path, value: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as output:
        output.write(json.dumps(value, sort_keys=True) + "\n")
        output.flush()
        os.fsync(output.fileno())


def main() -> int:
    real_docker = os.environ.get("XSH_OBSERVER_REAL_DOCKER")
    log_path = os.environ.get("XSH_OBSERVER_DOCKER_LOG")
    owner = os.environ.get("XSH_OBSERVER_RUN_ID")
    cidfile = os.environ.get("XSH_OBSERVER_CIDFILE")
    observer = os.environ.get("XSH_OBSERVER_CONTAINER_SCRIPT")
    resource_file = os.environ.get("XSH_OBSERVER_CONTAINER_RESOURCE")
    cpuset_cpus = os.environ.get("XSH_OBSERVER_CPUSET")
    if not all((real_docker, log_path, owner, cidfile, observer, resource_file, cpuset_cpus)):
        print("Docker observer configuration is incomplete", file=sys.stderr)
        return 78

    original = sys.argv[1:]
    instrumented = instrument_batch_argv(
        original,
        cidfile=cidfile,
        owner=owner,
        container_observer=observer,
        container_resource_file=resource_file,
        cpuset_cpus=cpuset_cpus,
    )
    if original[:1] == ["run"] and instrumented is None:
        print("unexpected Docker run during the canonical gate batch", file=sys.stderr)
        append_json_line(Path(log_path), {
            "time_utc": datetime.now(timezone.utc).isoformat(timespec="milliseconds"),
            "argv": [real_docker, *original],
            "instrumented": False,
            "rejected": True,
        })
        return 78

    effective = instrumented if instrumented is not None else original
    append_json_line(Path(log_path), {
        "time_utc": datetime.now(timezone.utc).isoformat(timespec="milliseconds"),
        "argv": [real_docker, *original],
        "effective_argv": [real_docker, *effective],
        "instrumented": instrumented is not None,
        "rejected": False,
    })
    os.execv(real_docker, [real_docker, *effective])
    return 127


if __name__ == "__main__":
    raise SystemExit(main())
