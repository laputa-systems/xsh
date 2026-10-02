"""Collect frozen baseline owner-drop evidence without building or executing scripts."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform


MODES = ("full", "compact", "combined-full-first", "combined-compact-first", "prepared", "prepared-symbol-context")
OBSERVATION_FIELDS = ("parse_errors", "full_output_present", "full_check_errors", "diagnostic_fingerprint", "full_expr_types", "source_files", "source_bytes")


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--helper", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--raw", type=Path, required=True)
    parser.add_argument("--resume", action="store_true", help="revalidate and reuse recorded child observations, then collect missing modes")
    args = parser.parse_args()
    bench = Path(__file__).resolve().parent
    helper = args.helper.resolve()
    output = args.output.resolve()
    raw = args.raw.resolve()
    if not args.resume and (output.exists() or raw.exists()):
        raise ValueError("retain prior collection; use new output and raw paths for a rerun")
    spec = importlib.util.spec_from_file_location("typing_measurement", bench / "run.py")
    measurement = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(measurement)
    cohort = json.loads((bench / "cohort-selection.json").read_text())
    contract = json.loads((bench / "retention-accounting.json").read_text())
    limits = json.loads((bench / "resource-limits.json").read_text())
    preflight = {row["id"]: row for row in json.loads((bench / "baseline-prepare-preflight.json").read_text())["rows"]}
    signatures = {row["file"]: row for row in json.loads((bench / "cohort/stabilized-signatures.json").read_text())["runs"]}
    stabilized = bench / "cohort/stabilized"
    environment = os.environ.copy()
    overrides = {"XSH_MODULE_PATH": os.pathsep.join((str(stabilized), str(stabilized / "dev")))}
    environment.update(overrides)
    provenance = contract["verification"]["build_provenance"]
    identities = {"helper_binary": {"path": str(helper), "sha256": provenance["helper_binary_sha256"]},
                  "helper_source": {"path": str(bench / "retained_facts.rs"), "sha256": provenance["helper_source_sha256"]},
                  "library": {"path": provenance["library_path"], "sha256": provenance["library_sha256"]},
                  "library_metadata": {"path": provenance["metadata_path"], "sha256": provenance["metadata_sha256"]}}
    for key, path in (("cohort", bench / "cohort-selection.json"), ("retention_contract", bench / "retention-accounting.json"),
                      ("resource_limits", bench / "resource-limits.json"), ("preparation_preflight", bench / "baseline-prepare-preflight.json"),
                      ("stabilized_signatures", bench / "cohort/stabilized-signatures.json"), ("collector", Path(__file__)),
                      ("process_harness", bench / "run.py")):
        identities[key] = {"path": str(path), "sha256": digest(path)}
    def verify_identities():
        for key, identity in identities.items():
            if digest(identity["path"]) != identity["sha256"]:
                raise ValueError(f"frozen identity changed: {key}")
        for source in cohort["sources"]:
            if digest(stabilized / source["path"]) != source["stabilized_sha256"]:
                raise ValueError(f"frozen stabilized source changed: {source['path']}")
    verify_identities()
    if len(cohort["workloads"]) != 23:
        raise ValueError("expected the 23 frozen source roots")
    report = {
        "schema_version": 1, "status": "running", "started_at": measurement.timestamp(),
        "baseline_revision": cohort["snapshot_commit"], "source_closure_sha256": cohort["source_closure_sha256"],
        "identities": identities, "build_provenance": provenance,
        "platform": platform.platform(), "machine": platform.machine(), "cwd": str(stabilized), "environment_overrides": overrides,
        "timeout_seconds": limits["product_process_wall_seconds"], "peak_rss_guard_bytes": limits["peak_process_rss_bytes"],
        "rss_guard": "evaluated after direct-child wait4 reap; does not prevent transient overshoot",
        "measurement_kind": "profiling-only allocation-request retention; wall time/RSS are collector health observations, excluded from production acceptance samples",
        "metric_contract": {
            "baseline_typing_interface_metric": "exact combined full CheckOutput + CompactDeclOutput + CompactBodyProbeOutput owner-drop heap union, including all fields and duplicated consumer views; an operational whole-finalized-facts metric, not a claim of isolated typing-node bytes",
            "source_and_global_exclusion": "parsed arena, source map and explicit SymbolOwner remain alive during fact destruction, so shared source/symbol backing is not charged repeatedly; later independent owner-drop snapshots expose their net release and final external/global residual",
            "candidate_obligation": "candidate comparison must retain all corresponding finalized solved/interface roots and consumer views, including any new graph/interface pool owners; no retained solved pool may be hidden in source/global exclusions",
            "prepared_at_gate_a": "auxiliary cold-preparation live/peak/traffic and signed teardown observations; opaque artifact typing-only/pool decomposition remains unavailable, never zero",
            "prepared_future_requirement": "prepared instruction/evidence counts and bytes plus after-frontend-drop ownership must be exposed for candidate acceptance; auxiliary opaque live totals alone cannot satisfy those future canonical pool/evidence requirements",
            "request_units": "allocator-request bytes; excludes allocator overhead, rounding, native library allocation and process RSS",
        },
        "raw_directory": str(raw), "workloads": [], "failures": [],
    }
    if args.resume:
        previous = json.loads(output.read_text())
        for key in ("helper_binary", "helper_source", "library", "library_metadata", "cohort", "retention_contract", "resource_limits", "preparation_preflight", "stabilized_signatures", "process_harness"):
            if identities[key] != previous["identities"][key]:
                raise ValueError(f"resume cannot change frozen observation identity: {key}")
        report["collection_attempts"] = [
            {"status": "metadata-validation-failed", "collector_identity": previous["identities"]["collector"],
             "source_snapshot": str(raw / "collector-first-pass.py"), "partial_summary": str(raw / "first-pass-partial-summary.json"),
             "failure_record": str(raw / "first-pass-failure.json"), "started_at": previous["started_at"]},
            {"status": "resumed-from-preserved-native-observations", "collector_identity": identities["collector"], "started_at": report["started_at"]},
        ]
    write_json(output, report)
    for index, workload in enumerate(cohort["workloads"]):
        workload_id = workload["id"]
        entry = stabilized / workload["entry"]
        observations = {}
        processes = []
        failures = []
        verify_identities()
        for mode in MODES:
            command = [str(helper), mode, str(entry)]
            record_path = raw / workload_id / f"{mode}.process.json"
            if args.resume and record_path.exists():
                process = json.loads(record_path.read_text())
                if process["command"] != command or process["cwd"] != str(stabilized) or process["environment_overrides"] != overrides:
                    raise ValueError("preserved process command/context differs from frozen collection")
                for stream in ("stdout", "stderr"):
                    if digest(process[f"{stream}_path"]) != process[f"{stream}_sha256"]:
                        raise ValueError("preserved process stream changed")
            else:
                process = measurement.run_process(command, stabilized, limits["product_process_wall_seconds"], raw / workload_id / mode,
                                                  {"exit_status": 0, "stderr_utf8": ""}, environment,
                                                  max_rss_bytes=limits["peak_process_rss_bytes"])
            process.update(workload_id=workload_id, mode=mode, environment_overrides=overrides, profiling_only=True)
            write_json(record_path, process)
            processes.append(process)
            if process["status"] != "passed":
                failures.append({"mode": mode, "reason": "process-collection-failed", "invalid_reasons": process["invalid_reasons"]})
            else:
                try:
                    observations[mode] = json.loads(Path(process["stdout_path"]).read_bytes())
                except (ValueError, UnicodeError) as error:
                    failures.append({"mode": mode, "reason": "native-report-invalid", "error": str(error)})
            if "resource-limit-exceeded" in process["invalid_reasons"] or "external-interruption" in process["invalid_reasons"]:
                report["status"] = "failed"
                report["failures"].append({"workload_id": workload_id, "failures": failures})
                write_json(output, report)
                raise RuntimeError("frozen RSS guard or external interruption stops collection")
        fact_rows = [observations.get(mode) for mode in MODES[:4]]
        expected_rejection = workload["frontend"] == "expected-rejection"
        if all(fact_rows):
            for field in OBSERVATION_FIELDS:
                if len({row[field] for row in fact_rows}) != 1:
                    failures.append({"reason": "fact-mode-observation-changed", "field": field})
            for row in fact_rows:
                if not row["tracking_active"] or not row["fact_drop_exact"] or row["fact_union_heap_bytes"] is None:
                    failures.append({"mode": row["mode"], "reason": "exact-fact-drop-unavailable"})
                if not row["reconciliation"]["reconciled"]:
                    failures.append({"mode": row["mode"], "reason": "owner-reconciliation-failed"})
                if row["parse_errors"] != 0 or not row["full_output_present"] or bool(row["full_check_errors"]) != expected_rejection:
                    failures.append({"mode": row["mode"], "reason": "frozen-frontend-status-changed"})
            if fact_rows[2]["fact_union_heap_bytes"] != fact_rows[3]["fact_union_heap_bytes"]:
                failures.append({"reason": "opposite-drop-order-union-changed"})
            for row in fact_rows[2:]:
                if row["full_drop_heap_bytes"] is None or row["compact_drop_heap_bytes"] is None or row["full_drop_heap_bytes"] + row["compact_drop_heap_bytes"] != row["fact_union_heap_bytes"]:
                    failures.append({"mode": row["mode"], "reason": "component-drop-deltas-do-not-sum"})
            # The frozen signature snapshot contains code/message pairs only.
            # Its one rejected root has two known dynamic-boundary errors; valid
            # roots have no diagnostics. Do not invent a missing severity field.
            frozen_diagnostics = signatures[workload["entry"]]["diagnostics"]
            frozen_errors = len(frozen_diagnostics) if expected_rejection else 0
            if fact_rows[0]["full_check_errors"] != frozen_errors:
                failures.append({"reason": "frozen-signature-check-error-count-changed", "expected": frozen_errors})
        else:
            failures.append({"reason": "fact-mode-report-missing"})
        expected_preparation = preflight[workload_id]["exit_status"]
        for mode in MODES[4:]:
            native = observations.get(mode)
            if native and (not native["tracking_active"] or native["preparation_succeeded"] != (expected_preparation == 0) or native["preparation_exit_status"] != (None if expected_preparation == 0 else expected_preparation)):
                failures.append({"mode": mode, "reason": "frozen-preparation-status-changed"})
        row = {"id": workload_id, "entry": workload["entry"], "stratum": workload["stratum"], "expected_rejection": expected_rejection,
               "expected_preparation_exit_status": expected_preparation, "entry_sha256": digest(entry), "status": "failed" if failures else "passed",
               "processes": processes, "observations": observations, "failures": failures,
               "baseline_typing_interface_heap_bytes": observations.get("combined-full-first", {}).get("fact_union_heap_bytes")}
        report["workloads"].append(row)
        if failures:
            report["failures"].append({"workload_id": workload_id, "failures": failures})
        write_json(output, report)
        print(f"{index + 1}/23 {workload_id}: {row['status']}, unique fact heap={row['baseline_typing_interface_heap_bytes']}", flush=True)
    verify_identities()
    valid = sum(not row["expected_rejection"] for row in report["workloads"])
    rejected = sum(row["expected_rejection"] for row in report["workloads"])
    report.update(status="failed" if report["failures"] else "passed", ended_at=measurement.timestamp(),
                  workload_count=len(report["workloads"]), process_count=sum(len(row["processes"]) for row in report["workloads"]),
                  valid_frontend_roots=valid, expected_rejected_frontend_roots=rejected,
                  opaque_prepared_typing_attribution_status="unavailable; auxiliary raw totals do not invent attribution", source_and_binary_identities_unchanged=True)
    write_json(output, report)
    if report["failures"] or valid != 22 or rejected != 1:
        raise RuntimeError("retention collection or frozen root counts did not pass")


if __name__ == "__main__":
    main()
