"""Assemble immutable inputs and observed workload contracts."""

import hashlib
import json
from pathlib import Path
import re


HERE = Path(__file__).resolve().parent


def read(name):
    return json.loads((HERE / name).read_text())


def fingerprint(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def handoff_revision(selection, annotations, build, correction=None):
    original = selection["snapshot_commit"]
    current = build["source_revision"]
    if any(not re.fullmatch(r"[0-9a-f]{40}", revision) for revision in (original, current)):
        raise ValueError("source revisions must be full commit identities")
    if annotations["snapshot_commit"] != original or build["status"] != "passed":
        raise ValueError("annotation snapshot or baseline build does not match the source handoff")
    if original != current:
        if not correction or correction.get("historical_revision") != original or correction.get("settled_handoff_revision") != current:
            raise ValueError("changed baseline requires an explicit matching handoff correction")
        if correction.get("frozen_denominators_unchanged") is not True or correction.get("frozen_cohort_inputs_unchanged") is not True or correction.get("inference_implementation_started") is not False:
            raise ValueError("handoff correction cannot change frozen scope or follow inference implementation")
    return current


def expected(row, status_key="exit_status"):
    result = {"exit_status": row[status_key], "stdout_sha256": row["stdout_sha256"]}
    if result["exit_status"] == 0:
        result["stderr_sha256"] = row["stderr_sha256"]
    else:
        result["stderr_contains"] = "check.dynamic-boundary"
    return result


def assemble():
    selection = read("cohort-selection.json")
    annotations = read("annotations.json")
    build = read("baseline-build.json")["build"]
    correction_path = HERE / "handoff-correction.json"
    correction = read("handoff-correction.json") if correction_path.exists() else None
    baseline = handoff_revision(selection, annotations, build, correction)
    if correction:
        historical = HERE / "history" / correction["historical_revision"][:8]
        stable_contracts = {"annotations.json", "cohort-selection.json", "resource-limits.json", "measurement-policy.json", "scaling-manifest.json"}
        amendment_path = HERE / "benchmark-amendment.json"
        if amendment_path.exists():
            amendment = read("benchmark-amendment.json")
            if amendment.get("authority") != "explicit user instruction" or amendment.get("historical_policy_sha256") != fingerprint(historical / "measurement-policy.json") or amendment.get("current_policy_sha256") != fingerprint(HERE / "measurement-policy.json"):
                raise ValueError("benchmark amendment does not identify the historical and current policies")
            stable_contracts.remove("measurement-policy.json")
        for entry in correction["historical_files"]:
            if fingerprint(historical / entry["path"]) != entry["sha256"]:
                raise ValueError(f"historical handoff artifact changed: {entry['path']}")
            if entry["path"] in stable_contracts and fingerprint(HERE / entry["path"]) != entry["sha256"]:
                raise ValueError(f"frozen scope or measurement contract changed: {entry['path']}")
        if stable_contracts - {entry["path"] for entry in correction["historical_files"]}:
            raise ValueError("handoff correction lacks historical frozen-contract fingerprints")
    preparation_name = "baseline-prepare-preflight-v2.json" if baseline != selection["snapshot_commit"] else "baseline-prepare-preflight.json"
    observation_names = ("baseline-frontend-preflight.json", preparation_name,
                         "baseline-runtime-preflight.json", "baseline-runtime-extra-preflight.json")
    observations = {name: read(name) for name in observation_names}
    if baseline != selection["snapshot_commit"]:
        for name, observation in observations.items():
            if observation.get("source_revision") != baseline or observation.get("status") != "passed":
                raise ValueError(f"corrected baseline observation is missing or unsuccessful: {name}")
    frontend = {row["id"]: row for row in observations["baseline-frontend-preflight.json"]["rows"]}
    prepared = {row["id"]: row for row in observations[preparation_name]["rows"]}
    if len(frontend) != len(selection["workloads"]) or set(frontend) != set(prepared):
        raise ValueError("workload observations do not cover frozen selection")
    files = []
    for source in selection["sources"]:
        for variant in ("original", "stabilized"):
            path = Path("cohort") / variant / source["path"]
            digest = fingerprint(HERE / path)
            if digest != source[variant + "_sha256"]:
                raise ValueError(f"frozen source changed: {path}")
            files.append({"path": str(path), "sha256": digest})
    for variant in ("original", "stabilized"):
        path = Path("cohort") / variant / "xsht-config.ini"
        files.append({"path": str(path), "sha256": fingerprint(HERE / path)})
    fixture_root = HERE / "runtime-fixtures"
    if fixture_root.exists():
        files.extend({"path": str(path.relative_to(HERE)), "sha256": fingerprint(path)}
                     for path in sorted(fixture_root.rglob("*")) if path.is_file())
    env = {"XSH_MODULE_PATH": "{root}/cohort/stabilized:{root}/cohort/stabilized/dev"}
    workloads = []
    runtime_applicability = []
    for work in selection["workloads"]:
        source = "cohort/stabilized/" + work["entry"]
        common = {"variant": "stabilized", "source": source, "selection_id": work["id"],
                  "stratum": work["stratum"], "cwd": "cohort/stabilized", "env": env,
                  "timeout_seconds": 60, "applicability": {"status": "applicable", "reason": None}}
        for kind, observation, command in (
            ("frontend", frontend[work["id"]], ["{xsht}", "check", "{source}"]),
            ("startup", prepared[work["id"]], ["{product_probe}", "prepare", "{source}"]),
        ):
            workloads.append({**common, "id": kind + "-" + work["id"], "kind": kind,
                              "commandTemplate": command, "expected": expected(observation),
                              "instrumented": {"frontend_command": ["{frontend_stats}", "--json", "{source}"]}})
        linux = work["entry"] in ("core/ifup.xsh", "core/ifdown.xsh", "core/lib/system_report_live.xsh", "core/lib/system_report_collect.xsh")
        runtime_applicability.append({"selection_id": work["id"],
                                      "live_host_status": "not-applicable" if linux else "fixture-or-safe-entry-observation",
                                      "reason": "Linux host mutation/discovery is excluded; frontend and verified preparation retained, host behavior covered by isolated native fixtures" if linux else "Runtime observations use the frozen safe entry paths or native fixture assertions; a module without an entry is observed through its consumers"})
    historical = observations["baseline-runtime-preflight.json"]["rows"]
    original = {row["id"]: row for row in historical if row["variant"] == "original"}
    for row in historical:
        if row["variant"] != "stabilized":
            continue
        old = original[row["id"]]
        status_key = "exit_status" if "exit_status" in row else "status"
        if (old[status_key], old["stdout_sha256"], old["stderr_sha256"]) != (row[status_key], row["stdout_sha256"], row["stderr_sha256"]):
            raise ValueError(f"stabilized observation differs: {row['id']}")
        args = row["command"][3:]
        workload = {"id": row["id"], "kind": "runtime", "variant": "stabilized",
                    "source": "cohort/stabilized/" + row["source"],
                    "commandTemplate": ["{xsh}", "{source}", "--", *args],
                    "cwd": "cohort/stabilized", "env": env, "timeout_seconds": 60,
                    "expected": expected(row, status_key), "applicability": {"status": "applicable", "reason": None}}
        if row["id"] == "runtime-jq":
            workload["stdin_utf8"] = '{"name":"typing"}\n'
        workloads.append(workload)
    extras = observations["baseline-runtime-extra-preflight.json"]
    workloads.extend(extras["workloads"])
    executions = []
    for work in workloads:
        if work["kind"] != "runtime":
            continue
        executions.append({**work, "id": "execution-" + work["id"], "kind": "runtime_execution",
                           "commandTemplate": ["{execution_probe}", "execute", "{report}", "{source}", *work["commandTemplate"][3:]],
                           "product_measurement": {"report": True, "metric_names": ["preparation_ms", "execution_ms", "total_ms"]},
                           "comparison_metric": "execution_ms", "corresponding_total_invocation": work["id"]})
    workloads.extend(executions)
    if any(work.get("timeout_seconds") != 60 for work in workloads + extras["observation_workloads"]):
        raise ValueError("workload timeout disagrees with the frozen resource guard")
    ids = [work["id"] for work in workloads]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate workload identity")
    contracts = ["cohort-selection.json", "annotations.json", "operations.json", "semantic-cases.json",
                 "measurement-policy.json", "resource-limits.json", "scaling-manifest.json",
                 "baseline-build.json", "owner-audit.json"]
    contracts.extend(observation_names)
    if correction:
        contracts.append("handoff-correction.json")
    if (HERE / "benchmark-amendment.json").exists():
        contracts.append("benchmark-amendment.json")
    return {"schema_version": 1, "status": "frozen-before-inference", "baseline_commit": baseline,
            "cohort_original_revision": selection["snapshot_commit"],
            "source": {"closure_sha256": selection["source_closure_sha256"], "files": files},
            "annotation_eligibility_sha256": annotations["eligibility_sha256"],
            "annotation_denominators": annotations["denominators"],
            "contracts": [{"path": path, "sha256": fingerprint(HERE / path)} for path in contracts],
            "build_settings": build,
            "resource_limits": {"product_process_wall_seconds": 60, "peak_process_rss_bytes": 4294967296},
            "workload_policy": "Each selected closure receives a full cold frontend and a load/check/verified-preparation process. Runtime uses safe entry paths and isolated native assertions. All timings include process launch and teardown; no user code executes in the preparation-only process.",
            "runtime_applicability": runtime_applicability, "workloads": workloads,
            "observation_workloads": extras["observation_workloads"]}


if __name__ == "__main__":
    target = HERE / "manifest.json"
    data = json.dumps(assemble(), indent=2) + "\n"
    if target.exists() and target.read_text() != data:
        raise SystemExit("existing frozen manifest differs; inspect the immutable inputs before replacing it")
    target.write_text(data)
    print(f"frozen {len(json.loads(data)['workloads'])} workload observations")
