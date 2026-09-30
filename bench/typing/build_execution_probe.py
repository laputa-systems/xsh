"""Link and verify an execution observer against an immutable release library."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def verify_probe(binary, ordinary, root):
    environment = os.environ.copy()
    environment.pop("XSH_COVERAGE_TRACE_DIR", None)
    environment.pop("XSH_MODULE_PATH", None)
    results = []
    with tempfile.TemporaryDirectory(prefix="xsh-execution-probe-") as directory:
        directory = Path(directory)
        source = directory / "clock.xsh"
        source.write_text('print "probe-output"\ntime.sleep(40ms)?\n')
        report = directory / "clock.json"
        command = [str(binary), "execute", str(report), str(source)]
        observed = subprocess.run(command, cwd=root, env=environment, input=b"", capture_output=True, timeout=20)
        expected = subprocess.run([str(ordinary), str(source)], cwd=root, env=environment, input=b"", capture_output=True, timeout=20)
        phases = json.loads(report.read_text())
        if (observed.returncode, observed.stdout, observed.stderr) != (expected.returncode, expected.stdout, expected.stderr) or observed.returncode != 0:
            raise ValueError(f"execution probe output differs from ordinary script: {observed.stderr!r}")
        if observed.stdout != b"probe-output\n" or phases["execution_ms"] < 35 or phases["preparation_ms"] < 0 or abs(phases["total_ms"] - phases["preparation_ms"] - phases["execution_ms"]) > 1e-6:
            raise ValueError("execution clock did not cover exactly one delayed script run")
        results.append({"name": "one-execution-clock-and-output", "status": "passed", "command": command, "phases": phases})
        invalid = directory / "invalid.xsh"
        invalid.write_text('let value: Int = "wrong"\n')
        rejected_report = directory / "rejected.json"
        rejected_command = [str(binary), "execute", str(rejected_report), str(invalid)]
        rejected = subprocess.run(rejected_command, cwd=root, env=environment, input=b"", capture_output=True, timeout=20)
        normal_rejected = subprocess.run([str(ordinary), str(invalid)], cwd=root, env=environment, input=b"", capture_output=True, timeout=20)
        rejection = json.loads(rejected_report.read_text())
        if (rejected.returncode, rejected.stdout, rejected.stderr) != (normal_rejected.returncode, normal_rejected.stdout, normal_rejected.stderr) or rejected.returncode != 2 or rejection["execution_ms"] is not None:
            raise ValueError("rejected preparation acquired execution timing or changed diagnostics")
        results.append({"name": "preparation-rejection-has-no-execution", "status": "passed", "command": rejected_command})
        for unavailable in [report, directory / "absent-parent" / "report.json"]:
            before = report.read_bytes()
            failing_command = [str(binary), "execute", str(unavailable), str(source)]
            failing = subprocess.run(failing_command, cwd=root, env=environment, input=b"", capture_output=True, timeout=20)
            if failing.returncode != 2 or failing.stdout or b"cannot create report" not in failing.stderr or report.read_bytes() != before:
                raise ValueError("report creation failure executed code or overwrote existing evidence")
            results.append({"name": "report-refusal-before-user-code", "status": "passed", "command": failing_command})
    return results


def verify_runtime(binary, preflight_path):
    preflight_path = Path(preflight_path).resolve()
    rows = json.loads(preflight_path.read_text())["rows"]
    root = Path(__file__).resolve().parent / "cohort" / "stabilized"
    environment = os.environ.copy()
    environment.pop("XSH_COVERAGE_TRACE_DIR", None)
    environment["XSH_MODULE_PATH"] = f"{root}:{root / 'dev'}"
    results = []
    with tempfile.TemporaryDirectory(prefix="xsh-execution-parity-") as directory:
        for row in rows:
            if row["variant"] != "stabilized":
                continue
            report = Path(directory) / (row["id"] + ".json")
            argv = row["command"][2:]
            if argv[:1] == ["--"]:
                argv = argv[1:]
            command = [str(binary), "execute", str(report), row["command"][1], *argv]
            stdin = b'{"name":"typing"}\n' if row["id"] == "runtime-jq" else b""
            ordinary = subprocess.run(row["command"], cwd=root, env=environment, input=stdin, capture_output=True, timeout=60)
            observed = subprocess.run(command, cwd=root, env=environment, input=stdin, capture_output=True, timeout=60)
            phases = json.loads(report.read_text())
            actual = (observed.returncode, observed.stdout, observed.stderr)
            expected = (ordinary.returncode, ordinary.stdout, ordinary.stderr)
            if actual != expected or observed.returncode != row["status"] or hashlib.sha256(observed.stdout).hexdigest() != row["stdout_sha256"] or hashlib.sha256(observed.stderr).hexdigest() != row["stderr_sha256"]:
                raise ValueError(f"runtime observations differ for {row['id']}: {observed.stderr!r}")
            if phases["status"] != observed.returncode or phases["execution_ms"] is None:
                raise ValueError(f"runtime phase report is invalid for {row['id']}")
            results.append({"id": row["id"], "status": "passed", "command": command, "ordinary_command": row["command"], "exit_status": observed.returncode, "stdout_sha256": hashlib.sha256(observed.stdout).hexdigest(), "stderr_sha256": hashlib.sha256(observed.stderr).hexdigest(), "stdin_sha256": hashlib.sha256(stdin).hexdigest(), "cwd": str(root), "environment_overrides": {"XSH_MODULE_PATH": environment["XSH_MODULE_PATH"]}, "report": phases})
    return {"preflight_sha256": sha256(preflight_path), "rows": results}


def build(provenance_path, output, runtime_preflight=None):
    provenance_path = Path(provenance_path).resolve()
    prior = json.loads(provenance_path.read_text())
    library_root = Path(prior["library_root"])
    for kind in ["library", "metadata"]:
        if sha256(prior[f"{kind}_path"]) != prior[f"{kind}_sha256"]:
            raise ValueError(f"frozen {kind} identity changed")
    source = Path(__file__).with_name("execution_probe.rs").resolve()
    output = Path(output).resolve()
    command = list(prior["product_probe"]["build_command"])
    command[command.index("typing_product_probe")] = "typing_execution_probe"
    command[command.index("--crate-name") + 2] = str(source)
    command[command.index("-o") + 1] = str(output)
    compiler_command = ["rustc", "--version", "--verbose"]
    compiler = subprocess.check_output(compiler_command, cwd=library_root, text=True).strip()
    if compiler != prior["rustc"]:
        raise ValueError("compiler differs from frozen baseline build")
    output.parent.mkdir(parents=True, exist_ok=True)
    completed = subprocess.run(command, cwd=library_root, text=True, capture_output=True)
    if completed.returncode:
        raise RuntimeError(f"execution-probe rustc failed ({completed.returncode}):\n{completed.stderr}")
    verification = verify_probe(output, library_root / "target" / "release" / "xsh", library_root)
    result = {
        "schema_version": 1, "library_root": str(library_root),
        "library_path": prior["library_path"], "library_sha256": prior["library_sha256"],
        "metadata_path": prior["metadata_path"], "metadata_sha256": prior["metadata_sha256"],
        "link_provenance_path": str(provenance_path), "link_provenance_sha256": sha256(provenance_path),
        "source_sha256": sha256(source), "builder_sha256": sha256(__file__),
        "binary": str(output), "binary_sha256": sha256(output),
        "build_command": command, "build_exit_status": completed.returncode,
        "compiler_command": compiler_command, "rustc": compiler,
        "allocator": "System, matching macOS production", "verification": verification,
        "measurement_scope": "cold preparation once and one normal prepared execution; output/report writes excluded; process startup/teardown separately owned by ordinary xsh invocation measurements",
        "instrumentation": "three Instant boundary reads, outside user operation loops; no counting allocator or operation counters",
        "product_binaries_modified": False,
    }
    if runtime_preflight:
        result["runtime_observations"] = verify_runtime(output, runtime_preflight)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--link-provenance", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--runtime-preflight", type=Path)
    args = parser.parse_args()
    result = build(args.link_provenance, args.output, args.runtime_preflight)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"execution probe built; {len(result['verification'])} focused process checks passed")


if __name__ == "__main__":
    main()
