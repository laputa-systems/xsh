#!/usr/bin/env python3
"""Run the sealed-source, focused final correctness gates; --plan is read-only."""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
import pathlib
import re
import shlex
import signal
import subprocess
import sys
import time
from typing import Any

sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"


ROOT = pathlib.Path("/home/josh/d/laputa-systems/xsh")
HERE = ROOT / ".work/final-review-evidence/final-correctness-gates"
PLAN_PATH = HERE / "manifest.json"
RUNNER = HERE / "run-native-final.sh"
INFRA = ROOT / ".work/consolidation/infra"
BASELINE_BINS = INFRA / "baseline-bin"
TARGET = "x86_64-unknown-linux-musl"
IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
CONTAINER_CPUS = 12
CONTAINER_MEMORY = "24g"
CONTAINER_SWAP = "disabled"
CARGO_LOCK_TIMEOUT_SECONDS = 900
CLEANUP_TIMEOUT_SECONDS = 10
PROCESS_TERM_TIMEOUT_SECONDS = 5
PROCESS_KILL_TIMEOUT_SECONDS = 5
BASE_RUNNER = INFRA / "run-native.sh"
NATIVE_LOCK = INFRA / "native-suite.lock"
NATIVE_CLEANUP_MARKER = INFRA / "native-suite.cleanup-unresolved"
BASE_RUNNER_SHA256 = "5c06b77b6c838d1bf7c63e6078c7a708db54e4ff12f6fd23e23b67f2915e9d15"
APPROVED_MANIFEST_BODY_SHA256 = "ee07f7608031dd5d9562200306b57c452465a4ad66bd4fe9b3027c3738b36eb1"
RUNTIME_CARGO_FORWARDER = HERE / "cargo-forwarder.sh"
RUNTIME_RUSTC_WRAPPER = HERE / "rustc-jemalloc.sh"
TOOL_HASHES = {
    pathlib.Path("/home/josh/.cargo/bin/kache"): "b826e99983057974663d6863eafbee4e1b25ca9373071406b15dd02961b2c37d",
    pathlib.Path("/usr/local/bin/mold"): "78e4b6aec99fa067b81326f4b540893b92ae78e5b4176ab32be0565dcbd049d3",
    INFRA / "mold-linker.sh": "49f116f6bd364f682439e87200281a5acccf081d3b5841170a5a5bf34daf5271",
    pathlib.Path("/usr/lib/libjemalloc.so.2"): "2db538572b752a3ed5b6b8d9a76cc2966f0670ad481892cb423617c9e4345284",
}
BASELINE_HASHES = {
    "xsh": "0f204cd7960bfb6867ad1809c914f72199fc9ced684a187c850300a32a31f157",
    "xsht": "27ec388ec31a6fa085edf1a5a8d4c9ee4e6ad85c950c337b8d6412fbea7864a0",
    "xsh-test-helper": "05a4fa42686b32cd27e7af77af846a132a4253c7748b89aac0be5d402693e536",
}


class GateFailure(RuntimeError):
    pass


class GateInterrupted(GateFailure):
    pass


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def git_snapshot() -> dict[str, str]:
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, check=True, text=True, capture_output=True).stdout.strip()
    tree = subprocess.run(["git", "rev-parse", "HEAD^{tree}"], cwd=ROOT, check=True, text=True, capture_output=True).stdout.strip()
    status = subprocess.run(["git", "status", "--porcelain=v1", "--untracked-files=all"], cwd=ROOT, check=True, text=True, capture_output=True).stdout
    return {"head": head, "tree": tree, "status": status}


def json_write(path: pathlib.Path, value: Any) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


def manifest_body_sha256(plan: dict[str, Any]) -> str:
    pinned_body = json.loads(json.dumps(plan))
    try:
        del pinned_body["environment"]["launch_sha256"]
    except KeyError as error:
        raise GateFailure("manifest must contain exactly one launcher self-hash field") from error
    encoded = json.dumps(pinned_body, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def unique_json_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise GateFailure(f"manifest contains duplicate JSON key {key!r}")
        value[key] = item
    return value


class Launch:
    def __init__(self, expected_commit: str, expected_tree: str, out: pathlib.Path):
        self.expected_commit = expected_commit
        self.expected_tree = expected_tree
        self.out = out
        self.target_root = out / "target"
        self.target_dir = self.target_root / "products"
        self.harness_target_dir = self.target_root / "harness"
        self.runtime_target_dir = self.target_root / "runtime"
        self.linux_priv_target_dir = self.target_root / "linux-priv"
        for target_dir in (
            self.target_dir,
            self.harness_target_dir,
            self.runtime_target_dir,
            self.linux_priv_target_dir,
        ):
            target_dir.mkdir(parents=True)
        self.runtime_cargo_home = out / "runtime-cargo-home"
        self.runtime_xdg_cache = out / "runtime-xdg-cache"
        self.runtime_cargo_home.mkdir()
        self.runtime_xdg_cache.mkdir()
        self.interrupt_started = False
        self.native_lock_file: Any = None
        self.logs = out / "logs"
        self.logs.mkdir(parents=True)
        self.cid_dir = out / "container-ids"
        self.cid_dir.mkdir()
        self.receipt: dict[str, Any] = {
            "schema": 1,
            "status": "running",
            "started_utc": utc_now(),
            "source_expected": {"commit": expected_commit, "tree": expected_tree},
            "environment": {"target": TARGET, "image": IMAGE},
            "cargo_target_dirs": {
                "products": str(self.target_dir),
                "harness": str(self.harness_target_dir),
                "runtime": str(self.runtime_target_dir),
                "linux-priv": str(self.linux_priv_target_dir),
            },
            "steps": [],
            "harnesses": {},
            "hashes": {},
            "qualification": [
                "The complete native suite is owned by the final scheduler and is intentionally absent here.",
                "Linux privilege cases that early-return for a missing capability require manual skip audit from --nocapture logs.",
                "Ignored date-clock and BTRFS cases require the disposable QEMU guest described in docs/TESTING.md.",
                "The interactive parity scenario driver is absent; the retained Linux PTY Rust case is included.",
            ],
        }
        self.step_number = 0

    def save(self) -> None:
        json_write(self.out / "receipt.json", self.receipt)

    def verify_source(self, where: str) -> dict[str, str]:
        current = git_snapshot()
        self.receipt.setdefault("source_checks", []).append({
            "where": where,
            "head": current["head"],
            "tree": current["tree"],
            "clean": current["status"] == "",
            "status": current["status"],
        })
        if current["head"] != self.expected_commit or current["tree"] != self.expected_tree or current["status"]:
            self.save()
            raise GateFailure(f"source changed or is dirty at {where}")
        return current

    def append_cleanup(self, log: pathlib.Path, argv: list[str]) -> dict[str, Any]:
        with log.open("ab") as output:
            output.write(("\ncleanup argv: " + json.dumps(argv) + "\n").encode())
            try:
                result = subprocess.run(
                    argv,
                    cwd=ROOT,
                    capture_output=True,
                    text=True,
                    check=False,
                    timeout=CLEANUP_TIMEOUT_SECONDS,
                )
            except subprocess.TimeoutExpired as error:
                output.write(f"cleanup timed out after {CLEANUP_TIMEOUT_SECONDS}s\n".encode())
                timed_stdout = error.stdout.decode(errors="replace") if isinstance(error.stdout, bytes) else error.stdout or ""
                timed_stderr = error.stderr.decode(errors="replace") if isinstance(error.stderr, bytes) else error.stderr or ""
                return {
                    "argv": argv,
                    "resolved": False,
                    "timed_out": True,
                    "stdout": timed_stdout,
                    "stderr": timed_stderr,
                }
            except OSError as error:
                output.write(f"cleanup command could not start: {error}\n".encode())
                return {"argv": argv, "resolved": False, "timed_out": False, "error": str(error), "stdout": "", "stderr": ""}
            else:
                output.write((result.stdout or "").encode())
                output.write((result.stderr or "").encode())
                output.write(f"cleanup exit: {result.returncode}\n".encode())
                return {
                    "argv": argv,
                    "resolved": result.returncode == 0,
                    "timed_out": False,
                    "exit_code": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                }

    def stop_owned_container(self, cidfile: pathlib.Path, log: pathlib.Path) -> dict[str, Any]:
        if not cidfile.exists():
            return {"resolved": False, "reason": "container cidfile is missing", "cidfile": str(cidfile), "commands": []}
        try:
            container_id = cidfile.read_text().strip()
        except OSError as error:
            return {"resolved": False, "reason": f"cannot read container cidfile: {error}", "cidfile": str(cidfile), "commands": []}
        if not container_id:
            return {"resolved": False, "reason": "container cidfile is empty", "cidfile": str(cidfile), "commands": []}
        if not re.fullmatch(r"[0-9a-fA-F]{12,64}", container_id):
            return {"resolved": False, "reason": "container cidfile has an invalid ID", "cidfile": str(cidfile), "commands": []}

        commands = [
            self.append_cleanup(log, ["docker", "stop", "--time", "5", container_id]),
            self.append_cleanup(log, ["docker", "rm", "--force", container_id]),
        ]
        presence = self.append_cleanup(
            log,
            ["docker", "ps", "--all", "--no-trunc", "--filter", f"id={container_id}", "--format", "{{.ID}}"],
        )
        commands.append(presence)
        remaining = presence.get("stdout", "").strip()
        resolved = not presence.get("timed_out") and presence.get("exit_code") == 0 and not remaining
        return {
            "resolved": resolved,
            "cidfile": str(cidfile),
            "container_id": container_id,
            "commands": commands,
            "remaining_container_ids": remaining.splitlines() if remaining else [],
            "reason": None if resolved else "Docker did not confirm that the owned container was removed",
        }

    @staticmethod
    def stop_runner_process(process: subprocess.Popen[bytes], log: pathlib.Path) -> dict[str, Any]:
        actions: list[dict[str, Any]] = []
        for signum, wait_seconds in (
            (signal.SIGTERM, PROCESS_TERM_TIMEOUT_SECONDS),
            (signal.SIGKILL, PROCESS_KILL_TIMEOUT_SECONDS),
        ):
            if process.poll() is not None:
                break
            try:
                os.killpg(process.pid, signum)
            except ProcessLookupError:
                pass
            except OSError as error:
                actions.append({"signal": signal.Signals(signum).name, "error": str(error)})
            try:
                process.wait(timeout=wait_seconds)
                actions.append({"signal": signal.Signals(signum).name, "reaped": True})
                break
            except subprocess.TimeoutExpired:
                actions.append({"signal": signal.Signals(signum).name, "reaped": False})
        reaped = process.poll() is not None
        with log.open("ab") as output:
            output.write(("runner process cleanup: " + json.dumps({"actions": actions, "reaped": reaped}) + "\n").encode())
        return {"resolved": reaped, "actions": actions}

    def retain_unresolved_cleanup(
        self,
        *,
        step_id: str,
        cidfile: pathlib.Path | None,
        container_cleanup: dict[str, Any],
        process_cleanup: dict[str, Any],
    ) -> None:
        marker = {
            "run_id": self.out.name,
            "step": step_id,
            "cidfile": str(cidfile) if cidfile is not None else None,
            "container_cleanup": container_cleanup,
            "process_cleanup": process_cleanup,
        }
        if cidfile is not None and not cidfile.exists():
            cidfile.write_text("")
        temporary = NATIVE_CLEANUP_MARKER.with_suffix(NATIVE_CLEANUP_MARKER.suffix + ".tmp")
        temporary.write_text(json.dumps(marker, indent=2, sort_keys=True) + "\n")
        with temporary.open("r+b") as source:
            os.fsync(source.fileno())
        temporary.replace(NATIVE_CLEANUP_MARKER)
        descriptor = os.open(INFRA, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
        self.receipt["unresolved_cleanup"] = marker
        self.save()

    def run(
        self,
        step_id: str,
        argv: list[str],
        *,
        env: dict[str, str] | None = None,
        timeout_seconds: int | None = None,
        cidfile: pathlib.Path | None = None,
        check_source: bool = True,
        pass_fds: tuple[int, ...] = (),
    ) -> dict[str, Any]:
        self.step_number += 1
        log = self.logs / f"{self.step_number:03d}-{step_id}.log"
        env_values = dict(env or {})
        if cidfile is not None:
            if cidfile.exists():
                cidfile.unlink()
            env_values["XSH_GATE_CIDFILE"] = str(cidfile)
        step: dict[str, Any] = {
            "id": step_id,
            "argv": argv,
            "env": env_values,
            "cwd": str(ROOT),
            "log": str(log),
            "started_utc": utc_now(),
        }
        if check_source:
            self.verify_source(f"before:{step_id}")
        monotonic_start = time.monotonic()
        interrupted: str | None = None
        process: subprocess.Popen[bytes] | None = None
        process_start_failed = False
        cleanup_result: dict[str, Any] | None = None
        with log.open("wb") as output:
            output.write(("argv: " + json.dumps(argv) + "\n").encode())
            output.write(("env: " + json.dumps(env_values, sort_keys=True) + "\n\n").encode())
            try:
                process = subprocess.Popen(
                    argv,
                    cwd=ROOT,
                    env=dict(os.environ, **env_values),
                    stdout=output,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                    pass_fds=pass_fds,
                )
                try:
                    exit_code = process.wait(timeout=timeout_seconds)
                    timed_out = False
                except subprocess.TimeoutExpired:
                    timed_out = True
                    self.interrupt_started = True
                    output.flush()
                    process_cleanup = self.stop_runner_process(process, log)
                    container_cleanup = self.stop_owned_container(cidfile, log) if cidfile is not None else {"resolved": True, "reason": "no container was started"}
                    cleanup_result = {
                        "resolved": process_cleanup["resolved"] and container_cleanup["resolved"],
                        "process": process_cleanup,
                        "container": container_cleanup,
                    }
                    exit_code = 124
                except (GateInterrupted, KeyboardInterrupt) as error:
                    interrupted = str(error) or "interrupted"
                    self.interrupt_started = True
                    output.flush()
                    process_cleanup = self.stop_runner_process(process, log)
                    container_cleanup = self.stop_owned_container(cidfile, log) if cidfile is not None else {"resolved": True, "reason": "no container was started"}
                    cleanup_result = {
                        "resolved": process_cleanup["resolved"] and container_cleanup["resolved"],
                        "process": process_cleanup,
                        "container": container_cleanup,
                    }
                    exit_code = 130
                    timed_out = False
            except OSError as error:
                output.write(f"launcher error: {error}\n".encode())
                exit_code = 127
                timed_out = False
                process_start_failed = True
        if cidfile is not None and not process_start_failed and exit_code not in (0, 124, 130):
            self.interrupt_started = True
            container_cleanup = self.stop_owned_container(cidfile, log)
            cleanup_result = {
                "resolved": container_cleanup["resolved"],
                "process": {"resolved": process is None or process.poll() is not None, "actions": []},
                "container": container_cleanup,
            }
        step["exit_code"] = exit_code
        step["timed_out"] = timed_out
        if cleanup_result is not None:
            step["cleanup"] = cleanup_result
        step["duration_seconds"] = round(time.monotonic() - monotonic_start, 3)
        step["log_sha256"] = sha256(log)
        step["finished_utc"] = utc_now()
        self.receipt["steps"].append(step)
        self.save()
        if cleanup_result is not None and not cleanup_result["resolved"]:
            self.retain_unresolved_cleanup(
                step_id=step_id,
                cidfile=cidfile,
                container_cleanup=cleanup_result["container"],
                process_cleanup=cleanup_result["process"],
            )
        elif cidfile is not None and cidfile.exists():
            cidfile.unlink()
        if check_source:
            self.verify_source(f"after:{step_id}")
        if cleanup_result is not None and not cleanup_result["resolved"]:
            raise GateFailure(f"{step_id} cleanup could not be confirmed; cidfile and native-suite lock marker were retained")
        if interrupted is not None:
            raise GateInterrupted(f"{step_id} interrupted: {interrupted}")
        if exit_code != 0:
            raise GateFailure(f"{step_id} exited {exit_code}; see {log}")
        return step

    def run_native(self, step_id: str, mode: str, args: list[str], *, timeout: int = 3600) -> dict[str, Any]:
        cidfile = self.cid_dir / f"{self.step_number + 1:03d}-{step_id}.cid"
        native_fd = self.native_lock_file.fileno() if self.native_lock_file is not None else None
        environment = {
            "XSH_GATE_TARGET_DIR": str(self.target_dir),
            "XSH_GATE_CARGO_LOCK_TIMEOUT": str(CARGO_LOCK_TIMEOUT_SECONDS),
        }
        pass_fds: tuple[int, ...] = ()
        if native_fd is not None:
            environment["XSH_GATE_NATIVE_LOCK_FD"] = str(native_fd)
            pass_fds = (native_fd,)
        return self.run(
            step_id,
            [str(RUNNER), str(ROOT), mode, *args],
            env=environment,
            timeout_seconds=timeout,
            cidfile=cidfile,
            pass_fds=pass_fds,
        )

    def target_dir_for(self, variant: str) -> pathlib.Path:
        return {
            "products": self.target_dir,
            "harness": self.harness_target_dir,
            "runtime": self.runtime_target_dir,
            "linux-priv": self.linux_priv_target_dir,
        }[variant]

    def run_target_native(
        self,
        step_id: str,
        mode: str,
        args: list[str],
        target_dir: pathlib.Path,
        *,
        timeout: int = 3600,
        extra_env: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        cidfile = self.cid_dir / f"{self.step_number + 1:03d}-{step_id}.cid"
        environment = {
            "XSH_GATE_TARGET_DIR": str(target_dir),
            "XSH_GATE_CARGO_LOCK_TIMEOUT": str(CARGO_LOCK_TIMEOUT_SECONDS),
        }
        environment.update(extra_env or {})
        native_fd = self.native_lock_file.fileno() if self.native_lock_file is not None else None
        pass_fds: tuple[int, ...] = ()
        if native_fd is not None:
            environment["XSH_GATE_NATIVE_LOCK_FD"] = str(native_fd)
            pass_fds = (native_fd,)
        return self.run(
            step_id,
            [str(RUNNER), str(ROOT), mode, *args],
            env=environment,
            timeout_seconds=timeout,
            cidfile=cidfile,
            pass_fds=pass_fds,
        )

    def container_env(self) -> dict[str, str]:
        verification = self.target_dir / TARGET / "verification"
        standard_path = (
            f"{BASELINE_BINS}"
            ":/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        )
        return {
            "XSH_BIN": str(verification / "xsh"),
            "PATH": str(verification) + ":" + standard_path,
        }

    def run_test_executable(
        self,
        step_id: str,
        executable: pathlib.Path,
        *,
        target_dir: pathlib.Path,
        filter_value: str = "",
        skip_filters: list[str] | None = None,
        threads: int = 4,
        timeout: int = 3600,
        nested_cargo: bool = False,
    ) -> None:
        arguments: list[str] = []
        if filter_value:
            arguments.append(filter_value)
        for skip_filter in skip_filters or []:
            arguments.extend(["--skip", skip_filter])
        arguments.extend(["--nocapture", f"--test-threads={threads}"])
        command_env = self.container_env()
        command_env["CARGO_TARGET_DIR"] = str(target_dir)
        mode = "run"
        extra_env: dict[str, str] = {}
        if nested_cargo:
            mode = "run-cargo"
            trace_path = target_dir / "nested-cargo.trace"
            stdout_path = target_dir / "nested-cargo.stdout.jsonl"
            for path in (trace_path, stdout_path):
                path.unlink(missing_ok=True)
            command_env.update({
                "CARGO_HOME": str(self.runtime_cargo_home),
                "XDG_CACHE_HOME": str(self.runtime_xdg_cache),
                "XSH_NESTED_CARGO_TRACE": str(trace_path),
                "XSH_NESTED_CARGO_STDOUT": str(stdout_path),
                "XSH_GATE_RUSTC_WRAPPER": "/usr/local/bin/xsh-rustc-jemalloc",
            })
            extra_env = {
                "XSH_GATE_CARGO_HOME": str(self.runtime_cargo_home),
                "XSH_GATE_XDG_CACHE_HOME": str(self.runtime_xdg_cache),
                "XSH_GATE_CARGO_FORWARDER": str(RUNTIME_CARGO_FORWARDER),
                "XSH_GATE_RUSTC_WRAPPER": str(RUNTIME_RUSTC_WRAPPER),
            }
        self.run_target_native(
            step_id,
            mode,
            ["/usr/bin/env", *[f"{key}={value}" for key, value in command_env.items()], str(executable), *arguments],
            target_dir,
            timeout=timeout,
            extra_env=extra_env,
        )

    def parse_test_artifacts(self, log: pathlib.Path, targets: list[str]) -> dict[str, pathlib.Path]:
        found: dict[str, pathlib.Path] = {}
        for raw_line in log.read_text(errors="replace").splitlines():
            try:
                item = json.loads(raw_line)
            except json.JSONDecodeError:
                continue
            if item.get("reason") != "compiler-artifact":
                continue
            target = item.get("target", {})
            name = target.get("name")
            executable = item.get("executable")
            if name not in targets or not executable or not target.get("test", False):
                continue
            path = pathlib.Path(executable).resolve()
            if not path.is_file() or not os.access(path, os.X_OK):
                continue
            found[name] = path
        missing = sorted(set(targets) - found.keys())
        if missing:
            raise GateFailure(f"Cargo JSON omitted test executables {missing}; inspect {log}")
        return found

    def parse_binary_artifacts(self, log: pathlib.Path) -> dict[str, pathlib.Path]:
        found: dict[str, pathlib.Path] = {}
        for raw_line in log.read_text(errors="replace").splitlines():
            try:
                item = json.loads(raw_line)
            except json.JSONDecodeError:
                continue
            if item.get("reason") != "compiler-artifact":
                continue
            target = item.get("target", {})
            name = target.get("name")
            kinds = target.get("kind", [])
            executable = item.get("executable")
            if not name or "bin" not in kinds or target.get("test", False) or not executable:
                continue
            path = pathlib.Path(executable).resolve()
            if path.is_file() and os.access(path, os.X_OK):
                found[name] = path
        return found

    def build_harnesses(self, plan: dict[str, Any]) -> None:
        for harness in plan["rust_harness_builds"]:
            profile = harness["profile"]
            args = ["cargo", "test", "--locked", "--target", TARGET]
            if profile == "verification":
                args.extend(["--profile", "verification"])
            args.extend(harness["args"])
            args.extend(["--no-run", "--message-format=json"])
            target_variant = harness["target_variant"]
            target_dir = self.target_dir_for(target_variant)
            step = self.run_target_native(
                f"build-harness-{harness['id']}",
                "build",
                args,
                target_dir,
                timeout=7200,
            )
            log = pathlib.Path(step["log"])
            found = self.parse_test_artifacts(log, harness["targets"])
            product_artifacts = self.parse_binary_artifacts(log)
            self.receipt["harnesses"][harness["id"]] = {
                "target_variant": target_variant,
                "target_dir": str(target_dir),
                "features": [harness["args"][harness["args"].index("--features") + 1]]
                if "--features" in harness["args"] else [],
                "product_artifacts": {
                    name: {"path": str(path), "sha256": sha256(path)}
                    for name, path in sorted(product_artifacts.items())
                    if name in {"xsh", "xsht", "xshi", "xsh-test-helper"}
                },
                "targets": {
                    target_name: {"path": str(path), "sha256": sha256(path)}
                    for target_name, path in sorted(found.items())
                },
            }
            self.save()

    def prepare_runtime_cargo(self) -> None:
        for directory in (self.runtime_cargo_home, self.runtime_xdg_cache):
            directory.mkdir(parents=True, exist_ok=True)
        self.run_target_native(
            "prepare-runtime-nested-cargo",
            "prepare-cargo",
            [],
            self.runtime_target_dir,
            timeout=3600,
            extra_env={
                "XSH_GATE_CARGO_HOME": str(self.runtime_cargo_home),
                "XSH_GATE_XDG_CACHE_HOME": str(self.runtime_xdg_cache),
            },
        )

    def record_product_hashes(self, label: str, expected: dict[str, str]) -> None:
        actual = self.binary_hashes()
        self.receipt["hashes"][label] = actual
        self.save()
        if actual != expected:
            raise GateFailure(f"candidate verification product hashes changed at {label}: {actual} != {expected}")

    def binary_hashes(self) -> dict[str, str]:
        verification = self.target_dir / TARGET / "verification"
        paths = {
            "xsh-verification": verification / "xsh",
            "xsht-verification": verification / "xsht",
            "xshi-verification": verification / "xshi",
            "xsh-fuzz-verification": verification / "xsh-fuzz",
            "xsh-test-helper-verification": verification / "xsh-test-helper",
        }
        return {name: sha256(path) for name, path in paths.items() if path.is_file()}

    def release_binary_hashes(self) -> dict[str, str]:
        release = self.target_dir / TARGET / "release"
        paths = {"xsh-release": release / "xsh", "xsht-release": release / "xsht"}
        return {name: sha256(path) for name, path in paths.items() if path.is_file()}

    def capture_nested_cargo_artifacts(self) -> None:
        trace = self.runtime_target_dir / "nested-cargo.trace"
        stdout = self.runtime_target_dir / "nested-cargo.stdout.jsonl"
        if not trace.is_file() or not stdout.is_file():
            raise GateFailure("the runtime integration test did not emit the managed nested Cargo trace")
        trace_text = trace.read_text()
        expected_args = (
            "build -p xshi -p xsht --bins --message-format=json "
            "--profile verification --target " + TARGET
        )
        expected_trace = (
            "cargo_path=/root/.cargo/bin/cargo\n"
            f"argv={expected_args}\n"
            f"target_dir={self.runtime_target_dir}\n"
            "cargo_net_offline=true\n"
        )
        if trace_text != expected_trace:
            raise GateFailure(f"nested Cargo command provenance mismatch: {trace_text!r}")

        runtime_artifacts: dict[str, pathlib.Path] = {}
        for raw_line in stdout.read_text(errors="replace").splitlines():
            try:
                item = json.loads(raw_line)
            except json.JSONDecodeError:
                continue
            if item.get("reason") != "compiler-artifact":
                continue
            target = item.get("target", {})
            name = target.get("name")
            executable = item.get("executable")
            if name not in {"xshi", "xsht"} or "bin" not in target.get("kind", []) or not executable:
                continue
            path = pathlib.Path(executable).resolve()
            if path.parent != (self.runtime_target_dir / TARGET / "verification").resolve():
                raise GateFailure(f"nested Cargo emitted {name} outside its isolated runtime target: {path}")
            if not path.is_file() or not os.access(path, os.X_OK):
                raise GateFailure(f"nested Cargo reported a missing {name} executable: {path}")
            runtime_artifacts[name] = path
        if set(runtime_artifacts) != {"xshi", "xsht"}:
            raise GateFailure(f"nested Cargo did not emit both expected products: {sorted(runtime_artifacts)}")

        runtime_xsh = self.receipt["harnesses"]["xsh-integration"]["product_artifacts"].get("xsh")
        if runtime_xsh is None:
            raise GateFailure("the runtime integration harness build did not report its CARGO_BIN_EXE_xsh product")
        candidate_xsh = self.target_dir / TARGET / "verification" / "xsh"
        if pathlib.Path(runtime_xsh["path"]).resolve() != (self.runtime_target_dir / TARGET / "verification" / "xsh").resolve():
            raise GateFailure("the runtime integration harness xsh path differs from its isolated target")
        if runtime_xsh["sha256"] != sha256(candidate_xsh):
            raise GateFailure("the runtime integration harness xsh is not byte-identical to the candidate product")

        self.receipt["runtime_nested_cargo"] = {
            "source": str(ROOT / "tests/runtime/common.rs"),
            "source_sha256": sha256(ROOT / "tests/runtime/common.rs"),
            "profile": "verification",
            "target": TARGET,
            "target_dir": str(self.runtime_target_dir),
            "packages": {
                "xshi": {
                    "explicit_features": [],
                    "default_features": [],
                    "manifest": str(ROOT / "crates/xshi/Cargo.toml"),
                    "manifest_sha256": sha256(ROOT / "crates/xshi/Cargo.toml"),
                },
                "xsht": {
                    "explicit_features": [],
                    "default_features": ["native-tests"],
                    "manifest": str(ROOT / "crates/xsht/Cargo.toml"),
                    "manifest_sha256": sha256(ROOT / "crates/xsht/Cargo.toml"),
                },
            },
            "uid": os.getuid(),
            "network": "none",
            "cargo_offline": True,
            "linker": "Clang 23 through pinned x86_64-unknown-linux-musl mold shim",
            "cargo_forwarder": {"path": str(RUNTIME_CARGO_FORWARDER), "sha256": sha256(RUNTIME_CARGO_FORWARDER)},
            "rustc_wrapper": {"path": str(RUNTIME_RUSTC_WRAPPER), "sha256": sha256(RUNTIME_RUSTC_WRAPPER)},
            "trace": {"path": str(trace), "sha256": sha256(trace), "text": trace_text},
            "stdout_json": {"path": str(stdout), "sha256": sha256(stdout)},
            "test_xsh": runtime_xsh,
            "artifacts": {
                name: {
                    "path": str(path),
                    "sha256": sha256(path),
                    "explicit_features": self.receipt["runtime_nested_cargo"]["packages"][name]["explicit_features"],
                    "default_features": self.receipt["runtime_nested_cargo"]["packages"][name]["default_features"],
                }
                for name, path in sorted(runtime_artifacts.items())
            },
        }
        self.receipt["hashes"]["runtime_target_after_nested_cargo"] = self.target_binary_hashes(self.runtime_target_dir)
        self.save()

    def capture_nested_cargo_partial(self) -> None:
        paths = {
            "trace": self.runtime_target_dir / "nested-cargo.trace",
            "stdout_json": self.runtime_target_dir / "nested-cargo.stdout.jsonl",
        }
        self.receipt["runtime_nested_cargo_partial"] = {
            name: {
                "path": str(path),
                "exists": path.is_file(),
                **({"sha256": sha256(path), "text": path.read_text(errors="replace")}
                   if path.is_file() and name == "trace" else {}),
                **({"sha256": sha256(path)} if path.is_file() and name != "trace" else {}),
            }
            for name, path in paths.items()
        }
        self.receipt["hashes"]["runtime_target_after_interruption_or_failure"] = self.target_binary_hashes(
            self.runtime_target_dir
        )
        self.save()

    @staticmethod
    def target_binary_hashes(target_dir: pathlib.Path) -> dict[str, str]:
        verification = target_dir / TARGET / "verification"
        paths = {name: verification / name for name in ("xsh", "xsht", "xshi", "xsh-test-helper")}
        return {name: sha256(path) for name, path in paths.items() if path.is_file()}

    def prepare_docs_dispatcher(self) -> tuple[pathlib.Path, pathlib.Path]:
        release = self.target_dir / TARGET / "release"
        driver = self.out / "prebuilt-docs-check.xsh"
        dispatcher = self.out / "xsh-dev-docs-check.sh"
        driver.write_text(
            "use docs as documentation\n\n"
            "proc main() [fs, process, env, error, io] -> Result[Unit] {\n"
            f"  documentation.check(p{json.dumps(str(ROOT))}, documentation.DocTools(\n"
            f"    xsh: p{json.dumps(str(release / 'xsh'))},\n"
            f"    xsht: p{json.dumps(str(release / 'xsht'))}))\n"
            "}\n\n"
            "main()?\n"
        )
        verification_xsh = self.target_dir / TARGET / "verification" / "xsh"
        release_xsh = release / "xsh"
        dispatcher.write_text(
            "#!/usr/bin/env bash\n"
            "set -euo pipefail\n"
            "if [[ \"$#\" -ne 4 || \"$1\" != \"dev/main.xsh\" || \"$2\" != \"--\" || \"$3\" != \"docs\" || \"$4\" != \"check\" ]]; then\n"
            "  echo 'expected the Makefile docs-check dispatch' >&2\n"
            "  exit 64\n"
            "fi\n"
            f"if [[ \"${{XSH_BIN:-}}\" != {shlex.quote(str(release_xsh))} ]]; then\n"
            "  echo 'docs tests must use the sealed release xsh' >&2\n"
            "  exit 65\n"
            "fi\n"
            f"exec {shlex.quote(str(verification_xsh))} {shlex.quote(str(driver))}\n"
        )
        dispatcher.chmod(0o755)
        self.receipt["docs_check_driver"] = {
            "driver": str(driver),
            "driver_sha256": sha256(driver),
            "dispatcher": str(dispatcher),
            "dispatcher_sha256": sha256(dispatcher),
            "verification_xsh": str(verification_xsh),
            "release_tools": {name: str(path) for name, path in {
                "xsh": release / "xsh",
                "xsht": release / "xsht",
            }.items()},
        }
        self.save()
        return driver, dispatcher

    def verify_toolchain_inputs(self, plan: dict[str, Any]) -> None:
        actual_manifest_body = manifest_body_sha256(plan)
        if actual_manifest_body != APPROVED_MANIFEST_BODY_SHA256:
            raise GateFailure(
                f"manifest body pin mismatch: {actual_manifest_body} != {APPROVED_MANIFEST_BODY_SHA256}"
            )
        if os.getuid() != 1000 or os.getgid() != 1000:
            raise GateFailure("the approved host runner identity is UID/GID 1000")
        if plan["environment"]["target"] != TARGET or plan["environment"]["image"] != IMAGE:
            raise GateFailure("the manifest target or image differs from the pinned launcher")
        if plan["environment"].get("native_runner_base_path") != str(BASE_RUNNER):
            raise GateFailure("the manifest's native runner path is not the in-tree infrastructure copy")
        if (
            plan["environment"].get("native_lock_path") != str(NATIVE_LOCK)
            or plan["environment"].get("native_cleanup_marker_path") != str(NATIVE_CLEANUP_MARKER)
            or plan["environment"].get("cleanup_command_timeout_seconds") != CLEANUP_TIMEOUT_SECONDS
            or plan["environment"].get("runner_term_timeout_seconds") != PROCESS_TERM_TIMEOUT_SECONDS
            or plan["environment"].get("runner_kill_timeout_seconds") != PROCESS_KILL_TIMEOUT_SECONDS
            or plan["environment"].get("cleanup_verification")
            != "docker ps --all must succeed and return no container matching the cidfile ID"
        ):
            raise GateFailure("the manifest's canonical exclusion lock or bounded cleanup policy differs from the launcher")
        self_test = plan.get("launcher_self_tests", [])
        self_test_relative_path = ".work/final-review-evidence/final-correctness-gates/test_launcher_cleanup.py"
        if len(self_test) != 1 or self_test[0].get("path") != self_test_relative_path:
            raise GateFailure("the manifest's no-Docker cleanup self-test inventory differs from the launcher")
        self_test_path = HERE / "test_launcher_cleanup.py"
        if not self_test_path.is_file():
            raise GateFailure("the no-Docker cleanup self-test is missing")
        if sha256(self_test_path) != plan["environment"].get("cleanup_self_test_sha256"):
            raise GateFailure("the no-Docker cleanup self-test changed after review")
        if (
            plan["environment"]["container_cpus"] != CONTAINER_CPUS
            or plan["environment"]["container_memory"] != CONTAINER_MEMORY
            or plan["environment"]["container_swap"] != CONTAINER_SWAP
            or plan["environment"]["cargo_lock_timeout_seconds"] != CARGO_LOCK_TIMEOUT_SECONDS
        ):
            raise GateFailure("the manifest resource limits differ from the bounded runner")
        actual_base = sha256(BASE_RUNNER)
        expected_base = plan["environment"]["native_runner_base_sha256"]
        if expected_base != BASE_RUNNER_SHA256:
            raise GateFailure("the manifest's native runner pin differs from the launcher")
        if actual_base != expected_base:
            raise GateFailure(f"native runner hash mismatch: {actual_base} != {expected_base}")
        if sha256(RUNNER) != plan["environment"]["final_runner_sha256"]:
            raise GateFailure("the prepared bounded runner changed after review")
        if sha256(HERE / "launch.py") != plan["environment"]["launch_sha256"]:
            raise GateFailure("the reviewed gate launcher changed after review")
        if sha256(RUNTIME_CARGO_FORWARDER) != plan["environment"]["runtime_cargo_forwarder_sha256"]:
            raise GateFailure("the nested Cargo forwarder changed after review")
        if sha256(RUNTIME_RUSTC_WRAPPER) != plan["environment"]["runtime_rustc_wrapper_sha256"]:
            raise GateFailure("the nested Rust compiler wrapper changed after review")
        for path, expected in TOOL_HASHES.items():
            if sha256(path) != expected:
                raise GateFailure(f"pinned build tool changed: {path}")
        wrapper_hash = sha256(RUNNER)
        self.receipt["environment"].update({
            "host_uid": os.getuid(),
            "host_gid": os.getgid(),
            "container_cpus": CONTAINER_CPUS,
            "container_memory": CONTAINER_MEMORY,
            "container_swap": CONTAINER_SWAP,
            "cargo_lock_timeout_seconds": CARGO_LOCK_TIMEOUT_SECONDS,
            "native_runner_base_sha256": actual_base,
            "native_lock_path": str(NATIVE_LOCK),
            "native_cleanup_marker_path": str(NATIVE_CLEANUP_MARKER),
            "native_lock_scope": "acquired before product, harness, Clippy and supplemental context runs",
            "no_docker_self_test_sha256": sha256(self_test_path),
            "final_runner_sha256": wrapper_hash,
            "launch_sha256": sha256(HERE / "launch.py"),
            "manifest_sha256": sha256(PLAN_PATH),
            "manifest_body_sha256": actual_manifest_body,
            "build_tools": {str(path): sha256(path) for path in TOOL_HASHES},
            "runtime_cargo_forwarder_sha256": sha256(RUNTIME_CARGO_FORWARDER),
            "runtime_rustc_wrapper_sha256": sha256(RUNTIME_RUSTC_WRAPPER),
        })
        actual_baseline = {name: sha256(BASELINE_BINS / name) for name in BASELINE_HASHES}
        if actual_baseline != BASELINE_HASHES:
            raise GateFailure("frozen baseline binary hashes changed")
        self.receipt["hashes"]["frozen_baseline"] = actual_baseline
        self.save()

    def require_native_lock(self) -> Any:
        if NATIVE_CLEANUP_MARKER.exists():
            marker = NATIVE_CLEANUP_MARKER.read_text(errors="replace").strip()
            raise GateFailure(f"native container cleanup is unresolved; inspect {NATIVE_CLEANUP_MARKER}: {marker}")
        lock_file = NATIVE_LOCK.open("a+")
        try:
            fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            lock_file.close()
            raise GateFailure("the shared native-suite lock is held; do not overlap this gate with the scheduled suite") from error
        if NATIVE_CLEANUP_MARKER.exists():
            marker = NATIVE_CLEANUP_MARKER.read_text(errors="replace").strip()
            fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)
            lock_file.close()
            raise GateFailure(f"native container cleanup is unresolved; inspect {NATIVE_CLEANUP_MARKER}: {marker}")
        self.native_lock_file = lock_file
        self.receipt["native_lock"] = {
            "path": str(NATIVE_LOCK),
            "acquired": True,
            "wait_policy": "nonblocking",
            "held_before_build": True,
            "unresolved_cleanup_marker": str(NATIVE_CLEANUP_MARKER),
        }
        self.save()
        return lock_file

    def release_native_lock(self) -> None:
        if self.native_lock_file is None:
            return
        fcntl.flock(self.native_lock_file.fileno(), fcntl.LOCK_UN)
        self.native_lock_file.close()
        self.native_lock_file = None

    def run_special_container(
        self,
        step_id: str,
        command: list[str],
        *,
        target_dir: pathlib.Path,
        privileged: bool,
        memory: str,
        cpus: str = "12",
        user: str = "0:0",
        network: str = "none",
        timeout: int = 3600,
    ) -> None:
        cidfile = self.cid_dir / f"{self.step_number + 1:03d}-{step_id}.cid"
        args = ["docker", "run", "--rm", "--init", "--cidfile", str(cidfile), "--platform", "linux/amd64", f"--cpus={cpus}", f"--memory={memory}", f"--memory-swap={memory}", f"--network={network}"]
        if privileged:
            args.append("--privileged")
        args.extend([
            "--tmpfs", "/tmp:rw,nodev,exec,nosuid,mode=1777",
            "-v", f"{ROOT}:{ROOT}",
            "-v", f"{ROOT}/.git:{ROOT}/.git:ro",
            "-v", "/home/josh/d/laputa-systems/laputa:/home/josh/d/laputa-systems/laputa:ro",
            "-v", f"{BASELINE_BINS}:{BASELINE_BINS}:ro",
            "-v", f"{target_dir}:{target_dir}:ro",
            "-w", str(ROOT),
            "-e", "LC_ALL=C", "-e", "TZ=UTC", "-e", "TMPDIR=/tmp",
            "-e", f"CARGO_TARGET_DIR={target_dir}",
            "-e", "HOME=/home/compat", "-e", "RUSTUP_HOME=/root/.rustup",
            "-e", f"TARGET={TARGET}", "-e", "DIST_PROFILE=verification",
            "-e", f"XSH_BIN={self.target_dir / TARGET / 'verification' / 'xsh'}",
            "-e", "XSH_LAPUTA_CORPUS=/home/josh/d/laputa-systems/laputa",
            "-e", f"PATH={self.target_dir / TARGET / 'verification'}:{BASELINE_BINS}:/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        ])
        args.extend(["--user", user, IMAGE, *command])
        self.run(step_id, args, timeout_seconds=timeout, cidfile=cidfile)

    def native(self, step_id: str, file: str, *, forced: bool = False) -> None:
        xsht = self.target_dir / TARGET / "verification" / "xsht"
        args = [str(xsht), "test", file, "-j", "1"]
        if forced:
            args.append("--force-frames")
        self.run_native(step_id, "run", ["/usr/bin/env", *[f"{key}={value}" for key, value in self.container_env().items()], *map(str, args)], timeout=1800)

    def execute(self, plan: dict[str, Any]) -> None:
        self.require_native_lock()
        self.verify_toolchain_inputs(plan)
        image_check = self.run(
            "verify-pinned-image-id",
            ["docker", "image", "inspect", "--format", "{{.Id}}", IMAGE],
            timeout_seconds=30,
        )
        image_id = pathlib.Path(image_check["log"]).read_text(errors="replace").splitlines()
        if not image_id or image_id[-1].strip() != IMAGE:
            raise GateFailure("the pinned Docker image is missing or has a mismatched ID; image builds are not part of this launcher")

        snapshot = self.verify_source("before-build")
        self.receipt["hashes"]["source_before"] = {"head": snapshot["head"], "tree": snapshot["tree"]}
        self.receipt["hashes"]["binaries_before_build"] = self.binary_hashes()
        self.receipt["product_variants"] = plan["product_variants"]
        self.save()

        for index, product in enumerate(plan["products"], start=1):
            packages = [product[position + 1] for position, item in enumerate(product[:-1]) if item == "-p"]
            product_name = "-".join(packages) if packages else "bin"
            self.run_native(f"build-product-{index:02d}-{product_name}", "build", product, timeout=7200)

        products_after_initial_build = self.binary_hashes()
        required_binaries = {
            "xsh-verification",
            "xsht-verification",
            "xshi-verification",
            "xsh-fuzz-verification",
            "xsh-test-helper-verification",
        }
        missing_binaries = sorted(required_binaries - products_after_initial_build.keys())
        if missing_binaries:
            raise GateFailure(f"required candidate binaries missing after product builds: {missing_binaries}")
        self.receipt["hashes"]["products_after_initial_build"] = products_after_initial_build
        self.save()

        self.build_harnesses(plan)
        self.record_product_hashes("products_after_harness_builds", products_after_initial_build)

        for index, clippy in enumerate(plan["warning_deny"], start=1):
            self.run_native(f"warning-deny-{index:02d}", "lint", clippy, timeout=7200)

        binaries_built = self.binary_hashes()
        if binaries_built != products_after_initial_build:
            raise GateFailure("candidate product set changed after the Clippy matrix")
        self.receipt["hashes"]["binaries_after_build"] = binaries_built
        self.receipt["hashes"]["binaries_before_tests"] = binaries_built
        self.receipt["hashes"]["harnesses_before_tests"] = {
            name: {target: value["sha256"] for target, value in entry["targets"].items()}
            for name, entry in self.receipt["harnesses"].items()
        }
        self.receipt["hashes"]["harness_variant_binaries_before_tests"] = {
            name: self.target_binary_hashes(pathlib.Path(entry["target_dir"]))
            for name, entry in self.receipt["harnesses"].items()
        }
        self.save()

        try:
            for index, special in enumerate(plan["linux_context_runs"], start=1):
                if special.get("harness"):
                    harness_set = self.receipt["harnesses"][special["harness"]]
                    target_name, target_entry = next(iter(harness_set["targets"].items()))
                    executable = pathlib.Path(target_entry["path"])
                    target_dir = pathlib.Path(harness_set["target_dir"])
                    command = [str(executable), "--nocapture", "--test-threads=1"]
                    if special.get("filter"):
                        command.insert(1, special["filter"])
                    if special.get("skip_filter"):
                        command.extend(["--skip", special["skip_filter"]])
                    self.run_special_container(
                        f"linux-special-{index:02d}-{special['harness']}",
                        command,
                        target_dir=target_dir,
                        privileged=special["privileged"],
                        memory=special["memory"],
                        cpus=special.get("cpus", "12"),
                        user=special.get("user", "0:0"),
                        network=special.get("network", "none"),
                        timeout=1800,
                    )
                else:
                    binary = self.target_dir / TARGET / "verification" / special["binary"]
                    task_id = special.get("id", f"binary-{special['binary']}")
                    self.run_special_container(
                        f"linux-special-{index:02d}-{task_id}",
                        ["/usr/bin/env", f"CARGO_TARGET_DIR={self.target_dir}", f"XSH_BIN={self.target_dir / TARGET / 'verification' / 'xsh'}", f"PATH={self.target_dir / TARGET / 'verification'}:/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin", str(binary), *special["argv"]],
                        target_dir=self.target_dir,
                        privileged=special["privileged"],
                        memory=special["memory"],
                        user=special.get("user", "0:0"),
                        network=special.get("network", "none"),
                        timeout=900,
                    )

            self.receipt["hashes"]["binaries_after_tests"] = self.binary_hashes()
            current_harnesses = {
                name: {target: sha256(pathlib.Path(value["path"])) for target, value in entry["targets"].items()}
                for name, entry in self.receipt["harnesses"].items()
            }
            self.receipt["hashes"]["harnesses_after_tests"] = current_harnesses
            self.receipt["hashes"]["harness_variant_binaries_after_tests"] = {
                name: self.target_binary_hashes(pathlib.Path(entry["target_dir"]))
                for name, entry in self.receipt["harnesses"].items()
            }
            self.save()
            if self.receipt["hashes"]["binaries_before_tests"] != self.receipt["hashes"]["binaries_after_tests"]:
                raise GateFailure("a verification-profile binary hash changed during tests")
            if current_harnesses != self.receipt["hashes"]["harnesses_before_tests"]:
                raise GateFailure("a test harness hash changed during tests")
            for harness_id, before in self.receipt["hashes"]["harness_variant_binaries_before_tests"].items():
                after = self.receipt["hashes"]["harness_variant_binaries_after_tests"][harness_id]
                if after != before:
                    raise GateFailure(f"test modified the isolated {harness_id} binary variant")
            self.run("tracked-diff-check", ["git", "diff", "--check"])
        finally:
            self.release_native_lock()

        self.verify_source("after-all-gates")
        self.receipt["hashes"]["frozen_baseline_after"] = {
            name: sha256(BASELINE_BINS / name) for name in BASELINE_HASHES
        }
        self.save()
        if self.receipt["hashes"]["frozen_baseline_after"] != BASELINE_HASHES:
            raise GateFailure("a frozen baseline binary changed during verification")
        self.receipt["hashes"]["source_after"] = self.verify_source("final")
        self.receipt["status"] = "commands-passed-needs-privilege-skip-and-close-review"
        self.receipt["finished_utc"] = utc_now()
        self.save()


def load_plan() -> dict[str, Any]:
    plan = json.loads(PLAN_PATH.read_text(), object_pairs_hook=unique_json_object)
    actual_body = manifest_body_sha256(plan)
    if actual_body != APPROVED_MANIFEST_BODY_SHA256:
        raise GateFailure(f"approved manifest body changed: {actual_body} != {APPROVED_MANIFEST_BODY_SHA256}")
    if sha256(HERE / "launch.py") != plan["environment"]["launch_sha256"]:
        raise GateFailure("launcher hash does not match its one-way manifest self-hash")
    return plan


def print_plan(plan: dict[str, Any]) -> None:
    print(json.dumps({
        "source_binding": plan["source"],
        "target": plan["environment"]["target"],
        "image": plan["environment"]["image"],
        "product_builds": len(plan["products"]),
        "rust_harness_builds": len(plan["rust_harness_builds"]),
        "rust_target_variants": sorted({item["target_variant"] for item in plan["rust_harness_builds"]}),
        "rust_harness_runs": len(plan["rust_harness_runs"]),
        "affected_native_files": len(plan["focused_native_files"]),
        "affected_native_invocations": len(plan["focused_native_files"]) * 2,
        "additional_native_invocations": len(plan["native_extra"]),
        "linux_privilege_context_invocations": len(plan["linux_context_runs"]),
        "warning_deny_matrix": len(plan["warning_deny"]),
        "native_exclusion_lock": str(NATIVE_LOCK),
        "native_lock_scope": plan["environment"]["native_lock_scope"],
        "cleanup_marker": str(NATIVE_CLEANUP_MARKER),
        "cleanup_policy": plan["environment"]["cleanup_verification"],
        "launcher_self_tests": plan["launcher_self_tests"],
        "broad_batch_owner": "cargo dev gate --batch",
        "full_native_suite": "owned by cargo dev gate --batch; no repeat here",
        "non_goals": plan["non_goals"],
    }, indent=2))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--plan", action="store_true", help="print the prepared gate inventory without building or testing")
    mode.add_argument("--execute", action="store_true", help="run gates against the exact clean sealed source supplied below")
    parser.add_argument("--expected-commit")
    parser.add_argument("--expected-tree")
    parser.add_argument("--out", type=pathlib.Path)
    args = parser.parse_args()
    try:
        plan = load_plan()
    except (GateFailure, OSError, json.JSONDecodeError) as error:
        parser.error(f"invalid prepared gate manifest: {error}")
    if args.plan:
        print_plan(plan)
        return 0
    if not args.expected_commit or not args.expected_tree or args.out is None:
        parser.error("--execute requires --expected-commit, --expected-tree, and --out")
    if len(args.expected_commit) != 40 or len(args.expected_tree) != 40:
        parser.error("source commit and tree must be full 40-character Git IDs")
    out = args.out.resolve()
    if out.parent != HERE.resolve():
        parser.error("--out must be a direct child of .work/final-review-evidence/final-correctness-gates")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", out.name):
        parser.error("--out's final component must contain only letters, digits, dot, underscore, and dash")
    if out.exists():
        parser.error("--out already exists; receipts are never overwritten")
    out.mkdir(parents=True)
    launch = Launch(args.expected_commit, args.expected_tree, out)
    launch.save()
    previous_handlers: dict[int, Any] = {}

    def interrupt(signum: int, _frame: Any) -> None:
        if launch.interrupt_started:
            return
        launch.interrupt_started = True
        name = signal.Signals(signum).name
        raise GateInterrupted(f"received {name}")

    for signum in (signal.SIGINT, signal.SIGTERM):
        previous_handlers[signum] = signal.signal(signum, interrupt)
    try:
        launch.execute(plan)
    except GateInterrupted as error:
        launch.receipt["status"] = "incomplete-cleanup" if NATIVE_CLEANUP_MARKER.exists() else "interrupted"
        launch.receipt["failure"] = str(error)
        launch.receipt["finished_utc"] = utc_now()
        launch.save()
        print(f"gate launch interrupted: {error}", file=sys.stderr)
        return 130
    except (GateFailure, OSError, subprocess.SubprocessError) as error:
        launch.receipt["status"] = "incomplete-cleanup" if NATIVE_CLEANUP_MARKER.exists() else "failed-or-incomplete"
        launch.receipt["failure"] = str(error)
        launch.receipt["finished_utc"] = utc_now()
        launch.save()
        print(f"gate launch stopped: {error}", file=sys.stderr)
        return 1
    finally:
        launch.release_native_lock()
        for signum, handler in previous_handlers.items():
            signal.signal(signum, handler)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
