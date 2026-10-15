"""Bounded cleanup for containers started by the final performance tools."""

from __future__ import annotations

from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import signal
import subprocess
from typing import Any, Callable


CLEANUP_TIMEOUT_SECONDS = 5
_cleanup_active = False


class SignalReceived(Exception):
    def __init__(self, signum: int):
        self.signum = signum
        super().__init__(signal.Signals(signum).name)


class InterruptedRun(Exception):
    def __init__(self, signum: int, cleanup: dict[str, Any]):
        self.signum = signum
        self.cleanup = cleanup
        super().__init__(signal.Signals(signum).name)


class CleanupUnresolved(Exception):
    def __init__(self, cleanup: dict[str, Any]):
        self.cleanup = cleanup
        super().__init__("owned container cleanup could not be verified")


def install_signal_handlers() -> dict[int, Any]:
    previous: dict[int, Any] = {}

    def handle(signum: int, _frame: Any) -> None:
        if _cleanup_active:
            return
        raise SignalReceived(signum)

    for signum in (signal.SIGINT, signal.SIGTERM):
        previous[signum] = signal.signal(signum, handle)
    return previous


def restore_signal_handlers(previous: dict[int, Any]) -> None:
    for signum, handler in previous.items():
        signal.signal(signum, handler)


@contextmanager
def cleanup_window():
    global _cleanup_active
    _cleanup_active = True
    try:
        yield
    finally:
        _cleanup_active = False


def owned_docker_command(
    command: list[str], *, cidfile: Path, owner: str, stage: str
) -> list[str]:
    if command[:2] != ["docker", "run"]:
        raise ValueError("owned container command must start with docker run")
    return command[:2] + [
        "--cidfile", str(cidfile),
        "--label", f"org.xsh.final-performance.owner={owner}",
        "--label", f"org.xsh.final-performance.stage={stage}",
    ] + command[2:]


def _command(
    argv: list[str], *, timeout: int = CLEANUP_TIMEOUT_SECONDS
) -> dict[str, Any]:
    try:
        result = subprocess.run(
            argv,
            text=True,
            capture_output=True,
            check=False,
            timeout=timeout,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return {
            "command": argv,
            "exit_code": None,
            "stdout": "",
            "stderr": str(error),
        }
    return {
        "command": argv,
        "exit_code": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def _owned_ids(owner: str, stage: str, cidfile: Path) -> tuple[list[str], list[dict[str, Any]], bool]:
    commands: list[dict[str, Any]] = []
    ids: list[str] = []
    resolved = True
    if cidfile.is_file():
        try:
            container_id = cidfile.read_text(encoding="utf-8").strip()
        except OSError as error:
            commands.append({"command": ["read", str(cidfile)], "exit_code": None, "stderr": str(error)})
            resolved = False
        else:
            if container_id:
                if not re.fullmatch(r"[0-9a-f]{12,64}", container_id):
                    resolved = False
                else:
                    ids.append(container_id)
    query = [
        "docker", "ps", "--all", "--quiet",
        "--filter", f"label=org.xsh.final-performance.owner={owner}",
        "--filter", f"label=org.xsh.final-performance.stage={stage}",
    ]
    result = _command(query)
    commands.append(result)
    if result["exit_code"] != 0:
        return ids, commands, False
    ids.extend(line.strip() for line in result["stdout"].splitlines() if line.strip())
    return sorted(set(ids)), commands, resolved


def _inspect(container_id: str) -> tuple[dict[str, Any] | None, dict[str, Any]]:
    command = ["docker", "inspect", container_id]
    result = _command(command)
    if result["exit_code"] != 0:
        stderr = result["stderr"].lower()
        absent = "no such object" in stderr or "no such container" in stderr
        return None, result | {"absent": absent}
    try:
        value = json.loads(result["stdout"])[0]
    except (IndexError, json.JSONDecodeError, TypeError):
        return None, result | {"absent": False, "invalid_inspect": True}
    return value, result | {"absent": False}


def cleanup_cidfile_container(
    cidfile: Path, *, owner: str, stage: str
) -> dict[str, Any]:
    ids, commands, resolved = _owned_ids(owner, stage, cidfile)
    cleaned: list[str] = []
    for container_id in ids:
        container, inspect_result = _inspect(container_id)
        commands.append(inspect_result)
        if container is None:
            if inspect_result.get("absent"):
                continue
            resolved = False
            continue
        labels = container.get("Config", {}).get("Labels", {}) or {}
        if (
            labels.get("org.xsh.final-performance.owner") != owner
            or labels.get("org.xsh.final-performance.stage") != stage
        ):
            resolved = False
            continue
        stop = _command(["docker", "stop", "--time", "3", container_id])
        commands.append(stop)
        remove = _command(["docker", "rm", "--force", container_id])
        commands.append(remove)
        remaining, final_inspect = _inspect(container_id)
        commands.append(final_inspect)
        if remaining is not None or not final_inspect.get("absent"):
            resolved = False
        else:
            cleaned.append(container_id)
    return {
        "resolved": resolved,
        "container_ids": ids,
        "cleaned_container_ids": cleaned,
        "commands": commands,
        "cidfile": str(cidfile),
    }


def cleanup_mount_container(
    *, scratch: Path, output_parent: Path, image_id: str
) -> dict[str, Any]:
    query = ["docker", "ps", "--all", "--quiet", "--filter", f"ancestor={image_id}"]
    result = _command(query)
    commands = [result]
    if result["exit_code"] != 0:
        return {"resolved": False, "container_ids": [], "commands": commands}
    owned: list[str] = []
    resolved = True
    scratch_path = str(scratch.resolve())
    output_path = str(output_parent.resolve())
    for container_id in (line.strip() for line in result["stdout"].splitlines() if line.strip()):
        container, inspect_result = _inspect(container_id)
        commands.append(inspect_result)
        if container is None:
            if inspect_result.get("absent"):
                continue
            resolved = False
            continue
        mounts = container.get("Mounts", [])
        has_scratch = any(
            mount.get("Source") == scratch_path and mount.get("Destination") == "/scratch"
            for mount in mounts
        )
        has_output = any(
            mount.get("Source") == output_path and mount.get("Destination") == "/results"
            for mount in mounts
        )
        if container.get("Image") != image_id or not (has_scratch and has_output):
            continue
        owned.append(container_id)
        stop = _command(["docker", "stop", "--time", "3", container_id])
        commands.append(stop)
        remove = _command(["docker", "rm", "--force", container_id])
        commands.append(remove)
        remaining, final_inspect = _inspect(container_id)
        commands.append(final_inspect)
        if remaining is not None or not final_inspect.get("absent"):
            resolved = False
    return {
        "resolved": resolved,
        "container_ids": owned,
        "commands": commands,
        "ownership_mounts": {"scratch": scratch_path, "output": output_path},
    }


def terminate_process_group(process: subprocess.Popen[Any]) -> dict[str, Any]:
    commands: list[dict[str, Any]] = []
    try:
        os.killpg(process.pid, signal.SIGTERM)
        commands.append({"signal": "SIGTERM", "process_group": process.pid})
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=CLEANUP_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
            commands.append({"signal": "SIGKILL", "process_group": process.pid})
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=CLEANUP_TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            return {"resolved": False, "commands": commands}
    return {"resolved": True, "commands": commands}


def wait_for_owned_process(
    process: subprocess.Popen[Any], cleanup: Callable[[], dict[str, Any]]
) -> tuple[int, bytes | None, bytes | None]:
    try:
        stdout, stderr = process.communicate()
    except (SignalReceived, KeyboardInterrupt) as error:
        signum = error.signum if isinstance(error, SignalReceived) else signal.SIGINT
        with cleanup_window():
            terminated = terminate_process_group(process)
            container_cleanup = cleanup()
        cleanup_result = {
            "resolved": terminated["resolved"] and container_cleanup["resolved"],
            "process": terminated,
            "containers": container_cleanup,
        }
        raise InterruptedRun(signum, cleanup_result) from error
    return process.returncode, stdout, stderr


def cleanup_container_process(
    process: subprocess.Popen[Any], cleanup: Callable[[], dict[str, Any]]
) -> dict[str, Any]:
    with cleanup_window():
        terminated = terminate_process_group(process)
        containers = cleanup()
    return {
        "resolved": terminated["resolved"] and containers["resolved"],
        "process": terminated,
        "containers": containers,
    }

