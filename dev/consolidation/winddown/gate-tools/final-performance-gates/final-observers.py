#!/usr/bin/env python3
"""Measure isolated repository lint and the canonical full Linux gate batch."""

from contextlib import contextmanager
from datetime import datetime, timezone
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from typing import Any, Callable

sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

from owned_containers import (
    InterruptedRun,
    SignalReceived,
    cleanup_cidfile_container,
    cleanup_container_process,
    install_signal_handlers,
    restore_signal_handlers,
)


ROOT = Path("/home/josh/d/laputa-systems/xsh")
HERE = ROOT / ".work/final-review-evidence/final-performance-gates"
INFRA = ROOT / ".work/consolidation/infra"
TARGET = "x86_64-unknown-linux-musl"
NATIVE_IMAGE_ID = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
IMAGE_REFERENCES = (NATIVE_IMAGE_ID, "xsh-test", "xsh-test:consolidation-native")
CPU_LIMIT = 12
MEMORY_LIMIT = "24g"
MEMORY_LIMIT_BYTES = 24 * 1024**3
BINARY_NAMES = ("xsh", "xsht", "xsh-test-helper")
RELEASE_OPT_LEVEL = 3
PROFILE_LOCK = INFRA / "final-performance.lock"
NATIVE_SUITE_LOCK = INFRA / "native-suite.lock"
NATIVE_CLEANUP_MARKER = INFRA / "native-suite.cleanup-unresolved"
CARGO_LOCK = INFRA / "cargo.lock"
ARTIFACT_MANIFEST = HERE / "final-artifact-manifest.json"
ARTIFACT_MANIFEST_DIGEST = HERE / "final-artifact-manifest.json.sha256"
FINAL_CORRECTNESS_DIR = ROOT / ".work/final-review-evidence/final-correctness-gates"
FINAL_CORRECTNESS_LAUNCHER = FINAL_CORRECTNESS_DIR / "launch.py"
SENSITIVE_ENV_NAME = re.compile(r"(TOKEN|PASSWORD|SECRET|CREDENTIAL|AUTH|COOKIE|PRIVATE|KEY)", re.IGNORECASE)


class ObserverFailure(RuntimeError):
    pass


def sha256(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def atomic_write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary_path = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=path.parent,
            prefix=f".{path.name}.", suffix=".tmp", delete=False,
        ) as temporary:
            temporary_path = Path(temporary.name)
            temporary.write(content)
            temporary.flush()
            os.fsync(temporary.fileno())
        os.replace(temporary_path, path)
        directory_fd = os.open(path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)


def json_write(path: Path, value: object) -> None:
    atomic_write_text(path, json.dumps(value, indent=2, sort_keys=True) + "\n")


@contextmanager
def deferred_signals():
    """Record interrupts while a cleanup receipt and ownership marker are finalized."""
    pending: dict[str, Any] = {"first": None, "counts": {}}
    previous: dict[int, Any] = {}

    def capture(signum: int, _frame: Any) -> None:
        name = signal.Signals(signum).name
        if pending["first"] is None:
            pending["first"] = signum
        pending["counts"][name] = pending["counts"].get(name, 0) + 1

    try:
        for signum in (signal.SIGINT, signal.SIGTERM):
            previous[signum] = signal.signal(signum, capture)
        yield pending
    finally:
        for signum, handler in previous.items():
            signal.signal(signum, handler)


def unique_json_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ObserverFailure(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def digest_sidecar(path: Path) -> str:
    try:
        fields = path.read_text(encoding="ascii").split()
    except OSError as error:
        raise ObserverFailure(f"cannot read hash sidecar {path}: {error}") from error
    if not fields or not re.fullmatch(r"[0-9a-f]{64}", fields[0]):
        raise ObserverFailure(f"invalid SHA-256 sidecar: {path}")
    return fields[0]


def verify_artifact_pins() -> dict[str, Any]:
    expected_manifest_hash = digest_sidecar(ARTIFACT_MANIFEST_DIGEST)
    observed_manifest_hash = sha256(ARTIFACT_MANIFEST)
    if observed_manifest_hash != expected_manifest_hash:
        raise ObserverFailure("final artifact manifest hash differs from its sidecar")
    manifest = json.loads(ARTIFACT_MANIFEST.read_text(encoding="utf-8"), object_pairs_hook=unique_json_object)
    pins = manifest.get("performance_observers", {}).get("files", {})
    required = (
        "final-observers.py",
        "capture-container-resources.py",
        "docker-observer-shim.py",
        "owned_containers.py",
        "test_final_observers.py",
        "launch-plan.md",
    )
    if set(pins) != set(required):
        raise ObserverFailure("performance observer pins do not name the complete launcher, helper, self-test, and checklist set")
    observed = {}
    for relative in required:
        path = HERE / relative
        actual = sha256(path)
        if actual != pins[relative]:
            raise ObserverFailure(f"performance observer hash mismatch for {relative}")
        observed[relative] = actual
    return {"manifest_sha256": observed_manifest_hash, "file_hashes": observed}


def source_identity(tree: Path, commit: str) -> dict[str, str]:
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ObserverFailure("source commit must be a full lowercase Git ID")
    head = subprocess.check_output(["git", "-C", str(tree), "rev-parse", "HEAD"], text=True).strip()
    if head != commit:
        raise ObserverFailure(f"source tree HEAD {head} differs from requested commit {commit}")
    tree_id = subprocess.check_output(["git", "-C", str(tree), "rev-parse", "HEAD^{tree}"], text=True).strip()
    status = subprocess.check_output(["git", "-C", str(tree), "status", "--porcelain=v1", "--untracked-files=all"], text=True)
    if status:
        raise ObserverFailure("source tree must be clean before final observer gates")
    return {"commit": head, "git_tree": tree_id, "status": "clean"}


def functional_close(path: Path, commit: str) -> dict[str, Any]:
    record = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_json_object)
    if record.get("functional_complete") is not True or record.get("commit") != commit:
        raise ObserverFailure("functional-close record must say complete and name the exact source commit")
    return record


def file_snapshot(path: Path) -> dict[str, Any]:
    try:
        info = path.lstat()
    except OSError as error:
        raise ObserverFailure(f"candidate binary is missing: {path}: {error}") from error
    if not stat.S_ISREG(info.st_mode) or not os.access(path, os.X_OK):
        raise ObserverFailure(f"candidate binary must be a regular executable file: {path}")
    return {
        "path": str(path.resolve()),
        "size_bytes": info.st_size,
        "mode": f"0o{stat.S_IMODE(info.st_mode):04o}",
        "sha256": sha256(path),
    }


def binary_set(directory: Path) -> dict[str, dict[str, Any]]:
    return {name: file_snapshot(directory / name) for name in BINARY_NAMES}


def sealed_binary_set(directory: Path, expected: dict[str, dict[str, Any]]) -> dict[str, dict[str, Any]]:
    observed = binary_set(directory)
    if observed != expected:
        raise ObserverFailure("sealed selected-release binaries changed during an observer stage")
    return observed


def load_profile_binaries(receipt_path: Path, tree: Path, commit: str, opt_level: int) -> dict[str, Any]:
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"), object_pairs_hook=unique_json_object)
    if receipt.get("state") != "complete" or receipt.get("source_commit") != commit:
        raise ObserverFailure("profile-cost receipt must be complete and name the exact source commit")
    if Path(receipt.get("source_tree", "")).resolve() != tree.resolve():
        raise ObserverFailure("profile-cost receipt names a different source tree")
    current = source_identity(tree, commit)
    if receipt.get("source_identity") != current:
        raise ObserverFailure("profile-cost receipt source identity differs from the frozen source")
    if opt_level != RELEASE_OPT_LEVEL:
        raise ObserverFailure("final observers require the selected release opt3 profile")
    profile = receipt.get("profile")
    if not isinstance(profile, dict) or any(
        profile.get(name) != expected
        for name, expected in (
            ("name", "release"),
            ("lto", "thin"),
            ("codegen_units", 16),
            ("opt_levels", [RELEASE_OPT_LEVEL]),
        )
    ):
        raise ObserverFailure("profile-cost receipt must identify the selected release opt3 profile")
    results = receipt.get("results")
    if not isinstance(results, list) or len(results) != 1 or not isinstance(results[0], dict) or results[0].get("name") != f"release-opt{RELEASE_OPT_LEVEL}":
        raise ObserverFailure("profile-cost receipt must contain exactly the selected release opt3 profile")
    result = results[0]
    if result.get("profile") != "release" or result.get("opt_level") != RELEASE_OPT_LEVEL or result.get("lto") != "thin" or result.get("codegen_units") != 16:
        raise ObserverFailure("selected profile-cost result has an unexpected release profile")
    recorded = result.get("binaries")
    if not isinstance(recorded, dict) or set(recorded) != set(BINARY_NAMES):
        raise ObserverFailure("selected profile-cost result must bind xsh, xsht, and xsh-test-helper")
    paths = {Path(recorded[name].get("path", "")).resolve() for name in BINARY_NAMES}
    if len(paths) != 3 or len({path.parent for path in paths}) != 1:
        raise ObserverFailure("selected release binaries must share one profile output directory")
    observed = binary_set(next(iter(paths)).parent)
    for name in BINARY_NAMES:
        if observed[name]["path"] != str(Path(recorded[name]["path"]).resolve()) or observed[name]["sha256"] != recorded[name].get("sha256"):
            raise ObserverFailure(f"selected release binary differs from the profile-cost receipt: {name}")
    return {
        "receipt": {"path": str(receipt_path.resolve()), "sha256": sha256(receipt_path)},
        "profile": {"name": result["name"], "profile": result["profile"], "opt_level": opt_level, "lto": result.get("lto")},
        "directory": str(next(iter(paths)).parent),
        "binaries": observed,
    }


def native_image(docker: str) -> dict[str, Any]:
    output = subprocess.check_output([docker, "image", "inspect", NATIVE_IMAGE_ID], text=True)
    info = json.loads(output)[0]
    if info.get("Id") != NATIVE_IMAGE_ID or info.get("Architecture") != "amd64" or info.get("Os") != "linux":
        raise ObserverFailure("native image must match the pinned x86_64 Linux image")
    return {"image_id": info["Id"], "architecture": info["Architecture"], "os": info["Os"]}


def no_active_native_containers(docker: str, image_id: str) -> None:
    output = subprocess.check_output([
        docker, "ps", "--filter", f"ancestor={image_id}", "--format", "{{.ID}} {{.Names}}",
    ], text=True).strip()
    if output:
        raise ObserverFailure("a pinned native build or test container is active; final gate timing needs an isolated window")


def cpu_list(cpu_ids: list[int]) -> str:
    if len(cpu_ids) < CPU_LIMIT:
        raise ObserverFailure(f"isolated lint requires at least {CPU_LIMIT} host-allowed CPUs")
    return ",".join(str(cpu) for cpu in cpu_ids[:CPU_LIMIT])


@contextmanager
def exclusive_lock(path: Path, *, reject_unresolved: bool = False):
    stream = path.open("a+")
    try:
        fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        stream.close()
        raise ObserverFailure(f"another build, suite, or final gate owns {path}") from error
    try:
        if reject_unresolved:
            stream.seek(0)
            marker = stream.read()
            if marker.startswith("cleanup-unresolved:"):
                raise ObserverFailure(f"owned-container cleanup remains unresolved in {path}: {marker.strip()}")
        yield stream
    finally:
        stream.close()


def retain_unresolved_lock(stream: Any, run_id: str, cleanup: dict[str, Any], stage: str) -> None:
    stream.seek(0)
    stream.truncate()
    stream.write("cleanup-unresolved:" + json.dumps({"run_id": run_id, "stage": stage, "cleanup": cleanup}, sort_keys=True) + "\n")
    stream.flush()
    os.fsync(stream.fileno())


def retain_native_cleanup_marker(run_id: str, cleanup: dict[str, Any], stage: str) -> None:
    atomic_write_text(
        NATIVE_CLEANUP_MARKER,
        "cleanup-unresolved:" + json.dumps({"run_id": run_id, "stage": stage, "cleanup": cleanup}, sort_keys=True) + "\n",
    )


def perf_gate_check(after: bool = False) -> dict[str, Any]:
    command = ["perf-gate-ready"] + (["-1"] if after else [])
    try:
        result = subprocess.run(command, text=True, capture_output=True, check=False, timeout=60)
        return {"command": command, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"command": command, "exit_code": None, "stdout": "", "stderr": str(error)}


def resource_receipt(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise ObserverFailure(f"container resource receipt is missing: {path}")
    value = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_json_object)
    if value.get("schema_version") != 1:
        raise ObserverFailure("container resource receipt has an unsupported schema")
    cgroup = value.get("cgroup_limits_and_peaks")
    if not isinstance(cgroup, dict):
        raise ObserverFailure("container resource receipt lacks cgroup limits and peaks")
    required = ("memory_peak", "memory_max", "memory_swap_peak", "memory_swap_max", "memory_events", "memory_swap_events", "cpu_max")
    missing = [name for name in required if name not in cgroup]
    if missing:
        raise ObserverFailure(f"container resource receipt lacks required cgroup fields: {missing}")
    unavailable = [name for name in required if cgroup[name] is None]
    if unavailable:
        raise ObserverFailure(f"container resource receipt has unavailable cgroup fields: {unavailable}")
    if not isinstance(cgroup["memory_events"], dict) or not {"oom", "oom_kill"}.issubset(cgroup["memory_events"]):
        raise ObserverFailure("container resource receipt lacks memory OOM event counters")
    if not isinstance(cgroup["memory_swap_events"], dict):
        raise ObserverFailure("container resource receipt lacks swap event counters")
    if value.get("status") != 0 or value.get("timing_discarded") is not False:
        raise ObserverFailure("container command or its resource observer did not complete successfully")
    if not isinstance(value.get("process_rss_peak_bytes"), int) or value["process_rss_peak_bytes"] <= 0:
        raise ObserverFailure("container resource receipt lacks a sampled RSS peak")
    return {"path": str(path), "sha256": sha256(path), "receipt": value}


def source_check(tree: Path, expected: dict[str, str]) -> dict[str, str]:
    observed = source_identity(tree, expected["commit"])
    if observed != expected:
        raise ObserverFailure("source identity changed during an observer stage")
    return observed


def signal_counts(pending: dict[str, Any]) -> dict[str, Any]:
    first = pending["first"]
    return {
        "first": signal.Signals(first).name if first is not None else None,
        "counts": dict(sorted(pending["counts"].items())),
    }


def mark_stage_interrupted(stage: dict[str, Any], signum: int) -> None:
    stage["interrupted"] = True
    stage.setdefault("signal", signal.Signals(signum).name)
    stage["observation_accepted"] = False
    stage["timing_accepted"] = False
    stage["timing_discarded"] = True
    if stage.get("wall_ns") is not None:
        stage["wall_ns_discarded"] = stage["wall_ns"]
        stage["wall_ns"] = None


def persist_stage_receipt(
    stage_dir: Path,
    stage: dict[str, Any],
    *,
    ownership_stream: Any,
    run_id: str,
    stage_name: str,
    pending: dict[str, Any],
) -> None:
    receipt_path = stage_dir / "receipt.json"

    def write() -> None:
        cleanup = stage.get("container_cleanup")
        if cleanup is not None and not cleanup.get("resolved", True):
            retain_unresolved_lock(ownership_stream, run_id, cleanup, stage_name)
            retain_native_cleanup_marker(run_id, cleanup, stage_name)
        state = "interrupted" if stage.get("interrupted") else "complete" if stage.get("observation_accepted") else "failed"
        json_write(receipt_path, {"schema_version": 1, "state": state, "stage": stage})

    stage["deferred_signals"] = signal_counts(pending)
    if pending["first"] is not None:
        mark_stage_interrupted(stage, pending["first"])
    write()

    observed_signals = signal_counts(pending)
    if observed_signals != stage["deferred_signals"]:
        stage["deferred_signals"] = observed_signals
        if pending["first"] is not None:
            mark_stage_interrupted(stage, pending["first"])
        write()


def cleanup_outcome(cleanup: Callable[[], dict[str, Any]]) -> dict[str, Any]:
    try:
        return cleanup()
    except Exception as error:
        return {"resolved": False, "container_ids": [], "commands": [], "error": str(error)}


def finalize_normal_stage(
    stage: dict[str, Any],
    stage_dir: Path,
    *,
    process: subprocess.Popen[Any] | None,
    status: int | None,
    elapsed: int | None,
    after: dict[str, Any] | None,
    run_id: str,
    stage_name: str,
    container_id_file: Path,
    resource_file: Path,
    ownership_stream: Any,
    validate_after: Callable[[], dict[str, Any]],
    pending: dict[str, Any],
) -> None:
    def clean_owned_work() -> dict[str, Any]:
        if process is not None and process.poll() is None:
            return cleanup_container_process(
                process,
                lambda: cleanup_cidfile_container(container_id_file, owner=run_id, stage=stage_name),
            )
        return cleanup_cidfile_container(container_id_file, owner=run_id, stage=stage_name)

    cleanup = cleanup_outcome(clean_owned_work)
    if cleanup.get("resolved"):
        try:
            container_id_file.unlink(missing_ok=True)
        except OSError as error:
            cleanup["resolved"] = False
            cleanup["cidfile_error"] = str(error)
    stage["status"] = status
    stage["performance_gate"] = {"before": stage["performance_gate"]["before"], "after": after}
    stage["container_cleanup"] = cleanup
    stage["stdout"] = log_snapshot(stage_dir / "stdout.log")
    stage["stderr"] = log_snapshot(stage_dir / "stderr.log")
    stage["container_resources"] = None
    resource_error = None
    try:
        stage["container_resources"] = resource_receipt(resource_file)
    except (OSError, ValueError, ObserverFailure) as error:
        resource_error = str(error)
    stage["container_resource_error"] = resource_error
    validation = None
    validation_error = None
    try:
        validation = validate_after()
    except Exception as error:
        validation_error = str(error)
    stage["post_stage_validation"] = validation
    stage["post_stage_validation_error"] = validation_error
    observation_accepted = (
        status == 0
        and cleanup.get("resolved") is True
        and resource_error is None
        and validation_error is None
    )
    timing_accepted = (
        observation_accepted
        and stage["performance_gate"]["before"]["exit_code"] == 0
        and after is not None
        and after["exit_code"] == 0
    )
    stage["observation_accepted"] = observation_accepted
    stage["timing_accepted"] = timing_accepted
    stage["timing_discarded"] = not timing_accepted
    stage["wall_ns"] = elapsed if timing_accepted else None
    if elapsed is not None and not timing_accepted:
        stage["wall_ns_discarded"] = elapsed
    if not timing_accepted:
        if stage["performance_gate"]["before"]["exit_code"] != 0:
            stage["timing_discard_reason"] = "pre-stage readiness failed; stage ran for resource observations only"
        elif after is not None and after["exit_code"] != 0:
            stage["timing_discard_reason"] = "post-stage readiness failed"
        elif not observation_accepted:
            stage["timing_discard_reason"] = "stage observations did not complete successfully"
    persist_stage_receipt(
        stage_dir, stage, ownership_stream=ownership_stream,
        run_id=run_id, stage_name=stage_name, pending=pending,
    )


def run_stage(
    command: list[str],
    stage_dir: Path,
    *,
    run_id: str,
    stage_name: str,
    environment: dict[str, str],
    container_id_file: Path,
    resource_file: Path,
    ownership_stream: Any,
    validate_after: Callable[[], dict[str, Any]],
) -> dict[str, Any]:
    stdout_path = stage_dir / "stdout.log"
    stderr_path = stage_dir / "stderr.log"
    cidfile = container_id_file
    cidfile.unlink(missing_ok=True)
    before = perf_gate_check()
    stage: dict[str, Any] = {
        "name": stage_name,
        "command": command,
        "environment": {key: ("<redacted>" if SENSITIVE_ENV_NAME.search(key) else value) for key, value in sorted(environment.items())},
        "stdout_path": str(stdout_path),
        "stderr_path": str(stderr_path),
        "performance_gate": {"before": before, "after": None},
        "status": None,
        "wall_ns": None,
        "observation_accepted": False,
        "timing_accepted": False,
        "timing_discarded": True,
    }
    process = None
    started = time.perf_counter_ns() if before["exit_code"] == 0 else None
    spawn_error = None
    try:
        with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
            try:
                process = subprocess.Popen(command, stdout=stdout, stderr=stderr, env=environment, start_new_session=True)
                status = process.wait()
            except OSError as error:
                spawn_error = error
            if spawn_error is None:
                elapsed = time.perf_counter_ns() - started if started is not None else None
                with deferred_signals() as pending:
                    after = perf_gate_check(after=True) if started is not None else None
                    stdout.flush()
                    stderr.flush()
                    finalize_normal_stage(
                        stage, stage_dir, process=process, status=status,
                        elapsed=elapsed, after=after, run_id=run_id,
                        stage_name=stage_name, container_id_file=cidfile,
                        resource_file=resource_file, ownership_stream=ownership_stream,
                        validate_after=validate_after, pending=pending,
                    )
    except SignalReceived as error:
        with deferred_signals() as pending:
            process_alive = process is not None and process.poll() is None
            if process_alive:
                cleanup = cleanup_outcome(lambda: cleanup_container_process(
                    process,
                    lambda: cleanup_cidfile_container(cidfile, owner=run_id, stage=stage_name),
                ))
            else:
                containers = cleanup_outcome(lambda: cleanup_cidfile_container(cidfile, owner=run_id, stage=stage_name))
                cleanup = {
                    "resolved": containers["resolved"],
                    "process": {"resolved": True, "commands": []},
                    "containers": containers,
                }
            after = perf_gate_check(after=True) if started is not None else None
            stage.update({
                "interrupted": True,
                "signal": signal.Signals(error.signum).name,
                "performance_gate": {"before": before, "after": after},
                "container_cleanup": cleanup,
                "status": None,
                "wall_ns": None,
                "observation_accepted": False,
                "timing_accepted": False,
                "timing_discarded": True,
                "container_resources": None,
                "container_resource_error": "resource validation was skipped after interruption",
            })
            stage["stdout"] = log_snapshot(stdout_path)
            stage["stderr"] = log_snapshot(stderr_path)
            if cleanup.get("resolved"):
                try:
                    cidfile.unlink(missing_ok=True)
                except OSError as cleanup_error:
                    cleanup["resolved"] = False
                    cleanup["cidfile_error"] = str(cleanup_error)
            persist_stage_receipt(
                stage_dir, stage, ownership_stream=ownership_stream,
                run_id=run_id, stage_name=stage_name, pending=pending,
            )
        return stage
    if spawn_error is not None:
        elapsed = time.perf_counter_ns() - started if started is not None else None
        status = None
        stage["spawn_error"] = str(spawn_error)
        with deferred_signals() as pending:
            after = perf_gate_check(after=True) if started is not None else None
            finalize_normal_stage(
                stage, stage_dir, process=process, status=status,
                elapsed=elapsed, after=after, run_id=run_id,
                stage_name=stage_name, container_id_file=cidfile,
                resource_file=resource_file, ownership_stream=ownership_stream,
                validate_after=validate_after, pending=pending,
            )
    return stage


def log_snapshot(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"path": str(path), "sha256": None, "size_bytes": 0}
    return {"path": str(path), "sha256": sha256(path), "size_bytes": path.stat().st_size}


def lint_command(
    docker: str,
    tree: Path,
    profile_dir: Path,
    output_dir: Path,
    cpus: str,
    run_id: str,
) -> list[str]:
    observer = "/work/.work/final-review-evidence/final-performance-gates/capture-container-resources.py"
    cidfile = output_dir / "container.cid"
    resource_file = output_dir / "container-resources.json"
    resource_in_container = "/observer/container-resources.json"
    return [
        docker, "run", "--rm", "--init",
        "--cidfile", str(cidfile),
        "--label", f"org.xsh.final-performance.owner={run_id}",
        "--label", "org.xsh.final-performance.stage=isolated-repository-lint",
        "--platform", "linux/amd64",
        "--cpuset-cpus", cpus,
        "--cpus", str(CPU_LIMIT),
        "--memory", MEMORY_LIMIT,
        "--memory-swap", MEMORY_LIMIT,
        "--network", "none",
        "--tmpfs", "/tmp:rw,exec,nosuid,mode=1777",
        "-v", f"{tree}:/work:ro",
        "-v", f"{profile_dir}:/release:ro",
        "-v", f"{output_dir}:/observer:rw",
        "-w", "/work",
        "-e", "LC_ALL=C",
        "-e", "TZ=UTC",
        "-e", "TMPDIR=/tmp",
        "-e", "XSH_BIN=/release/xsh",
        "-e", "PATH=/release:/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        NATIVE_IMAGE_ID,
        "/usr/bin/python3", observer, resource_in_container, "--", "/release/xsht", "lint",
    ]


def gate_batch_command() -> list[str]:
    return ["cargo", "dev", "gate", "--batch"]


def ensure_native_marker_clear() -> None:
    if NATIVE_CLEANUP_MARKER.exists():
        raise ObserverFailure(f"canonical native-suite cleanup marker exists: {NATIVE_CLEANUP_MARKER}")


def manifest_relative_output(output: Path, tree: Path) -> str:
    try:
        return output.resolve().relative_to(tree.resolve()).as_posix()
    except ValueError as error:
        raise ObserverFailure("observer output must be inside the sealed source tree") from error


def paths_overlap(first: Path, second: Path) -> bool:
    return first == second or first in second.parents or second in first.parents


def gate_target_directory(
    requested: Path | None,
    *,
    output: Path,
    tree: Path,
    selected_release_directory: Path,
    source_worktrees: Path,
    sealed_inputs: tuple[Path, ...],
) -> tuple[Path, bool]:
    if requested is None:
        target = output / "gate-batch-target"
        if target.exists() or target.is_symlink():
            raise ObserverFailure("fresh observer output already contains a Cargo target directory")
        return target, False

    absolute = requested.absolute()
    current = Path(absolute.anchor)
    try:
        for component in absolute.parts[1:]:
            current = current / component
            metadata = current.lstat()
            if stat.S_ISLNK(metadata.st_mode):
                raise ObserverFailure("Cargo target directory must not contain symlink components")
        target = absolute.resolve(strict=True)
    except ObserverFailure:
        raise
    except (OSError, RuntimeError) as error:
        raise ObserverFailure(f"Cargo target directory must already exist: {requested}: {error}") from error

    if not target.is_dir():
        raise ObserverFailure("Cargo target directory must already be an existing directory")
    work_root = (ROOT / ".work").resolve(strict=True)
    try:
        target.relative_to(work_root)
    except ValueError as error:
        raise ObserverFailure("Cargo target directory must be below the source .work directory") from error
    if target == work_root:
        raise ObserverFailure("Cargo target directory must be below the source .work directory")

    protected_paths = (
        ("source tree", tree, True),
        ("source Git directory", tree / ".git", True),
        ("source worktrees", source_worktrees, False),
        ("observer output", output, False),
        ("selected release binaries", selected_release_directory, False),
        *(("sealed input", path, False) for path in sealed_inputs),
    )
    for label, protected, exact_only in protected_paths:
        protected = protected.resolve(strict=False)
        if target == protected or (not exact_only and paths_overlap(target, protected)):
            raise ObserverFailure(f"Cargo target directory must not overlap the {label}")
    return target, True


def gate_target_build_input(requested: Path | None, actual: Path, reused: bool) -> dict[str, Any]:
    return {
        "requested_path": str(requested) if requested is not None else None,
        "actual_path": str(actual),
        "reused_existing_directory": reused,
        "existing_contents_are_untrusted_until_cargo_revalidates_them": reused,
    }


def prepare_docker_shim(output: Path, environment: dict[str, str], docker: str, run_id: str, batch_stage: Path) -> Path:
    shim_dir = output / "host-bin"
    shim_dir.mkdir()
    shim_path = shim_dir / "docker"
    shutil.copyfile(HERE / "docker-observer-shim.py", shim_path)
    shim_path.chmod(0o755)
    environment["XSH_OBSERVER_REAL_DOCKER"] = docker
    environment["XSH_OBSERVER_DOCKER_LOG"] = str(batch_stage / "docker-commands.jsonl")
    environment["XSH_OBSERVER_RUN_ID"] = run_id
    environment["XSH_OBSERVER_CIDFILE"] = str(batch_stage / "container.cid")
    environment["XSH_OBSERVER_CONTAINER_SCRIPT"] = "/work/.work/final-review-evidence/final-performance-gates/capture-container-resources.py"
    output_relative = manifest_relative_output(batch_stage / "container-resources.json", ROOT)
    environment["XSH_OBSERVER_CONTAINER_RESOURCE"] = "/work/" + output_relative
    environment["PATH"] = str(shim_dir) + os.pathsep + environment["PATH"]
    return shim_path


def read_docker_observer_log(path: Path) -> list[dict[str, Any]]:
    if not path.is_file():
        return []
    return [json.loads(line, object_pairs_hook=unique_json_object) for line in path.read_text(encoding="utf-8").splitlines() if line]


def run(args: argparse.Namespace) -> int:
    tree = args.tree.resolve()
    output = args.output.resolve()
    if tree != ROOT.resolve():
        raise ObserverFailure("this launcher is bound to the real-main checkout")
    if output.parent != HERE.resolve() or output.exists() or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", output.name):
        raise ObserverFailure("choose a new output directory directly under final-performance-gates")
    if Path("/tmp") in output.parents:
        raise ObserverFailure("observer output must remain under the ignored artifact root")
    if sys.platform != "linux" or not hasattr(os, "sched_getaffinity"):
        raise ObserverFailure("final observers require the Linux host that owns the pinned Docker image")

    pins = verify_artifact_pins()
    supplemental_handoff = plan_data()["post_batch_supplemental_launcher"]
    source = source_identity(tree, args.commit)
    close = functional_close(args.functional_close.resolve(), args.commit)
    selected = load_profile_binaries(args.profile_receipt.resolve(), tree, args.commit, args.release_opt_level)
    docker = shutil.which("docker")
    cargo = shutil.which("cargo")
    if docker is None or cargo is None:
        raise ObserverFailure("docker and cargo must both be present on PATH")
    docker = str(Path(docker).resolve())
    native = native_image(docker)
    cpus_allowed = sorted(os.sched_getaffinity(0))
    lint_cpus = cpu_list(cpus_allowed)
    target_dir, reused_target = gate_target_directory(
        args.gate_target_dir,
        output=output,
        tree=tree,
        selected_release_directory=Path(selected["directory"]),
        source_worktrees=ROOT / ".work/wt",
        sealed_inputs=(
            args.functional_close.resolve(),
            args.profile_receipt.resolve(),
            ARTIFACT_MANIFEST,
            ARTIFACT_MANIFEST_DIGEST,
            FINAL_CORRECTNESS_DIR,
            INFRA,
        ),
    )
    output.mkdir(parents=True, mode=0o755)
    lint_dir = output / "isolated-repository-lint"
    batch_dir = output / "canonical-gate-batch"
    scratch_dir = output / "tmp"
    for directory in (lint_dir, batch_dir, scratch_dir):
        directory.mkdir()
    if not reused_target:
        target_dir.mkdir()
    run_id = secrets.token_hex(16)
    source_path = manifest_relative_output(output, tree)
    baseline_environment = os.environ.copy()
    baseline_environment.update({
        "TARGET": TARGET,
        "DIST_PROFILE": "release",
        "CARGO_TARGET_DIR": str(target_dir),
        "TMPDIR": str(scratch_dir),
        "LC_ALL": "C",
        "TZ": "UTC",
        "XSH_TEST_IMAGE": NATIVE_IMAGE_ID,
        "XSH_TEST_IMAGE_BUILD": "0",
        "DOCKER_PLATFORM": "linux/amd64",
    })
    host_env = dict(baseline_environment)
    batch_shim = prepare_docker_shim(output, host_env, docker, run_id, batch_dir)
    batch_env = dict(host_env)
    batch_env["PATH"] = str(batch_shim.parent) + os.pathsep + os.environ.get("PATH", "")
    profile_before = selected["binaries"]
    lint_command_argv = lint_command(docker, tree, Path(selected["directory"]), lint_dir, lint_cpus, run_id)
    batch_command_argv = gate_batch_command()
    receipt_path = output / "receipt.json"
    receipt: dict[str, Any] = {
        "schema_version": 1,
        "state": "running",
        "run_id": run_id,
        "started_utc": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "source": {**source, "root": str(tree), "relative_output": source_path},
        "functional_close": {
            "path": str(args.functional_close.resolve()),
            "sha256": sha256(args.functional_close.resolve()),
            "record": close,
        },
        "artifact_pins": pins,
        "native_image": native,
        "target": TARGET,
        "build_inputs": {
            "cargo_target_directory": gate_target_build_input(args.gate_target_dir, target_dir, reused_target),
        },
        "release_profile": selected["profile"],
        "profile_cost_receipt": selected["receipt"],
        "selected_release_binaries": profile_before,
        "locks": {
            "performance": str(PROFILE_LOCK),
            "native_suite": str(NATIVE_SUITE_LOCK),
            "cargo": str(CARGO_LOCK),
            "held_for_both_stages": True,
        },
        "resource_policy": {
            "isolated_lint_cpuset": lint_cpus,
            "isolated_lint_cpu_limit": CPU_LIMIT,
            "isolated_lint_memory_limit": MEMORY_LIMIT,
            "isolated_lint_swap_limit": MEMORY_LIMIT,
            "gate_batch_limits": "record the pinned Docker command's actual cgroup limits without changing its canonical settings",
            "rss_method": "container-local sum of resident pages sampled every 100 ms; retain kernel cgroup peak and events independently",
        },
        "timing_policy": "run timed observations only when pre-stage readiness passes and post-stage readiness passes immediately afterward; run memory and binary observations without timing when the host is busy",
        "canonical_batch_rust_exclusions": ["cp::privileged::test_cp_recursive_block_device_"],
        "supplemental_isolated_tests": {
            "owner": str(FINAL_CORRECTNESS_LAUNCHER),
            "stage": "core-compat-extra-privileged",
            "filter": "cp::privileged::test_cp_recursive_block_device_",
            "context": "pinned x86_64 musl image, no network, nodev /tmp, privileged only for this test target",
        },
        "post_batch_supplemental_launcher": supplemental_handoff,
        "stages": {},
    }
    json_write(receipt_path, receipt)

    ownership = exclusive_lock(PROFILE_LOCK, reject_unresolved=True)
    native_lock = exclusive_lock(NATIVE_SUITE_LOCK)
    cargo_lock = exclusive_lock(CARGO_LOCK)
    with ownership as ownership_stream, native_lock, cargo_lock:
        try:
            ensure_native_marker_clear()
            no_active_native_containers(docker, native["image_id"])
            lint_stage = run_stage(
                lint_command_argv,
                lint_dir,
                run_id=run_id,
                stage_name="isolated-repository-lint",
                environment=baseline_environment,
                container_id_file=lint_dir / "container.cid",
                resource_file=lint_dir / "container-resources.json",
                ownership_stream=ownership_stream,
                validate_after=lambda: {
                    "source": source_check(tree, source),
                    "selected_release_binaries": sealed_binary_set(Path(selected["directory"]), profile_before),
                },
            )
            receipt["stages"]["isolated_repository_lint"] = lint_stage
            after_lint = binary_set(Path(selected["directory"]))
            receipt["selected_release_binaries_after_lint"] = after_lint
            json_write(receipt_path, receipt)
            if lint_stage.get("interrupted"):
                receipt["state"] = "interrupted"
                receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
                json_write(receipt_path, receipt)
                return 130
            if after_lint != profile_before:
                raise ObserverFailure("sealed selected-release binaries changed during repository lint")
            if not lint_stage["observation_accepted"]:
                receipt["state"] = "failed"
                receipt["error"] = "isolated repository lint did not produce valid observations"
                receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
                json_write(receipt_path, receipt)
                return 1

            no_active_native_containers(docker, native["image_id"])
            gate_target_binaries = target_dir / TARGET / "release"
            gate_resource_path = batch_dir / "container-resources.json"
            gate_stage = run_stage(
                batch_command_argv,
                batch_dir,
                run_id=run_id,
                stage_name="canonical-gate-batch",
                environment=batch_env,
                container_id_file=batch_dir / "container.cid",
                resource_file=gate_resource_path,
                ownership_stream=ownership_stream,
                validate_after=lambda: {
                    "source": source_check(tree, source),
                    "selected_release_binaries": sealed_binary_set(Path(selected["directory"]), profile_before),
                    "gate_batch_release_binaries": binary_set(gate_target_binaries),
                    "docker_commands": read_docker_observer_log(batch_dir / "docker-commands.jsonl"),
                },
            )
            receipt["stages"]["canonical_gate_batch"] = gate_stage
            batch_receipt_path = batch_dir / "receipt.json"
            json_write(batch_receipt_path, {
                "schema_version": 1,
                "state": "interrupted" if gate_stage.get("interrupted") else "complete" if gate_stage["observation_accepted"] else "failed",
                "source": source,
                "command": gate_batch_command(),
                "stage": gate_stage,
            })
            receipt["canonical_gate_batch_receipt"] = {
                "command": gate_batch_command(),
                "path": str(batch_receipt_path),
                "sha256": sha256(batch_receipt_path),
                "is_single_broad_correctness_gate": True,
            }
            receipt["selected_release_binaries_after_gate_batch"] = binary_set(Path(selected["directory"]))
            post_stage_validation = gate_stage.get("post_stage_validation") or {}
            receipt["gate_batch_release_binaries"] = post_stage_validation.get("gate_batch_release_binaries")
            receipt["gate_batch_docker_commands"] = post_stage_validation.get("docker_commands")
            if gate_stage.get("interrupted"):
                receipt["state"] = "interrupted"
                receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
                json_write(receipt_path, receipt)
                return 130
            if not gate_stage["observation_accepted"]:
                receipt["state"] = "failed"
                receipt["error"] = "canonical cargo dev gate --batch did not produce valid observations"
                receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
                json_write(receipt_path, receipt)
                return 1

            if receipt["selected_release_binaries_after_gate_batch"] != profile_before:
                raise ObserverFailure("selected-release binaries changed during the canonical gate batch")
            if len(receipt["gate_batch_docker_commands"] or []) != 2 or sum(
                1 for item in receipt["gate_batch_docker_commands"] if item.get("instrumented") is True
            ) != 1:
                raise ObserverFailure("canonical gate batch did not use exactly one instrumented pinned Docker run")
            receipt["source_after"] = source_check(tree, source)
            receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
            receipt["state"] = "complete"
            json_write(receipt_path, receipt)
            return 0
        except SignalReceived as error:
            receipt["state"] = "interrupted"
            receipt["error"] = f"received {signal.Signals(error.signum).name} outside an active stage"
            receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
            json_write(receipt_path, receipt)
            return 130
        except Exception as error:
            receipt["state"] = "failed"
            receipt["error"] = str(error)
            receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
            json_write(receipt_path, receipt)
            raise


def plan_data() -> dict[str, Any]:
    final_correctness_manifest = json.loads(
        (FINAL_CORRECTNESS_DIR / "manifest.json").read_text(encoding="utf-8"),
        object_pairs_hook=unique_json_object,
    )
    contexts = final_correctness_manifest.get("linux_context_runs")
    if not isinstance(contexts, list) or len(contexts) != 5:
        raise ObserverFailure("final-correctness launcher must own exactly five supplemental Linux contexts")
    broad_test_lists = ("rust_harness_runs", "focused_native_files", "native_extra")
    if any(final_correctness_manifest.get(name) != [] for name in broad_test_lists):
        raise ObserverFailure("final-correctness launcher has broad Rust or native test loops that overlap the canonical batch")
    cp_runs = [
        context for context in contexts
        if context.get("harness") == "core-compat-extra-privileged"
        and context.get("filter") == "cp::privileged::test_cp_recursive_block_device_"
        and context.get("privileged") is True
    ]
    if len(cp_runs) != 1:
        raise ObserverFailure("final-correctness launcher must own the recursive cp block-device filter exactly once")
    product_builds = final_correctness_manifest.get("products", [])
    harness_builds = final_correctness_manifest.get("rust_harness_builds", [])
    clippy_commands = final_correctness_manifest.get("warning_deny", [])
    if not product_builds or not harness_builds or not clippy_commands:
        raise ObserverFailure("final-correctness supplemental contexts require their product, harness, and Clippy prerequisites")

    return {
        "target": TARGET,
        "native_image_id": NATIVE_IMAGE_ID,
        "stages": [
            {"id": "isolated-repository-lint", "command": "pinned Docker xsht lint using selected sealed release xsh/xsht/helper"},
            {"id": "canonical-gate-batch", "command": gate_batch_command(), "full_native_suite_count": 1},
        ],
        "canonical_batch_rust_exclusions": ["cp::privileged::test_cp_recursive_block_device_"],
        "supplemental_isolated_tests": {
            "owner": str(FINAL_CORRECTNESS_LAUNCHER),
            "stage": "core-compat-extra-privileged",
            "filter": "cp::privileged::test_cp_recursive_block_device_",
            "context": "pinned x86_64 musl image, no network, nodev /tmp, privileged only for this test target",
        },
        "post_batch_supplemental_launcher": {
            "after_stage": "canonical-gate-batch",
            "launcher": str(FINAL_CORRECTNESS_LAUNCHER),
            "command": [
                "python3", "-B", str(FINAL_CORRECTNESS_LAUNCHER), "--execute",
                "--expected-commit", "$CLOSE", "--expected-tree", "$CLOSE_TREE",
                "--out", "$SUPPLEMENTAL_OUT",
            ],
            "linux_context_runs": len(contexts),
            "broad_test_run_counts": {
                "rust_harness_runs": len(final_correctness_manifest["rust_harness_runs"]),
                "focused_native_files": len(final_correctness_manifest["focused_native_files"]),
                "native_extra": len(final_correctness_manifest["native_extra"]),
                "docs_gate": 0,
                "full_native_suite": 0,
            },
            "separate_prerequisites": {
                "product_builds": len(product_builds),
                "rust_harness_builds": len(harness_builds),
                "clippy_commands": len(clippy_commands),
            },
            "recursive_cp_filter_count": len(cp_runs),
            "execution_note": "Product and harness builds prepare the isolated contexts; the Clippy matrix is distinct. There are no broad Rust, native, docs, or full-suite test loops.",
        },
        "required_locks": [str(PROFILE_LOCK), str(NATIVE_SUITE_LOCK), str(CARGO_LOCK)],
        "readiness": {"before": ["perf-gate-ready"], "after": ["perf-gate-ready", "-1"]},
    }


def print_plan() -> None:
    print(json.dumps(plan_data(), indent=2, sort_keys=True))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--plan", action="store_true", help="print the static observer inventory")
    mode.add_argument("--execute", action="store_true", help="run both observers for one clean frozen source")
    parser.add_argument("--tree", type=Path, default=ROOT)
    parser.add_argument("--commit")
    parser.add_argument("--functional-close", type=Path)
    parser.add_argument("--profile-receipt", type=Path)
    parser.add_argument("--release-opt-level", type=int, choices=(RELEASE_OPT_LEVEL,))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--gate-target-dir", type=Path)
    args = parser.parse_args()
    if args.plan:
        print_plan()
        return 0
    missing = [name for name in ("commit", "functional_close", "profile_receipt", "release_opt_level", "output") if getattr(args, name) is None]
    if missing:
        parser.error("--execute requires --" + ", --".join(name.replace("_", "-") for name in missing))
    previous = install_signal_handlers()
    try:
        return run(args)
    except SignalReceived as error:
        print(f"interrupted by {signal.Signals(error.signum).name}", file=sys.stderr)
        return 130
    except Exception as error:
        if args.output is not None:
            receipt_path = args.output.resolve() / "receipt.json"
            if receipt_path.is_file():
                try:
                    receipt = json.loads(receipt_path.read_text(encoding="utf-8"), object_pairs_hook=unique_json_object)
                    if receipt.get("state") == "running":
                        receipt["state"] = "failed"
                        receipt["error"] = str(error)
                        receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
                        json_write(receipt_path, receipt)
                except Exception as receipt_error:
                    print(f"could not mark observer receipt failed: {receipt_error}", file=sys.stderr)
        print(f"final observer preparation failed: {error}", file=sys.stderr)
        return 1
    finally:
        restore_signal_handlers(previous)


if __name__ == "__main__":
    raise SystemExit(main())
