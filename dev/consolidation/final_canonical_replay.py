#!/usr/bin/env python3

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
import pathlib
import re
import signal
import subprocess
import sys
import time
from typing import Any


IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
TARGET = "x86_64-unknown-linux-musl"
PROFILE = "verification"
RUSTFLAGS = "-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol"
LOCK_PATH = pathlib.Path(".work/consolidation/infra/native-suite.lock")
CARGO_LOCK_PATH = pathlib.Path(".work/consolidation/infra/cargo.lock")
CARGO_LOCK_CONTAINER_PATH = "/work/.work/consolidation/infra/cargo.lock"
METRICS_PATH = pathlib.Path(".work/consolidation")
METRICS_CONTAINER_PATH = "/work/.work/consolidation"
SCRATCH_CONTAINER_PATH = "/work/.work/tmp"
BOOTSTRAP_CARGO_ARGV = [
    "cargo",
    "build",
    "--locked",
    "--profile",
    PROFILE,
    "--target",
    TARGET,
    "-p",
    "xsh",
    "--bin",
    "xsh",
    "--message-format=json-render-diagnostics",
]
BOOTSTRAP_CONTAINER_TARGET_DIR = "/work/target"
CPU_LIMIT = 12
MEMORY_LIMIT_BYTES = 24 * 1024 * 1024 * 1024
SWAP_LIMIT_BYTES = 0
CASE_TIMEOUT_SECONDS = 4 * 60 * 60
SAMPLE_INTERVAL_SECONDS = 0.1
TERM_GRACE_SECONDS = 30


class ReplayError(Exception):
    pass


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def run_text(argv: list[str], cwd: pathlib.Path) -> str:
    result = subprocess.run(argv, cwd=cwd, check=True, capture_output=True, text=True)
    return result.stdout.strip()


def git_snapshot(root: pathlib.Path) -> dict[str, Any]:
    status = run_text(["git", "status", "--porcelain=v1", "--untracked-files=all"], root)
    return {
        "head": run_text(["git", "rev-parse", "HEAD"], root),
        "tree": run_text(["git", "rev-parse", "HEAD^{tree}"], root),
        "clean": status == "",
        "status": status,
    }


def host_file(path: pathlib.Path, executable: bool = False) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file() or (executable and not os.access(path, os.X_OK)):
        raise ReplayError(f"required host file is missing, unsafe, or not executable: {path}")
    return {"path": str(path.resolve()), "size_bytes": path.stat().st_size, "sha256": sha256_file(path)}


def cargo_home() -> pathlib.Path:
    configured = os.environ.get("CARGO_HOME", "").strip()
    return pathlib.Path(configured).expanduser() if configured else pathlib.Path.home() / ".cargo"


def resolved_path(value: pathlib.Path, root: pathlib.Path) -> pathlib.Path:
    return (value if value.is_absolute() else root / value).resolve(strict=False)


def require_below(path: pathlib.Path, parent: pathlib.Path) -> pathlib.Path:
    resolved = path.resolve(strict=False)
    parent_resolved = parent.resolve(strict=True)
    if not resolved.is_relative_to(parent_resolved):
        raise ReplayError(f"path must stay below {parent_resolved}: {resolved}")
    return resolved


def strict_int(value: Any, label: str, minimum: int = 0) -> int:
    if type(value) is not int or value < minimum:
        raise ReplayError(f"resource field {label} must be an integer >= {minimum}")
    return value


def require_scratch_file(value: Any, root: pathlib.Path, scratch_dir: pathlib.Path, label: str) -> pathlib.Path:
    if not isinstance(value, str) or not value:
        raise ReplayError(f"{label} path is missing")
    candidate = pathlib.Path(value)
    candidate = pathlib.Path(os.path.abspath(candidate if candidate.is_absolute() else root / candidate))
    scratch = scratch_dir.resolve(strict=True)
    if not candidate.is_relative_to(scratch) or candidate == scratch:
        raise ReplayError(f"{label} must be under {scratch}")
    relative = candidate.relative_to(scratch)
    cursor = scratch
    for part in relative.parts:
        cursor = cursor / part
        if cursor.is_symlink():
            raise ReplayError(f"{label} path contains a symlink: {cursor}")
    if not candidate.is_file():
        raise ReplayError(f"{label} file is missing: {candidate}")
    return candidate


def validate_build_json(path: pathlib.Path, artifact_path: pathlib.Path) -> dict[str, Any]:
    try:
        records = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ReplayError(f"bootstrap Cargo JSON is invalid: {error}") from error
    if not records:
        raise ReplayError("bootstrap Cargo JSON is empty")
    finished = [record for record in records if isinstance(record, dict) and record.get("reason") == "build-finished"]
    if len(finished) != 1 or finished[0].get("success") is not True:
        raise ReplayError("bootstrap Cargo JSON must contain one successful build-finished record")
    expected_executable = f"{BOOTSTRAP_CONTAINER_TARGET_DIR}/{TARGET}/{PROFILE}/xsh"
    matching_artifacts = [
        record
        for record in records
        if isinstance(record, dict)
        and record.get("reason") == "compiler-artifact"
        and isinstance(record.get("target"), dict)
        and record["target"].get("name") == "xsh"
        and isinstance(record["target"].get("kind"), list)
        and "bin" in record["target"]["kind"]
        and record.get("executable") == expected_executable
        and isinstance(record.get("profile"), dict)
        and record["profile"].get("test") is False
    ]
    if len(matching_artifacts) != 1 or artifact_path.name != "xsh":
        raise ReplayError("bootstrap Cargo JSON does not identify the expected xsh verification executable")
    return {"record_count": len(records), "executable": expected_executable}


def validate_bootstrap_receipt(
    receipt_path: pathlib.Path,
    root: pathlib.Path,
    scratch_dir: pathlib.Path,
    source: dict[str, Any],
    bootstrap_path: pathlib.Path,
) -> dict[str, Any]:
    receipt_path = require_scratch_file(str(receipt_path), root, scratch_dir, "bootstrap receipt")
    try:
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ReplayError(f"bootstrap receipt is invalid: {error}") from error
    if (
        not isinstance(receipt, dict)
        or type(receipt.get("schema_version")) is not int
        or receipt.get("schema_version") != 1
        or receipt.get("status") != "passed"
    ):
        raise ReplayError("bootstrap receipt must be a passed schema version 1 build")
    receipt_source = receipt.get("source")
    if (
        not isinstance(receipt_source, dict)
        or receipt_source.get("commit") != source["head"]
        or receipt_source.get("tree") != source["tree"]
        or type(receipt_source.get("clean")) is not bool
        or receipt_source["clean"] is not True
    ):
        raise ReplayError("bootstrap receipt source does not match the clean source freeze")
    if (
        receipt.get("image") != IMAGE
        or receipt.get("target") != TARGET
        or receipt.get("profile") != PROFILE
        or receipt.get("rustflags") != RUSTFLAGS
        or receipt.get("cargo_argv") != BOOTSTRAP_CARGO_ARGV
        or receipt.get("cargo_target_dir") != BOOTSTRAP_CONTAINER_TARGET_DIR
    ):
        raise ReplayError("bootstrap receipt build configuration does not match the pinned build")
    build_json = receipt.get("build_json")
    if not isinstance(build_json, dict) or type(build_json.get("exit_code")) is not int or build_json["exit_code"] != 0:
        raise ReplayError("bootstrap receipt must record a successful Cargo JSON build")
    build_json_path = require_scratch_file(build_json.get("path"), root, scratch_dir, "bootstrap Cargo JSON")
    if not isinstance(build_json.get("sha256"), str) or build_json["sha256"] != sha256_file(build_json_path):
        raise ReplayError("bootstrap Cargo JSON hash does not match its file")
    build_json_summary = validate_build_json(build_json_path, bootstrap_path)
    artifact = receipt.get("artifact")
    if not isinstance(artifact, dict):
        raise ReplayError("bootstrap receipt artifact is missing")
    recorded_artifact_path = artifact.get("path")
    if not isinstance(recorded_artifact_path, str) or pathlib.Path(recorded_artifact_path) != bootstrap_path:
        raise ReplayError(f"bootstrap receipt artifact path must be {bootstrap_path}")
    if type(artifact.get("size_bytes")) is not int or artifact["size_bytes"] <= 0:
        raise ReplayError("bootstrap receipt artifact size is missing or invalid")
    actual_artifact = host_file(bootstrap_path, executable=True)
    if artifact.get("size_bytes") != actual_artifact["size_bytes"] or artifact.get("sha256") != actual_artifact["sha256"]:
        raise ReplayError("bootstrap receipt artifact hash or size does not match the executable")
    return {
        "path": str(receipt_path),
        "sha256": sha256_file(receipt_path),
        "record": receipt,
        "build_json": {
            "path": str(build_json_path),
            "sha256": build_json["sha256"],
            "summary": build_json_summary,
        },
        "artifact": actual_artifact,
    }


def validate_inputs(args: argparse.Namespace) -> dict[str, Any]:
    root = args.repo.resolve(strict=True)
    if not (root / ".git").exists():
        raise ReplayError(f"repository metadata is missing under {root}")
    if re.fullmatch(r"[0-9a-f]{40}", args.head) is None or re.fullmatch(r"[0-9a-f]{40}", args.tree) is None:
        raise ReplayError("--head and --tree must be full lowercase Git object IDs")
    source = git_snapshot(root)
    if (source["head"], source["tree"], source["clean"]) != (args.head, args.tree, True):
        raise ReplayError(f"source does not match the explicit clean freeze: {source}")

    work_dir = root / ".work"
    scratch_dir = work_dir / "tmp"
    if work_dir.is_symlink() or scratch_dir.is_symlink() or not scratch_dir.is_dir() or not os.access(scratch_dir, os.W_OK):
        raise ReplayError(".work/tmp must exist as a writable real directory")
    metrics_dir = root / METRICS_PATH
    if metrics_dir.is_symlink() or not metrics_dir.is_dir() or not os.access(metrics_dir, os.W_OK):
        raise ReplayError(f"writable consolidation metrics mount is missing or unsafe: {metrics_dir}")
    ignored = subprocess.run(
        ["git", "check-ignore", "--quiet", "--no-index", ".work"], cwd=root, check=False
    )
    if ignored.returncode != 0:
        raise ReplayError(".work must be ignored through .git/info/exclude")

    output_root = resolved_path(args.output, root)
    require_below(output_root, scratch_dir)
    if output_root.parent.resolve(strict=False) != scratch_dir.resolve(strict=True):
        raise ReplayError("output must be one fresh run directory directly under .work/tmp")
    if output_root.name != args.run_name:
        raise ReplayError("--run-name must match the output directory name")
    if output_root.exists() or output_root.is_symlink():
        raise ReplayError(f"output directory must not already exist: {output_root}")

    if ":" in str(root) or ":" in str(output_root):
        raise ReplayError("Docker bind paths cannot contain a colon")
    target_dir = root / ".work/target-final-canonical"
    if target_dir.is_symlink() or not target_dir.is_dir() or not os.access(target_dir, os.W_OK):
        raise ReplayError(f"root target directory must exist as a writable real directory: {target_dir}")
    target_mount = root / "target"
    if target_mount.is_symlink() or (target_mount.exists() and not target_mount.is_dir()):
        raise ReplayError(f"container target mountpoint must be a real directory: {target_mount}")
    bootstrap_xsh = target_dir / TARGET / PROFILE / "xsh"
    if any(path.is_symlink() for path in (target_dir / TARGET, target_dir / TARGET / PROFILE, bootstrap_xsh)):
        raise ReplayError(f"bootstrap product path must not contain symlinks: {bootstrap_xsh}")
    bootstrap = host_file(bootstrap_xsh, executable=True)
    bootstrap_receipt = validate_bootstrap_receipt(
        args.bootstrap_receipt,
        root,
        scratch_dir,
        source,
        bootstrap_xsh,
    )

    lock_path = root / LOCK_PATH
    if lock_path.is_symlink() or not lock_path.is_file() or not os.access(lock_path, os.W_OK):
        raise ReplayError(f"shared native-suite lock is missing, unsafe, or not writable: {lock_path}")
    cargo_lock_path = root / CARGO_LOCK_PATH
    if (
        cargo_lock_path.parent.is_symlink()
        or cargo_lock_path.is_symlink()
        or not cargo_lock_path.is_file()
        or not os.access(cargo_lock_path, os.W_OK)
    ):
        raise ReplayError(f"shared Cargo lock is missing, unsafe, or not writable: {cargo_lock_path}")

    common_git = pathlib.Path(run_text(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"], root))
    if not common_git.is_dir() or ":" in str(common_git):
        raise ReplayError(f"Git common directory cannot be mounted read-only: {common_git}")

    tools = {
        "kache": host_file(cargo_home() / "bin" / "kache", executable=True),
        "mold": host_file(pathlib.Path("/usr/local/bin/mold"), executable=True),
        "linker": host_file(root / "dev/musl-linker.sh", executable=True),
        "jemalloc": host_file(pathlib.Path("/usr/lib/libjemalloc.so.2")),
    }
    return {
        "root": root,
        "source": source,
        "output_root": output_root,
        "scratch_dir": scratch_dir,
        "metrics_dir": metrics_dir,
        "target_dir": target_dir,
        "bootstrap": bootstrap,
        "bootstrap_receipt": bootstrap_receipt,
        "common_git": common_git.resolve(strict=True),
        "lock_path": lock_path,
        "cargo_lock_path": cargo_lock_path,
        "tools": tools,
        "prior_attempt": {
            "source_prefix": "fff13082f",
            "handoff": "dev/consolidation/HANDOFF.md",
            "output": "dev/consolidation/evidence/resume-fff13082-canonical.log",
            "resources": "dev/consolidation/evidence/resume-fff13082-resources.json",
            "disposition": "retained failed prior attempt; no result is inherited by this run",
        },
        "args": args,
    }


def batch_command() -> list[str]:
    return [
        f"/work/target/{TARGET}/{PROFILE}/xsh",
        "dev/main.xsh",
        "--",
        "internal",
        "gate-batch",
    ]


def docker_command(validated: dict[str, Any], container_name: str, cid_file: pathlib.Path) -> list[str]:
    root = validated["root"]
    output_root = validated["output_root"]
    common_git = validated["common_git"]
    target_dir = validated["target_dir"]
    metrics_dir = validated["metrics_dir"]
    cargo_lock_path = validated["cargo_lock_path"]
    tools = validated["tools"]
    inner_resource_path = f"/scratch/{output_root.name}/container-resources.json"
    capture_script_path = "/work/dev/consolidation/final_canonical_replay.py"
    linker_host_path = pathlib.Path(tools["linker"]["path"])
    rustflags_variable = f"CARGO_TARGET_{TARGET.replace('-', '_').upper()}_RUSTFLAGS"
    batch_command_path = batch_command()
    argv = [
        "docker",
        "run",
        "--rm",
        "--init",
        "--platform",
        "linux/amd64",
        "--privileged",
        "--network",
        "none",
        f"--cpus={CPU_LIMIT}",
        f"--memory={MEMORY_LIMIT_BYTES // (1024 * 1024 * 1024)}g",
        f"--memory-swap={(MEMORY_LIMIT_BYTES + SWAP_LIMIT_BYTES) // (1024 * 1024 * 1024)}g",
        "--cidfile",
        str(cid_file),
        "--name",
        container_name,
        "--volume",
        f"{root}:/work:ro",
        "--volume",
        f"{target_dir}:/work/target:rw",
        "--volume",
        f"{validated['scratch_dir']}:/scratch:rw",
        "--volume",
        f"{validated['scratch_dir']}:{SCRATCH_CONTAINER_PATH}:rw",
        "--volume",
        f"{metrics_dir}:{METRICS_CONTAINER_PATH}:rw",
        "--volume",
        f"{cargo_lock_path}:{CARGO_LOCK_CONTAINER_PATH}:rw",
        "--volume",
        "xsh-cargo-registry:/root/.cargo/registry",
        "--volume",
        "xsh-consolidation-kache:/root/.cache/kache",
        "--volume",
        f"{common_git}:{common_git}:ro",
        "--volume",
        f"{linker_host_path}:/usr/local/bin/xsh-mold-linker:ro",
        "--volume",
        f"{tools['mold']['path']}:/usr/local/bin/mold:ro",
        "--volume",
        f"{tools['kache']['path']}:/usr/local/bin/kache:ro",
        "--volume",
        f"{tools['jemalloc']['path']}:/usr/lib/libjemalloc.so.2:ro",
        "--workdir",
        "/work",
    ]
    environment = {
        "TARGET": TARGET,
        "DIST_PROFILE": PROFILE,
        "CARGO_TARGET_DIR": "/work/target",
        "CARGO_BUILD_WARNINGS": "deny",
        "CARGO_BUILD_JOBS": "12",
        "CARGO_NET_OFFLINE": "true",
        "CARGO_HOME": "/root/.cargo",
        "XDG_CACHE_HOME": "/root/.cache",
        "TMPDIR": "/scratch",
        "HOST_UID": str(os.getuid()),
        "HOST_GID": str(os.getgid()),
        "RUSTC_WRAPPER": "/usr/local/bin/kache",
        "XSH_DEV_BUILD_PRELOAD": "/usr/lib/libjemalloc.so.2",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER": "/usr/local/bin/xsh-mold-linker",
        rustflags_variable: RUSTFLAGS,
    }
    for key, value in environment.items():
        argv.extend(["--env", f"{key}={value}"])
    return [
        *argv,
        IMAGE,
        "/usr/bin/python3",
        capture_script_path,
        "capture",
        inner_resource_path,
        "--",
        *batch_command_path,
    ]


def read_text(path: pathlib.Path) -> str | None:
    try:
        return path.read_text(encoding="utf-8").strip()
    except OSError:
        return None


def parse_pairs(value: str | None) -> dict[str, int] | None:
    if value is None:
        return None
    pairs: dict[str, int] = {}
    for line in value.splitlines():
        fields = line.split()
        if len(fields) != 2 or fields[0] in pairs:
            raise ReplayError("cgroup counter file has an invalid or duplicate row")
        try:
            value = int(fields[1])
        except ValueError as error:
            raise ReplayError("cgroup counter file contains a non-integer value") from error
        if value < 0:
            raise ReplayError("cgroup counter file contains a negative value")
        pairs[fields[0]] = value
    return pairs


def process_rss() -> tuple[int, int]:
    page_size = os.sysconf("SC_PAGE_SIZE")
    total = 0
    count = 0
    try:
        entries = pathlib.Path("/proc").iterdir()
        for entry in entries:
            if not entry.name.isdecimal():
                continue
            try:
                fields = (entry / "statm").read_text(encoding="ascii").split()
                total += int(fields[1]) * page_size
                count += 1
            except (OSError, IndexError, ValueError):
                continue
    except OSError:
        return 0, 0
    return total, count


def cgroup_snapshot() -> dict[str, Any]:
    cgroup = pathlib.Path("/sys/fs/cgroup")
    return {
        "cpu_max": read_text(cgroup / "cpu.max"),
        "cpu_stat": parse_pairs(read_text(cgroup / "cpu.stat")),
        "memory_current": read_text(cgroup / "memory.current"),
        "memory_peak": read_text(cgroup / "memory.peak"),
        "memory_max": read_text(cgroup / "memory.max"),
        "memory_events": parse_pairs(read_text(cgroup / "memory.events")),
        "memory_swap_current": read_text(cgroup / "memory.swap.current"),
        "memory_swap_peak": read_text(cgroup / "memory.swap.peak"),
        "memory_swap_max": read_text(cgroup / "memory.swap.max"),
        "memory_swap_events": parse_pairs(read_text(cgroup / "memory.swap.events")),
        "pids_current": read_text(cgroup / "pids.current"),
        "pids_peak": read_text(cgroup / "pids.peak"),
        "pids_max": read_text(cgroup / "pids.max"),
    }


def capture_snapshot() -> dict[str, Any]:
    rss_bytes, process_count = process_rss()
    return {
        "sampled_utc": dt.datetime.now(dt.timezone.utc).isoformat(timespec="milliseconds"),
        "process_rss_sum_bytes": rss_bytes,
        "visible_process_count": process_count,
        "cgroup": cgroup_snapshot(),
    }


def validate_cgroup_snapshot(snapshot: Any) -> None:
    if not isinstance(snapshot, dict):
        raise ReplayError("resource cgroup snapshot is missing")
    cpu_max = snapshot.get("cpu_max")
    if not isinstance(cpu_max, str) or re.fullmatch(r"[0-9]+ [0-9]+", cpu_max) is None:
        raise ReplayError("resource CPU limit is missing or invalid")
    quota, period = (int(value) for value in cpu_max.split())
    if period < 1 or quota != CPU_LIMIT * period:
        raise ReplayError("observed CPU limit does not match the pinned container limit")
    cpu_stat = snapshot.get("cpu_stat")
    if not isinstance(cpu_stat, dict):
        raise ReplayError("resource CPU counters are missing")
    for name in ("usage_usec", "user_usec", "system_usec", "nr_periods", "nr_throttled", "throttled_usec"):
        strict_int(cpu_stat.get(name), f"cgroup.cpu_stat.{name}")
    for name in ("memory_current", "memory_peak", "memory_max", "memory_swap_current", "memory_swap_peak", "memory_swap_max", "pids_current", "pids_peak"):
        value = snapshot.get(name)
        if not isinstance(value, str) or re.fullmatch(r"[0-9]+", value) is None:
            raise ReplayError(f"resource cgroup field {name} is missing or invalid")
    if int(snapshot["memory_max"]) != MEMORY_LIMIT_BYTES:
        raise ReplayError("observed memory limit does not match the pinned container limit")
    if int(snapshot["memory_swap_max"]) != SWAP_LIMIT_BYTES:
        raise ReplayError("observed swap limit does not match the pinned container limit")
    if int(snapshot["memory_current"]) > int(snapshot["memory_peak"]) or int(snapshot["memory_peak"]) > int(snapshot["memory_max"]):
        raise ReplayError("observed memory current or peak exceeds its reported limit")
    if int(snapshot["memory_swap_current"]) > int(snapshot["memory_swap_peak"]) or int(snapshot["memory_swap_peak"]) > SWAP_LIMIT_BYTES:
        raise ReplayError("observed swap current or peak exceeds its reported limit")
    if int(snapshot["pids_current"]) > int(snapshot["pids_peak"]):
        raise ReplayError("observed process count exceeds its reported peak")
    pids_max = snapshot.get("pids_max")
    if (
        not isinstance(pids_max, str)
        or (pids_max != "max" and re.fullmatch(r"[0-9]+", pids_max) is None)
        or (pids_max.isdecimal() and int(pids_max) < 1)
    ):
        raise ReplayError("resource process limit is missing or invalid")
    for name in ("memory_events", "memory_swap_events"):
        counters = snapshot.get(name)
        if not isinstance(counters, dict):
            raise ReplayError(f"resource cgroup {name} are missing")
        for key, value in counters.items():
            strict_int(value, f"cgroup.{name}.{key}")
    memory_events = snapshot["memory_events"]
    for name in ("oom", "oom_kill", "oom_group_kill"):
        strict_int(memory_events.get(name), f"cgroup.memory_events.{name}")
        if memory_events[name] != 0:
            raise ReplayError("container resource sample recorded an OOM event")
    swap_events = snapshot["memory_swap_events"]
    for name in ("fail", "high", "max"):
        strict_int(swap_events.get(name), f"cgroup.memory_swap_events.{name}")


def validate_resource_record(record: Any, expected_exit_code: int | None = None) -> None:
    if not isinstance(record, dict) or type(record.get("schema_version")) is not int or record["schema_version"] != 1:
        raise ReplayError("container resource record has an unsupported schema")
    command = record.get("command")
    if not isinstance(command, list) or not command or any(not isinstance(arg, str) for arg in command):
        raise ReplayError("container resource command is missing or invalid")
    if type(record.get("exit_code")) is not int or (expected_exit_code is not None and record["exit_code"] != expected_exit_code):
        raise ReplayError("container resource exit code does not match the batch result")
    if record.get("spawn_error") is not None and not isinstance(record.get("spawn_error"), str):
        raise ReplayError("container resource spawn error is invalid")
    received_signal = record.get("received_signal")
    if received_signal is not None and (not isinstance(received_signal, str) or received_signal not in signal.Signals.__members__):
        raise ReplayError("container resource signal is invalid")
    for name in ("started_utc", "finished_utc"):
        value = record.get(name)
        if not isinstance(value, str):
            raise ReplayError(f"container resource {name} is missing")
        try:
            dt.datetime.fromisoformat(value)
        except ValueError as error:
            raise ReplayError(f"container resource {name} is invalid") from error
    duration = record.get("duration_seconds")
    if isinstance(duration, bool) or not isinstance(duration, (int, float)) or not 0 <= duration < float("inf"):
        raise ReplayError("container resource duration is missing or invalid")
    if record.get("timing_claim") != "none" or record.get("resource_sample_interval_ms") != 100:
        raise ReplayError("container resource sampling metadata is invalid")
    if record.get("process_rss_peak_method") != "sum of resident pages for visible container processes, sampled at 100 ms":
        raise ReplayError("container process RSS method is missing or unsupported")
    process_rss_peak = strict_int(record.get("process_rss_peak_bytes"), "process_rss_peak_bytes")
    strict_int(record.get("resource_samples"), "resource_samples", minimum=2)
    snapshots = []
    process_rss_samples = []
    for name in ("initial", "final"):
        snapshot = record.get(name)
        if not isinstance(snapshot, dict):
            raise ReplayError(f"container resource {name} sample is missing")
        sampled = snapshot.get("sampled_utc")
        if not isinstance(sampled, str):
            raise ReplayError(f"container resource {name} timestamp is missing")
        try:
            dt.datetime.fromisoformat(sampled)
        except ValueError as error:
            raise ReplayError(f"container resource {name} timestamp is invalid") from error
        process_rss_samples.append(strict_int(snapshot.get("process_rss_sum_bytes"), f"{name}.process_rss_sum_bytes"))
        strict_int(snapshot.get("visible_process_count"), f"{name}.visible_process_count", minimum=1)
        validate_cgroup_snapshot(snapshot.get("cgroup"))
        snapshots.append(snapshot["cgroup"])
    if process_rss_peak < max(process_rss_samples):
        raise ReplayError("container process RSS peak is below a recorded sample")
    for name in ("cpu_max", "memory_max", "memory_swap_max"):
        if snapshots[0][name] != snapshots[1][name]:
            raise ReplayError(f"container {name} changed between resource samples")
    final_cgroup = snapshots[1]
    if record.get("cgroup_limits_and_peaks") != final_cgroup:
        raise ReplayError("container cgroup summary does not match the final resource sample")
    strict_int(record.get("oom_events"), "oom_events")
    strict_int(record.get("oom_kill_events"), "oom_kill_events")
    if record["oom_events"] != 0 or record["oom_kill_events"] != 0:
        raise ReplayError("container resource record contains OOM events")
    if record.get("oom_events") != final_cgroup["memory_events"]["oom"]:
        raise ReplayError("container OOM counter does not match the final resource sample")
    if record.get("oom_kill_events") != final_cgroup["memory_events"]["oom_kill"]:
        raise ReplayError("container OOM-kill counter does not match the final resource sample")
    swap_events = record.get("swap_events")
    if not isinstance(swap_events, dict):
        raise ReplayError("container swap counters are missing")
    for name, value in swap_events.items():
        strict_int(value, f"swap_events.{name}")
    if swap_events != final_cgroup["memory_swap_events"]:
        raise ReplayError("container swap counters do not match the final resource sample")


def write_json(path: pathlib.Path, value: Any) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(temporary, path)


def capture_command(output_path: pathlib.Path, command: list[str]) -> int:
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        raise ReplayError("capture requires the command after --")
    initial = capture_snapshot()
    rss_peak = int(initial["process_rss_sum_bytes"])
    sample_count = 1
    child: subprocess.Popen[bytes] | None = None
    received_signal: int | None = None
    signal_time: float | None = None

    def forward_signal(signum: int, _frame: Any) -> None:
        nonlocal received_signal, signal_time
        received_signal = signum
        signal_time = time.monotonic()
        if child is not None and child.poll() is None:
            try:
                os.killpg(child.pid, signum)
            except ProcessLookupError:
                pass

    previous = {signum: signal.signal(signum, forward_signal) for signum in (signal.SIGINT, signal.SIGTERM)}
    start_time = time.monotonic()
    started = utc_now()
    status: int | None = None
    spawn_error: str | None = None
    try:
        child = subprocess.Popen(command, cwd="/work", start_new_session=True, stdin=subprocess.DEVNULL)
        while True:
            try:
                status = child.wait(timeout=SAMPLE_INTERVAL_SECONDS)
                break
            except subprocess.TimeoutExpired:
                sample = capture_snapshot()
                rss_peak = max(rss_peak, int(sample["process_rss_sum_bytes"]))
                sample_count += 1
                if signal_time is not None and time.monotonic() - signal_time > TERM_GRACE_SECONDS:
                    try:
                        os.killpg(child.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    signal_time = None
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
                status = child.wait(timeout=TERM_GRACE_SECONDS)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                status = child.wait()
        final = capture_snapshot()
        rss_peak = max(rss_peak, int(final["process_rss_sum_bytes"]))
        sample_count += 1
        for signum, handler in previous.items():
            signal.signal(signum, handler)

    duration_seconds = round(time.monotonic() - start_time, 3)
    cgroup = final["cgroup"]
    result = {
        "schema_version": 1,
        "command": command,
        "started_utc": started,
        "finished_utc": utc_now(),
        "exit_code": status,
        "spawn_error": spawn_error,
        "received_signal": signal.Signals(received_signal).name if received_signal else None,
        "duration_seconds": duration_seconds,
        "timing_claim": "none",
        "resource_sample_interval_ms": int(SAMPLE_INTERVAL_SECONDS * 1000),
        "process_rss_peak_bytes": rss_peak,
        "process_rss_peak_method": "sum of resident pages for visible container processes, sampled at 100 ms",
        "resource_samples": sample_count,
        "initial": initial,
        "final": final,
        "cgroup_limits_and_peaks": cgroup,
        "oom_events": (cgroup.get("memory_events") or {}).get("oom"),
        "oom_kill_events": (cgroup.get("memory_events") or {}).get("oom_kill"),
        "swap_events": cgroup.get("memory_swap_events"),
    }
    write_json(output_path, result)
    return status if status is not None else 1


def product_outputs(root: pathlib.Path, target_dir: pathlib.Path) -> dict[str, Any]:
    paths = {
        "xsh-verification": target_dir / TARGET / PROFILE / "xsh",
        "xsht-verification": target_dir / TARGET / PROFILE / "xsht",
        "xshi-verification": target_dir / TARGET / PROFILE / "xshi",
    }
    products: dict[str, Any] = {}
    for name, path in paths.items():
        if path.is_file() and not path.is_symlink():
            products[name] = {
                "path": str(path.relative_to(root)),
                "size_bytes": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        else:
            products[name] = {"path": str(path.relative_to(root)), "present": False}
    return products


def source_check(validated: dict[str, Any], point: str) -> dict[str, Any]:
    snapshot = git_snapshot(validated["root"])
    if (snapshot["head"], snapshot["tree"]) != (validated["args"].head, validated["args"].tree):
        raise ReplayError(f"source freeze changed at {point}: {snapshot}")
    if not snapshot["clean"]:
        raise ReplayError(f"source became dirty at {point}: {snapshot['status']}")
    return {"where": point, **snapshot}


def stop_container(cid_file: pathlib.Path, container_name: str | None = None) -> dict[str, Any]:
    try:
        container_id = cid_file.read_text(encoding="utf-8").strip()
    except OSError as error:
        container_id = ""
        read_error = str(error)
    else:
        read_error = None
    if re.fullmatch(r"[0-9a-fA-F]{12,64}", container_id) is None:
        if container_name is None:
            return {"resolved": False, "error": read_error or "container ID is missing or malformed"}
        argv = ["docker", "ps", "--all", "--no-trunc", "--filter", f"name=^/{container_name}$", "--format", "{{.ID}}"]
        try:
            result = subprocess.run(argv, capture_output=True, text=True, timeout=35, check=False)
            absence = {"argv": list(argv), "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
        except (OSError, subprocess.TimeoutExpired) as error:
            absence = {"argv": list(argv), "error": str(error)}
        resolved = "exit_code" in absence and absence["exit_code"] == 0 and absence.get("stdout", "").strip() == ""
        return {"resolved": resolved, "error": read_error, "final_absence_check": absence}
    results: list[dict[str, Any]] = []
    cleanup_commands = (
        ["docker", "stop", "--time", "25", container_id],
        ["docker", "rm", "--force", container_id],
        ["docker", "ps", "--all", "--no-trunc", "--filter", f"id={container_id}", "--format", "{{.ID}}"],
    )
    for argv in cleanup_commands:
        try:
            result = subprocess.run(argv, capture_output=True, text=True, timeout=35, check=False)
            results.append({"argv": list(argv), "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
        except (OSError, subprocess.TimeoutExpired) as error:
            results.append({"argv": list(argv), "error": str(error)})
    presence = results[-1]
    resolved = (
        "exit_code" in presence
        and presence["exit_code"] == 0
        and presence.get("stdout", "").strip() == ""
    )
    return {"resolved": resolved, "container_id": container_id, "commands": results}


def run_batch(validated: dict[str, Any]) -> int:
    root = validated["root"]
    output_root = validated["output_root"]
    args = validated["args"]
    lock_path = validated["lock_path"]
    lock = lock_path.open("r+")
    try:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        before = source_check(validated, "before")
        image_id = run_text(["docker", "image", "inspect", "--format", "{{.Id}}", IMAGE], root)
        if image_id != IMAGE:
            raise ReplayError(f"pinned Docker image ID differs: {image_id}")
        output_root.mkdir()
        inner = output_root / "container-resources.json"
        stdout_path = output_root / "stdout.log"
        stderr_path = output_root / "stderr.log"
        cid_file = output_root / "container.cid"
        for path in (stdout_path, stderr_path):
            path.touch()
        target_mount.mkdir(exist_ok=True)
        container_name = f"xsh-canonical-{args.run_name}"
        command = docker_command(validated, container_name, cid_file)
        receipt = {
            "schema_version": 1,
            "kind": "canonical_gate_batch_replay",
            "acceptance_credit": "none from this runner; the canonical gate result must be reviewed separately",
            "status": "running",
            "run_name": args.run_name,
            "started_utc": utc_now(),
            "source_expected": {"commit": args.head, "tree": args.tree},
            "source_checks": [before],
            "prior_attempt": validated["prior_attempt"],
            "image": {"id": image_id, "platform": "linux/amd64"},
            "profile": PROFILE,
            "release_profile": "reserved for later final performance checks",
            "resource_limits": {
                "cpus": CPU_LIMIT,
                "memory_bytes": MEMORY_LIMIT_BYTES,
                "swap_bytes": SWAP_LIMIT_BYTES,
                "network": "none",
            },
            "network_comparison": {
                "retained_prior_attempt": "Docker default bridge",
                "this_run": "network none",
                "reason": "the batch test network uses are loopback or mocked; Cargo is offline",
            },
            "tooling": validated["tools"],
            "bootstrap": {
                "artifact": validated["bootstrap"],
                "target": TARGET,
                "profile": PROFILE,
                "build_receipt": validated["bootstrap_receipt"],
            },
            "cargo_lock": {
                "host_path": str(validated["cargo_lock_path"]),
                "container_path": CARGO_LOCK_CONTAINER_PATH,
            },
            "command": batch_command(),
            "docker_argv": command,
            "logs": {},
            "container_resources": None,
            "products_after": None,
            "failure": None,
        }
        receipt_path = output_root / "canonical-replay.json"
        write_json(receipt_path, receipt)
        host_environment = dict(os.environ)
        host_environment.pop("LD_PRELOAD", None)
        host_environment["TMPDIR"] = str(validated["scratch_dir"])
        started = time.monotonic()
        process: subprocess.Popen[bytes] | None = None
        timed_out = False
        interrupted: int | None = None
        cleanup: dict[str, Any] | None = None
        previous_handlers: dict[int, Any] = {}

        def note_signal(signum: int, _frame: Any) -> None:
            nonlocal interrupted
            interrupted = signum

        previous_handlers = {signum: signal.signal(signum, note_signal) for signum in (signal.SIGINT, signal.SIGTERM)}
        try:
            with stdout_path.open("wb") as stdout_log, stderr_path.open("wb") as stderr_log:
                process = subprocess.Popen(
                    command,
                    cwd=root,
                    env=host_environment,
                    stdin=subprocess.DEVNULL,
                    stdout=stdout_log,
                    stderr=stderr_log,
                    start_new_session=True,
                )
                while process.poll() is None:
                    if interrupted is not None:
                        cleanup = stop_container(cid_file, container_name)
                        try:
                            process.wait(timeout=TERM_GRACE_SECONDS)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid, signal.SIGKILL)
                            process.wait()
                        break
                    if time.monotonic() - started > args.timeout_seconds:
                        timed_out = True
                        cleanup = stop_container(cid_file, container_name)
                        try:
                            process.wait(timeout=TERM_GRACE_SECONDS)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid, signal.SIGKILL)
                            process.wait()
                        break
                    time.sleep(0.2)
                exit_code = process.returncode if process.returncode is not None else 124
        except OSError as error:
            exit_code = 127
            receipt["failure"] = str(error)
        finally:
            for signum, handler in previous_handlers.items():
                signal.signal(signum, handler)

        earlier_cleanup = cleanup
        cleanup = stop_container(cid_file, container_name)
        if earlier_cleanup is not None:
            cleanup["before_process_exit"] = earlier_cleanup

        after = git_snapshot(root)
        receipt["source_checks"].append(after)
        source_after_ok = (
            after["head"] == args.head
            and after["tree"] == args.tree
            and after["clean"]
        )
        if not source_after_ok:
            receipt["failure"] = f"source freeze changed after the batch: {after}"
        receipt["finished_utc"] = utc_now()
        receipt["exit_code"] = exit_code
        receipt["timed_out"] = timed_out
        receipt["received_signal"] = signal.Signals(interrupted).name if interrupted else None
        receipt["duration_seconds"] = round(time.monotonic() - started, 3)
        receipt["timing_claim"] = "none"
        receipt["cleanup"] = cleanup
        receipt["products_after"] = product_outputs(root, validated["target_dir"])
        resource_capture_ok = inner.is_file()
        if resource_capture_ok:
            try:
                with inner.open(encoding="utf-8") as resource_file:
                    resource_record = json.load(resource_file)
                receipt["container_resources"] = {
                    "path": str(inner.relative_to(output_root)),
                    "sha256": sha256_file(inner),
                    "record": resource_record,
                }
                validate_resource_record(resource_record, exit_code)
                resource_capture_ok = True
            except (OSError, UnicodeError, json.JSONDecodeError, AttributeError, ReplayError) as error:
                receipt["failure"] = f"container resource capture is invalid: {error}"
                resource_capture_ok = False
        else:
            receipt["failure"] = "container resource capture is missing"
        receipt["logs"] = {
            "stdout": {"path": str(stdout_path.relative_to(output_root)), "sha256": sha256_file(stdout_path), "size_bytes": stdout_path.stat().st_size},
            "stderr": {"path": str(stderr_path.relative_to(output_root)), "sha256": sha256_file(stderr_path), "size_bytes": stderr_path.stat().st_size},
        }
        cleanup_ok = cleanup.get("resolved") is True
        if not cleanup_ok:
            receipt["failure"] = f"container cleanup is unresolved: {cleanup}"
        receipt["status"] = "passed" if exit_code == 0 and not timed_out and interrupted is None and source_after_ok and resource_capture_ok and cleanup_ok else "failed-or-incomplete"
        write_json(receipt_path, receipt)
        print(f"receipt={receipt_path}")
        print(f"status={receipt['status']} exit_code={exit_code} timing_claim=none acceptance_credit=none")
        if not cleanup_ok:
            raise ReplayError(f"container cleanup is unresolved: {cleanup}")
        return 0 if receipt["status"] == "passed" else 1
    finally:
        fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
        lock.close()


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="mode", required=True)
    host = subparsers.add_parser("execute")
    host.add_argument("--repo", type=pathlib.Path, required=True)
    host.add_argument("--head", required=True)
    host.add_argument("--tree", required=True)
    host.add_argument("--output", type=pathlib.Path, required=True)
    host.add_argument("--run-name", required=True)
    host.add_argument("--bootstrap-receipt", type=pathlib.Path, required=True)
    host.add_argument("--timeout-seconds", type=int, default=CASE_TIMEOUT_SECONDS)
    capture = subparsers.add_parser("capture")
    capture.add_argument("output", type=pathlib.Path)
    capture.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.mode == "capture":
        try:
            return capture_command(args.output, args.command)
        except Exception as error:
            print(f"container capture failed: {error}", file=sys.stderr)
            return 125
    if re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}", args.run_name) is None:
        parser.error("--run-name must be a path-safe label of at most 64 characters")
    if args.timeout_seconds < 1 or args.timeout_seconds > CASE_TIMEOUT_SECONDS:
        parser.error(f"--timeout-seconds must be between 1 and {CASE_TIMEOUT_SECONDS}")
    try:
        validated = validate_inputs(args)
        return run_batch(validated)
    except (ReplayError, OSError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
