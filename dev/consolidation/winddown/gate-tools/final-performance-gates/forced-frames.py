#!/usr/bin/env python3
"""Run the sealed full native suite with forced frame scheduling."""

from __future__ import annotations

import argparse
from contextlib import contextmanager
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
from typing import Any

sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

from owned_containers import (
    InterruptedRun,
    SignalReceived,
    cleanup_cidfile_container,
    cleanup_container_process,
    cleanup_window,
    install_signal_handlers,
    owned_docker_command,
    restore_signal_handlers,
    wait_for_owned_process,
)


ROOT = Path("/home/josh/d/laputa-systems/xsh")
PERFORMANCE_DIR = ROOT / ".work/final-review-evidence/final-performance-gates"
INFRA = ROOT / ".work/consolidation/infra"
PROFILE_OUTPUT_ROOT = ROOT / ".work/consolidation/final-performance-gates"
FUNCTIONAL_CLOSE_ROOT = ROOT / ".work/final-review-evidence"
NATIVE_LOCK = INFRA / "native-suite.lock"
NATIVE_CLEANUP_MARKER = INFRA / "native-suite.cleanup-unresolved"
TARGET = "x86_64-unknown-linux-musl"
IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
CPU_LIMIT = 12
MEMORY_LIMIT = "24g"
BINARY_NAMES = ("xsh", "xsht", "xsh-test-helper")
OWNER_HELPER_SHA256 = "06a9b1e5c9984a41d64b4b79f11ba5fdd67e6b5b2a11c0b63517a4439ccb5403"
STAGE = "forced-frames-full-native-suite"


class GateFailure(RuntimeError):
    pass


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def unique_json_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise GateFailure(f"JSON contains duplicate key {key!r}")
        value[key] = item
    return value


def read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_json_object)
    except (OSError, json.JSONDecodeError) as error:
        raise GateFailure(f"cannot read JSON input {path}: {error}") from error
    if not isinstance(value, dict):
        raise GateFailure(f"JSON input must be an object: {path}")
    return value


def write_json(path: Path, value: dict[str, Any]) -> None:
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    try:
        with temporary.open("x", encoding="utf-8") as output:
            output.write(json.dumps(value, indent=2, sort_keys=True) + "\n")
            output.flush()
            os.fsync(output.fileno())
        temporary.replace(path)
        descriptor = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    finally:
        temporary.unlink(missing_ok=True)


def reject_symlinks_below_root(path: Path) -> None:
    absolute = Path(os.path.abspath(path))
    root = ROOT.resolve(strict=True)
    try:
        relative = absolute.relative_to(root)
    except ValueError as error:
        raise GateFailure(f"path must remain under the primary checkout: {path}") from error
    cursor = root
    for part in relative.parts:
        cursor = cursor / part
        try:
            mode = cursor.lstat().st_mode
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(mode):
            raise GateFailure(f"symlinks are not accepted in gate paths: {cursor}")


def require_exact_path(observed: Path, expected: Path, *, must_exist: bool) -> Path:
    candidate = Path(os.path.abspath(observed))
    expected = Path(os.path.abspath(expected))
    if candidate != expected:
        raise GateFailure(f"expected path {expected}, received {observed}")
    reject_symlinks_below_root(candidate)
    if must_exist and not candidate.exists():
        raise GateFailure(f"required path is missing: {candidate}")
    return candidate


def require_attempt_path(
    observed: Path,
    parent: Path,
    stem: str,
    *,
    must_exist: bool,
) -> Path:
    candidate = Path(os.path.abspath(observed))
    parent = require_exact_path(parent, parent, must_exist=True)
    if candidate.parent != parent:
        raise GateFailure(f"attempt path must be a direct child of {parent}: {observed}")
    if not re.fullmatch(re.escape(stem) + r"(?:-attempt-[A-Za-z0-9][A-Za-z0-9._-]*)?", candidate.name):
        raise GateFailure(f"attempt path must use {stem} or a fresh attempt suffix: {observed}")
    reject_symlinks_below_root(candidate)
    if must_exist and not candidate.exists():
        raise GateFailure(f"required path is missing: {candidate}")
    return candidate


def source_identity(tree: Path, commit: str) -> dict[str, str]:
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise GateFailure("source commit must be a full lowercase Git ID")
    try:
        head = subprocess.check_output(["git", "-C", str(tree), "rev-parse", "HEAD"], text=True, timeout=30).strip()
        tree_id = subprocess.check_output(["git", "-C", str(tree), "rev-parse", "HEAD^{tree}"], text=True, timeout=30).strip()
        status = subprocess.check_output(
            ["git", "-C", str(tree), "status", "--porcelain=v1", "--untracked-files=all"],
            text=True,
            timeout=30,
        )
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        raise GateFailure(f"cannot inspect frozen source: {error}") from error
    if head != commit:
        raise GateFailure(f"source HEAD {head} differs from requested close {commit}")
    if status:
        raise GateFailure("source tree must be clean before the forced-frame suite")
    return {"commit": head, "git_tree": tree_id, "status": "clean"}


def executable_snapshot(path: Path) -> dict[str, Any]:
    try:
        info = path.lstat()
    except OSError as error:
        raise GateFailure(f"cannot inspect release executable {path}: {error}") from error
    if not stat.S_ISREG(info.st_mode) or not os.access(path, os.X_OK):
        raise GateFailure(f"release binary must be a regular executable file: {path}")
    return {
        "path": str(path),
        "size_bytes": info.st_size,
        "mode": f"0o{stat.S_IMODE(info.st_mode):04o}",
        "sha256": digest(path),
    }


def load_functional_close(path: Path, commit: str) -> dict[str, Any]:
    close = read_json(path)
    if close.get("functional_complete") is not True or close.get("commit") != commit:
        raise GateFailure("functional-close record must be complete and name the exact source commit")
    return {"path": str(path), "sha256": digest(path), "record": close}


def load_profile(receipt_path: Path, tree: Path, commit: str, source: dict[str, str]) -> dict[str, Any]:
    receipt = read_json(receipt_path)
    if receipt.get("state") != "complete" or receipt.get("source_commit") != commit:
        raise GateFailure("profile-cost receipt must be complete and name the exact source commit")
    if Path(receipt.get("source_tree", "")).resolve() != tree.resolve():
        raise GateFailure("profile-cost receipt names a different source tree")
    if receipt.get("source_identity") != source or receipt.get("target") != TARGET:
        raise GateFailure("profile-cost receipt does not match the frozen source and target")
    profile = receipt.get("profile")
    if not isinstance(profile, dict) or any(
        profile.get(name) != expected
        for name, expected in (
            ("name", "release"),
            ("lto", "thin"),
            ("codegen_units", 16),
            ("opt_levels", [3]),
        )
    ):
        raise GateFailure("profile-cost receipt must identify the selected release opt3 profile")
    results = receipt.get("results")
    if not isinstance(results, list) or len(results) != 1 or not isinstance(results[0], dict) or results[0].get("name") != "release-opt3":
        raise GateFailure("profile-cost receipt must contain exactly the selected release opt3 profile")
    result = results[0]
    if result.get("profile") != "release" or result.get("opt_level") != 3 or result.get("lto") != "thin" or result.get("codegen_units") != 16:
        raise GateFailure("selected opt3 binaries must use release profile with thin LTO and 16 codegen units")
    if result.get("native_suite", {}).get("status") != 0:
        raise GateFailure("opt3 native suite did not pass in the profile-cost receipt")
    profile_dir = receipt_path.parent / "opt3" / "target" / TARGET / "release"
    if not profile_dir.is_dir():
        raise GateFailure(f"opt3 release directory is missing: {profile_dir}")
    recorded = result.get("binaries")
    if not isinstance(recorded, dict) or set(recorded) != set(BINARY_NAMES):
        raise GateFailure("opt3 profile result must bind XSH, XSHT, and xsh-test-helper")
    binaries: dict[str, dict[str, Any]] = {}
    for name in BINARY_NAMES:
        expected = profile_dir / name
        record = recorded[name]
        if not isinstance(record, dict) or Path(record.get("path", "")).resolve() != expected.resolve():
            raise GateFailure(f"profile-cost receipt names an unexpected opt3 binary path: {name}")
        observed = executable_snapshot(expected)
        if record.get("sha256") != observed["sha256"]:
            raise GateFailure(f"opt3 binary hash differs from the profile-cost receipt: {name}")
        binaries[name] = observed
    return {
        "receipt": {"path": str(receipt_path), "sha256": digest(receipt_path)},
        "name": result["name"],
        "profile": result["profile"],
        "opt_level": result["opt_level"],
        "directory": str(profile_dir),
        "binaries": binaries,
    }


def verify_inputs(tree: Path, commit: str, expected_source: dict[str, str], close: dict[str, Any], profile: dict[str, Any]) -> dict[str, Any]:
    current_source = source_identity(tree, commit)
    if current_source != expected_source:
        raise GateFailure("source identity changed during the forced-frame suite")
    if digest(Path(close["path"])) != close["sha256"]:
        raise GateFailure("functional-close record changed during the forced-frame suite")
    if digest(Path(profile["receipt"]["path"])) != profile["receipt"]["sha256"]:
        raise GateFailure("profile-cost receipt changed during the forced-frame suite")
    current_binaries = {name: executable_snapshot(Path(profile["directory"]) / name) for name in BINARY_NAMES}
    if current_binaries != profile["binaries"]:
        raise GateFailure("sealed opt3 release binaries changed during the forced-frame suite")
    return {"source": current_source, "binaries": current_binaries}


def validate_image_info(info: dict[str, Any]) -> dict[str, str]:
    if info.get("Id") != IMAGE or info.get("Architecture") != "amd64" or info.get("Os") != "linux":
        raise GateFailure("native test image must match the pinned x86_64 Linux image")
    return {"image_id": info["Id"], "architecture": info["Architecture"], "os": info["Os"]}


def inspect_native_image(docker: str) -> dict[str, str]:
    try:
        result = subprocess.run(
            [docker, "image", "inspect", IMAGE],
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise GateFailure(f"cannot inspect pinned native image: {error}") from error
    if result.returncode != 0:
        raise GateFailure(f"pinned native image inspection failed: {result.stderr.strip()}")
    try:
        values = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise GateFailure("pinned native image inspection returned invalid JSON") from error
    if not isinstance(values, list) or len(values) != 1 or not isinstance(values[0], dict):
        raise GateFailure("pinned native image inspection must return one image")
    return validate_image_info(values[0])


def no_active_native_containers(docker: str) -> None:
    try:
        result = subprocess.run(
            [docker, "ps", "--filter", f"ancestor={IMAGE}", "--format", "{{.ID}} {{.Names}}"],
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise GateFailure(f"cannot inspect active native containers: {error}") from error
    if result.returncode != 0:
        raise GateFailure(f"active native container check failed: {result.stderr.strip()}")
    if result.stdout.strip():
        raise GateFailure("another pinned native-image container is active")


def cpu_set() -> str:
    if sys.platform != "linux" or not hasattr(os, "sched_getaffinity"):
        raise GateFailure("forced-frame acceptance requires the Linux host that owns the pinned Docker image")
    cpus = sorted(os.sched_getaffinity(0))
    if len(cpus) < CPU_LIMIT:
        raise GateFailure(f"forced-frame acceptance requires {CPU_LIMIT} host-allowed CPUs")
    return ",".join(str(cpu) for cpu in cpus[:CPU_LIMIT])


def container_command(
    tree: Path,
    profile_dir: Path,
    git_common: Path,
    laputa_corpus: Path,
    repository_tmp: Path,
    cpus: str,
) -> list[str]:
    work = ROOT / ".work/consolidation"
    xsh = profile_dir / "xsh"
    xsht = profile_dir / "xsht"
    path = os.pathsep.join((str(profile_dir), "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"))
    command = [
        "docker",
        "run",
        "--rm",
        "--init",
        "--platform",
        "linux/amd64",
        "--cpuset-cpus",
        cpus,
        "--cpus",
        str(CPU_LIMIT),
        "--memory",
        MEMORY_LIMIT,
        "--memory-swap",
        MEMORY_LIMIT,
        "--network",
        "none",
        "--tmpfs",
        "/tmp:rw,exec,nosuid,mode=1777",
        "-v",
        f"{tree}:{tree}",
        "-v",
        f"{work}:{work}",
        "-v",
        f"{git_common}:{git_common}:ro",
        "-v",
        f"{laputa_corpus}:{laputa_corpus}:ro",
        "-v",
        f"{profile_dir}:{profile_dir}:ro",
        "-w",
        str(tree),
        "-e",
        "LC_ALL=C",
        "-e",
        "TZ=UTC",
        "-e",
        f"TMPDIR={repository_tmp}",
        "-e",
        f"XSH_BIN={xsh}",
        "-e",
        f"XSH_LAPUTA_CORPUS={laputa_corpus}",
        "-e",
        f"PATH={path}",
        "--user",
        f"{os.getuid()}:{os.getgid()}",
        "-e",
        "HOME=/home/compat",
        IMAGE,
        str(xsht),
        "test",
        "--force-frames",
    ]
    if any(Path(argument).name == "cargo" for argument in command):
        raise GateFailure("forced-frame container command must not invoke Cargo")
    return command


@contextmanager
def native_suite_lock():
    reject_symlinks_below_root(NATIVE_LOCK)
    reject_symlinks_below_root(NATIVE_CLEANUP_MARKER)
    if NATIVE_CLEANUP_MARKER.exists() or NATIVE_CLEANUP_MARKER.is_symlink():
        raise GateFailure(f"native-suite cleanup marker exists: {NATIVE_CLEANUP_MARKER}")
    try:
        lock_file = NATIVE_LOCK.open("a+")
    except OSError as error:
        raise GateFailure(f"cannot open native-suite lock: {error}") from error
    try:
        try:
            fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise GateFailure("the shared native-suite lock is held; do not overlap this full suite") from error
        if NATIVE_CLEANUP_MARKER.exists() or NATIVE_CLEANUP_MARKER.is_symlink():
            raise GateFailure(f"native-suite cleanup marker exists: {NATIVE_CLEANUP_MARKER}")
        yield lock_file
    finally:
        try:
            if lock_file.fileno() >= 0:
                fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)
        finally:
            lock_file.close()


def cleanup_owned_container(cidfile: Path, run_id: str) -> dict[str, Any]:
    try:
        return cleanup_cidfile_container(cidfile, owner=run_id, stage=STAGE)
    except Exception as error:
        return {"resolved": False, "container_ids": [], "commands": [], "error": str(error)}


@contextmanager
def defer_signals():
    pending: dict[str, int | None] = {"first": None}
    previous: dict[int, Any] = {}

    def capture(signum: int, _frame: Any) -> None:
        if pending["first"] is None:
            pending["first"] = signum

    try:
        for signum in (signal.SIGINT, signal.SIGTERM):
            previous[signum] = signal.signal(signum, capture)
        yield pending
    finally:
        for signum, handler in previous.items():
            signal.signal(signum, handler)


def cleanup_running_process(
    process: subprocess.Popen[bytes], cleanup_fn: Any
) -> dict[str, Any]:
    try:
        return cleanup_container_process(process, cleanup_fn)
    except Exception as error:
        with cleanup_window():
            containers = cleanup_fn()
        return {
            "resolved": False,
            "process": {"resolved": False, "error": str(error)},
            "containers": containers,
        }


def run_container(
    command: list[str], log_path: Path, cidfile: Path, run_id: str
) -> dict[str, Any]:
    cidfile.unlink(missing_ok=True)
    owned_command = owned_docker_command(command, cidfile=cidfile, owner=run_id, stage=STAGE)
    process: subprocess.Popen[bytes] | None = None
    status: int | None = None
    interrupted: int | None = None
    process_cleanup: dict[str, Any] = {"resolved": True, "commands": []}
    container_cleanup: dict[str, Any] | None = None
    error: str | None = None
    try:
        with log_path.open("xb") as log:
            with defer_signals() as pending:
                process = subprocess.Popen(
                    owned_command,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                )
            if pending["first"] is not None:
                raise SignalReceived(pending["first"])
            status, _, _ = wait_for_owned_process(
                process,
                lambda: cleanup_owned_container(cidfile, run_id),
            )
    except InterruptedRun as caught:
        interrupted = caught.signum
        process_cleanup = caught.cleanup.get("process", {"resolved": True, "commands": []})
        container_cleanup = caught.cleanup.get("containers", {"resolved": False, "commands": []})
    except SignalReceived as caught:
        interrupted = caught.signum
        cleanup_fn = lambda: cleanup_owned_container(cidfile, run_id)
        if process is not None and process.poll() is None:
            cleanup = cleanup_running_process(process, cleanup_fn)
            process_cleanup = cleanup.get("process", {"resolved": False, "commands": []})
            container_cleanup = cleanup.get("containers", {"resolved": False, "commands": []})
        else:
            with cleanup_window():
                container_cleanup = cleanup_fn()
    except Exception as caught:
        error = str(caught)
        cleanup_fn = lambda: cleanup_owned_container(cidfile, run_id)
        if process is not None and process.poll() is None:
            cleanup = cleanup_running_process(process, cleanup_fn)
            process_cleanup = cleanup.get("process", {"resolved": False, "commands": []})
            container_cleanup = cleanup.get("containers", {"resolved": False, "commands": []})
        else:
            with cleanup_window():
                container_cleanup = cleanup_fn()
    else:
        with cleanup_window():
            container_cleanup = cleanup_owned_container(cidfile, run_id)
    if container_cleanup is None:
        with cleanup_window():
            container_cleanup = cleanup_owned_container(cidfile, run_id)
    cleanup_resolved = process_cleanup.get("resolved") is True and container_cleanup.get("resolved") is True
    if cleanup_resolved:
        try:
            cidfile.unlink(missing_ok=True)
        except OSError as caught:
            cleanup_resolved = False
            container_cleanup["cidfile_error"] = str(caught)
    return {
        "command": owned_command,
        "status": status,
        "interrupted_signal": signal.Signals(interrupted).name if interrupted is not None else None,
        "error": error,
        "log": str(log_path),
        "container_id_file": str(cidfile) if not cleanup_resolved else None,
        "process_cleanup": process_cleanup,
        "container_cleanup": container_cleanup,
        "cleanup_resolved": cleanup_resolved,
    }


def retain_unresolved_cleanup(run_id: str, stage: dict[str, Any]) -> dict[str, Any]:
    marker = {
        "run_id": run_id,
        "step": STAGE,
        "cidfile": stage.get("container_id_file"),
        "container_cleanup": stage.get("container_cleanup"),
        "process_cleanup": stage.get("process_cleanup"),
    }
    write_json(NATIVE_CLEANUP_MARKER, marker)
    return marker


def validate_static_paths(args: argparse.Namespace) -> tuple[Path, Path, Path, Path, Path]:
    root = ROOT.resolve(strict=True)
    tree = args.tree.resolve(strict=True)
    if tree != root:
        raise GateFailure("final forced-frame acceptance must use the primary checkout")
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        raise GateFailure("source commit must be a full lowercase 40-character Git ID")
    close_path = require_exact_path(
        args.functional_close,
        FUNCTIONAL_CLOSE_ROOT / f"functional-close-{args.commit}.json",
        must_exist=True,
    )
    output_parent = require_exact_path(PROFILE_OUTPUT_ROOT, PROFILE_OUTPUT_ROOT, must_exist=True)
    profile_name = f"profile-cost-{args.commit}"
    profile_dir = require_attempt_path(
        args.profile_receipt.parent,
        output_parent,
        profile_name,
        must_exist=True,
    )
    receipt_path = require_exact_path(
        args.profile_receipt,
        profile_dir / "receipt.json",
        must_exist=True,
    )
    output = require_attempt_path(
        args.output,
        output_parent,
        f"forced-frames-{args.commit}",
        must_exist=False,
    )
    if output.exists() or output.is_symlink():
        raise GateFailure("forced-frame output already exists; use a fresh commit-specific path")
    if not output_parent.is_dir():
        raise GateFailure(f"forced-frame output parent is not a directory: {output_parent}")
    if digest(PERFORMANCE_DIR / "owned_containers.py") != OWNER_HELPER_SHA256:
        raise GateFailure("owned-container cleanup helper differs from its inspected version")
    return tree, close_path, receipt_path, output, output_parent


def ensure_repository_tmp() -> Path:
    path = require_exact_path(ROOT / ".work/tmp", ROOT / ".work/tmp", must_exist=False)
    exclude_path = ROOT / ".git/info/exclude"
    try:
        patterns = {
            line.strip()
            for line in exclude_path.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        }
    except OSError as error:
        raise GateFailure(f"cannot read local Git excludes: {error}") from error
    if not patterns.intersection({".work", ".work/", "/.work", "/.work/"}):
        raise GateFailure("repository .work directory must be excluded by .git/info/exclude")
    path.mkdir(parents=True, exist_ok=True)
    path = require_exact_path(path, ROOT / ".work/tmp", must_exist=True)
    if not path.is_dir():
        raise GateFailure(f"repository temporary directory is not a directory: {path}")
    try:
        result = subprocess.run(
            ["git", "-C", str(ROOT), "check-ignore", "--quiet", "--no-index", ".work/tmp"],
            check=False,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise GateFailure(f"cannot verify repository temporary directory is ignored: {error}") from error
    if result.returncode != 0:
        raise GateFailure("repository temporary directory is not ignored by Git")
    return path


def run(args: argparse.Namespace) -> int:
    tree, close_path, receipt_path, output, _ = validate_static_paths(args)
    repository_tmp = ensure_repository_tmp()
    source = source_identity(tree, args.commit)
    close = load_functional_close(close_path, args.commit)
    profile = load_profile(receipt_path, tree, args.commit, source)
    docker = shutil.which("docker")
    if docker is None:
        raise GateFailure("docker must be available on PATH")
    docker = str(Path(docker).resolve())
    cpus = cpu_set()
    git_common_raw = subprocess.check_output(
        ["git", "-C", str(tree), "rev-parse", "--path-format=absolute", "--git-common-dir"],
        text=True,
        timeout=30,
    ).strip()
    git_common = Path(git_common_raw).resolve(strict=True)
    if not git_common.is_dir():
        raise GateFailure(f"Git common directory is not a directory: {git_common}")
    laputa_corpus = Path("/home/josh/d/laputa-systems/laputa").resolve(strict=True)
    if not laputa_corpus.is_dir():
        raise GateFailure(f"required Laputa corpus is not a directory: {laputa_corpus}")
    run_id = secrets.token_hex(16)
    log_path = output / "native-suite.log"
    cidfile = output / "container.cid"
    receipt_path_out = output / "receipt.json"
    receipt: dict[str, Any] = {
        "schema": 1,
        "state": "preparing",
        "run_id": run_id,
        "source": source,
        "functional_close": close,
        "profile_cost_receipt": profile["receipt"],
        "selected_profile": {key: profile[key] for key in ("name", "profile", "opt_level", "directory")},
        "binaries_before": profile["binaries"],
        "native_image": {"image_id": IMAGE, "os": "linux", "architecture": "amd64"},
        "command": [str(Path(profile["directory"]) / "xsht"), "test", "--force-frames"],
        "environment": {
            "XSH_BIN": str(Path(profile["directory"]) / "xsh"),
            "PATH_prefix": profile["directory"],
            "XSH_LAPUTA_CORPUS": str(laputa_corpus),
            "TMPDIR": str(repository_tmp),
            "network": "none",
            "cpuset_cpus": cpus,
            "cpu_limit": CPU_LIMIT,
            "memory_limit": MEMORY_LIMIT,
            "memory_swap_limit": MEMORY_LIMIT,
        },
        "native_suite_lock": {"path": str(NATIVE_LOCK), "held_across_gate": False},
        "timing_claim": "none; correctness-only forced-frame full-suite run",
    }
    output_created = False
    with native_suite_lock():
        previous_handlers = install_signal_handlers()
        try:
            output.mkdir(mode=0o755)
            output_created = True
            receipt["native_suite_lock"]["held_across_gate"] = True
            receipt["state"] = "running"
            write_json(receipt_path_out, receipt)
            image = inspect_native_image(docker)
            no_active_native_containers(docker)
            git_common_arg = str(git_common)
            command = container_command(
                tree,
                Path(profile["directory"]),
                Path(git_common_arg),
                laputa_corpus,
                repository_tmp,
                cpus,
            )
            before = verify_inputs(tree, args.commit, source, close, profile)
            receipt["inputs_before"] = before
            receipt["native_image"] = image
            receipt["docker_command"] = owned_docker_command(
                command, cidfile=cidfile, owner=run_id, stage=STAGE
            )
            receipt["stage"] = {"status": "running", "log": str(log_path), "container_id_file": str(cidfile)}
            write_json(receipt_path_out, receipt)
            stage = run_container(command, log_path, cidfile, run_id)
            receipt["stage"] = stage
            try:
                after = verify_inputs(tree, args.commit, source, close, profile)
                receipt["inputs_after"] = after
                inputs_unchanged = before == after
            except Exception as error:
                receipt["input_validation_error"] = str(error)
                inputs_unchanged = False
            receipt["inputs_unchanged"] = inputs_unchanged
            if not stage["cleanup_resolved"]:
                receipt["unresolved_cleanup"] = retain_unresolved_cleanup(run_id, stage)
                receipt["state"] = "incomplete-cleanup"
                receipt["error"] = "owned process or container cleanup could not be verified"
                write_json(receipt_path_out, receipt)
                return 1
            if stage["interrupted_signal"] is not None:
                receipt["state"] = "interrupted"
                receipt["error"] = f"received {stage['interrupted_signal']} during forced-frame full suite"
                write_json(receipt_path_out, receipt)
                return 130
            if stage["error"] is not None:
                receipt["state"] = "failed"
                receipt["error"] = stage["error"]
                write_json(receipt_path_out, receipt)
                return 1
            if stage["status"] != 0 or not inputs_unchanged:
                receipt["state"] = "failed"
                receipt["error"] = "forced-frame suite failed or sealed inputs changed"
                write_json(receipt_path_out, receipt)
                return stage["status"] if isinstance(stage["status"], int) and stage["status"] > 0 else 1
            receipt["state"] = "complete"
            write_json(receipt_path_out, receipt)
            print(json.dumps(receipt, indent=2, sort_keys=True))
            return 0
        except SignalReceived as error:
            receipt["state"] = "interrupted"
            receipt["error"] = f"received {signal.Signals(error.signum).name} during forced-frame acceptance"
            if output_created:
                write_json(receipt_path_out, receipt)
            return 130
        except Exception as error:
            receipt["state"] = "failed"
            receipt["error"] = str(error)
            if output_created:
                write_json(receipt_path_out, receipt)
            raise
        finally:
            restore_signal_handlers(previous_handlers)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tree", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--functional-close", type=Path, required=True)
    parser.add_argument("--profile-receipt", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args(argv)


def main() -> int:
    args = parse_args()
    try:
        return run(args)
    except SignalReceived as error:
        print(f"interrupted by {signal.Signals(error.signum).name} before the suite started", file=sys.stderr)
        return 130
    except Exception as error:
        print(f"forced-frame acceptance failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
