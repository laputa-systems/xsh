#!/usr/bin/env python3
"""Record a release binary and run the bounded final comparative gate."""

import argparse
from collections import Counter
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import signal
import statistics
import subprocess
import sys
import time

sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

from owned_containers import (
    InterruptedRun,
    SignalReceived,
    cleanup_container_process,
    cleanup_mount_container,
    cleanup_window,
    install_signal_handlers,
    restore_signal_handlers,
    wait_for_owned_process,
)

ROOT = Path("/home/josh/d/laputa-systems/xsh")
INFRA = ROOT / ".work/consolidation/infra"
TARGET = "x86_64-unknown-linux-musl"
RELEASE_OPT_LEVEL = 3
TOOLS = ("xsh", "dash", "bash", "nushell", "ysh", "elvish", "python")
WORKLOADS = ("startup", "spawn", "pipeline", "directory", "json")
BASELINE_BINARY_PATHS = {
    "dash": "/opt/comparators/bin/dash",
    "bash": "/bin/bash",
    "nushell": "/opt/comparators/bin/nu",
    "ysh": "/opt/comparators/bin/ysh",
    "elvish": "/opt/comparators/bin/elvish",
    "python": "/usr/bin/python3",
}
PARITY_LOCK = INFRA / "comparative-final.lock"
OWNERSHIP_LOCK = INFRA / "final-performance.lock"


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def json_write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


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


def run_logged(command, log, *, scratch, output_parent, image_id, verify_after=None):
    precheck = perf_gate_check()
    if precheck["exit_code"] != 0:
        return None, {
            "accepted": False,
            "discarded": True,
            "wall_ns": None,
            "before": precheck,
            "after": None,
        }
    process = None
    validation = None
    validation_error = None
    try:
        with log.open("wb") as output:
            started = time.perf_counter_ns()
            process = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
            status, _, _ = wait_for_owned_process(
                process,
                lambda: cleanup_mount_container(scratch=scratch, output_parent=output_parent, image_id=image_id),
            )
            elapsed = time.perf_counter_ns() - started
            postcheck = perf_gate_check(after=True)
            if verify_after is not None:
                try:
                    validation = verify_after()
                except Exception as error:
                    validation_error = str(error)
    except InterruptedRun as error:
        return None, {
            "accepted": False,
            "discarded": True,
            "interrupted": True,
            "signal": signal.Signals(error.signum).name,
            "wall_ns": None,
            "before": precheck,
            "after": None,
            "cleanup": error.cleanup,
        }
    except SignalReceived as error:
        cleanup_fn = lambda: cleanup_mount_container(scratch=scratch, output_parent=output_parent, image_id=image_id)
        if process is not None and process.poll() is None:
            cleanup = cleanup_container_process(process, cleanup_fn)
        else:
            with cleanup_window():
                cleanup = cleanup_fn()
        return None, {
            "accepted": False,
            "discarded": True,
            "interrupted": True,
            "signal": signal.Signals(error.signum).name,
            "wall_ns": None,
            "before": precheck,
            "after": None,
            "cleanup": cleanup,
        }
    cleanup = cleanup_mount_container(scratch=scratch, output_parent=output_parent, image_id=image_id)
    accepted = postcheck["exit_code"] == 0 and cleanup["resolved"] and validation_error is None
    return status, {
        "accepted": accepted,
        "discarded": not accepted,
        "wall_ns": elapsed if accepted else None,
        "timing_claim_eligible": True,
        "before": precheck,
        "after": postcheck,
        "cleanup": cleanup,
        "post_stage_validation": validation,
        "post_stage_validation_error": validation_error,
    }


def source_state(tree, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("source commit must be a full lowercase Git ID")
    head = subprocess.check_output(["git", "-C", str(tree), "rev-parse", "HEAD"], text=True).strip()
    if head != commit:
        raise ValueError(f"source tree HEAD {head} differs from requested commit {commit}")
    tree_id = subprocess.check_output(["git", "-C", str(tree), "rev-parse", "HEAD^{tree}"], text=True).strip()
    status = subprocess.check_output(["git", "-C", str(tree), "status", "--porcelain=v1", "--untracked-files=all"], text=True)
    if status:
        raise ValueError("source tree must be clean before final acceptance")
    return {"commit": head, "git_tree": tree_id, "status": "clean"}


def close_record(path, commit):
    close = json.loads(path.read_text())
    if close.get("functional_complete") is not True or close.get("commit") != commit:
        raise ValueError("functional-close record must name the accepted source commit")
    return close


def native_image():
    wrapper = INFRA / "run-native.sh"
    info = json.loads(subprocess.check_output(["docker", "image", "inspect", "xsh-test:consolidation-native"]))[0]
    pinned = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
    if info["Id"] != pinned or info["Architecture"] != "amd64" or pinned not in wrapper.read_text():
        raise ValueError("native build image or run-native pin differs from the accepted x86_64 image")
    return {"image_id": info["Id"], "architecture": info["Architecture"], "wrapper_sha256": digest(wrapper)}


def no_native_containers(image_id):
    active = subprocess.check_output(["docker", "ps", "--filter", "ancestor=" + image_id, "--format", "{{.ID}} {{.Names}}"], text=True).strip()
    if active:
        raise ValueError("native test/build containers are active; final measurement needs an isolated window")


def acquire_lock(path, *, reject_unresolved=False):
    path.parent.mkdir(parents=True, exist_ok=True)
    stream = path.open("a+")
    try:
        fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        stream.close()
        raise ValueError(f"another final performance gate owns {path}") from error
    if reject_unresolved:
        stream.seek(0)
        marker = stream.read()
        if marker.startswith("cleanup-unresolved:"):
            stream.close()
            raise ValueError(f"owned-container cleanup remains unresolved in {path}: {marker.strip()}")
    return stream


def retain_unresolved_lock(stream, run_id, cleanup):
    stream.seek(0)
    stream.truncate()
    stream.write("cleanup-unresolved:" + json.dumps({"run_id": run_id, "cleanup": cleanup}, sort_keys=True) + "\n")
    stream.flush()
    os.fsync(stream.fileno())


def terminal_receipt(receipt, receipt_path, ownership, run_id, state, message, cleanup):
    receipt["state"] = state if cleanup["resolved"] else "incomplete-cleanup"
    receipt["error"] = message
    receipt["cleanup"] = cleanup
    receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
    if not cleanup["resolved"]:
        retain_unresolved_lock(ownership, run_id, cleanup)
    json_write(receipt_path, receipt)


def image_metadata(reference, lock):
    info = json.loads(subprocess.check_output(["docker", "image", "inspect", reference]))[0]
    expected_parent = lock["base_images"]["amd64"]
    parent = info.get("Config", {}).get("Labels", {}).get("org.xsh.comparative.base")
    if info["Architecture"] != "amd64" or parent != expected_parent:
        raise ValueError("comparative image is not an amd64 overlay of the pinned native image")
    return {"reference": reference, "image_id": info["Id"], "base_image": parent, "architecture": info["Architecture"]}


def fixture_hash(bench):
    sys.path.insert(0, str(bench))
    from run import validate_fixtures

    return validate_fixtures


def profile_cost_receipt_binding(path, tree, commit, binary):
    receipt_path = path.resolve(strict=True)
    receipt = json.loads(receipt_path.read_text())
    if receipt.get("state") != "complete" or receipt.get("source_commit") != commit:
        raise ValueError("profile-cost receipt must be complete and name the measured source commit")
    if Path(receipt.get("source_tree", "")).resolve() != tree.resolve():
        raise ValueError("profile-cost receipt names a different source tree")
    source_identity = receipt.get("source_identity")
    current_source = source_state(tree, commit)
    if source_identity != current_source:
        raise ValueError("profile-cost receipt source-tree identity differs from the measured tree")
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
        raise ValueError("profile-cost receipt must identify the selected release opt3 profile")
    results = receipt.get("results")
    if not isinstance(results, list) or len(results) != 1 or not isinstance(results[0], dict) or results[0].get("name") != f"release-opt{RELEASE_OPT_LEVEL}":
        raise ValueError("profile-cost receipt must contain exactly the selected release opt3 profile")
    result = results[0]
    if result.get("profile") != "release" or result.get("opt_level") != RELEASE_OPT_LEVEL or result.get("lto") != "thin" or result.get("codegen_units") != 16:
        raise ValueError("selected profile-cost result has an unexpected release profile")
    recorded = result.get("binaries", {}).get("xsh")
    if not recorded or Path(recorded.get("path", "")).resolve() != binary.resolve():
        raise ValueError("profile-cost receipt does not bind the selected release XSH binary")
    observed = {"path": str(binary.resolve()), "sha256": digest(binary)}
    if recorded != observed:
        raise ValueError("release XSH binary differs from its profile-cost result")
    return {
        "path": str(receipt_path),
        "sha256": digest(receipt_path),
        "run_id": receipt.get("run_id"),
        "profile": result.get("name"),
        "binary": observed,
    }


def verify_profile_cost_receipt(binding, tree, commit, binary):
    observed = profile_cost_receipt_binding(Path(binding["path"]), tree, commit, binary)
    if observed != binding:
        raise ValueError("profile-cost receipt or bound release binary changed")
    return observed


def assert_hash_snapshot(expected, observed, label):
    if expected != observed:
        raise ValueError(f"{label} binary hashes changed during the timed stage")
    return {"expected": expected, "observed": observed, "unchanged": True}


def verify_container_tool_hashes(image_id, tools, xsh_binary):
    if set(tools) != set(TOOLS):
        raise ValueError("comparative report must bind all seven measured tool binaries")
    expected = {}
    for tool, path in BASELINE_BINARY_PATHS.items():
        record = tools[tool]
        if record.get("executable") != path or not re.fullmatch(r"[0-9a-f]{64}", record.get("sha256", "")):
            raise ValueError(f"comparative report has an invalid binary identity for {tool}")
        expected[tool] = record["sha256"]
    if tools["xsh"].get("executable") != "/bench/xsh":
        raise ValueError("comparative report does not name the mounted candidate XSH binary")
    xsh_hash = digest(xsh_binary)
    if xsh_hash != tools["xsh"].get("sha256"):
        raise ValueError("candidate XSH binary changed during the comparative measurement")
    script = (
        "import hashlib,json,sys; paths=json.loads(sys.argv[1]); "
        "print(json.dumps({name:hashlib.file_digest(open(path,'rb'),'sha256').hexdigest() "
        "for name,path in paths.items()},sort_keys=True))"
    )
    command = [
        "docker", "run", "--rm", "--network", "none", "--read-only", "--platform", "linux/amd64",
        "--memory", str(4 * 1024**3), "--memory-swap", str(4 * 1024**3),
        "-v", f"{xsh_binary.resolve()}:/bench/xsh:ro",
        image_id, "/usr/bin/python3", "-B", "-c", script, json.dumps(BASELINE_BINARY_PATHS, sort_keys=True),
    ]
    result = subprocess.run(command, text=True, capture_output=True, check=False, timeout=30)
    if result.returncode != 0:
        raise ValueError(f"post-measurement comparator binary check failed: {result.stderr.strip()}")
    try:
        observed = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ValueError("post-measurement comparator binary check returned invalid JSON") from error
    assert_hash_snapshot(expected, observed, "seven-tool baseline")
    return {"baseline": observed, "candidate_xsh": xsh_hash}


def verify_measurement_inputs(
    *, tree, commit, source, binary, binary_hash, binary_receipt_path,
    binary_receipt_hash, profile_binding, functional_close_path,
    functional_close_hash, image_id, report_path,
):
    observed_source = source_state(tree, commit)
    if observed_source != source:
        raise ValueError("source tree identity changed during the timed comparative stage")
    observed_xsh_hash = digest(binary)
    if observed_xsh_hash != binary_hash:
        raise ValueError("candidate XSH binary changed during the timed comparative stage")
    if digest(binary_receipt_path) != binary_receipt_hash:
        raise ValueError("candidate binary receipt changed during the timed comparative stage")
    observed_profile = verify_profile_cost_receipt(profile_binding, tree, commit, binary)
    if digest(functional_close_path) != functional_close_hash:
        raise ValueError("functional-close record changed during the timed comparative stage")
    report = json.loads(report_path.read_text())
    tools = report.get("tools")
    if not isinstance(tools, dict):
        raise ValueError("comparative report is missing the pre-measurement tool binary receipts")
    tool_hashes = verify_container_tool_hashes(image_id, tools, binary)
    return {
        "source": observed_source,
        "candidate_xsh_sha256": observed_xsh_hash,
        "binary_receipt_sha256": binary_receipt_hash,
        "profile_cost_receipt": observed_profile,
        "functional_close_sha256": functional_close_hash,
        "tool_binary_receipts_before": tools,
        "tool_binary_hashes_after": tool_hashes,
    }


def verify_release_binary(tree, commit, binary, receipt, profile_receipt):
    state = source_state(tree, commit)
    profile_binding = profile_cost_receipt_binding(profile_receipt, tree, commit, binary.resolve())
    native = native_image()
    executable = binary.resolve()
    if not executable.is_relative_to(tree.resolve()):
        raise ValueError("release xsh binary must be inside the measured source tree")
    if executable.name != "xsh" or executable.parent.name != "release" or executable.parent.parent.name != TARGET:
        raise ValueError("comparative measurement requires an xsh binary in the x86_64 release profile directory")
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise ValueError("release xsh binary is missing or not executable")
    if receipt.exists():
        raise ValueError("binary receipts are immutable; choose a new receipt path")
    record = {
        "schema": 1,
        "source": state,
        "profile": "release",
        "target": TARGET,
        "binary": str(executable),
        "binary_sha256": digest(executable),
        "cargo_lock_sha256": digest(tree / "Cargo.lock"),
        "cargo_toml_sha256": digest(tree / "Cargo.toml"),
        "rust_toolchain_sha256": digest(tree / "rust-toolchain.toml"),
        "native_build_image": native,
        "profile_cost_receipt": profile_binding,
    }
    json_write(receipt, record)
    return record


def verify_report(report, commit, image, lock, lock_hash, fixtures_hash, binary_hash, close):
    if report.get("complete") is not True or report.get("mode") != "measure":
        raise ValueError("comparative report is incomplete or is not a measurement")
    if report.get("source_commit") != commit or report.get("image_id") != image["image_id"] or report.get("base_image") != image["base_image"]:
        raise ValueError("comparative report source or image identity differs from its inputs")
    if report.get("target") != TARGET or report.get("machine") != "x86_64":
        raise ValueError("comparative report target differs from x86_64 musl acceptance")
    if report.get("lock_sha256") != lock_hash or report.get("fixtures_sha256") != fixtures_hash:
        raise ValueError("comparative report lock or fixture hash differs from its inputs")
    if report["tools"]["xsh"]["sha256"] != binary_hash or report.get("functional_close") != close:
        raise ValueError("comparative report binary or functional-close record differs from its inputs")
    for tool in TOOLS:
        expected_version = commit if tool == "xsh" else lock["versions"][tool]
        if report["tools"][tool]["source_version"] != expected_version:
            raise ValueError(f"tool version input differs from the pin for {tool}")
    resources = report.get("resources", {})
    host_cpus = resources.get("host_allowed_cpus", [])
    cpuset = int(resources.get("cpuset_cpus", "-1"))
    if not host_cpus or cpuset != host_cpus[0] or resources.get("affinity_cpus") != [cpuset] or resources.get("memory_limit_bytes") != 4 * 1024**3 or resources.get("memory_swap_max_bytes") != 0:
        raise ValueError("comparative report did not retain the verified CPU and cgroup resource limits")
    expected_pairs = {(tool, workload) for tool in TOOLS for workload in WORKLOADS}
    parity = report.get("parity", [])
    parity_pairs = {(row["tool"], row["workload"]) for row in parity}
    if len(parity) != 35 or parity_pairs != expected_pairs or any(row.get("equal") is not True for row in parity):
        raise ValueError("final measurement requires all 35 successful parity cases")
    samples = report.get("samples", [])
    if len(samples) != 3780 or any(row.get("equal") is not True for row in samples):
        raise ValueError("final measurement requires 3,780 successful raw samples")
    counts = Counter((row["tool"], row["workload"], row["round"]) for row in samples)
    cycles = {}
    for row in samples:
        cycles.setdefault((row["workload"], row["round"], row["sample"]), []).append(row)
    expected_cycles = {
        (workload, round_index, sample_index)
        for workload in WORKLOADS
        for round_index in range(3)
        for sample_index in range(60 if workload == "startup" else 30)
    }
    if set(cycles) != expected_cycles:
        raise ValueError("raw samples do not cover each expected workload, round, and sample exactly once")
    for (workload, _, _), rows in cycles.items():
        if len(rows) != 7 or {row["tool"] for row in rows} != set(TOOLS) or {row["position"] for row in rows} != set(range(7)):
            raise ValueError(f"sample cycle is incomplete for {workload}")
        orders = {tuple(row["order"]) for row in rows}
        if len(orders) != 1:
            raise ValueError(f"tools in one sample cycle disagree on order for {workload}")
    for tool, workload in expected_pairs:
        expected_per_round = 60 if workload == "startup" else 30
        if any(counts[(tool, workload, round_index)] != expected_per_round for round_index in range(3)):
            raise ValueError(f"unexpected raw sample count for {tool}/{workload}")
        rows = [row for row in samples if row["tool"] == tool and row["workload"] == workload]
        for row in rows:
            if len(row["order"]) != 7 or row["order"][row["position"]] != tool:
                raise ValueError(f"sample order metadata is inconsistent for {tool}/{workload}")
        summary = report["summary"][f"{tool}/{workload}"]
        values = [row["wall_ns"] for row in rows]
        median = statistics.median(values)
        mad = statistics.median(abs(value - median) for value in values)
        rounds = [[row["wall_ns"] for row in rows if row["round"] == index] for index in range(3)]
        if summary["median_ns"] != median or summary["mad_ns"] != mad or summary["round_medians_ns"] != [statistics.median(values) for values in rounds]:
            raise ValueError(f"summary medians do not match raw samples for {tool}/{workload}")


def record_binary(args):
    tree = args.tree.resolve()
    record = verify_release_binary(tree, args.commit, args.xsh, args.receipt, args.profile_cost_receipt)
    print(json.dumps(record, sort_keys=True))


def measure(args):
    tree = args.tree.resolve()
    bench = tree / "bench/consolidation"
    lock_path = bench / "versions.lock.json"
    lock = json.loads(lock_path.read_text())
    source = source_state(tree, args.commit)
    close = close_record(args.functional_close, args.commit)
    binary_receipt = json.loads(args.binary_receipt.read_text())
    binary = args.xsh.resolve()
    if binary_receipt.get("source") != source or binary_receipt.get("profile") != "release" or binary_receipt.get("target") != TARGET:
        raise ValueError("binary receipt must bind this clean source commit to the release target")
    if binary_receipt.get("binary") != str(binary) or binary_receipt.get("binary_sha256") != digest(binary):
        raise ValueError("release binary differs from its recorded path or hash")
    binary_receipt_path = args.binary_receipt.resolve(strict=True)
    binary_receipt_hash = digest(binary_receipt_path)
    profile_binding = binary_receipt.get("profile_cost_receipt")
    if not isinstance(profile_binding, dict):
        raise ValueError("release binary receipt must bind its profile-cost output receipt")
    verify_profile_cost_receipt(profile_binding, tree, args.commit, binary)
    native = native_image()
    if binary_receipt.get("native_build_image") != native:
        raise ValueError("release binary receipt names a different native build image")
    no_native_containers(native["image_id"])
    image = image_metadata(args.image, lock)
    validate_fixtures = fixture_hash(bench)
    fixtures_hash = validate_fixtures(args.fixtures.resolve())
    output = args.output.resolve()
    work_root = (tree / ".work/consolidation").resolve()
    if not output.is_relative_to(work_root) or Path("/tmp") in output.parents:
        raise ValueError("final receipts and raw reports must be under the source tree's .work/consolidation")
    if output.exists():
        raise ValueError("choose a new output directory; prior raw reports are preserved")
    run_id = secrets.token_hex(16)
    ownership = acquire_lock(OWNERSHIP_LOCK, reject_unresolved=True)
    held_locks = [ownership]
    try:
        native_suite = acquire_lock(INFRA / "native-suite.lock")
        held_locks.append(native_suite)
        cargo = acquire_lock(INFRA / "cargo.lock")
        held_locks.append(cargo)
        parity = acquire_lock(PARITY_LOCK)
        held_locks.append(parity)
        no_native_containers(native["image_id"])
        output.mkdir(parents=True)
        report_path = output / "comparative.json"
        scratch = output / "scratch"
        command = [sys.executable, "-B", str(bench / "run.py"), "--image", args.image, "--xsh", str(binary), "--fixtures", str(args.fixtures.resolve()), "--output", str(report_path), "--scratch", str(scratch), "--source-commit", args.commit, "--measure", "--functional-close", str(args.functional_close.resolve())]
        receipt = {
            "schema": 1,
            "state": "running",
            "run_id": run_id,
            "ownership_lock": str(OWNERSHIP_LOCK),
            "native_suite_lock": str(INFRA / "native-suite.lock"),
            "native_suite_lock_held_across_gate": True,
            "source": source,
            "profile": "release",
            "target": TARGET,
            "binary": binary_receipt,
            "binary_receipt": {"path": str(binary_receipt_path), "sha256": binary_receipt_hash},
            "profile_cost_receipt": profile_binding,
            "functional_close": {"path": str(args.functional_close.resolve()), "sha256": digest(args.functional_close.resolve()), "record": close},
            "native_build_image": native,
            "comparative_image": image,
            "lock_sha256": digest(lock_path),
            "fixtures_sha256": fixtures_hash,
            "command": command,
            "run_harness_sha256": digest(bench / "run.py"),
            "expected_parity_cases": 35,
            "expected_timing_samples": 3780,
        }
        receipt_path = output / "receipt.json"
        json_write(receipt_path, receipt)
        try:
            status, timing = run_logged(
                command,
                output / "comparative.log",
                scratch=scratch,
                output_parent=output.parent,
                image_id=image["image_id"],
                verify_after=lambda: verify_measurement_inputs(
                    tree=tree,
                    commit=args.commit,
                    source=source,
                    binary=binary,
                    binary_hash=binary_receipt["binary_sha256"],
                    binary_receipt_path=binary_receipt_path,
                    binary_receipt_hash=binary_receipt_hash,
                    profile_binding=profile_binding,
                    functional_close_path=args.functional_close.resolve(),
                    functional_close_hash=digest(args.functional_close.resolve()),
                    image_id=image["image_id"],
                    report_path=report_path,
                ),
            )
            receipt["timing"] = timing
            receipt["run_status"] = status
            json_write(receipt_path, receipt)
            if timing.get("interrupted"):
                terminal_receipt(
                    receipt,
                    receipt_path,
                    ownership,
                    run_id,
                    "interrupted",
                    f"received {timing['signal']} during comparative measurement",
                    timing["cleanup"],
                )
                return 130
            if timing.get("cleanup", {}).get("resolved") is False:
                terminal_receipt(
                    receipt,
                    receipt_path,
                    ownership,
                    run_id,
                    "incomplete-cleanup",
                    "comparative container cleanup could not be verified",
                    timing["cleanup"],
                )
                return 1
            if not timing["accepted"]:
                reason = timing.get("post_stage_validation_error") or "readiness, cleanup, or source/binary validation failed"
                raise ValueError(f"comparative timing was discarded: {reason}")
            if status != 0:
                raise ValueError(f"comparative runner failed; inspect {output / 'comparative.log'}")
            report = json.loads(report_path.read_text())
            verify_report(report, args.commit, image, lock, digest(lock_path), fixtures_hash, binary_receipt["binary_sha256"], close)
            receipt["state"] = "complete"
            receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
            receipt["comparative_report_sha256"] = digest(report_path)
            receipt["parity_cases"] = len(report["parity"])
            receipt["timing_samples"] = len(report["samples"])
            receipt["tool_versions"] = {tool: report["tools"][tool]["source_version"] for tool in TOOLS}
            receipt["resources"] = report["resources"]
            json_write(receipt_path, receipt)
            print(json.dumps(receipt, sort_keys=True))
        except InterruptedRun as error:
            terminal_receipt(
                receipt, receipt_path, ownership, run_id, "interrupted",
                f"received {signal.Signals(error.signum).name} during comparative measurement",
                error.cleanup,
            )
            return 130
        except SignalReceived as error:
            with cleanup_window():
                cleanup = cleanup_mount_container(scratch=scratch, output_parent=output.parent, image_id=image["image_id"])
            terminal_receipt(
                receipt, receipt_path, ownership, run_id, "interrupted",
                f"received {signal.Signals(error.signum).name} while no owned container was running",
                cleanup,
            )
            return 130
        except Exception as error:
            receipt["state"] = "failed"
            receipt["error"] = str(error)
            receipt["finished_utc"] = datetime.now(timezone.utc).isoformat(timespec="seconds")
            json_write(receipt_path, receipt)
            raise
    finally:
        for stream in reversed(held_locks):
            stream.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    record = commands.add_parser("record-binary")
    record.add_argument("--tree", type=Path, required=True)
    record.add_argument("--commit", required=True)
    record.add_argument("--xsh", type=Path, required=True)
    record.add_argument("--profile-cost-receipt", type=Path, required=True)
    record.add_argument("--receipt", type=Path, required=True)
    record.set_defaults(run=record_binary)
    final = commands.add_parser("measure")
    final.add_argument("--tree", type=Path, required=True)
    final.add_argument("--commit", required=True)
    final.add_argument("--functional-close", type=Path, required=True)
    final.add_argument("--binary-receipt", type=Path, required=True)
    final.add_argument("--xsh", type=Path, required=True)
    final.add_argument("--image", required=True)
    final.add_argument("--fixtures", type=Path, required=True)
    final.add_argument("--output", type=Path, required=True)
    final.set_defaults(run=measure)
    args = parser.parse_args()
    previous = install_signal_handlers()
    try:
        result = args.run(args)
        return 0 if result is None else result
    except SignalReceived as error:
        print(f"interrupted by {signal.Signals(error.signum).name} before an owned container started", file=sys.stderr)
        return 130
    finally:
        restore_signal_handlers(previous)


if __name__ == "__main__":
    raise SystemExit(main())
