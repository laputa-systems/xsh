#!/usr/bin/env python3

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
import subprocess
import sys
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / ".work"
SCRATCH = WORK / "tmp"
IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
TARGET = "x86_64-unknown-linux-musl"
PROFILE = "verification"
NATIVE_LOCK = WORK / "consolidation/infra/native-suite.lock"
CLEANUP_MARKER = WORK / "consolidation/infra/native-suite.cleanup-unresolved"
LAPUTA = ROOT.parent / "laputa"
TRACEROUTE_CASES = 20
BLOCK_DEVICE_CASES = 2
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
PRIVILEGED_HARNESS_NAME = re.compile(r"^core_compat_extra_privileged-[0-9a-f]+$")
XSHT_SUMMARY = re.compile(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) skipped")
RUST_TEST_SUMMARY = re.compile(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out")


class RunnerFailure(RuntimeError):
    pass


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def relative(path: Path, root: Path) -> str:
    return path.resolve(strict=True).relative_to(root.resolve(strict=True)).as_posix()


def require_work_path(path: Path, label: str, must_exist: bool) -> Path:
    candidate = path.expanduser()
    if not candidate.is_absolute():
        candidate = ROOT / candidate
    resolved = candidate.resolve(strict=must_exist)
    try:
        resolved.relative_to(WORK.resolve(strict=True))
    except ValueError as error:
        raise RunnerFailure(f"{label} must stay under {WORK}") from error
    return resolved


def git_output(*args: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(ROOT), *args],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if completed.returncode != 0:
        raise RunnerFailure(f"git {' '.join(args)} failed: {completed.stderr.strip()}")
    return completed.stdout.strip()


def source_snapshot(expected_head: str, expected_tree: str) -> dict[str, str]:
    head = git_output("rev-parse", "HEAD")
    tree = git_output("rev-parse", "HEAD^{tree}")
    status = git_output("status", "--porcelain=v1", "--untracked-files=all")
    if head != expected_head:
        raise RunnerFailure(f"source HEAD mismatch: expected {expected_head}, got {head}")
    if tree != expected_tree:
        raise RunnerFailure(f"source tree mismatch: expected {expected_tree}, got {tree}")
    if status:
        raise RunnerFailure(f"source tree is dirty: {status}")
    return {"head": head, "tree": tree, "status": status}


def products(args: argparse.Namespace) -> dict[str, dict[str, str]]:
    target_dir = require_work_path(Path(args.target_dir), "Cargo target directory", True)
    expected = {
        "xsh": args.xsh_sha256,
        "xsht": args.xsht_sha256,
        "xshi": args.xshi_sha256,
    }
    actual: dict[str, dict[str, str]] = {}
    for name, expected_hash in expected.items():
        if not HEX64.fullmatch(expected_hash):
            raise RunnerFailure(f"{name} SHA-256 must be 64 lowercase hexadecimal characters")
        path = target_dir / TARGET / PROFILE / name
        if not path.is_file() or not os.access(path, os.X_OK):
            raise RunnerFailure(f"required verification product is not executable: {path}")
        path = path.resolve(strict=True)
        try:
            relative(path, WORK)
        except ValueError as error:
            raise RunnerFailure(f"product escaped {WORK}: {path}") from error
        observed_hash = digest(path)
        if observed_hash != expected_hash:
            raise RunnerFailure(f"{name} product hash mismatch: expected {expected_hash}, got {observed_hash}")
        actual[name] = {"path": path.relative_to(ROOT).as_posix(), "sha256": observed_hash}
    if args.mode == "root-block-device":
        harness = privileged_harness_path(args, target_dir)
        expected_hash = args.privileged_harness_sha256
        if not HEX64.fullmatch(expected_hash):
            raise RunnerFailure("privileged harness SHA-256 must be 64 lowercase hexadecimal characters")
        observed_hash = digest(harness)
        if observed_hash != expected_hash:
            raise RunnerFailure(f"privileged harness SHA-256 mismatch: expected {expected_hash}, got {observed_hash}")
        actual["privileged_harness"] = {"path": harness.relative_to(ROOT).as_posix(), "sha256": observed_hash}
    return actual


def privileged_harness_path(args: argparse.Namespace, target_dir: Path) -> Path:
    path = require_work_path(Path(args.privileged_harness), "privileged harness", True)
    profile_root = (target_dir / TARGET / PROFILE).resolve(strict=True)
    try:
        profile_root.relative_to(target_dir)
        profile_root.relative_to(WORK.resolve(strict=True))
    except ValueError as error:
        raise RunnerFailure(f"verification profile directory escaped selected target root: {profile_root}") from error
    resolved = path.resolve(strict=True)
    try:
        resolved.relative_to(profile_root)
    except ValueError as error:
        raise RunnerFailure(f"privileged harness escaped selected verification profile {profile_root}") from error
    if not PRIVILEGED_HARNESS_NAME.fullmatch(resolved.name):
        raise RunnerFailure("privileged harness basename must match core_compat_extra_privileged-<hex>")
    if not resolved.is_file() or not os.access(resolved, os.X_OK):
        raise RunnerFailure(f"privileged harness is not executable: {resolved}")
    return resolved


def observe_source() -> dict[str, str]:
    return {
        "head": git_output("rev-parse", "HEAD"),
        "tree": git_output("rev-parse", "HEAD^{tree}"),
        "status": git_output("status", "--porcelain=v1", "--untracked-files=all"),
    }


def observed_products(args: argparse.Namespace) -> dict[str, dict[str, str]]:
    target_dir = Path(args.target_dir).expanduser()
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    target_dir = target_dir.resolve(strict=True)
    actual: dict[str, dict[str, str]] = {}
    for name in ("xsh", "xsht", "xshi"):
        path = target_dir / TARGET / PROFILE / name
        if path.is_file():
            actual[name] = {"path": path.resolve(strict=True).relative_to(ROOT).as_posix(), "sha256": digest(path)}
        else:
            actual[name] = {"path": path.relative_to(ROOT).as_posix(), "missing": "true"}
    if args.mode == "root-block-device":
        supplied_path = Path(args.privileged_harness).expanduser()
        if not supplied_path.is_absolute():
            supplied_path = ROOT / supplied_path
        if supplied_path.is_file():
            resolved = supplied_path.resolve(strict=True)
            try:
                resolved.relative_to((target_dir / TARGET / PROFILE).resolve(strict=True))
                resolved.relative_to(WORK)
                if not PRIVILEGED_HARNESS_NAME.fullmatch(resolved.name):
                    raise ValueError("unexpected harness basename")
                actual["privileged_harness"] = {
                    "path": resolved.relative_to(ROOT).as_posix(),
                    "sha256": digest(resolved),
                }
            except ValueError:
                actual["privileged_harness"] = {"path": str(supplied_path), "invalid": "escaped selected target dependencies"}
        else:
            actual["privileged_harness"] = {"path": str(supplied_path), "missing": "true"}
    return actual


def save_receipt(path: Path, receipt: dict[str, Any]) -> None:
    temporary = path.with_suffix(".json.tmp")
    try:
        with temporary.open("w", encoding="utf-8") as output:
            json.dump(receipt, output, indent=2, sort_keys=True)
            output.write("\n")
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


def write_step_output(directory: Path, step_id: str, completed: subprocess.CompletedProcess[bytes]) -> dict[str, str | int]:
    stdout_path = directory / f"{step_id}.stdout"
    stderr_path = directory / f"{step_id}.stderr"
    status_path = directory / f"{step_id}.status"
    stdout_path.write_bytes(completed.stdout)
    stderr_path.write_bytes(completed.stderr)
    status_path.write_text(f"{completed.returncode}\n", encoding="ascii")
    return {
        "stdout": stdout_path.name,
        "stderr": stderr_path.name,
        "status_file": status_path.name,
        "status": completed.returncode,
    }


@contextmanager
def native_suite_lock() -> Any:
    if CLEANUP_MARKER.exists():
        raise RunnerFailure(f"unresolved native-suite cleanup marker exists: {CLEANUP_MARKER}")
    NATIVE_LOCK.parent.mkdir(parents=True, exist_ok=True)
    descriptor = NATIVE_LOCK.open("a+b")
    locked = False
    try:
        fcntl.flock(descriptor.fileno(), fcntl.LOCK_EX)
        locked = True
        if CLEANUP_MARKER.exists():
            raise RunnerFailure(f"unresolved native-suite cleanup marker exists: {CLEANUP_MARKER}")
        yield
    finally:
        if locked:
            fcntl.flock(descriptor.fileno(), fcntl.LOCK_UN)
        descriptor.close()


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(
        description="Run one source-pinned final correctness residual in the approved Linux test image."
    )
    result.add_argument("--mode", required=True, choices=("forced-native", "traceroute-nonroot", "root-block-device"))
    result.add_argument("--head", required=True, help="full expected Git commit ID")
    result.add_argument("--tree", required=True, help="full expected Git tree ID")
    result.add_argument("--target-dir", required=True, help="target root under .work containing the frozen verification products")
    result.add_argument("--xsh-sha256", required=True)
    result.add_argument("--xsht-sha256", required=True)
    result.add_argument("--xshi-sha256", required=True)
    result.add_argument("--canonical-native-stdout", help="forced-native mode: captured normal canonical xsht test stdout")
    result.add_argument("--privileged-harness", help="root-block-device mode: explicit Cargo-reported executable under the selected verification profile")
    result.add_argument("--privileged-harness-sha256", help="root-block-device mode: qualified test executable SHA-256")
    result.add_argument("--scratch-dir", default=str(SCRATCH), help="disk-backed scratch directory under .work")
    result.add_argument("--timeout-seconds", type=int, default=0, help="optional whole-container timeout; zero has no timeout")
    return result


def check_arguments(args: argparse.Namespace) -> tuple[Path, Path, dict[str, dict[str, str]], int | None]:
    if not HEX40.fullmatch(args.head) or not HEX40.fullmatch(args.tree):
        raise RunnerFailure("--head and --tree must be full lowercase 40-character Git IDs")
    if args.timeout_seconds < 0:
        raise RunnerFailure("--timeout-seconds must be zero or positive")
    if bool(args.canonical_native_stdout) != (args.mode == "forced-native"):
        raise RunnerFailure("--canonical-native-stdout is required only for forced-native")
    if bool(args.privileged_harness) != (args.mode == "root-block-device"):
        raise RunnerFailure("--privileged-harness is required only for root-block-device")
    if bool(args.privileged_harness_sha256) != (args.mode == "root-block-device"):
        raise RunnerFailure("--privileged-harness-sha256 is required only for root-block-device")
    if os.environ.get("LD_PRELOAD") or os.environ.get("XSH_DEV_BUILD_PRELOAD"):
        raise RunnerFailure("clear LD_PRELOAD and XSH_DEV_BUILD_PRELOAD before invoking this runner")
    ignored = subprocess.run(
        ["git", "-C", str(ROOT), "check-ignore", "--quiet", "--no-index", ".work/tmp"],
        check=False,
    )
    if ignored.returncode != 0:
        raise RunnerFailure(".work/tmp is not ignored by Git")
    target_dir = require_work_path(Path(args.target_dir), "Cargo target directory", True)
    scratch = require_work_path(Path(args.scratch_dir), "scratch directory", True)
    if not scratch.is_dir() or not os.access(scratch, os.W_OK | os.X_OK):
        raise RunnerFailure(f"scratch directory is not writable: {scratch}")
    products(args)
    expected_total: int | None = None
    if args.mode == "forced-native":
        summary_path = Path(args.canonical_native_stdout).expanduser()
        if not summary_path.is_absolute():
            summary_path = ROOT / summary_path
        summary_path = summary_path.resolve(strict=True)
        try:
            relative(summary_path, WORK)
        except ValueError as error:
            raise RunnerFailure("canonical native stdout must be under .work") from error
        canonical_summary = parse_xsht_summary(summary_path.read_bytes())
        if canonical_summary["state"] != "ok" or canonical_summary["passed"] == 0 or canonical_summary["failed"] != 0:
            raise RunnerFailure("canonical native stdout lacks a successful full-suite XSHT summary")
        expected_total = int(canonical_summary["selected"])
        if expected_total <= 0:
            raise RunnerFailure("canonical native summary selects zero cases")
        args.canonical_stdout_path = summary_path
        args.canonical_stdout_sha256 = digest(summary_path)
    else:
        args.canonical_stdout_path = None
        args.canonical_stdout_sha256 = None
    return target_dir, scratch, products(args), expected_total


def docker_base(
    args: argparse.Namespace,
    run_dir: Path,
    target_dir: Path,
    scratch: Path,
    step_id: str,
) -> tuple[list[str], dict[str, str]]:
    block_run = args.mode == "root-block-device" and step_id == "root-block-device-run"
    memory = "1g" if block_run else "16g"
    tmpdir = "/scratch"
    argv = [
        "docker", "run", "--rm", "--init", "--platform", "linux/amd64",
        "--cpus=8", f"--memory={memory}", f"--memory-swap={memory}",
        "--tmpfs", "/tmp:rw,nodev,exec,mode=1777",
        "--volume", f"{ROOT}:/work:ro",
        "--volume", f"{target_dir}:/work/target:ro",
        "--volume", f"{scratch}:/scratch:rw",
        "--workdir", "/work",
    ]
    network = "none"
    argv += ["--network", network]
    if args.mode == "traceroute-nonroot":
        argv += ["--user", "1000:1000"]
    if block_run:
        argv += [
            "--cap-add=SYS_ADMIN",
            "--cap-add=MKNOD",
            "--device-cgroup-rule",
            "b *:* m",
        ]
    cidfile = run_dir / f"{step_id}.cid"
    argv += ["--cidfile", str(cidfile)]
    product_root = f"/work/target/{TARGET}/{PROFILE}"
    env = {
        "TMPDIR": tmpdir,
        "LD_PRELOAD": "",
        "XSH_DEV_BUILD_PRELOAD": "",
        "XSH_BIN": f"{product_root}/xsh",
        "PATH": f"{product_root}:/usr/local/bin:/usr/bin:/bin",
    }
    argv += ["--env", f"TMPDIR={tmpdir}"]
    argv += ["--env", "LD_PRELOAD=", "--env", "XSH_DEV_BUILD_PRELOAD="]
    argv += ["--env", f"XSH_BIN={product_root}/xsh", "--env", f"PATH={product_root}:/usr/local/bin:/usr/bin:/bin"]
    if args.mode == "forced-native":
        if not LAPUTA.is_dir():
            raise RunnerFailure(f"Laputa corpus directory is unavailable: {LAPUTA}")
        corpus = LAPUTA.resolve(strict=True)
        argv += ["--volume", f"{corpus}:/laputa:ro", "--env", "XSH_LAPUTA_CORPUS=/laputa"]
        env["XSH_LAPUTA_CORPUS"] = "/laputa"
    argv += [IMAGE]
    policy = {
        "network": network,
        "memory": memory,
        "swap": "disabled",
        "cpus": "8",
        "uid": "1000:1000" if args.mode == "traceroute-nonroot" else "0",
        "capabilities": ["SYS_ADMIN", "MKNOD"] if block_run else [],
        "device_cgroup_rule": "b *:* m" if block_run else None,
        "host_device_mounts": [],
        "host_pid_namespace": False,
        "host_clock_access": False,
        "nodev_tmpfs": True,
        "tmpdir": tmpdir,
        "environment": env,
    }
    return argv, policy


def cleanup_container(cidfile: Path) -> str:
    if not cidfile.is_file():
        return ""
    container_id = cidfile.read_text(encoding="ascii", errors="replace").strip()
    if not container_id:
        cidfile.unlink(missing_ok=True)
        return ""
    try:
        completed = subprocess.run(
            ["docker", "rm", "--force", "--time", "0", container_id],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except OSError as error:
        cidfile.unlink(missing_ok=True)
        return str(error)
    cidfile.unlink(missing_ok=True)
    if completed.returncode != 0:
        return completed.stderr.decode("utf-8", errors="replace").strip()
    return ""


def run_container(
    args: argparse.Namespace,
    run_dir: Path,
    target_dir: Path,
    scratch: Path,
    receipt: dict[str, Any],
    step_id: str,
    inner_argv: list[str],
    expected_source: dict[str, str],
    expected_products: dict[str, dict[str, str]],
) -> tuple[bytes, bytes, int, dict[str, str]]:
    base, environment = docker_base(args, run_dir, target_dir, scratch, step_id)
    argv = base + inner_argv
    step: dict[str, Any] = {
        "id": step_id,
        "argv": argv,
        "container_policy": environment,
        "started": True,
        "state": "running",
        "source_before": source_snapshot(args.head, args.tree),
        "products_before": products(args),
    }
    receipt["steps"].append(step)
    save_receipt(run_dir / "receipt.json", receipt)
    cidfile = run_dir / f"{step_id}.cid"
    cleanup_note = ""
    try:
        completed = subprocess.run(
            argv,
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=args.timeout_seconds or None,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout or b""
        stderr = error.stderr or b""
        cleanup_note = cleanup_container(cidfile)
        suffix = b"\ncontainer command timed out\n"
        if cleanup_note:
            suffix += f"container cleanup failed: {cleanup_note}\n".encode("utf-8", errors="replace")
        completed = subprocess.CompletedProcess(argv, 124, stdout, stderr + suffix)
    except KeyboardInterrupt:
        cleanup_note = cleanup_container(cidfile)
        if cleanup_note:
            print(f"container cleanup failed: {cleanup_note}", file=sys.stderr)
        raise
    outputs = write_step_output(run_dir, step_id, completed)
    after_source = observe_source()
    after_products = observed_products(args)
    container_id = cidfile.read_text(encoding="ascii", errors="replace").strip() if cidfile.is_file() else ""
    cidfile.unlink(missing_ok=True)
    step.update({
        "state": "finished" if completed.returncode == 0 else "failed",
        "outputs": outputs,
        "source_after": after_source,
        "products_after": after_products,
        "container_id": container_id,
        "exit_status": completed.returncode,
    })
    if after_source != expected_source:
        step["postcondition_error"] = "source identity changed during execution"
    if after_products != expected_products:
        step["postcondition_error"] = "verification product hashes changed during execution"
    save_receipt(run_dir / "receipt.json", receipt)
    if completed.returncode != 0:
        raise RunnerFailure(f"{step_id} exited {completed.returncode}")
    if after_source != expected_source:
        raise RunnerFailure(f"source identity changed during {step_id}")
    if after_products != expected_products:
        raise RunnerFailure(f"product hashes changed during {step_id}")
    return completed.stdout, completed.stderr, completed.returncode, outputs


def verify_image() -> str:
    docker = shutil.which("docker")
    if docker is None:
        raise RunnerFailure("docker is unavailable on PATH")
    completed = subprocess.run(
        [docker, "image", "inspect", "--format", "{{.Id}}", IMAGE],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    observed = completed.stdout.strip()
    if completed.returncode != 0 or observed != IMAGE:
        raise RunnerFailure(f"pinned image check failed: expected {IMAGE}; got {observed}; {completed.stderr.strip()}")
    return observed


def parse_xsht_summary(output: bytes) -> dict[str, int | str]:
    matches = XSHT_SUMMARY.findall(output.decode("utf-8", errors="replace"))
    if len(matches) != 1:
        raise RunnerFailure(f"expected one XSHT summary, found {len(matches)}")
    state, passed, failed, skipped = matches[0]
    return {"state": state, "passed": int(passed), "failed": int(failed), "skipped": int(skipped), "selected": int(passed) + int(failed) + int(skipped)}


def parse_rust_test_summary(output: bytes) -> dict[str, int | str]:
    matches = RUST_TEST_SUMMARY.findall(output.decode("utf-8", errors="replace"))
    if len(matches) != 1:
        raise RunnerFailure(f"expected one Rust test-harness summary, found {len(matches)}")
    state, passed, failed, ignored, measured, filtered = matches[0]
    return {"state": state, "passed": int(passed), "failed": int(failed), "ignored": int(ignored), "measured": int(measured), "filtered_out": int(filtered), "selected": int(passed) + int(failed) + int(ignored)}


def execute(args: argparse.Namespace) -> Path:
    target_dir, scratch, expected_products, expected_total = check_arguments(args)
    expected_source = source_snapshot(args.head, args.tree)
    scratch.mkdir(parents=True, exist_ok=True)
    run_root = scratch / "final-residual-runs"
    run_root.mkdir(parents=True, exist_ok=True)
    run_dir = run_root / f"{args.mode}-{secrets.token_hex(8)}"
    run_dir.mkdir(mode=0o755)
    receipt: dict[str, Any] = {
        "schema": 1,
        "state": "preparing",
        "mode": args.mode,
        "source": expected_source,
        "target": TARGET,
        "profile": PROFILE,
        "pinned_image": IMAGE,
        "product_hashes": expected_products,
        "canonical_native_stdout": None if args.canonical_stdout_path is None else {
            "path": args.canonical_stdout_path.relative_to(ROOT).as_posix(),
            "sha256": args.canonical_stdout_sha256,
            "expected_total": expected_total,
        },
        "container_policy": {
            "runtime_preload": "none",
            "tmpdir": "/scratch",
            "scratch_host_path": scratch.relative_to(ROOT).as_posix(),
            "cpu_limit": "8",
            "memory_limit": "1g for block-device execution; 16g otherwise",
            "swap": "disabled",
            "block_device_mount_policy": "container-private /scratch bind remounted nodev; fail before harness unless statvfs confirms ST_NODEV",
        },
        "steps": [],
    }
    if "privileged_harness" in expected_products:
        receipt["prepared_harness"] = expected_products["privileged_harness"]
    save_receipt(run_dir / "receipt.json", receipt)
    lock_acquired = False
    try:
        with native_suite_lock():
            lock_acquired = True
            verify_image()
            source_snapshot(args.head, args.tree)
            products(args)
            receipt["native_suite_lock"] = {"path": str(NATIVE_LOCK), "held": True}
            receipt["pinned_image_verified"] = IMAGE
            save_receipt(run_dir / "receipt.json", receipt)
            if args.mode == "forced-native":
                inner = [f"/work/target/{TARGET}/{PROFILE}/xsht", "test", "--force-frames"]
                stdout, _, _, _ = run_container(args, run_dir, target_dir, scratch, receipt, "forced-native", inner, expected_source, expected_products)
                summary = parse_xsht_summary(stdout)
                receipt["selected_cases"] = summary
                save_receipt(run_dir / "receipt.json", receipt)
                if summary["state"] != "ok" or summary["failed"] != 0 or summary["selected"] != expected_total:
                    raise RunnerFailure(f"forced full-suite summary differs from canonical selection: {summary}, expected total {expected_total}")
                if summary["passed"] == 0 or summary["passed"] + summary["skipped"] != expected_total:
                    raise RunnerFailure(f"forced native suite did not account for every selected case: {summary}")
            elif args.mode == "traceroute-nonroot":
                inner = [f"/work/target/{TARGET}/{PROFILE}/xsht", "test", "core/tests/test-traceroute.xsh"]
                stdout, _, _, _ = run_container(args, run_dir, target_dir, scratch, receipt, "traceroute-nonroot", inner, expected_source, expected_products)
                summary = parse_xsht_summary(stdout)
                receipt["selected_cases"] = summary
                save_receipt(run_dir / "receipt.json", receipt)
                text = stdout.decode("utf-8", errors="replace")
                expected_skip = "test_traceroute_gateway_adds_a_source_route"
                if summary["state"] != "ok" or summary["passed"] == 0 or summary["failed"] != 0 or summary["selected"] != TRACEROUTE_CASES:
                    raise RunnerFailure(f"traceroute file selection mismatch: {summary}, expected {TRACEROUTE_CASES} cases")
                if not re.search(expected_skip + r"[^\n]*skipped: IPv4 source-route IP_OPTIONS requires CAP_NET_RAW", text):
                    raise RunnerFailure("traceroute output lacks the expected nonroot source-route skip")
                receipt["required_observation"] = "gateway source-route test passed its exact EPERM assertion and reported the CAP_NET_RAW skip"
            else:
                harness_path = privileged_harness_path(args, target_dir)
                harness_container_path = "/work/target/" + harness_path.relative_to(target_dir).as_posix()
                nodev_guard = (
                    "mount -o remount,bind,nodev /scratch\n"
                    "python3 -c 'import os; flags = os.statvfs(\"/scratch\").f_flag; "
                    "assert flags & os.ST_NODEV; "
                    "print(\"scratch_statvfs_f_flag=%d nodev=True\" % flags)'\n"
                    "exec \"$@\""
                )
                harness_args = [harness_container_path, "--nocapture", "cp::privileged::test_cp_recursive_block_device_"]
                guarded_test_args = ["/bin/sh", "-eu", "-c", nodev_guard, "block-device", *harness_args]
                test_stdout, _, _, _ = run_container(args, run_dir, target_dir, scratch, receipt, "root-block-device-run", guarded_test_args, expected_source, expected_products)
                summary = parse_rust_test_summary(test_stdout)
                receipt["selected_cases"] = summary
                save_receipt(run_dir / "receipt.json", receipt)
                text = test_stdout.decode("utf-8", errors="replace")
                expected_names = (
                    "core_compat_extra::cp::privileged::test_cp_recursive_block_device_recursive",
                    "core_compat_extra::cp::privileged::test_cp_recursive_block_device_archive",
                )
                for name in expected_names:
                    if not re.search(r"test " + re.escape(name) + r" \.\.\. ok", text):
                        raise RunnerFailure(f"block-device output lacks successful selected case {name}")
                nodev_flags = re.search(r"scratch_statvfs_f_flag=(\d+) nodev=True", text)
                if nodev_flags is None:
                    raise RunnerFailure("block-device run lacks a successful /scratch ST_NODEV statvfs assertion")
                if summary["state"] != "ok" or summary["passed"] != BLOCK_DEVICE_CASES or summary["failed"] != 0 or summary["selected"] != BLOCK_DEVICE_CASES:
                    raise RunnerFailure(f"block-device case count mismatch: {summary}, expected {BLOCK_DEVICE_CASES} passes")
                receipt["prepared_harness"]["unchanged_after_run"] = True
                receipt["scratch_nodev"] = {"statvfs_f_flag": int(nodev_flags.group(1)), "verified": True}
    except BaseException as error:
        receipt["state"] = "failed"
        receipt["error"] = str(error)
        receipt["source_after"] = observe_source()
        receipt["products_after"] = observed_products(args)
        receipt["native_suite_lock"] = {"path": str(NATIVE_LOCK), "acquired": lock_acquired, "held": False}
        save_receipt(run_dir / "receipt.json", receipt)
        raise
    else:
        receipt["state"] = "passed"
        receipt["source_after"] = source_snapshot(args.head, args.tree)
        receipt["products_after"] = products(args)
        receipt["native_suite_lock"] = {"path": str(NATIVE_LOCK), "acquired": lock_acquired, "held": False}
        save_receipt(run_dir / "receipt.json", receipt)
    return run_dir / "receipt.json"


def main() -> int:
    args = parser().parse_args()
    try:
        receipt = execute(args)
    except (OSError, RunnerFailure, subprocess.SubprocessError) as error:
        print(f"final residual replay failed: {error}", file=sys.stderr)
        return 1
    print(receipt)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
