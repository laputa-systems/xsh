#!/usr/bin/env python3
"""Own and instrument the one Docker run used by the canonical gate batch."""

import json
import os
from pathlib import Path
import sys
from datetime import datetime, timezone


NATIVE_IMAGE_ID = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
IMAGE_REFERENCES = {NATIVE_IMAGE_ID, "xsh-test", "xsh-test:consolidation-native"}
GATE_BATCH_TAIL = [
    "cargo", "run", "--quiet", "--target", "x86_64-unknown-linux-musl",
    "-p", "xsh", "--bin", "xsh", "--",
    "dev/main.xsh", "--", "internal", "gate-batch",
]


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
) -> list[str] | None:
    if not arguments or arguments[0] != "run":
        return None
    image_at = image_index(arguments)
    if image_at is None or arguments[image_at + 1:] != GATE_BATCH_TAIL:
        return None
    ownership_options = [
        "--cidfile", cidfile,
        "--label", f"org.xsh.final-performance.owner={owner}",
        "--label", "org.xsh.final-performance.stage=canonical-gate-batch",
    ]
    command = [
        "/usr/bin/python3", container_observer, container_resource_file, "--",
        "/root/.cargo/bin/cargo", *GATE_BATCH_TAIL[1:],
    ]
    return arguments[:1] + ownership_options + arguments[1:image_at + 1] + command


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
    if not all((real_docker, log_path, owner, cidfile, observer, resource_file)):
        print("Docker observer configuration is incomplete", file=sys.stderr)
        return 78

    original = sys.argv[1:]
    instrumented = instrument_batch_argv(
        original,
        cidfile=cidfile,
        owner=owner,
        container_observer=observer,
        container_resource_file=resource_file,
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
