#!/usr/bin/env python3
"""Run paired native-suite lists and correctness gates against one sealed tree."""
from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
from pathlib import PurePosixPath
import re
import secrets
import subprocess
import sys
import time
from datetime import datetime, timezone

sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "final-performance-gates"))
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
from outcomes import compare_pair, manifest_sha256, read_manifest, validate_log


PINNED_IMAGE = "xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
TARGET = "x86_64-unknown-linux-musl"
ROOT = Path(__file__).resolve().parents[5]
INFRA = ROOT / ".work/consolidation/infra"
SHARED_LOCK_PATH = INFRA / "native-suite.lock"
PERFORMANCE_LOCK_PATH = INFRA / "final-performance.lock"
CPUSETS = "0-31"
CPU_QUOTA = 32
MEMORY_LIMIT = 40 * 1024**3
SUITE_ROUNDS = 3
WORKERS = {"reference": 8, "candidate": 32}
BINARIES = ("xsht", "xsh", "xsh-test-helper")
LABEL_RE = re.compile(r"^[a-z0-9][a-z0-9-]{0,47}$")
REFERENCE_OVERLAY_PATHS = {
    "crates/xsht/src/xsht/test.rs",
    "crates/xsht/src/commands.rs",
    "docs/XSHT.md",
}
HERE = Path(__file__).resolve().parent
CONTAINER_RUNNER = HERE / "container-run.sh"
OUTCOMES_MODULE = HERE / "outcomes.py"
VALIDATOR = HERE / "validator.py"
REFERENCE_AGGREGATE_PATCH_RELATIVE = Path(
    "dev/consolidation/winddown/unfinished/475-native-scheduler-reference-2083821edb768d2485479c7386289b82e43b1c7c.patch"
)
REFERENCE_AGGREGATE_PATCH_SHA256 = "b9b1d14e53bda60cd57523936f0a7a0fcc3c021fad5465edb05d85269076b26c"
REFERENCE_SCHEDULER_COMMITS = (
    ("bounded-admission", "7205a0", "7205a0b6afce7d9a0072b6727cae36a0b8301963"),
    ("default-width", "aa5aca3", "aa5aca3acbd2dafd03bf3b5a4fb2cb63f399f896"),
    ("round-robin", "3a6c3", "3a6c3daacf226f8f2c6049dbadf200dd38756c86"),
)


def parse_cpu_set(raw: str) -> list[int]:
    cpus = []
    for field in raw.split(","):
        bounds = field.split("-", 1)
        try:
            start = int(bounds[0])
            end = int(bounds[1]) if len(bounds) == 2 else start
        except ValueError as error:
            raise ValueError(f"invalid cgroup CPU set: {raw!r}") from error
        if start < 0 or end < start:
            raise ValueError(f"invalid cgroup CPU range: {field!r}")
        cpus.extend(range(start, end + 1))
    if not cpus or len(cpus) != len(set(cpus)):
        raise ValueError(f"invalid or duplicate cgroup CPU set: {raw!r}")
    return sorted(cpus)


def parse_cgroup_snapshot(text: str) -> dict:
    sections: dict[str, dict | None] = {}
    current: str | None = None
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            current = line[1:-1].rsplit("/", 1)[-1]
            if current in sections:
                raise ValueError(f"duplicate cgroup section: {current}")
            sections[current] = {}
            continue
        if current is None:
            raise ValueError("cgroup snapshot content appeared before its section header")
        if line == "unavailable":
            sections[current] = None
            continue
        fields = line.split()
        section = sections[current]
        if section is None:
            raise ValueError(f"cgroup section {current} is unavailable")
        if current == "cpuset.cpus.effective":
            if set(section) or len(fields) != 1:
                raise ValueError("invalid or duplicate cpuset.cpus.effective value")
            section["value"] = fields[0]
            continue
        if current == "cpu.max":
            if set(section) or len(fields) != 2:
                raise ValueError("invalid or duplicate cpu.max value")
            try:
                quota_us, period_us = (int(value) for value in fields)
            except ValueError as error:
                raise ValueError(f"invalid cgroup CPU quota: {line!r}") from error
            section.update({"quota_us": quota_us, "period_us": period_us})
            continue
        if current == "pids.max":
            if set(section) or len(fields) != 1:
                raise ValueError("invalid or duplicate pids.max value")
            if fields[0] == "max":
                section["value"] = None
            else:
                try:
                    section["value"] = int(fields[0])
                except ValueError as error:
                    raise ValueError(f"invalid cgroup pids.max value: {line!r}") from error
            continue
        try:
            if len(fields) == 1:
                key, value = "value", int(fields[0])
            elif len(fields) == 2:
                key, value = fields[0], int(fields[1])
            else:
                raise ValueError
        except ValueError as error:
            raise ValueError(f"invalid cgroup value in {current}: {line!r}") from error
        if section is None or key in section:
            raise ValueError(f"invalid or duplicate cgroup value in {current}: {key}")
        section[key] = value

    def scalar(name: str) -> int:
        section = sections.get(name)
        if not isinstance(section, dict) or set(section) != {"value"}:
            raise ValueError(f"cgroup snapshot is missing scalar {name}")
        return section["value"]

    events = sections.get("memory.events")
    if not isinstance(events, dict) or not {"oom", "oom_kill"} <= events.keys():
        raise ValueError("cgroup snapshot is missing memory OOM event counters")
    cpu_max = sections.get("cpu.max")
    if not isinstance(cpu_max, dict) or set(cpu_max) != {"quota_us", "period_us"}:
        raise ValueError("cgroup snapshot is missing cpu.max")
    cpuset = sections.get("cpuset.cpus.effective")
    if not isinstance(cpuset, dict) or set(cpuset) != {"value"}:
        raise ValueError("cgroup snapshot is missing cpuset.cpus.effective")
    pids_max = sections.get("pids.max")
    if not isinstance(pids_max, dict) or set(pids_max) != {"value"}:
        raise ValueError("cgroup snapshot is missing pids.max")
    return {
        "cpu_quota_us": cpu_max["quota_us"],
        "cpu_period_us": cpu_max["period_us"],
        "cpuset_cpus": parse_cpu_set(cpuset["value"]),
        "memory_max_bytes": scalar("memory.max"),
        "memory_current_bytes": scalar("memory.current"),
        "memory_peak_bytes": scalar("memory.peak"),
        "memory_swap_current_bytes": scalar("memory.swap.current"),
        "memory_swap_max_bytes": scalar("memory.swap.max"),
        "memory_events": events,
        "pids_current": scalar("pids.current"),
        "pids_peak": scalar("pids.peak"),
        "pids_max_limit": pids_max["value"],
    }


def validate_cgroup_snapshot(snapshot: dict, *, memory_limit_bytes: int = MEMORY_LIMIT) -> dict:
    expected_quota = CPU_QUOTA * 100_000
    if snapshot["cpu_quota_us"] != expected_quota or snapshot["cpu_period_us"] != 100_000:
        raise ValueError("container cgroup CPU quota differs from the 32-CPU contract")
    if snapshot["cpuset_cpus"] != parse_cpu_set(CPUSETS):
        raise ValueError("container cgroup cpuset differs from the 0-31 contract")
    if snapshot["memory_max_bytes"] != memory_limit_bytes:
        raise ValueError("container cgroup memory.max differs from the 40-GiB contract")
    if snapshot["memory_current_bytes"] > memory_limit_bytes or snapshot["memory_peak_bytes"] > memory_limit_bytes:
        raise ValueError("container cgroup memory current or peak exceeded its configured limit")
    if snapshot["memory_swap_max_bytes"] != 0 or snapshot["memory_swap_current_bytes"] != 0:
        raise ValueError("container cgroup swap current and max must both be zero")
    if snapshot["pids_current"] > snapshot["pids_peak"]:
        raise ValueError("container cgroup pids current exceeds its observed peak")
    pids_max = snapshot["pids_max_limit"]
    if pids_max is not None and (pids_max < 0 or snapshot["pids_peak"] > pids_max):
        raise ValueError("container cgroup pids peak exceeds its configured limit")
    events = snapshot["memory_events"]
    if events["oom"] != 0 or events["oom_kill"] != 0 or events.get("oom_group_kill", 0) != 0:
        raise ValueError("container cgroup reports an OOM event")
    return snapshot


def validate_cgroup_snapshots(before_text: str, after_text: str) -> dict:
    before = validate_cgroup_snapshot(parse_cgroup_snapshot(before_text))
    after = validate_cgroup_snapshot(parse_cgroup_snapshot(after_text))
    for key in ("oom", "oom_kill", "oom_group_kill"):
        if after["memory_events"].get(key, 0) < before["memory_events"].get(key, 0):
            raise ValueError(f"container cgroup OOM event counter moved backwards: {key}")
    return {"before": before, "after": after, "contract_valid": True}


def suite_execution_order(round_index: int) -> tuple[str, str]:
    if round_index < 0:
        raise ValueError("suite round index must be nonnegative")
    return ("reference", "candidate") if round_index % 2 == 0 else ("candidate", "reference")


def summarize_suite_samples(samples: list[float]) -> dict:
    if len(samples) != SUITE_ROUNDS or any(sample is None for sample in samples):
        raise ValueError(f"suite timing summary requires {SUITE_ROUNDS} complete rounds")
    minimum = min(samples)
    maximum = max(samples)
    return {"samples": samples, "minimum": minimum, "maximum": maximum, "spread": maximum - minimum}


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def git_diff_sha256(root: Path) -> str:
    patch = subprocess.check_output(["git", "-C", str(root), "diff", "--binary", "HEAD"])
    return sha256_bytes(patch)


def capture_source(root: Path, expected_commit: str) -> dict:
    if git(root, "rev-parse", "--show-toplevel") != str(root):
        raise ValueError(f"source path is not the Git root: {root}")
    commit = git(root, "rev-parse", "HEAD")
    if commit != expected_commit:
        raise ValueError(f"source commit is {commit}, expected sealed commit {expected_commit}")
    status = git(root, "status", "--porcelain", "--untracked-files=normal")
    if status:
        raise ValueError(f"sealed source worktree is dirty:\n{status}")

    rows = []
    for relative in git(root, "ls-files", "-z").split("\0"):
        if not relative:
            continue
        path = root / relative
        if not path.is_file() and not path.is_symlink():
            raise ValueError(f"tracked source file is missing or not a file: {relative}")
        stat = path.lstat()
        if path.is_symlink():
            contents = os.fsencode(os.readlink(path))
            rows.append({
                "path": relative,
                "kind": "symlink",
                "sha256": sha256_bytes(contents),
                "size_bytes": len(contents),
                "mode": oct(stat.st_mode & 0o777),
            })
        else:
            rows.append({
                "path": relative,
                "kind": "file",
                "sha256": sha256_file(path),
                "size_bytes": stat.st_size,
                "mode": oct(stat.st_mode & 0o777),
            })
    rows.sort(key=lambda row: row["path"])
    canonical = json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()
    return {
        "commit": commit,
        "worktree_status": "clean",
        "source_diff_sha256": git_diff_sha256(root),
        "tracked_file_count": len(rows),
        "tracked_tree_sha256": sha256_bytes(canonical),
        "files": rows,
    }


def capture_reference_source(root: Path, base_commit: str) -> dict:
    """Capture a scheduler-only working overlay rooted at the sealed source HEAD."""
    if git(root, "rev-parse", "--show-toplevel") != str(root):
        raise ValueError(f"reference source path is not the Git root: {root}")
    head = git(root, "rev-parse", "HEAD")
    if head != base_commit:
        raise ValueError(f"reference source base HEAD is {head}, expected sealed source {base_commit}")
    untracked = [
        path for path in git(root, "ls-files", "--others", "--exclude-standard", "-z").split("\0")
        if path
    ]
    if untracked:
        raise ValueError(f"reference source has untracked files: {untracked[:8]}")
    changed = [
        path for path in git(root, "diff", "--name-only", "-z", "HEAD", "--").split("\0")
        if path
    ]
    disallowed = sorted(set(changed) - REFERENCE_OVERLAY_PATHS)
    if disallowed:
        raise ValueError(f"reference source changes paths outside the scheduler overlay: {disallowed}")
    rows = []
    for relative in git(root, "ls-files", "-z").split("\0"):
        if not relative:
            continue
        path = root / relative
        if not path.is_file() and not path.is_symlink():
            raise ValueError(f"tracked reference source file is missing: {relative}")
        stat = path.lstat()
        if path.is_symlink():
            contents = os.fsencode(os.readlink(path))
            row = {
                "path": relative,
                "kind": "symlink",
                "sha256": sha256_bytes(contents),
                "size_bytes": len(contents),
                "mode": oct(stat.st_mode & 0o777),
            }
        else:
            row = {
                "path": relative,
                "kind": "file",
                "sha256": sha256_file(path),
                "size_bytes": stat.st_size,
                "mode": oct(stat.st_mode & 0o777),
            }
        rows.append(row)
    rows.sort(key=lambda row: row["path"])
    canonical = json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()
    return {
        "base_head": head,
        "worktree_status": "tracked scheduler overlay; no untracked files",
        "changed_paths": sorted(set(changed)),
        "source_diff_sha256": git_diff_sha256(root),
        "tracked_file_count": len(rows),
        "tracked_tree_sha256": sha256_bytes(canonical),
        "files": rows,
    }


def source_summary(snapshot: dict) -> dict:
    return {
        "commit": snapshot["commit"],
        "worktree_status": snapshot["worktree_status"],
        "tracked_file_count": snapshot["tracked_file_count"],
        "tracked_tree_sha256": snapshot["tracked_tree_sha256"],
    }


def capture_binary_snapshot(reference_dir: Path, candidate_dir: Path) -> dict:
    result = {}
    for pair, root in (("reference", reference_dir), ("candidate", candidate_dir)):
        binaries = {}
        for name in BINARIES:
            path = root / name
            record = {"path": str(path)}
            try:
                if not path.is_file():
                    record["error"] = "missing or not a regular file"
                else:
                    stat = path.stat()
                    record.update({
                        "sha256": sha256_file(path),
                        "size_bytes": stat.st_size,
                        "mode": oct(stat.st_mode & 0o777),
                        "executable": os.access(path, os.X_OK),
                    })
            except OSError as exc:
                record["error"] = str(exc)
            binaries[name] = record
        result[pair] = binaries
    return result


def capture_binaries(reference_dir: Path, candidate_dir: Path) -> dict:
    result = capture_binary_snapshot(reference_dir, candidate_dir)
    for pair in ("reference", "candidate"):
        for name in BINARIES:
            record = result[pair][name]
            if record.get("error") or not record.get("executable"):
                raise ValueError(f"required executable is missing or not executable: {record['path']}")
    for name in ("xsh", "xsh-test-helper"):
        if result["reference"][name]["sha256"] != result["candidate"][name]["sha256"]:
            raise ValueError(f"paired child binary differs: {name}")
    if result["reference"]["xsht"]["sha256"] == result["candidate"]["xsht"]["sha256"]:
        raise ValueError("reference and candidate xsht binaries unexpectedly match")
    return result


def inspect_pinned_image(image: str) -> dict:
    command = ["docker", "image", "inspect", image]
    output = subprocess.check_output(command, text=True)
    inspected = json.loads(output)
    if not isinstance(inspected, list) or len(inspected) != 1:
        raise ValueError("docker image inspect did not return exactly one image")
    info = inspected[0]
    expected_id = image.split("@", 1)[1]
    image_id = info.get("Id")
    repo_digests = info.get("RepoDigests") or []
    if image_id != expected_id:
        raise ValueError(f"Docker image ID is {image_id!r}, expected pinned ID {expected_id!r}")
    if image not in repo_digests:
        raise ValueError(f"pinned image reference is absent from RepoDigests: {repo_digests}")
    if info.get("Os") != "linux" or info.get("Architecture") != "amd64":
        raise ValueError(
            f"pinned image platform is {info.get('Os')}/{info.get('Architecture')}, expected linux/amd64"
        )
    return {
        "inspect_command": command,
        "image_reference": image,
        "image_id": image_id,
        "repo_digests": sorted(repo_digests),
        "os": info.get("Os"),
        "architecture": info.get("Architecture"),
    }


def validate_build_receipt(
    path: Path, *, label: str, source_root: Path, source_commit: str,
    source_manifest: dict, binaries: dict,
) -> dict:
    raw = path.read_bytes()
    receipt = json.loads(raw)
    if not isinstance(receipt, dict):
        raise ValueError("reference build receipt must be a JSON object")
    required = {
        "schema_version": 1,
        "source_root": str(source_root),
        "base_head": source_commit,
        "source_diff_sha256": source_manifest["source_diff_sha256"],
        "working_directory": str(source_root),
        "target": TARGET,
        "profile": "release",
        "build_image": PINNED_IMAGE,
    }
    for field, expected in required.items():
        if receipt.get(field) != expected:
            raise ValueError(
                f"{label} build receipt field {field!r} is {receipt.get(field)!r}, expected {expected!r}"
            )
    recorded_binaries = receipt.get("binaries")
    if not isinstance(recorded_binaries, dict) or set(recorded_binaries) != set(binaries):
        raise ValueError(f"{label} build receipt must list exactly these binaries: {sorted(binaries)}")
    for name, binary in binaries.items():
        expected_binary = {"path": binary["path"], "sha256": binary["sha256"]}
        if recorded_binaries.get(name) != expected_binary:
            raise ValueError(
                f"{label} build receipt binary {name!r} is {recorded_binaries.get(name)!r}, "
                f"expected {expected_binary!r}"
            )
    command = receipt.get("build_command")
    if not isinstance(command, list) or not command or not all(isinstance(item, str) for item in command):
        raise ValueError(f"{label} build receipt needs a nonempty exact build_command string array")
    return {
        "path": str(path),
        "sha256": sha256_bytes(raw),
        "receipt": receipt,
        "input_copy_name": f"{label}-build-receipt.input.json",
    }


def validate_reference_overlay(
    candidate_source: dict, reference_source: dict, reference_source_root: Path
) -> list[str]:
    candidate_files = {row["path"]: row for row in candidate_source["files"]}
    reference_files = {row["path"]: row for row in reference_source["files"]}
    if candidate_files.keys() != reference_files.keys():
        raise ValueError("reference source tracked paths differ from the sealed candidate source")
    changed = sorted(
        path for path in candidate_files
        if candidate_files[path] != reference_files[path]
    )
    if set(changed) - REFERENCE_OVERLAY_PATHS:
        raise ValueError(f"reference source content differs outside the scheduler overlay: {changed}")
    required = {"crates/xsht/src/xsht/test.rs", "crates/xsht/src/commands.rs"}
    if not required <= set(changed):
        raise ValueError(f"reference source does not contain the required scheduler/help overlay: {changed}")
    if set(changed) != set(reference_source["changed_paths"]):
        raise ValueError("reference source Git diff and tracked-file hashes disagree")
    reference_diff = subprocess.check_output([
        "git", "-C", str(reference_source_root), "diff", "--binary", "--full-index", "HEAD"
    ])
    if sha256_bytes(reference_diff) != REFERENCE_AGGREGATE_PATCH_SHA256:
        raise ValueError("reference source diff does not match the pinned scheduler reference overlay")
    return changed


def record_binary_checkpoint(
    receipt: dict, receipt_path: Path, *, checkpoint: str,
    reference_dir: Path, candidate_dir: Path, initial: dict,
) -> bool:
    current = capture_binary_snapshot(reference_dir, candidate_dir)
    differences = []
    for pair in ("reference", "candidate"):
        for name in BINARIES:
            before = initial[pair][name]
            after = current[pair][name]
            if before != after:
                differences.append({"pair": pair, "binary": name, "before": before, "after": after})
    entry = {
        "checkpoint": checkpoint,
        "created_utc": datetime.now(timezone.utc).isoformat(),
        "binaries": current,
        "changed_vs_initial": differences,
    }
    receipt.setdefault("binary_snapshots", []).append(entry)
    if differences:
        receipt.setdefault("binary_integrity_errors", []).append(
            {"checkpoint": checkpoint, "changed_binaries": differences}
        )
    write_json(receipt_path, receipt)
    return not differences


def verify_timed_stage_inputs(
    *, source_root: Path, expected_source: dict, reference_dir: Path,
    candidate_dir: Path, expected_binaries: dict,
) -> dict:
    source = capture_source(source_root, expected_source["commit"])
    if source["tracked_tree_sha256"] != expected_source["tracked_tree_sha256"]:
        raise ValueError("sealed source tree changed during the timed scheduler stage")
    binaries = capture_binaries(reference_dir, candidate_dir)
    if binaries != expected_binaries:
        raise ValueError("one or more timed scheduler binaries changed during the stage")
    return {"source": source_summary(source), "binaries": binaries, "unchanged": True}


def capture_reference_derivation_patch(path: Path) -> dict:
    if not path.is_file():
        raise ValueError(f"reference derivation patch is missing: {path}")
    actual_hash = sha256_file(path)
    if actual_hash != REFERENCE_AGGREGATE_PATCH_SHA256:
        raise ValueError(f"reference derivation patch hash changed: {path}")
    return {"sha256": actual_hash, "size_bytes": path.stat().st_size}


def validate_reference_scheduler_ancestry(source_root: Path, base_commit: str) -> None:
    for name, abbreviation, expected_commit in REFERENCE_SCHEDULER_COMMITS:
        try:
            resolved_commit = git(source_root, "rev-parse", "--verify", f"{abbreviation}^{{commit}}")
        except subprocess.CalledProcessError as error:
            raise ValueError(f"reference scheduler commit cannot be resolved: {name} {abbreviation}") from error
        if resolved_commit != expected_commit:
            raise ValueError(
                f"reference scheduler commit resolved unexpectedly: {name} {resolved_commit}"
            )
        result = subprocess.run(
            ["git", "-C", str(source_root), "merge-base", "--is-ancestor", resolved_commit, base_commit],
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode == 1:
            raise ValueError(
                f"reference scheduler commit {name} {resolved_commit} is not an ancestor of {base_commit}"
            )
        if result.returncode != 0:
            raise ValueError(
                f"could not verify reference scheduler ancestry for {name}: {result.stderr.strip()}"
            )


def capture_reference_derivation_patches(source_root: Path, base_commit: str) -> dict:
    path = ROOT / REFERENCE_AGGREGATE_PATCH_RELATIVE
    patch = capture_reference_derivation_patch(path)
    validate_reference_scheduler_ancestry(source_root, base_commit)
    return {str(path): patch}


def capture_native_test_files(source_root: Path, source_manifest: dict, ids: list[str]) -> dict:
    tracked = {row["path"]: row["sha256"] for row in source_manifest["files"]}
    files = {}
    for test_id in ids:
        source_name = test_id.split("::", 1)[0]
        relative = PurePosixPath(source_name)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError(f"native test ID has a non-relative source path: {test_id}")
        relative_name = relative.as_posix()
        if relative_name not in tracked:
            raise ValueError(f"native test source is not tracked in the sealed tree: {relative_name}")
        path = source_root / relative_name
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"native test source is missing or not a regular file: {relative_name}")
        files[relative_name] = tracked[relative_name]
    return {
        "test_case_count": len(ids),
        "source_file_count": len(files),
        "files": dict(sorted(files.items())),
    }


def environment_for(pair: str) -> dict[str, str]:
    return {
        "LC_ALL": "C",
        "TZ": "UTC",
        "TMPDIR": "/tmp",
        "HOME": "/home/compat",
        "XSH_BIN": f"/opt/{pair}/xsh",
        "XSH_MODULE_PATH": "/work/core",
        "PATH": f"/opt/{pair}:/root/.cargo/bin:/usr/local/bin:/usr/local/sbin:/usr/bin:/usr/sbin:/bin:/sbin",
    }


def docker_command(
    *, source_root: Path, binary_dir: Path, output_dir: Path, image: str,
    mode: str, pair: str, label: str, container_name: str,
) -> list[str]:
    uid, gid = os.getuid(), os.getgid()
    command = [
        "docker", "run", "--rm", "--init", "--pull=never", "--platform", "linux/amd64",
        "--name", container_name,
        f"--cpuset-cpus={CPUSETS}", f"--cpus={CPU_QUOTA}",
        "--memory=40g", "--memory-swap=40g",
        "--user", f"{uid}:{gid}", "--tmpfs", "/tmp:rw,nodev,exec,mode=1777",
        "-v", f"{source_root}:/work:ro",
        "-v", f"{binary_dir}:/opt/{pair}:ro",
        "-v", f"{output_dir}:/results:rw",
        "-v", f"{CONTAINER_RUNNER}:/bench/container-run.sh:ro",
        "-w", "/work",
    ]
    for key, value in environment_for(pair).items():
        command.extend(["-e", f"{key}={value}"])
    command.extend([image, "/bin/sh", "/bench/container-run.sh", mode, pair, label])
    return command


def perf_gate_check(after: bool = False) -> dict:
    command = ["perf-gate-ready"]
    if after:
        command.append("-1")
    result = subprocess.run(command, text=True, capture_output=True, check=False)
    return {
        "command": command,
        "exit_code": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def run_stage(
    *, source_root: Path, binary_dir: Path, output_dir: Path, image: str,
    pair: str, mode: str, label: str, run_id: str, verify_after=None,
) -> dict:
    container_name = f"xshsch-{run_id}-{pair}-{mode}"
    command = docker_command(
        source_root=source_root,
        binary_dir=binary_dir,
        output_dir=output_dir,
        image=image,
        mode=mode,
        pair=pair,
        label=label,
        container_name=container_name,
    )
    docker_log = output_dir / f"{label}.docker.log"
    cidfile = output_dir / f"{label}.cid"
    cidfile.unlink(missing_ok=True)
    command = owned_docker_command(command, cidfile=cidfile, owner=run_id, stage=label)
    precheck = perf_gate_check()
    if precheck["exit_code"] != 0:
        report = {
            "mode": mode,
            "pair": pair,
            "label": label,
            "container_name": container_name,
            "command": command,
            "environment": environment_for(pair),
            "exit_code": None,
            "container_exit_code": None,
            "elapsed_seconds": None,
            "timing_accepted": False,
            "timing_claim_eligible": True,
            "timing_discarded": True,
            "performance_gate": {"before": precheck, "after": None},
            "interrupted": False,
            "launch_error": "performance readiness precheck failed; no timed stage was launched",
            "container_cleanup": {"resolved": True, "container_ids": [], "commands": []},
            "cidfile": None,
            "docker_log": str(docker_log),
            "docker_log_sha256": None,
            "result_log": str(output_dir / f"{label}.log"),
            "result_log_sha256": None,
            "artifact_hashes": {},
        }
        write_json(output_dir / f"{label}.stage.json", report)
        return report
    started_ns = time.monotonic_ns()
    interrupted = False
    signal_name = None
    cleanup = {"resolved": True, "container_ids": [], "commands": []}
    launch_error = None
    return_code = None
    process = None
    ended_ns = started_ns
    postcheck = None
    stage_validation = None
    validation_error = None
    cgroup_validation = None
    cgroup_validation_error = None
    try:
        with docker_log.open("wb") as output:
            process = subprocess.Popen(
                command,
                stdout=output,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            return_code, _, _ = wait_for_owned_process(
                process,
                lambda: cleanup_cidfile_container(cidfile, owner=run_id, stage=label),
            )
        ended_ns = time.monotonic_ns()
        postcheck = perf_gate_check(after=True)
        if verify_after is not None:
            try:
                stage_validation = verify_after()
            except Exception as exc:
                validation_error = str(exc)
    except InterruptedRun as error:
        interrupted = True
        signal_name = signal.Signals(error.signum).name
        cleanup = error.cleanup
        ended_ns = time.monotonic_ns()
    except SignalReceived as error:
        interrupted = True
        signal_name = signal.Signals(error.signum).name
        cleanup_fn = lambda: cleanup_cidfile_container(cidfile, owner=run_id, stage=label)
        if process is not None and process.poll() is None:
            cleanup = cleanup_container_process(process, cleanup_fn)
        else:
            with cleanup_window():
                container_cleanup = cleanup_fn()
            cleanup = {
                "resolved": container_cleanup["resolved"],
                "process": {"resolved": True, "commands": []},
                "containers": container_cleanup,
            }
        ended_ns = time.monotonic_ns()
    except OSError as exc:
        launch_error = str(exc)
        ended_ns = time.monotonic_ns()
    if not interrupted and postcheck is not None:
        with cleanup_window():
            cleanup = cleanup_cidfile_container(cidfile, owner=run_id, stage=label)
        try:
            before_path = output_dir / f"{label}.cgroup-before.txt"
            after_path = output_dir / f"{label}.cgroup-after.txt"
            cgroup_validation = validate_cgroup_snapshots(
                before_path.read_text(encoding="utf-8"),
                after_path.read_text(encoding="utf-8"),
            )
        except Exception as exc:
            cgroup_validation_error = str(exc)
        timing_accepted = (
            postcheck["exit_code"] == 0
            and cleanup["resolved"]
            and validation_error is None
            and cgroup_validation_error is None
        )
    else:
        timing_accepted = False
    if cleanup["resolved"]:
        cidfile.unlink(missing_ok=True)

    exit_code_path = output_dir / f"{label}.exit-code"
    container_exit_code = None
    if exit_code_path.exists():
        try:
            container_exit_code = int(exit_code_path.read_text(encoding="utf-8").strip())
        except ValueError:
            launch_error = f"invalid container exit-code file: {exit_code_path}"
    report = {
        "mode": mode,
        "pair": pair,
        "label": label,
        "container_name": container_name,
        "command": command,
        "environment": environment_for(pair),
        "exit_code": return_code,
        "container_exit_code": container_exit_code,
        "elapsed_seconds": round((ended_ns - started_ns) / 1_000_000_000, 3) if timing_accepted else None,
        "timing_accepted": timing_accepted,
        "timing_claim_eligible": True,
        "timing_discarded": not timing_accepted,
        "performance_gate": {"before": precheck, "after": postcheck},
        "host_monotonic_start_ns": started_ns,
        "host_monotonic_end_ns": ended_ns,
        "interrupted": interrupted,
        "signal": signal_name,
        "launch_error": launch_error,
        "post_stage_validation": stage_validation,
        "post_stage_validation_error": validation_error,
        "cgroup_validation": cgroup_validation,
        "cgroup_validation_error": cgroup_validation_error,
        "container_cleanup": cleanup,
        "cidfile": str(cidfile) if not cleanup["resolved"] else None,
        "docker_log": str(docker_log),
        "docker_log_sha256": sha256_file(docker_log) if docker_log.exists() else None,
        "result_log": str(output_dir / f"{label}.log"),
        "result_log_sha256": sha256_file(output_dir / f"{label}.log")
        if (output_dir / f"{label}.log").exists()
        else None,
        "cgroup_before": str(output_dir / f"{label}.cgroup-before.txt"),
        "cgroup_after": str(output_dir / f"{label}.cgroup-after.txt"),
        "environment_record": str(output_dir / f"{label}.environment.txt"),
        "artifact_hashes": {
            suffix: sha256_file(output_dir / f"{label}{suffix}")
            for suffix in (
                ".log",
                ".environment.txt",
                ".cgroup-before.txt",
                ".cgroup-after.txt",
                ".exit-code",
            )
            if (output_dir / f"{label}{suffix}").is_file()
        },
    }
    write_json(output_dir / f"{label}.stage.json", report)
    return report


def acquire_lock(lock_path: Path, *, create: bool = False, reject_unresolved: bool = False):
    if create:
        lock_path.parent.mkdir(parents=True, exist_ok=True)
    elif not lock_path.is_file():
        raise ValueError(f"shared native-suite lock file does not exist: {lock_path}")
    lock_file = lock_path.open("a+b")
    try:
        fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        lock_file.close()
        raise RuntimeError(f"native suite lock is busy: {lock_path}")
    if reject_unresolved:
        lock_file.seek(0)
        marker = lock_file.read().decode("utf-8", errors="replace")
        if marker.startswith("cleanup-unresolved:"):
            lock_file.close()
            raise RuntimeError(f"owned-container cleanup remains unresolved in {lock_path}: {marker.strip()}")
    return lock_file


def retain_unresolved_lock(lock_file, run_id: str, cleanup: dict) -> None:
    lock_file.seek(0)
    lock_file.truncate()
    lock_file.write(("cleanup-unresolved:" + json.dumps({"run_id": run_id, "cleanup": cleanup}, sort_keys=True) + "\n").encode())
    lock_file.flush()
    os.fsync(lock_file.fileno())


def terminal_receipt(
    receipt: dict, receipt_path: Path, ownership_lock, run_id: str,
    state: str, message: str, cleanup: dict,
) -> None:
    resolved = cleanup.get("resolved", True)
    receipt["state"] = state if resolved else "incomplete-cleanup"
    receipt["complete"] = receipt["state"] == "complete"
    receipt["error"] = message
    receipt["cleanup"] = cleanup
    receipt["finished_utc"] = datetime.now(timezone.utc).isoformat()
    if not resolved and ownership_lock is not None:
        retain_unresolved_lock(ownership_lock, run_id, cleanup)
    write_json(receipt_path, receipt)


def record_stage(stage: dict, receipt: dict, receipt_path: Path, ownership_lock) -> int | None:
    receipt["stages"].append(stage)
    write_json(receipt_path, receipt)
    cleanup = stage.get("container_cleanup", {"resolved": True})
    if stage.get("interrupted"):
        terminal_receipt(
            receipt, receipt_path, ownership_lock, receipt["container_run_id"],
            "interrupted", f"received {stage.get('signal')} during {stage['label']}", cleanup,
        )
        return 130
    if not cleanup.get("resolved", True):
        terminal_receipt(
            receipt, receipt_path, ownership_lock, receipt["container_run_id"],
            "incomplete-cleanup", f"owned container cleanup after {stage['label']} could not be verified", cleanup,
        )
        return 1
    if not stage.get("timing_accepted"):
        terminal_receipt(
            receipt, receipt_path, ownership_lock, receipt["container_run_id"],
            "failed", f"{stage['label']} timing was discarded because the performance readiness check failed", cleanup,
        )
        return 1
    return None


def _manifest_stage(
    *, pair: str, run_id: str, source_root: Path, binary_dirs: dict[str, Path],
    output_dir: Path, image: str, verify_after,
) -> dict:
    label = f"{pair}-list"
    return run_stage(
        source_root=source_root,
        binary_dir=binary_dirs[pair],
        output_dir=output_dir,
        image=image,
        mode="list",
        pair=pair,
        label=label,
        run_id=run_id,
        verify_after=verify_after,
    )


def _help_stage(
    *, pair: str, run_id: str, source_root: Path, binary_dirs: dict[str, Path],
    output_dir: Path, image: str, verify_after,
) -> dict:
    label = f"{pair}-help"
    return run_stage(
        source_root=source_root,
        binary_dir=binary_dirs[pair],
        output_dir=output_dir,
        image=image,
        mode="help",
        pair=pair,
        label=label,
        run_id=run_id,
        verify_after=verify_after,
    )


def _suite_stage(
    *, pair: str, run_id: str, source_root: Path, binary_dirs: dict[str, Path],
    output_dir: Path, image: str, round_index: int, verify_after,
) -> dict:
    label = f"round-{round_index + 1}-{pair}-suite"
    return run_stage(
        source_root=source_root,
        binary_dir=binary_dirs[pair],
        output_dir=output_dir,
        image=image,
        mode="suite",
        pair=pair,
        label=label,
        run_id=run_id,
        verify_after=verify_after,
    )


def run_comparison(args: argparse.Namespace) -> int:
    source_root = args.source_root.resolve(strict=True)
    reference_source_root = args.reference_source_root.resolve(strict=True)
    reference_dir = args.reference_bin_dir.resolve(strict=True)
    candidate_dir = args.candidate_bin_dir.resolve(strict=True)
    reference_build_receipt_path = args.reference_build_receipt.resolve(strict=True)
    candidate_build_receipt_path = args.candidate_build_receipt.resolve(strict=True)
    output_dir = args.output_dir.resolve(strict=False)
    manifest_path = args.manifest_path.resolve(strict=False)
    lock_path = args.lock_path.resolve(strict=True)
    expected_lock_path = SHARED_LOCK_PATH.resolve(strict=True)
    if lock_path != expected_lock_path:
        raise ValueError(f"must use the shared native-suite lock: {expected_lock_path}")
    if args.image != PINNED_IMAGE:
        raise ValueError(f"image must be the pinned test image: {PINNED_IMAGE}")
    if not LABEL_RE.fullmatch(args.run_id):
        raise ValueError("run ID must be a short lowercase label using letters, digits, or hyphens")
    if output_dir.exists():
        raise ValueError(f"output directory already exists; refusing to overwrite: {output_dir}")
    if manifest_path == output_dir or output_dir not in manifest_path.parents:
        raise ValueError("manifest path must be inside the new output directory")
    if manifest_path.exists():
        raise ValueError(f"manifest path already exists; refusing to overwrite: {manifest_path}")
    if not CONTAINER_RUNNER.is_file() or not VALIDATOR.is_file():
        raise ValueError("runner artifacts are incomplete")

    source = capture_source(source_root, args.source_commit)
    reference_derivation_patches = capture_reference_derivation_patches(
        source_root, args.source_commit
    )
    reference_source = capture_reference_source(reference_source_root, args.source_commit)
    reference_changed_paths = validate_reference_overlay(
        source, reference_source, reference_source_root
    )
    binary_dirs = {"reference": reference_dir, "candidate": candidate_dir}
    binaries = capture_binaries(reference_dir, candidate_dir)

    def verify_stage_inputs() -> dict:
        return verify_timed_stage_inputs(
            source_root=source_root,
            expected_source=source,
            reference_dir=reference_dir,
            candidate_dir=candidate_dir,
            expected_binaries=binaries,
        )

    reference_build_receipt = validate_build_receipt(
        reference_build_receipt_path,
        label="reference",
        source_root=reference_source_root,
        source_commit=args.source_commit,
        source_manifest=reference_source,
        binaries={"xsht": binaries["reference"]["xsht"]},
    )
    candidate_build_receipt = validate_build_receipt(
        candidate_build_receipt_path,
        label="candidate",
        source_root=source_root,
        source_commit=args.source_commit,
        source_manifest=source,
        binaries=binaries["candidate"],
    )
    image_snapshot = inspect_pinned_image(args.image)
    output_dir.mkdir(parents=True, exist_ok=False)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    source_manifest_path = output_dir / "source-manifest.json"
    reference_source_manifest_path = output_dir / "reference-source-manifest.json"
    binaries_path = output_dir / "binary-manifest.json"
    write_json(source_manifest_path, source)
    write_json(reference_source_manifest_path, reference_source)
    write_json(binaries_path, binaries)
    (output_dir / reference_build_receipt["input_copy_name"]).write_bytes(
        reference_build_receipt_path.read_bytes()
    )
    (output_dir / candidate_build_receipt["input_copy_name"]).write_bytes(
        candidate_build_receipt_path.read_bytes()
    )

    receipt = {
        "schema_version": 1,
        "classification": "three interleaved paired native scheduler correctness and wall-time signal",
        "created_utc": datetime.now(timezone.utc).isoformat(),
        "source_root": str(source_root),
        "source_commit_expected": args.source_commit,
        "source_manifest": str(source_manifest_path),
        "source_manifest_sha256": sha256_file(source_manifest_path),
        "reference_source_root": str(reference_source_root),
        "reference_source_manifest": str(reference_source_manifest_path),
        "reference_source_manifest_sha256": sha256_file(reference_source_manifest_path),
        "reference_source_base_head": reference_source["base_head"],
        "reference_source_changed_paths": reference_changed_paths,
        "reference_build_receipt": {
            key: value for key, value in reference_build_receipt.items() if key != "receipt"
        },
        "reference_build_receipt_copy": str(output_dir / reference_build_receipt["input_copy_name"]),
        "reference_build_receipt_content": reference_build_receipt["receipt"],
        "candidate_build_receipt": {
            key: value for key, value in candidate_build_receipt.items() if key != "receipt"
        },
        "candidate_build_receipt_copy": str(output_dir / candidate_build_receipt["input_copy_name"]),
        "candidate_build_receipt_content": candidate_build_receipt["receipt"],
        "binary_manifest": str(binaries_path),
        "binary_manifest_sha256": sha256_file(binaries_path),
        "reference_scheduler_derivation_patches": reference_derivation_patches,
        "image": args.image,
        "docker_image_inspect": image_snapshot,
        "target": TARGET,
        "binary_profile_expectation": "release binaries supplied by caller; this runner does not build",
        "cpuset": CPUSETS,
        "cpu_quota": CPU_QUOTA,
        "memory_limit_bytes": MEMORY_LIMIT,
        "memory_swap_limit_bytes": MEMORY_LIMIT,
        "timeout_per_case": "120s",
        "jobs_argument": "omitted on both arms; binary defaults are used",
        "expected_default_workers": WORKERS,
        "default_worker_formulas": {
            "reference": "(available_parallelism / 2).clamp(1, 8)",
            "candidate": "available_parallelism.clamp(1, 32)",
        },
        "suite_round_count": SUITE_ROUNDS,
        "suite_execution_order": [],
        "suite_time_samples_seconds": {"reference": [], "candidate": []},
        "shared_lock_path": str(lock_path),
        "shared_lock_mechanism": "fcntl.flock(LOCK_EX | LOCK_NB)",
        "shared_lock_across_lists_help_and_all_suites": True,
        "performance_ownership_lock_path": str(PERFORMANCE_LOCK_PATH),
        "performance_ownership_lock_mechanism": "fcntl.flock(LOCK_EX | LOCK_NB)",
        "manifest_path": str(manifest_path),
        "output_dir": str(output_dir),
        "runner_artifacts": {
            str(path): sha256_file(path)
            for path in (Path(__file__).resolve(), OUTCOMES_MODULE, CONTAINER_RUNNER, VALIDATOR)
        },
        "container_run_id": f"{args.run_id}-{secrets.token_hex(3)}",
        "stages": [],
        "binary_snapshots": [],
        "binary_integrity_errors": [],
        "complete": False,
        "state": "running",
    }
    receipt_path = output_dir / "receipt.json"
    write_json(receipt_path, receipt)

    performance_lock = None
    try:
        performance_lock = acquire_lock(PERFORMANCE_LOCK_PATH, create=True, reject_unresolved=True)
        lock_file = acquire_lock(lock_path)
    except Exception as exc:
        if performance_lock is not None:
            terminal_receipt(
                receipt, receipt_path, performance_lock, receipt["container_run_id"],
                "failed", str(exc), {"resolved": True, "container_ids": [], "commands": []},
            )
            performance_lock.close()
        else:
            terminal_receipt(
                receipt, receipt_path, None, receipt["container_run_id"],
                "failed", str(exc), {"resolved": True, "container_ids": [], "commands": []},
            )
        raise

    try:
        for pair in ("reference", "candidate"):
            stage = _manifest_stage(
                pair=pair,
                run_id=receipt["container_run_id"],
                source_root=source_root,
                binary_dirs=binary_dirs,
                output_dir=output_dir,
                image=args.image,
                verify_after=verify_stage_inputs,
            )
            stage_status = record_stage(stage, receipt, receipt_path, performance_lock)
            if stage_status is not None:
                return stage_status
            if not record_binary_checkpoint(
                receipt,
                receipt_path,
                checkpoint=f"after_{pair}_list",
                reference_dir=reference_dir,
                candidate_dir=candidate_dir,
                initial=binaries,
            ):
                receipt["error"] = f"binary files changed during or after {pair} test-list stage"
                write_json(receipt_path, receipt)
                return 1
            if stage["exit_code"] != 0 or stage["container_exit_code"] != 0:
                receipt["error"] = f"{pair} test-list command failed"
                write_json(receipt_path, receipt)
                return 1

        source_after_lists = capture_source(source_root, args.source_commit)
        if source_after_lists["tracked_tree_sha256"] != source["tracked_tree_sha256"]:
            receipt["error"] = "source tree changed while listing test IDs"
            receipt["source_after_lists"] = source_after_lists
            write_json(receipt_path, receipt)
            return 1
        receipt["source_after_lists"] = source_summary(source_after_lists)
        write_json(receipt_path, receipt)

        reference_list = output_dir / "reference-list.log"
        candidate_list = output_dir / "candidate-list.log"
        try:
            reference_ids = read_manifest(reference_list)
            candidate_ids = read_manifest(candidate_list)
        except ValueError as exc:
            receipt["error"] = f"invalid test-list output: {exc}"
            write_json(receipt_path, receipt)
            return 1
        if reference_ids != candidate_ids:
            receipt["error"] = "fresh reference and candidate ID lists differ"
            receipt["reference_manifest_count"] = len(reference_ids)
            receipt["candidate_manifest_count"] = len(candidate_ids)
            write_json(receipt_path, receipt)
            return 1

        try:
            corpus_files = capture_native_test_files(source_root, source, reference_ids)
        except ValueError as exc:
            receipt["error"] = f"native test corpus is not a subset of the sealed tracked tree: {exc}"
            write_json(receipt_path, receipt)
            return 1

        manifest_bytes = ("\n".join(reference_ids) + "\n").encode("utf-8")
        manifest_path.write_bytes(manifest_bytes)
        manifest_record = {
            "format": "newline-delimited raw xsht test IDs",
            "source_commit": source["commit"],
            "image": args.image,
            "count": len(reference_ids),
            "ids_sha256": manifest_sha256(reference_ids),
            "file_sha256": sha256_bytes(manifest_bytes),
            "reference_list_log_sha256": sha256_file(reference_list),
            "candidate_list_log_sha256": sha256_file(candidate_list),
            "native_test_case_count": corpus_files["test_case_count"],
            "native_test_source_file_count": corpus_files["source_file_count"],
            "native_test_source_files_sha256": sha256_bytes(
                json.dumps(corpus_files["files"], sort_keys=True, separators=(",", ":")).encode()
            ),
        }
        write_json(output_dir / "manifest.json", manifest_record)
        write_json(output_dir / "native-test-source-files.json", corpus_files)
        receipt["manifest"] = manifest_record
        write_json(receipt_path, receipt)

        for pair, expected_text in (
            ("reference", "default: half the CPUs"),
            ("candidate", "default: all CPUs capped at 32; max 8 per file"),
        ):
            stage = _help_stage(
                pair=pair,
                run_id=receipt["container_run_id"],
                source_root=source_root,
                binary_dirs=binary_dirs,
                output_dir=output_dir,
                image=args.image,
                verify_after=verify_stage_inputs,
            )
            stage_status = record_stage(stage, receipt, receipt_path, performance_lock)
            if stage_status is not None:
                return stage_status
            if not record_binary_checkpoint(
                receipt,
                receipt_path,
                checkpoint=f"after_{pair}_help",
                reference_dir=reference_dir,
                candidate_dir=candidate_dir,
                initial=binaries,
            ):
                receipt["error"] = f"binary files changed during or after {pair} help stage"
                write_json(receipt_path, receipt)
                return 1
            help_log = output_dir / f"{pair}-help.log"
            if stage["exit_code"] != 0 or stage["container_exit_code"] != 0:
                receipt["error"] = f"{pair} test help command failed"
                write_json(receipt_path, receipt)
                return 1
            if expected_text not in help_log.read_text(encoding="utf-8", errors="replace"):
                receipt["error"] = f"{pair} xsht binary does not report its expected default worker policy"
                write_json(receipt_path, receipt)
                return 1

        source_after_help = capture_source(source_root, args.source_commit)
        if source_after_help["tracked_tree_sha256"] != source["tracked_tree_sha256"]:
            receipt["error"] = "source tree changed while checking paired binary help"
            receipt["source_after_help"] = source_after_help
            write_json(receipt_path, receipt)
            return 1
        receipt["source_after_help"] = source_summary(source_after_help)
        receipt["default_policy_help"] = {
            "reference_expected_text": "default: half the CPUs",
            "candidate_expected_text": "default: all CPUs capped at 32; max 8 per file",
            "both_matched": True,
        }
        write_json(receipt_path, receipt)

        run_reports = {}
        for round_index in range(SUITE_ROUNDS):
            order = suite_execution_order(round_index)
            receipt["suite_execution_order"].append({"round": round_index + 1, "pairs": list(order)})
            write_json(receipt_path, receipt)
            for pair in order:
                stage = _suite_stage(
                    pair=pair,
                    run_id=receipt["container_run_id"],
                    source_root=source_root,
                    binary_dirs=binary_dirs,
                    output_dir=output_dir,
                    image=args.image,
                    round_index=round_index,
                    verify_after=verify_stage_inputs,
                )
                stage_status = record_stage(stage, receipt, receipt_path, performance_lock)
                if stage_status is not None:
                    return stage_status
                run_reports[(round_index, pair)] = stage
                receipt["suite_time_samples_seconds"][pair].append(stage["elapsed_seconds"])
                if not record_binary_checkpoint(
                    receipt,
                    receipt_path,
                    checkpoint=f"round_{round_index + 1}_after_{pair}_suite",
                    reference_dir=reference_dir,
                    candidate_dir=candidate_dir,
                    initial=binaries,
                ):
                    receipt["error"] = f"binary files changed during or after round {round_index + 1} {pair} native suite"
                    write_json(receipt_path, receipt)
                    return 1
                try:
                    source_at_stage_end = capture_source(source_root, args.source_commit)
                except (OSError, ValueError) as exc:
                    receipt["error"] = f"source state check failed after round {round_index + 1} {pair}: {exc}"
                    write_json(receipt_path, receipt)
                    return 1
                if source_at_stage_end["tracked_tree_sha256"] != source["tracked_tree_sha256"]:
                    receipt["error"] = f"source tree changed during round {round_index + 1} {pair} suite"
                    receipt[f"source_after_round_{round_index + 1}_{pair}"] = source_summary(source_at_stage_end)
                    write_json(receipt_path, receipt)
                    return 1
                receipt[f"source_after_round_{round_index + 1}_{pair}"] = source_summary(source_at_stage_end)
                write_json(receipt_path, receipt)

        round_validations = []
        for round_index in range(SUITE_ROUNDS):
            reference_stage = run_reports[(round_index, "reference")]
            candidate_stage = run_reports[(round_index, "candidate")]
            try:
                reference_result = validate_log(Path(reference_stage["result_log"]), reference_ids)
                candidate_result = validate_log(Path(candidate_stage["result_log"]), candidate_ids)
            except (OSError, ValueError) as exc:
                receipt["error"] = f"strict result validation failed in round {round_index + 1}: {exc}"
                write_json(receipt_path, receipt)
                return 1
            pair_report = compare_pair(
                reference_result,
                candidate_result,
                reference_stage["exit_code"],
                candidate_stage["exit_code"],
            )
            if reference_stage["container_exit_code"] != reference_stage["exit_code"]:
                pair_report["issues"].append("reference host/container exit codes differ")
                pair_report["accepted"] = False
            if candidate_stage["container_exit_code"] != candidate_stage["exit_code"]:
                pair_report["issues"].append("candidate host/container exit codes differ")
                pair_report["accepted"] = False
            pair_report["round"] = round_index + 1
            pair_report["execution_order"] = receipt["suite_execution_order"][round_index]["pairs"]
            write_json(output_dir / f"pair-validation-round-{round_index + 1}.json", pair_report)
            round_validations.append({
                "round": round_index + 1,
                "path": str(output_dir / f"pair-validation-round-{round_index + 1}.json"),
                "sha256": sha256_file(output_dir / f"pair-validation-round-{round_index + 1}.json"),
                "accepted": pair_report["accepted"],
                "issues": pair_report["issues"],
            })

        pair_report = {"accepted": all(item["accepted"] for item in round_validations), "rounds": round_validations}
        write_json(output_dir / "pair-validation.json", pair_report)
        receipt["pair_validation"] = {
            "path": str(output_dir / "pair-validation.json"),
            "sha256": sha256_file(output_dir / "pair-validation.json"),
            "accepted": pair_report["accepted"],
        }
        receipt["wall_time_min_spread_seconds"] = {
            pair: summarize_suite_samples(samples)
            for pair, samples in receipt["suite_time_samples_seconds"].items()
        }
        receipt["wall_time_note"] = "Three interleaved rounds report descriptive minimum and spread; they do not establish a speedup or 4x acceptance result."
        receipt["complete"] = True
        receipt["state"] = "complete"
        receipt["finished_utc"] = datetime.now(timezone.utc).isoformat()
        write_json(receipt_path, receipt)
        print(
            f"interleaved native suites: accepted={pair_report['accepted']}; rounds={SUITE_ROUNDS}; "
            f"reference={reference_result.counts}; candidate={candidate_result.counts}; "
            f"IDs={len(reference_ids)}"
        )
        print(f"receipt: {receipt_path}")
        return 0 if pair_report["accepted"] else 1
    except SignalReceived as error:
        terminal_receipt(
            receipt, receipt_path, performance_lock, receipt["container_run_id"],
            "interrupted", f"received {signal.Signals(error.signum).name} between owned stages",
            {"resolved": True, "container_ids": [], "commands": []},
        )
        return 130
    finally:
        if receipt.get("state") == "running":
            terminal_receipt(
                receipt, receipt_path, performance_lock, receipt["container_run_id"],
                "failed", receipt.get("error", "comparison stopped before all stages completed"),
                {"resolved": True, "container_ids": [], "commands": []},
            )
        lock_file.close()
        performance_lock.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--reference-source-root", type=Path, required=True)
    parser.add_argument("--reference-build-receipt", type=Path, required=True)
    parser.add_argument("--candidate-build-receipt", type=Path, required=True)
    parser.add_argument("--reference-bin-dir", type=Path, required=True)
    parser.add_argument("--candidate-bin-dir", type=Path, required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--manifest-path", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--lock-path", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    previous = install_signal_handlers()
    try:
        return run_comparison(args)
    except SignalReceived as error:
        print(f"interrupted by {signal.Signals(error.signum).name} before comparison receipt creation", file=sys.stderr)
        return 130
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"runner error: {exc}", file=sys.stderr)
        return 2
    finally:
        restore_signal_handlers(previous)


if __name__ == "__main__":
    raise SystemExit(main())
