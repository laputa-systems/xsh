#!/usr/bin/env python3
"""Qualify release opt3 artifacts or measure their build and native-suite cost."""

from contextlib import contextmanager, nullcontext
import argparse
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import signal
import stat
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

from owned_containers import (
    CleanupUnresolved,
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
INFRA = ROOT / ".work/consolidation/infra"
TARGET = "x86_64-unknown-linux-musl"
NATIVE_IMAGE_ID = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
CPU_LIMIT = 12
MEMORY_LIMIT = "24g"
MEMORY_LIMIT_BYTES = 24 * 1024**3
PROFILE_OPT_LEVEL = 3
PROFILE_LOCK = INFRA / "final-performance.lock"
KACHE = Path("/home/josh/.cargo/bin/kache")
MOLD = Path("/usr/local/bin/mold")
LINKER = INFRA / "mold-linker.sh"
KACHE_SHA256 = "b826e99983057974663d6863eafbee4e1b25ca9373071406b15dd02961b2c37d"
MOLD_SHA256 = "78e4b6aec99fa067b81326f4b540893b92ae78e5b4176ab32be0565dcbd049d3"
LINKER_SHA256 = "49f116f6bd364f682439e87200281a5acccf081d3b5841170a5a5bf34daf5271"


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def check_source(tree, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("source commit must be a full lowercase Git ID")
    head = subprocess.check_output(["git", "-C", str(tree), "rev-parse", "HEAD"], text=True).strip()
    if head != commit:
        raise ValueError(f"source tree HEAD {head} differs from requested commit {commit}")
    tree_id = subprocess.check_output(["git", "-C", str(tree), "rev-parse", "HEAD^{tree}"], text=True).strip()
    status = subprocess.check_output(["git", "-C", str(tree), "status", "--porcelain=v1", "--untracked-files=all"], text=True)
    if status:
        raise ValueError("source tree must be clean before profile-cost comparison")
    return {"commit": head, "git_tree": tree_id, "status": "clean"}


def source_tree_path(requested):
    root = ROOT.resolve(strict=True)
    tree = Path(os.path.abspath(requested))
    if tree != root and tree.parent != root / ".work/wt":
        raise ValueError(f"source tree must be the main checkout or an owned checkout below {root / '.work/wt'}")
    current = Path(tree.anchor)
    for component in tree.parts[1:]:
        current /= component
        try:
            if stat.S_ISLNK(current.lstat().st_mode):
                raise ValueError(f"source tree path cannot contain symlinks: {current}")
        except FileNotFoundError:
            raise ValueError(f"source tree path does not exist: {current}")
    if tree.resolve(strict=True) != tree:
        raise ValueError(f"source tree path cannot contain symlinks: {tree}")
    try:
        common_dir = Path(subprocess.check_output(
            ["git", "-C", str(tree), "rev-parse", "--path-format=absolute", "--git-common-dir"],
            text=True,
            timeout=30,
        ).strip()).resolve(strict=True)
        top_level = Path(subprocess.check_output(
            ["git", "-C", str(tree), "rev-parse", "--show-toplevel"],
            text=True,
            timeout=30,
        ).strip()).resolve(strict=True)
        expected_common_dir = (root / ".git").resolve(strict=True)
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        raise ValueError(f"cannot inspect source checkout identity: {error}") from error
    if common_dir != expected_common_dir or top_level != tree:
        raise ValueError("source tree must be a checkout of this repository")
    return tree


def absolute_without_symlinks(path, label):
    absolute = Path(os.path.abspath(path))
    cursor = Path(absolute.anchor)
    for component in absolute.parts[1:]:
        cursor /= component
        try:
            if stat.S_ISLNK(cursor.lstat().st_mode):
                raise ValueError(f"{label} path cannot contain symlinks: {cursor}")
        except FileNotFoundError:
            continue
    return absolute


def verify_profile_stage(tree, source_identity, binary_paths, expected_binaries=None):
    observed_source = check_source(tree, source_identity["commit"])
    if observed_source != source_identity:
        raise ValueError("source tree identity changed during the profile stage")
    binaries = {}
    for name, path in binary_paths.items():
        if not path.is_file() or not os.access(path, os.X_OK):
            raise ValueError(f"profile stage binary is missing or not executable: {path}")
        observed = {"path": str(path), "sha256": digest(path)}
        if expected_binaries is not None and expected_binaries.get(name) != observed:
            raise ValueError(f"profile stage binary changed during the stage: {name}")
        binaries[name] = observed
    if expected_binaries is not None and set(binaries) != set(expected_binaries):
        raise ValueError("profile stage binary set differs from its recorded build")
    return {"source": observed_source, "binary_hashes": binaries}


def check_close(path, commit):
    close = json.loads(path.read_text())
    if close.get("functional_complete") is not True or close.get("commit") != commit:
        raise ValueError("functional-close record must name the measured source commit")
    return close


def image_identity():
    image = json.loads(subprocess.check_output(["docker", "image", "inspect", "xsh-test:consolidation-native"]))[0]
    wrapper = INFRA / "run-native.sh"
    if image["Id"] != NATIVE_IMAGE_ID or image["Architecture"] != "amd64" or NATIVE_IMAGE_ID not in wrapper.read_text():
        raise ValueError("release profile comparison requires the pinned x86_64 native image and runner")
    env = set(image["Config"].get("Env", []))
    required_env = {
        "CC=/usr/bin/clang-23",
        "AR=/usr/local/bin/llvm-ar",
        "CFLAGS_x86_64_unknown_linux_musl=--target=x86_64-unknown-linux-musl -B/usr/lib/xsh-crt/x86_64-unknown-linux-musl -I/usr/include",
    }
    wrapper_text = wrapper.read_text()
    if not required_env <= env or "RUSTC_WRAPPER=/usr/local/bin/kache" not in wrapper_text or "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker" not in wrapper_text:
        raise ValueError("native build image or wrapper no longer selects the pinned Clang, kache and mold toolchain")
    tools = {"kache": KACHE, "mold": MOLD, "linker": LINKER}
    expected = {"kache": KACHE_SHA256, "mold": MOLD_SHA256, "linker": LINKER_SHA256}
    observed = {name: {"path": str(path), "sha256": digest(path)} for name, path in tools.items()}
    if any(observed[name]["sha256"] != expected[name] for name in tools):
        raise ValueError("host kache/mold linker inputs differ from the verified tool versions")
    kache_version = subprocess.check_output([str(KACHE), "--version"], text=True).strip()
    mold_version = subprocess.check_output([str(MOLD), "--version"], text=True).splitlines()[0]
    if kache_version != "kache 1.0.0" or not mold_version.startswith("mold 3.0.0 "):
        raise ValueError("host kache/mold version differs from the verified build tooling")
    return {"image_id": image["Id"], "architecture": image["Architecture"], "runner_sha256": digest(wrapper), "clang_driver": "/usr/bin/clang-23", "kache_version": kache_version, "mold_version": mold_version, "tool_hashes": observed}


def no_active_native_containers(image_id):
    active = subprocess.check_output(["docker", "ps", "--filter", "ancestor=" + image_id, "--format", "{{.ID}} {{.Names}}"], text=True).strip()
    if active:
        raise ValueError("native build or test containers are active; profile timing needs an isolated window")


def reject_unresolved_cleanup(path, stream):
    cleanup_marker = path.with_suffix(".cleanup-unresolved")
    if cleanup_marker.exists() or cleanup_marker.is_symlink():
        raise ValueError(f"owned-container cleanup marker exists: {cleanup_marker}")
    stream.seek(0)
    if stream.read(19) == "cleanup-unresolved:":
        raise ValueError(f"owned-container cleanup remains unresolved in {path}")


@contextmanager
def exclusive_lock(path, *, reject_unresolved=True):
    path.parent.mkdir(parents=True, exist_ok=True)
    stream = path.open("a+")
    try:
        fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        stream.close()
        raise ValueError(f"another build, suite, or final gate owns {path}") from error
    try:
        if reject_unresolved:
            reject_unresolved_cleanup(path, stream)
        yield stream
    finally:
        stream.close()


def retain_unresolved_lock(stream, run_id, cleanup):
    stream.seek(0)
    stream.truncate()
    stream.write("cleanup-unresolved:" + json.dumps({"run_id": run_id, "cleanup": cleanup}, sort_keys=True) + "\n")
    stream.flush()
    os.fsync(stream.fileno())


def terminal_receipt(receipt, receipt_path, ownership_stream, run_id, state, message, cleanup):
    receipt["state"] = state if cleanup["resolved"] else "incomplete-cleanup"
    receipt["error"] = message
    receipt["cleanup"] = cleanup
    receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
    if not cleanup["resolved"]:
        retain_unresolved_lock(ownership_stream, run_id, cleanup)
    write_json(receipt_path, receipt)


def cpu_list(cpu_ids):
    if len(cpu_ids) < CPU_LIMIT:
        raise ValueError(f"profile comparison requires at least {CPU_LIMIT} host-allowed CPUs")
    return ",".join(str(cpu) for cpu in cpu_ids[:CPU_LIMIT])


def expand_cpu_list(raw):
    result = set()
    for field in raw.split(","):
        bounds = field.split("-", 1)
        start = int(bounds[0])
        end = int(bounds[1]) if len(bounds) == 2 else start
        result.update(range(start, end + 1))
    return sorted(result)


def repository_tmp_parent(root=None):
    root = Path(root if root is not None else ROOT).resolve(strict=True)
    path = root / ".work/tmp"
    cursor = root
    for component in path.relative_to(root).parts:
        cursor /= component
        if cursor.is_symlink():
            raise ValueError(f"repository temporary path cannot contain symlinks: {cursor}")
    try:
        exclude_path = Path(subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "--path-format=absolute", "--git-path", "info/exclude"],
            text=True,
            timeout=30,
        ).strip())
        patterns = {
            line.strip()
            for line in exclude_path.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        }
    except OSError as error:
        raise ValueError(f"cannot read local Git excludes: {error}") from error
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        raise ValueError(f"cannot locate local Git excludes: {error}") from error
    if not patterns.intersection({".work", ".work/", "/.work", "/.work/"}):
        raise ValueError("repository .work directory must be excluded by .git/info/exclude")
    path.mkdir(parents=True, exist_ok=True)
    if path.is_symlink() or not path.is_dir():
        raise ValueError(f"repository temporary path is not a directory: {path}")
    try:
        ignored = subprocess.run(
            ["git", "-C", str(root), "check-ignore", "--quiet", "--no-index", ".work/tmp"],
            check=False,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ValueError(f"cannot verify repository temporary path is ignored: {error}") from error
    if ignored.returncode != 0:
        raise ValueError("repository temporary path is not ignored by Git")
    return path


@contextmanager
def repository_tmp_dir(run_id, root=None):
    parent = repository_tmp_parent(root)
    temporary = tempfile.TemporaryDirectory(prefix=f"profile-cost-{run_id}-", dir=parent)
    path = Path(temporary.name)
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            os.fchmod(descriptor, 0o700)
            mode = os.fstat(descriptor).st_mode
        finally:
            os.close(descriptor)
        if not stat.S_ISDIR(mode) or stat.S_IMODE(mode) != 0o700:
            raise ValueError(f"profile temporary directory is not private: {path}")
        yield path
    except BaseException as error:
        try:
            temporary.cleanup()
        except BaseException as cleanup_error:
            error.add_note(f"profile temporary cleanup also failed: {cleanup_error!r}")
        raise
    else:
        temporary.cleanup()


def docker_base(tree, target_dir, cpus, native_image, repository_tmp, user=False, candidate_xsh=None):
    git_common = Path(subprocess.check_output(["git", "-C", str(tree), "rev-parse", "--path-format=absolute", "--git-common-dir"], text=True).strip())
    if user:
        if candidate_xsh is None:
            raise ValueError("runtime profile container requires an explicit candidate XSH executable")
        candidate_xsh = Path(candidate_xsh)
        if not candidate_xsh.is_absolute():
            raise ValueError("candidate XSH executable path must be absolute")
        candidate_xsh_mode = candidate_xsh.lstat().st_mode
        if (
            not stat.S_ISREG(candidate_xsh_mode)
            or not (candidate_xsh_mode & 0o111)
            or not os.access(candidate_xsh, os.X_OK)
        ):
            raise ValueError(
                f"candidate XSH executable is not a regular executable file: {candidate_xsh}"
            )
    elif candidate_xsh is not None:
        raise ValueError("build profile container does not accept a candidate XSH executable")
    work = tree / ".work/consolidation"
    laputa = Path("/home/josh/d/laputa-systems/laputa")
    uid, gid = os.getuid(), os.getgid()
    path_prefix = f"{candidate_xsh.parent}:" if candidate_xsh is not None else ""
    command = [
        "docker", "run", "--rm", "--init", "--platform", "linux/amd64",
        "--cpuset-cpus", cpus, "--cpus", str(CPU_LIMIT), "--memory", MEMORY_LIMIT,
        "--memory-swap", MEMORY_LIMIT, "--tmpfs", "/tmp:rw,exec,nosuid,mode=1777",
        "-v", f"{tree}:{tree}", "-v", f"{work}:{work}", "-v", f"{git_common}:{git_common}:ro",
        "-v", f"{repository_tmp}:/t:rw",
        "-v", f"{laputa}:{laputa}:ro",
        "-v", f"{KACHE}:/usr/local/bin/kache:ro", "-v", f"{MOLD}:/usr/local/bin/mold:ro",
        "-v", f"{LINKER}:/usr/local/bin/xsh-mold-linker:ro",
        "-v", "xsh-consolidation-kache:/root/.cache/kache",
        "-w", str(tree), "-e", "LC_ALL=C", "-e", "TZ=UTC", "-e", "TMPDIR=/t",
        "-e", f"CARGO_TARGET_DIR={target_dir}", "-e", f"CARGO_BUILD_JOBS={CPU_LIMIT}",
        "-e", f"XSH_LAPUTA_CORPUS={laputa}",
        "-e", f"PATH={path_prefix}/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        "-e", "CC=/usr/bin/clang-23", "-e", "RUSTC_WRAPPER=/usr/local/bin/kache",
        "-e", "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker",
        "-e", "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=-Wl,--defsym=__isoc23_sscanf=sscanf -C link-arg=-Wl,--defsym=__isoc23_strtol=strtol",
    ]
    if user:
        command += [
            "--user", f"{uid}:{gid}", "-e", "HOME=/home/compat", "-e", "RUSTUP_HOME=/root/.rustup",
            "-e", f"XSH_BIN={candidate_xsh}",
        ]
    else:
        command += [
            "-v", "/usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro",
            "-v", "xsh-cargo-registry:/root/.cargo/registry",
            "-e", f"TASK_UID={uid}", "-e", f"TASK_GID={gid}",
            "-e", "CARGO_PROFILE_RELEASE_LTO=thin", "-e", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16",
        ]
    command.append(native_image)
    return command


def perf_gate_check(after=False):
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


def run_stage(command, log, *, run_id, stage, verify_after=None, measure_timing=True):
    cidfile = log.with_suffix(".cid")
    cidfile.unlink(missing_ok=True)
    owned_command = owned_docker_command(command, cidfile=cidfile, owner=run_id, stage=stage)
    precheck = perf_gate_check() if measure_timing else None
    if measure_timing and precheck["exit_code"] != 0:
        return {
            "command": owned_command,
            "status": None,
            "wall_ns": None,
            "log_sha256": None,
            "timing_accepted": False,
            "timing_discarded": True,
            "performance_gate": {"before": precheck, "after": None},
            "container_cleanup": {"resolved": True, "container_ids": [], "commands": []},
        }
    process = None
    stage_validation = None
    validation_error = None
    try:
        started = time.perf_counter_ns() if measure_timing else None
        with log.open("wb") as output:
            process = subprocess.Popen(
                owned_command,
                stdout=output,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            status, _, _ = wait_for_owned_process(
                process,
                lambda: cleanup_cidfile_container(cidfile, owner=run_id, stage=stage),
            )
        elapsed = time.perf_counter_ns() - started if started is not None else None
        postcheck = perf_gate_check(after=True) if started is not None else None
        if verify_after is not None:
            try:
                stage_validation = verify_after()
            except Exception as error:
                validation_error = str(error)
    except InterruptedRun as error:
        cleanup = error.cleanup
        if error.cleanup["resolved"]:
            cidfile.unlink(missing_ok=True)
        return {
            "command": owned_command,
            "status": None,
            "wall_ns": None,
            "log_sha256": digest(log) if log.exists() else None,
            "timing_accepted": False,
            "timing_discarded": True,
            "interrupted": True,
            "signal": signal.Signals(error.signum).name,
            "cleanup_resolved": error.cleanup["resolved"],
            "container_cleanup": cleanup,
            "performance_gate": {"before": precheck, "after": None},
            "cidfile": str(cidfile) if not error.cleanup["resolved"] else None,
        }
    except SignalReceived as error:
        cleanup_fn = lambda: cleanup_cidfile_container(cidfile, owner=run_id, stage=stage)
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
        if cleanup["resolved"]:
            cidfile.unlink(missing_ok=True)
        return {
            "command": owned_command,
            "status": None,
            "wall_ns": None,
            "log_sha256": digest(log) if log.exists() else None,
            "timing_accepted": False,
            "timing_discarded": True,
            "interrupted": True,
            "signal": signal.Signals(error.signum).name,
            "cleanup_resolved": cleanup["resolved"],
            "container_cleanup": cleanup,
            "performance_gate": {"before": precheck, "after": None},
            "cidfile": str(cidfile) if not cleanup["resolved"] else None,
            "stage_validation": stage_validation,
            "stage_validation_error": validation_error,
        }
    with cleanup_window():
        cleanup = cleanup_cidfile_container(cidfile, owner=run_id, stage=stage)
    if cleanup["resolved"]:
        cidfile.unlink(missing_ok=True)
    accepted = postcheck is not None and postcheck["exit_code"] == 0 and cleanup["resolved"] and validation_error is None
    result = {
        "command": owned_command,
        "status": status,
        "wall_ns": elapsed if accepted else None,
        "timing_claim_eligible": False,
        "log_sha256": digest(log),
        "timing_accepted": accepted,
        "timing_discarded": not accepted,
        "performance_gate": {"before": precheck, "after": postcheck},
        "container_cleanup": cleanup,
        "cidfile": str(cidfile) if not cleanup["resolved"] else None,
        "stage_validation": stage_validation,
        "stage_validation_error": validation_error,
    }
    if not measure_timing:
        result["timing_discard_reason"] = "artifacts-only mode"
    return result


def profile_build(tree, target_dir, level, cpus, image, repository_tmp):
    command = docker_base(tree, target_dir, cpus, image, repository_tmp)
    profile_env = ["-e", f"CARGO_PROFILE_RELEASE_OPT_LEVEL={level}"]
    image_index = command.index(image)
    command[image_index:image_index] = profile_env
    wrapped = ["/bin/bash", "-c", 'LD_PRELOAD=/usr/lib/libjemalloc.so.2 "$@"', "bash", "cargo", "build", "--locked", "--release", "--target", TARGET, "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht", "-p", "xsh", "--bin", "xsh-test-helper"]
    return command + wrapped


def profile_test(tree, target_dir, cpus, image, repository_tmp):
    binary_dir = target_dir / TARGET / "release"
    candidate_xsh = binary_dir / "xsh"
    command = docker_base(tree, target_dir, cpus, image, repository_tmp, user=True, candidate_xsh=candidate_xsh)
    command += [str(binary_dir / "xsht"), "test"]
    return command


def probe_profile_resources(tree, target_dir, cpus, image, candidate_xsh, *, run_id, output_dir, repository_tmp):
    command = docker_base(tree, target_dir, cpus, image, repository_tmp, user=True, candidate_xsh=candidate_xsh)
    command += ["/bin/sh", "-c", "grep '^Cpus_allowed_list:' /proc/self/status; cat /sys/fs/cgroup/memory.max; cat /sys/fs/cgroup/memory.swap.max"]
    stage = "resource-probe"
    cidfile = output_dir / "resource-probe.cid"
    cidfile.unlink(missing_ok=True)
    command = owned_docker_command(command, cidfile=cidfile, owner=run_id, stage=stage)
    process = None
    try:
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
        status, stdout, stderr = wait_for_owned_process(
            process,
            lambda: cleanup_cidfile_container(cidfile, owner=run_id, stage=stage),
        )
    except InterruptedRun as error:
        if error.cleanup["resolved"]:
            cidfile.unlink(missing_ok=True)
        raise
    except SignalReceived as error:
        cleanup_fn = lambda: cleanup_cidfile_container(cidfile, owner=run_id, stage=stage)
        if process is not None and process.poll() is None:
            cleanup = cleanup_container_process(process, cleanup_fn)
        else:
            with cleanup_window():
                cleanup = cleanup_fn()
        if cleanup["resolved"]:
            cidfile.unlink(missing_ok=True)
        raise InterruptedRun(error.signum, cleanup) from error
    with cleanup_window():
        cleanup = cleanup_cidfile_container(cidfile, owner=run_id, stage=stage)
    if cleanup["resolved"]:
        cidfile.unlink(missing_ok=True)
    if not cleanup["resolved"]:
        raise CleanupUnresolved(cleanup)
    if status != 0:
        raise ValueError(f"native profile resource probe failed: {stderr}")
    output = (stdout or "").splitlines()
    if len(output) != 3 or not output[0].startswith("Cpus_allowed_list:"):
        raise ValueError("native profile container did not report its CPU and cgroup limits")
    actual_cpus = expand_cpu_list(output[0].split(":", 1)[1].strip())
    actual_memory = int(output[1])
    actual_swap = int(output[2])
    expected_cpus = [int(cpu) for cpu in cpus.split(",")]
    if actual_cpus != expected_cpus or actual_memory != MEMORY_LIMIT_BYTES or actual_swap != 0:
        raise ValueError("native profile container affinity or memory/swap cgroup differs from its requested limits")
    return {"affinity_cpus": actual_cpus, "memory_limit_bytes": actual_memory, "memory_swap_max_bytes": actual_swap}


def run(args):
    tree = source_tree_path(args.tree)
    output = absolute_without_symlinks(args.output, "profile output")
    source_identity = check_source(tree, args.commit)
    close = check_close(args.functional_close, args.commit)
    image = image_identity()
    allowed_cpus = sorted(os.sched_getaffinity(0))
    cpus = cpu_list(allowed_cpus)
    work_root = (tree / ".work/consolidation").resolve()
    if not output.is_relative_to(work_root) or output.exists() or Path("/tmp") in output.parents:
        raise ValueError("choose a new output path under the source tree's .work/consolidation")
    run_id = secrets.token_hex(16)
    ownership = exclusive_lock(PROFILE_LOCK, reject_unresolved=True)
    native_suite_lock = nullcontext() if args.artifacts_only else exclusive_lock(INFRA / "native-suite.lock")
    with ownership as ownership_stream, native_suite_lock, repository_tmp_dir(run_id, tree) as repository_tmp:
        if args.artifacts_only:
            with (INFRA / "native-suite.lock").open() as native_lock:
                reject_unresolved_cleanup(INFRA / "native-suite.lock", native_lock)
        no_active_native_containers(image["image_id"])
        output.mkdir(parents=True)
        receipt_path = output / "receipt.json"
        receipt = {
            "schema": 1,
            "state": "running",
            "run_id": run_id,
            "ownership_lock": str(PROFILE_LOCK),
            "native_suite_lock": str(INFRA / "native-suite.lock"),
            "native_suite_lock_held_across_gate": not args.artifacts_only,
            "source_commit": args.commit,
            "source_tree": str(tree),
            "source_status": "clean",
            "source_identity": source_identity,
            "functional_close": {"path": str(args.functional_close.resolve()), "sha256": digest(args.functional_close.resolve()), "record": close},
            "profile": {"name": "release", "lto": "thin", "codegen_units": 16, "opt_levels": [PROFILE_OPT_LEVEL]},
            "target": TARGET,
            "native_image": image,
            "kache_cache_volume": "xsh-consolidation-kache",
            "toolchain": {"clang_driver": image["clang_driver"], **image["tool_hashes"]},
            "resources": {"host_allowed_cpus": allowed_cpus, "cpuset_cpus": cpus, "cpu_limit": CPU_LIMIT, "memory_limit": MEMORY_LIMIT, "memory_swap_limit": MEMORY_LIMIT, "cargo_build_jobs": CPU_LIMIT},
            "timing_claim_policy": "artifacts-only mode does not measure timing" if args.artifacts_only else "one-shot profile timings are resource and correctness diagnostics only; they do not support speed comparisons or profile selection",
            "cargo_lock_sha256": digest(tree / "Cargo.lock"),
            "cargo_toml_sha256": digest(tree / "Cargo.toml"),
            "rust_toolchain_sha256": digest(tree / "rust-toolchain.toml"),
            "results": [],
        }
        if args.artifacts_only:
            receipt["qualification_mode"] = "artifacts-only"
            tool_path = Path(__file__).resolve()
            receipt["qualification_tool"] = {"path": str(tool_path), "sha256": digest(tool_path)}
        write_json(receipt_path, receipt)
        try:
            for level in (PROFILE_OPT_LEVEL,):
                profile_dir = output / f"opt{level}"
                target_dir = profile_dir / "target"
                profile_dir.mkdir()
                binary_dir = target_dir / TARGET / "release"
                binaries = {name: binary_dir / name for name in ("xsh", "xsht", "xsh-test-helper")}
                build_command = profile_build(tree, target_dir, level, cpus, image["image_id"], repository_tmp)
                with exclusive_lock(INFRA / "cargo.lock"):
                    build = run_stage(
                        build_command,
                        profile_dir / "build.log",
                        run_id=run_id,
                        stage=f"release-opt{level}-build",
                        verify_after=lambda: verify_profile_stage(tree, source_identity, binaries),
                        measure_timing=not args.artifacts_only,
                    )
                result = {"name": f"release-opt{level}", "profile": "release", "opt_level": level, "lto": "thin", "codegen_units": 16, "build_command": build["command"], "build": build}
                receipt["results"].append(result)
                write_json(receipt_path, receipt)
                if build.get("interrupted"):
                    terminal_receipt(receipt, receipt_path, ownership_stream, run_id, "interrupted", f"received {build['signal']} during release opt{level} build", build["container_cleanup"])
                    return 130
                if not build["container_cleanup"]["resolved"]:
                    terminal_receipt(receipt, receipt_path, ownership_stream, run_id, "incomplete-cleanup", f"container cleanup after release opt{level} build could not be verified", build["container_cleanup"])
                    return 1
                if not args.artifacts_only and not build["timing_accepted"]:
                    reason = build.get("stage_validation_error") or "performance readiness or cleanup validation failed"
                    raise ValueError(f"release opt{level} build timing was discarded: {reason}")
                if args.artifacts_only and build.get("stage_validation_error"):
                    raise ValueError(f"release opt{level} build validation failed: {build['stage_validation_error']}")
                if build["status"] != 0:
                    raise ValueError(f"release opt{level} build failed; inspect {profile_dir / 'build.log'}")
                result["binaries"] = build["stage_validation"]["binary_hashes"]
                result["container_resources"] = probe_profile_resources(tree, target_dir, cpus, image["image_id"], binaries["xsh"], run_id=run_id, output_dir=profile_dir, repository_tmp=repository_tmp)
                verify_profile_stage(tree, source_identity, binaries, result["binaries"])
                if args.artifacts_only:
                    tool = receipt["qualification_tool"]
                    if digest(Path(tool["path"])) != tool["sha256"]:
                        raise ValueError("artifacts-only profile tool changed during qualification")
                    receipt["state"] = "artifact-qualified"
                    receipt["finished_utc"] = datetime.now(timezone.utc).isoformat()
                    write_json(receipt_path, receipt)
                    print(json.dumps(receipt, sort_keys=True))
                    return 0
                test_command = profile_test(tree, target_dir, cpus, image["image_id"], repository_tmp)
                no_active_native_containers(image["image_id"])
                test = run_stage(
                    test_command,
                    profile_dir / "native-suite.log",
                    run_id=run_id,
                    stage=f"release-opt{level}-native-suite",
                    verify_after=lambda: verify_profile_stage(tree, source_identity, binaries, result["binaries"]),
                )
                result["native_suite"] = test
                if test.get("interrupted"):
                    terminal_receipt(receipt, receipt_path, ownership_stream, run_id, "interrupted", f"received {test['signal']} during release opt{level} native suite", test["container_cleanup"])
                    return 130
                if not test["container_cleanup"]["resolved"]:
                    terminal_receipt(receipt, receipt_path, ownership_stream, run_id, "incomplete-cleanup", f"container cleanup after release opt{level} native suite could not be verified", test["container_cleanup"])
                    return 1
                if not test["timing_accepted"]:
                    write_json(receipt_path, receipt)
                    reason = test.get("stage_validation_error") or "performance readiness or cleanup validation failed"
                    raise ValueError(f"release opt{level} native-suite timing was discarded: {reason}")
                result["native_suite_command"] = test_command
                write_json(receipt_path, receipt)
                if test["status"] != 0:
                    raise ValueError(f"release opt{level} native suite failed; inspect {profile_dir / 'native-suite.log'}")
            receipt["state"] = "complete"
            receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
            write_json(receipt_path, receipt)
            print(json.dumps(receipt, sort_keys=True))
        except InterruptedRun as error:
            terminal_receipt(
                receipt, receipt_path, ownership_stream, run_id, "interrupted",
                f"received {signal.Signals(error.signum).name} during native profile resource probe",
                error.cleanup,
            )
            return 130
        except CleanupUnresolved as error:
            terminal_receipt(
                receipt, receipt_path, ownership_stream, run_id, "incomplete-cleanup",
                str(error), error.cleanup,
            )
            return 1
        except SignalReceived as error:
            terminal_receipt(
                receipt, receipt_path, ownership_stream, run_id, "interrupted",
                f"received {signal.Signals(error.signum).name} with no active owned container",
                {"resolved": True, "commands": []},
            )
            return 130
        except Exception as error:
            receipt["state"] = "failed"
            receipt["error"] = str(error)
            receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
            write_json(receipt_path, receipt)
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tree", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--functional-close", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--artifacts-only", action="store_true", help="write a separate artifact-qualified opt3 receipt without timing or running the native suite")
    args = parser.parse_args()
    previous = install_signal_handlers()
    try:
        return run(args)
    except SignalReceived as error:
        print(f"interrupted by {signal.Signals(error.signum).name} before an owned container started", file=sys.stderr)
        return 130
    finally:
        restore_signal_handlers(previous)


if __name__ == "__main__":
    raise SystemExit(main())
