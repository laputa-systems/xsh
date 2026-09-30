"""Collect reproducible process observations and resource measurements."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import signal
import statistics
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
PHASES = [
    "source_load_parse", "name_dependency_resolution", "constraint_generation",
    "type_row_operation_effect_solving", "generalization_interfaces",
    "finalization_consumer_views", "indexed_lower_evidence_verify", "execution",
]
COUNTERS = [
    "type_nodes", "row_nodes", "scheme_nodes", "constraint_nodes", "reason_nodes",
    "unifications", "occurs_visits", "level_visits", "instantiations", "row_visits",
    "candidate_tests", "rejected_candidate_tests", "queue_visits", "wakeups",
    "revisits", "effect_edge_visits", "source_visits", "body_visits", "module_loads",
    "module_solves", "interface_instantiations", "flow_fixed_point_visits",
    "diagnostic_rendering_visits",
]
PRODUCT_PHASE_METRICS = {"preparation_ms", "execution_ms", "total_ms"}


def timestamp():
    return datetime.now(timezone.utc).isoformat()


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_files(root, files):
    for entry in files:
        path = (Path(root) / entry["path"]).resolve()
        if not path.is_relative_to(Path(root).resolve()):
            raise ValueError(f"input escapes frozen source root: {entry['path']}")
        actual = sha256_file(path)
        if actual != entry["sha256"]:
            raise ValueError(f"frozen input changed: {entry['path']} expected {entry['sha256']}, got {actual}")


def statistics_for(values, final=False):
    values = list(values)
    if final and len(values) != 200:
        raise ValueError("final statistics require exactly 200 accepted observations")
    return {
        "sample_count": len(values),
        "median": statistics.median(values) if values else None,
        "p95": sorted(values)[math.ceil(0.95 * len(values)) - 1] if final else None,
        "p95_status": "passed" if final else "not-run",
        "p95_reason": None if final else "pilot/observation sets do not establish final tail evidence",
        "raw_samples": values,
    }


def rss_bytes(raw, system):
    if system == "Darwin":
        return raw
    if system == "Linux":
        return raw * 1024
    return None


class ProcessDeadline(Exception):
    pass


def run_process(command, cwd, timeout, capture, expected, env, stdin_path=None, max_rss_bytes=None, product_measurement=None):
    """Redirect to files so arbitrarily full pipes cannot deadlock wait/reap."""
    if timeout <= 0 or not math.isfinite(timeout):
        raise ValueError("timeout must be finite and positive")
    capture = Path(capture)
    capture.parent.mkdir(parents=True, exist_ok=True)
    report_path = capture.with_suffix(".json")
    if product_measurement is not None:
        if product_measurement.get("report") is not True or not product_measurement.get("metric_names") or not set(product_measurement["metric_names"]).issubset(PRODUCT_PHASE_METRICS):
            raise ValueError("product_measurement requires a report and supported phase metric names")
        report_path.unlink(missing_ok=True)
    stdout_path = capture.with_suffix(".stdout")
    stderr_path = capture.with_suffix(".stderr")
    error_read, error_write = os.pipe()
    os.set_inheritable(error_write, False)
    started_at = timestamp()
    load_before = list(os.getloadavg())
    reasons = []
    timed_out = False
    interrupted = False
    with open(stdout_path, "wb") as out, open(stderr_path, "wb") as err, open(stdin_path or os.devnull, "rb") as child_input:
        started = time.perf_counter_ns()
        pid = os.fork()
        if pid == 0:
            try:
                os.close(error_read)
                os.setsid()
                os.dup2(child_input.fileno(), 0)
                os.dup2(out.fileno(), 1)
                os.dup2(err.fileno(), 2)
                os.chdir(cwd)
                os.execvpe(command[0], command, env)
            except BaseException as error:
                os.write(error_write, str(error).encode("utf-8", errors="replace")[:2000])
                os._exit(127)
        os.close(error_write)
        def deadline(signum, frame):
            raise ProcessDeadline()
        previous_handler = signal.signal(signal.SIGALRM, deadline)
        previous_timer = signal.setitimer(signal.ITIMER_REAL, timeout)
        try:
            _, wait_status, usage = os.wait4(pid, 0)
        except ProcessDeadline:
            timed_out = True
        except KeyboardInterrupt:
            interrupted = True
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            signal.signal(signal.SIGALRM, previous_handler)
            if previous_timer[0] or previous_timer[1]:
                signal.setitimer(signal.ITIMER_REAL, *previous_timer)
            if timed_out or interrupted:
                try:
                    os.killpg(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                # setsid may not have run before an extremely short deadline.
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                _, wait_status, usage = os.wait4(pid, 0)
            elapsed = (time.perf_counter_ns() - started) / 1e6
            exec_error = os.read(error_read, 2000).decode("utf-8", errors="replace")
            os.close(error_read)
    exit_status = os.WEXITSTATUS(wait_status) if os.WIFEXITED(wait_status) else None
    child_signal = os.WTERMSIG(wait_status) if os.WIFSIGNALED(wait_status) else None
    if timed_out:
        reasons.append("timeout")
    if interrupted:
        reasons.append("external-interruption")
    if exec_error:
        reasons.append("harness-exec-failure")
    if child_signal is not None:
        reasons.append("process-signal")
    if exit_status != expected["exit_status"]:
        reasons.append("exit-status-mismatch")
    peak_rss = rss_bytes(usage.ru_maxrss, platform.system())
    if max_rss_bytes is not None and peak_rss is not None and peak_rss > max_rss_bytes:
        reasons.append("resource-limit-exceeded")
    fingerprints = {}
    for name, path in [("stdout", stdout_path), ("stderr", stderr_path)]:
        digest = sha256_file(path)
        fingerprints[f"{name}_sha256"] = digest
        fingerprints[f"{name}_bytes"] = path.stat().st_size
        wanted = expected.get(f"{name}_sha256")
        if f"{name}_utf8" in expected:
            wanted = hashlib.sha256(expected[f"{name}_utf8"].encode()).hexdigest()
        if wanted is not None and digest != wanted:
            reasons.append(f"{name}-mismatch")
        contents = None
        if expected.get(f"{name}_contains") is not None or expected.get(f"{name}_pattern") is not None:
            contents = path.read_text(errors="replace")
        required = expected.get(f"{name}_contains", [])
        if isinstance(required, str):
            required = [required]
        if any(text not in contents for text in required) or (expected.get(f"{name}_pattern") is not None and re.search(expected[f"{name}_pattern"], contents, re.MULTILINE) is None):
            if f"{name}-mismatch" not in reasons:
                reasons.append(f"{name}-mismatch")
    phase_metrics = None
    product_report = None
    if product_measurement is not None:
        product_report = {"path": str(report_path), "sha256": None, "native_report": None, "status": "failed", "reason": None}
        try:
            product_report["sha256"] = sha256_file(report_path)
            native = json.loads(report_path.read_text())
            product_report["native_report"] = native
            if not isinstance(native, dict) or type(native.get("schema_version")) is not int or native["schema_version"] != 1:
                raise ValueError("phase report must be an object with schema_version 1")
            if type(native.get("status")) is not int or native["status"] != exit_status:
                raise ValueError("phase report status differs from reaped process status")
            metrics = {name: native.get(name) for name in product_measurement["metric_names"]}
            for name, value in metrics.items():
                if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
                    raise ValueError(f"{name} must be finite and nonnegative")
            for stream in ["stdout", "stderr"]:
                if f"{stream}_bytes" in native and native[f"{stream}_bytes"] != fingerprints[f"{stream}_bytes"]:
                    raise ValueError(f"phase report {stream} size differs from captured output")
            phase_metrics = metrics
            product_report.update(status="passed", reason=None)
        except (OSError, ValueError, OverflowError) as error:
            reasons.append("harness-collection-failure")
            product_report["reason"] = str(error)
    result = {
        "command": command, "cwd": str(cwd), "pid": pid,
        "started_at": started_at, "ended_at": timestamp(), "wall_ms": elapsed,
        "raw_wait_status": wait_status, "exit_status": exit_status, "signal": child_signal,
        "user_cpu_ms": usage.ru_utime * 1000, "system_cpu_ms": usage.ru_stime * 1000,
        "peak_rss_bytes": peak_rss, "max_rss_bytes": max_rss_bytes,
        "raw_maxrss": usage.ru_maxrss,
        "raw_maxrss_unit": "bytes" if platform.system() == "Darwin" else "KiB" if platform.system() == "Linux" else None,
        "collector": "os.wait4 direct-child rusage", "timeout_seconds": timeout,
        "load_before": load_before, "load_after": list(os.getloadavg()),
        "stdout_path": str(stdout_path), "stderr_path": str(stderr_path),
        "status": "failed" if reasons else "passed", "invalid_reasons": reasons,
        "exec_error": exec_error or None, **fingerprints,
        "stdin_path": str(stdin_path) if stdin_path else os.devnull,
        "stdin_sha256": sha256_file(stdin_path) if stdin_path else hashlib.sha256(b"").hexdigest(),
        "environment_sha256": hashlib.sha256(json.dumps(env, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
        "phase_metrics_ms": phase_metrics, "product_report": product_report,
    }
    return result


def collect_pairs(sides, sample_count, execute, emit, warmups=5, max_invalid_pairs=10):
    for pair_id in range(warmups):
        for side in sides if pair_id % 2 == 0 else list(reversed(sides)):
            row = execute(side, "warmup", pair_id, pair_id)
            row.update(side=side, phase="warmup", pair_id=pair_id, attempt_id=pair_id, pair_valid=None)
            emit(row)
            if row["status"] != "passed":
                return {"accepted_pairs": [], "invalid_pairs": 0, "status": "failed", "reason": "warmup observation failed"}
    accepted = []
    invalid = 0
    attempt = 0
    while len(accepted) < sample_count:
        order = sides if attempt % 2 == 0 else list(reversed(sides))
        pair = {}
        for side in order:
            row = execute(side, "sample", len(accepted), attempt)
            row.update(side=side, phase="sample", pair_id=len(accepted), attempt_id=attempt, order=list(order))
            pair[side] = row
        valid = all(row["status"] == "passed" for row in pair.values())
        for row in pair.values():
            row["pair_valid"] = valid
            emit(row)
        if valid:
            accepted.append(pair)
        else:
            invalid += 1
            if invalid >= max_invalid_pairs or any(set(row.get("invalid_reasons", [])) & {"external-interruption", "resource-limit-exceeded"} for row in pair.values()):
                return {"accepted_pairs": accepted, "invalid_pairs": invalid, "status": "failed", "reason": "invalid-pair limit, external interruption, or frozen resource guard"}
        attempt += 1
    return {"accepted_pairs": accepted, "invalid_pairs": invalid, "status": "passed", "reason": None}


def summarize_pairs(pairs, final=False):
    sides = list(pairs[0]) if pairs else []
    result = {"builds": {}}
    for side in sides:
        result["builds"][side] = {
            metric: statistics_for([pair[side][metric] for pair in pairs], final)
            for metric in ["wall_ms", "peak_rss_bytes"]
            if all(pair[side].get(metric) is not None for pair in pairs)
        }
        result["builds"][side]["peak_rss_max_bytes"] = max(pair[side]["peak_rss_bytes"] for pair in pairs) if all(pair[side].get("peak_rss_bytes") is not None for pair in pairs) else None
        phase_names = set.intersection(*(set((pair[side].get("phase_metrics_ms") or {}).keys()) for pair in pairs))
        if phase_names:
            result["builds"][side]["product_phase_metrics"] = {name: statistics_for([pair[side]["phase_metrics_ms"][name] for pair in pairs], final) for name in sorted(phase_names)}
    if sides == ["baseline", "candidate"] or set(sides) == {"baseline", "candidate"}:
        result["paired_wall_differences_ms"] = [pair["candidate"]["wall_ms"] - pair["baseline"]["wall_ms"] for pair in pairs]
        result["paired_wall_ratios"] = [pair["candidate"]["wall_ms"] / pair["baseline"]["wall_ms"] if pair["baseline"]["wall_ms"] else None for pair in pairs]
        result["paired_rss_differences_bytes"] = [pair["candidate"]["peak_rss_bytes"] - pair["baseline"]["peak_rss_bytes"] if pair["baseline"].get("peak_rss_bytes") is not None and pair["candidate"].get("peak_rss_bytes") is not None else None for pair in pairs]
        phase_names = set(result["builds"]["baseline"].get("product_phase_metrics", {})) & set(result["builds"]["candidate"].get("product_phase_metrics", {}))
        if phase_names:
            result["paired_phase_differences_ms"] = {name: [pair["candidate"]["phase_metrics_ms"][name] - pair["baseline"]["phase_metrics_ms"][name] for pair in pairs] for name in sorted(phase_names)}
            result["paired_phase_ratios"] = {name: [pair["candidate"]["phase_metrics_ms"][name] / pair["baseline"]["phase_metrics_ms"][name] if pair["baseline"]["phase_metrics_ms"][name] else None for pair in pairs] for name in sorted(phase_names)}
    return result


def unavailable_profile(reason):
    return {
        "status": "not-run", "reason": reason, "native_report": None,
        "phases": [{"name": name, "wall_ms": None, "allocated_bytes": None, "allocation_count": None, "peak_live_bytes": None, "retained_bytes": None, "status": "not-run", "reason": reason} for name in PHASES],
        "work_counters": dict.fromkeys(COUNTERS),
        "unique_retained_typing_interface_bytes": None,
        "prepared_instruction_bytes": None, "prepared_evidence_bytes": None,
    }


def git_identity(root):
    command = ["git", "-C", str(root), "rev-parse", "HEAD"]
    revision = subprocess.check_output(command, text=True).strip()
    status_command = ["git", "-C", str(root), "status", "--porcelain", "--untracked-files=no"]
    status = subprocess.check_output(status_command, text=True)
    diff_command = ["git", "-C", str(root), "diff", "HEAD", "--binary"]
    diff_digest = hashlib.sha256(subprocess.check_output(diff_command)).hexdigest()
    return {"path": str(root), "revision": revision, "tracked_changes": status.splitlines(), "tracked_diff_sha256": diff_digest, "commands": [command, status_command, diff_command]}


def product_paths(path, tooling, frontend, runtime, product_probe=None, execution_probe=None):
    path = Path(path).resolve()
    directory = path if path.is_dir() else path.parent
    products = {"xsh": directory / "xsh" if path.is_dir() else path, "xsht": Path(tooling).resolve() if tooling else directory / "xsht", "frontend_stats": Path(frontend).resolve() if frontend else directory / "xsh-frontend-stats", "runtime_stats": Path(runtime).resolve() if runtime else directory / "xsh-runtime-stats"}
    if product_probe:
        products["product_probe"] = Path(product_probe).resolve()
    if execution_probe:
        products["execution_probe"] = Path(execution_probe).resolve()
    return {name: {"path": str(product), "sha256": sha256_file(product) if product.is_file() else None} for name, product in products.items()}


def expand_command(template, products, root, workload, report):
    replacements = {name: value["path"] for name, value in products.items()}
    replacements.update(binary=products[workload.get("product", "xsht" if workload.get("kind") in ("frontend", "cold_frontend") else "xsh")]["path"], root=str(root), source=str((root / workload.get("source", "")).resolve()), report=str(report))
    command = []
    for token in template:
        for name, value in replacements.items():
            token = token.replace("{" + name + "}", value)
        if re.search(r"\{[a-z_]+\}", token):
            raise ValueError(f"unknown command placeholder: {token}")
        command.append(token)
    if not command or not Path(command[0]).is_absolute():
        raise ValueError("workload executable must expand to an absolute path")
    if any(token in ("fmt", "lint", "--fix") for token in command[1:]):
        raise ValueError("formatter/linter commands are not authorized")
    return command


def expand_environment(overrides, products, root):
    replacements = {name: value["path"] for name, value in products.items()}
    replacements["root"] = str(root)
    result = {}
    for name, value in overrides.items():
        for placeholder, replacement in replacements.items():
            value = value.replace("{" + placeholder + "}", replacement)
        if re.search(r"\{[a-z_]+\}", value):
            raise ValueError(f"unknown environment placeholder: {name}")
        result[name] = value
    return result


def host_conditions():
    command = ["ps", "-axo", "pid=,comm="]
    try:
        processes = subprocess.check_output(command, text=True).splitlines()
        builds = [line.strip() for line in processes if Path(line.strip().split(maxsplit=1)[-1]).name in ("cargo", "rustc", "clang", "cc", "ld", "lld", "make", "ninja")]
    except (OSError, subprocess.SubprocessError) as error:
        builds = None
        processes = [str(error)]
    return {"system": platform.system(), "release": platform.release(), "machine": platform.machine(), "processor": platform.processor() or None, "cpu_count": os.cpu_count(), "load": list(os.getloadavg()), "build_process_observation": builds, "build_process_command": command, "thermal_condition": None, "power_condition": None, "filesystem_caches_flushed": False}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--baseline", required=True, help="absolute xsh product or product directory")
    parser.add_argument("--candidate")
    parser.add_argument("--baseline-source", required=True)
    parser.add_argument("--candidate-source")
    for side in ["baseline", "candidate"]:
        parser.add_argument(f"--{side}-tooling")
        parser.add_argument(f"--{side}-frontend-stats")
        parser.add_argument(f"--{side}-runtime-stats")
        parser.add_argument(f"--{side}-product-probe")
        parser.add_argument(f"--{side}-execution-probe")
    parser.add_argument("--out", required=True, help="append-only JSONL; raw captures go beside this file")
    parser.add_argument("--mode", choices=["observe", "pilot", "final", "profile"], required=True)
    parser.add_argument("--workload", action="append", help="select exact workload IDs; omission runs the complete manifest")
    parser.add_argument("--conditions", help="JSON containing independently observed thermal/power/background conditions")
    parser.add_argument("--observations-only", action="store_true", help="run manifest observation_workloads; requires mode observe")
    args = parser.parse_args(argv)
    if args.observations_only and args.mode != "observe":
        parser.error("--observations-only requires --mode observe")
    if platform.system() != "Darwin":
        parser.error("campaign execution is authorized only on macOS")
    for path in [args.baseline, args.candidate]:
        if path is not None and not Path(path).is_absolute():
            parser.error("product paths must be absolute")
    manifest_path = Path(args.manifest).resolve()
    root = manifest_path.parent
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("schema_version") != 1:
        parser.error("manifest schema_version must be 1")
    policy_path = HERE / "measurement-policy.json"
    policy = json.loads(policy_path.read_text())
    frozen = manifest.get("source", {}).get("files", manifest.get("files", []))
    frozen += [entry for workload in manifest["workloads"] + manifest.get("observation_workloads", []) for entry in workload.get("inputs", [])]
    if not frozen:
        parser.error("manifest must identify frozen input files and SHA-256 hashes")
    verify_files(root, frozen)
    identities = {"baseline": git_identity(Path(args.baseline_source).resolve())}
    if not identities["baseline"]["revision"].startswith("877a114d") or identities["baseline"]["tracked_changes"]:
        parser.error("baseline must be immutable clean revision 877a114d")
    if args.candidate:
        if not args.candidate_source:
            parser.error("--candidate-source is required with --candidate")
        identities["candidate"] = git_identity(Path(args.candidate_source).resolve())
    products = {side: product_paths(getattr(args, side), getattr(args, f"{side}_tooling"), getattr(args, f"{side}_frontend_stats"), getattr(args, f"{side}_runtime_stats"), getattr(args, f"{side}_product_probe"), getattr(args, f"{side}_execution_probe")) for side in identities}
    if args.observations_only and "observation_workloads" not in manifest:
        parser.error("manifest has no observation_workloads")
    workloads = manifest["observation_workloads"] if args.observations_only else manifest["workloads"]
    if args.workload:
        missing = set(args.workload) - {workload["id"] for workload in workloads}
        if missing:
            parser.error(f"unknown workload IDs: {sorted(missing)}")
        workloads = [workload for workload in workloads if workload["id"] in args.workload]
    run_id = uuid.uuid4().hex
    output = Path(args.out).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    captures = output.parent / (output.stem + "-captures") / run_id
    conditions = host_conditions()
    if args.conditions:
        conditions["provided_conditions"] = json.loads(Path(args.conditions).read_text())
        conditions["provided_conditions_sha256"] = sha256_file(args.conditions)
    provenance = {"manifest_sha256": sha256_file(manifest_path), "policy_sha256": sha256_file(policy_path), "harness_sha256": sha256_file(__file__), "source_identities": identities, "products": products, "build_settings": manifest.get("build_settings"), "frozen_inputs": frozen, "conditions": conditions, "argv": [sys.executable, str(Path(__file__).resolve()), *(argv if argv is not None else sys.argv[1:])]}
    failed = False
    with open(output, "a", encoding="utf-8") as handle:
        def emit(record):
            record.update(schema_version=1, run_id=run_id, mode=args.mode)
            handle.write(json.dumps(record, sort_keys=True) + "\n")
            handle.flush()
        emit({"record_type": "run_start", "started_at": timestamp(), "provenance": provenance})
        if args.mode == "final" and conditions["build_process_observation"] is None:
            emit({"record_type": "run_end", "status": "not-run", "reason": "background build observation unavailable", "ended_at": timestamp()})
            return 1
        if args.mode == "final" and conditions["build_process_observation"]:
            emit({"record_type": "run_end", "status": "not-run", "reason": "concurrent build processes observed; final set requires quiet environment", "ended_at": timestamp()})
            return 1
        for workload in workloads:
            workload_id = workload["id"]
            applicability = workload.get("applicability", {"status": "applicable", "reason": None})
            if applicability["status"] != "applicable":
                emit({"record_type": "workload", "workload_id": workload_id, "status": "not-applicable", "reason": applicability.get("reason")})
                continue
            template = workload.get("commandTemplate", workload.get("command"))
            expected = workload["expected"]
            if "exit_status" not in expected:
                raise ValueError(f"{workload_id}: expected exit_status is required")
            cwd = (root / workload.get("cwd", ".")).resolve()
            limits = manifest.get("resource_limits", {})
            timeout = workload.get("timeout_seconds", limits.get("product_process_wall_seconds", 30))
            max_rss = workload.get("max_rss_bytes", limits.get("peak_process_rss_bytes"))
            environment = os.environ.copy()
            for inherited in ["XSH_MODULE_PATH", "XSH_COVERAGE_TRACE_DIR"]:
                environment.pop(inherited, None)
            stdin_path = (root / workload["stdin_file"]).resolve() if workload.get("stdin_file") else None
            if stdin_path is not None and not any((root / entry["path"]).resolve() == stdin_path for entry in frozen):
                raise ValueError(f"{workload_id}: stdin_file must be a frozen hashed input")
            if "stdin_utf8" in workload:
                if stdin_path is not None:
                    raise ValueError("workload must specify only one stdin source")
                stdin_path = captures / workload_id / "input.stdin"
                stdin_path.parent.mkdir(parents=True, exist_ok=True)
                stdin_path.write_bytes(workload["stdin_utf8"].encode())
            def execute(side, phase, pair_id, attempt):
                capture = captures / workload_id / f"{phase}-{attempt:04d}-{side}"
                command = expand_command(template, products[side], root, workload, capture.with_suffix(".json"))
                overrides = expand_environment(workload.get("env", {}), products[side], root)
                side_environment = {**environment, **overrides}
                try:
                    row = run_process(command, cwd, timeout, capture, expected, side_environment, stdin_path, max_rss, workload.get("product_measurement"))
                except (OSError, ValueError) as error:
                    row = {"status": "failed", "invalid_reasons": ["harness-collection-failure"], "reason": str(error), "wall_ms": None, "peak_rss_bytes": None, "command": command, "cwd": str(cwd)}
                row.update(record_type="process", workload_id=workload_id, variant=workload.get("variant", "stabilized"), measurement_kind=workload.get("kind", workload.get("measurement_kind")), expected=expected, environment_overrides=overrides)
                return row
            if args.mode == "profile":
                for side in identities:
                    overrides = expand_environment(workload.get("env", {}), products[side], root)
                    side_environment = {**environment, **overrides}
                    instrumented = workload.get("instrumented", {})
                    for profiler in ["frontend", "runtime"]:
                        profile = unavailable_profile("instrumented command or product unavailable")
                        profile.update(record_type="profile", workload_id=workload_id, side=side, profiler=profiler)
                        profile_template = instrumented.get(f"{profiler}_command")
                        product_name = f"{profiler}_stats"
                        if profile_template and products[side][product_name]["sha256"] is not None:
                            capture = captures / workload_id / f"profile-{profiler}-{side}"
                            report = capture.with_suffix(".json")
                            command = expand_command(profile_template, products[side], root, workload, report)
                            profile_expected = expected if profiler == "runtime" else {"exit_status": 0}
                            process = run_process(command, cwd, timeout, capture, profile_expected, side_environment, stdin_path, max_rss)
                            profile["process"] = process
                            try:
                                native = json.loads((report if profiler == "runtime" else Path(process["stdout_path"])).read_text())
                                profile.update(native_report=native, status=process["status"], reason="native report collected; separate absent phase/work metrics remain unavailable")
                                profile["retention_accounting"] = "legacy representation estimator; not unique retained typing/interface bytes" if profiler == "frontend" else "thread-attributed allocation traffic; worker pressure is not process RSS"
                            except (OSError, ValueError) as error:
                                profile.update(status="failed", reason=f"native report collection failed: {error}")
                        failed |= profile["status"] == "failed"
                        emit(profile)
                continue
            if args.mode == "observe":
                for side in identities:
                    row = execute(side, "observation", 0, 0)
                    row.update(side=side, phase="observation", pair_id=None, attempt_id=0, pair_valid=None)
                    emit(row)
                    failed |= row["status"] != "passed"
            else:
                count = policy["final_pairs"] if args.mode == "final" else policy["pilot_pairs"]
                result = collect_pairs(list(identities), count, execute, emit, policy["warmups_per_build_workload"], policy["max_invalid_pairs_per_workload"])
                failed |= result["status"] != "passed"
                summary = summarize_pairs(result["accepted_pairs"], final=args.mode == "final" and result["status"] == "passed")
                emit({"record_type": "summary", "workload_id": workload_id, "status": result["status"], "reason": result["reason"], "invalid_pairs": result["invalid_pairs"], **summary})
            print(f"{args.mode}: collected {workload_id}", flush=True)
        try:
            verify_files(root, frozen)
            if sha256_file(manifest_path) != provenance["manifest_sha256"]:
                raise ValueError("manifest changed during collection")
            if sha256_file(policy_path) != provenance["policy_sha256"] or sha256_file(__file__) != provenance["harness_sha256"]:
                raise ValueError("measurement policy or harness changed during collection")
            for side, identity in identities.items():
                if git_identity(Path(identity["path"])) != identity:
                    raise ValueError(f"{side} source identity changed during collection")
            for side, side_products in products.items():
                for name, product in side_products.items():
                    if product["sha256"] is not None and sha256_file(product["path"]) != product["sha256"]:
                        raise ValueError(f"{side} {name} binary changed during collection")
        except (OSError, ValueError) as error:
            failed = True
            emit({"record_type": "provenance_check", "status": "failed", "reason": str(error), "invalid_reasons": ["provenance-changed"]})
        emit({"record_type": "run_end", "ended_at": timestamp(), "status": "failed" if failed else "passed", "reason": "collection/observation status only; campaign acceptance evaluated independently"})
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
