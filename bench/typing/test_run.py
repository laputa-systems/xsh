import hashlib
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("typing_measurement", Path(__file__).with_name("run.py"))
measurement = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(measurement)
BASELINE_REVISION = "d6f09bc54305515b4c34d3b872d7f9f13b874061"
HISTORICAL_REVISION = "877a114d6dc403db32c50f035b38aab7f193147a"


class SamplingTests(unittest.TestCase):
    def test_lightweight_statistics_report_median_and_range_without_tail_claim(self):
        result = measurement.statistics_for([8, 3, 5, 1, 12])
        self.assertEqual(result["median"], 5)
        self.assertEqual(result["min"], 1)
        self.assertEqual(result["max"], 12)
        self.assertIsNone(result["p95"])
        self.assertEqual(result["p95_status"], "not-claimed")
        self.assertEqual(result["sample_count"], 5)

    def test_single_observation_does_not_claim_tail_evidence(self):
        result = measurement.statistics_for([42])
        self.assertEqual(result["median"], 42)
        self.assertIsNone(result["p95"])
        self.assertEqual(result["p95_status"], "not-claimed")

    def test_invalid_pair_preserves_both_runs_and_alternates_replacement(self):
        calls = []
        emitted = []
        def execute(side, phase, pair_id, attempt):
            calls.append((side, phase, pair_id, attempt))
            return {"status": "failed" if phase == "sample" and attempt == 0 and side == "candidate" else "passed", "wall_ms": 10 if side == "baseline" else 12, "peak_rss_bytes": 100}
        result = measurement.collect_pairs(["baseline", "candidate"], 2, execute, emitted.append, max_invalid_pairs=2)
        self.assertEqual(len([call for call in calls if call[1] == "warmup"]), 2)
        samples = [call for call in calls if call[1] == "sample"]
        self.assertEqual([call[0] for call in samples], ["baseline", "candidate", "candidate", "baseline", "baseline", "candidate"])
        self.assertEqual([row["pair_valid"] for row in emitted if row["phase"] == "sample"], [False, False, True, True, True, True])
        self.assertEqual(len(result["accepted_pairs"]), 2)
        self.assertEqual(result["invalid_pairs"], 1)

    def test_slow_valid_samples_are_kept(self):
        emitted = []
        def execute(side, phase, pair_id, attempt):
            return {"status": "passed", "wall_ms": 100000, "peak_rss_bytes": 1}
        result = measurement.collect_pairs(["baseline"], 1, execute, emitted.append)
        self.assertEqual(result["accepted_pairs"][0]["baseline"]["wall_ms"], 100000)
        self.assertEqual(result["invalid_pairs"], 0)

    def test_frozen_resource_guard_terminates_set_without_replacing_failure(self):
        emitted = []
        def execute(side, phase, pair_id, attempt):
            return {"status": "passed" if phase == "warmup" else "failed", "invalid_reasons": [] if phase == "warmup" else ["resource-limit-exceeded"], "wall_ms": 1, "peak_rss_bytes": 5}
        result = measurement.collect_pairs(["baseline"], 5, execute, emitted.append)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["invalid_pairs"], 1)
        self.assertEqual(len([row for row in emitted if row["phase"] == "sample"]), 1)


class ProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)

    def run_child(self, source, timeout=3, expected=None):
        return measurement.run_process([sys.executable, "-c", source], self.directory, timeout, self.directory / "capture", expected or {"exit_status": 0}, os.environ.copy())

    def test_exact_bytes_nonzero_exit_and_wait_resource_units(self):
        output = b"hello\x00\xff"
        result = self.run_child("import os; os.write(1, b'hello\\x00\\xff'); os.write(2, b'warn'); raise SystemExit(7)", expected={"exit_status": 7, "stdout_sha256": hashlib.sha256(output).hexdigest(), "stderr_sha256": hashlib.sha256(b"warn").hexdigest()})
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["exit_status"], 7)
        self.assertEqual(Path(result["stdout_path"]).read_bytes(), output)
        self.assertGreater(result["peak_rss_bytes"], 0)
        self.assertEqual(measurement.rss_bytes(2048, "Darwin"), 2048)
        self.assertEqual(measurement.rss_bytes(2048, "Linux"), 2048 * 1024)

    @unittest.skipUnless(sys.platform == "darwin", "Darwin collector unit witness")
    def test_darwin_rusage_reports_bytes_for_touched_resident_allocation(self):
        result = self.run_child("memory = bytearray(32 * 1024 * 1024); memory[::4096] = b'x' * (len(memory) // 4096)")
        self.assertEqual(result["status"], "passed")
        self.assertGreaterEqual(result["raw_maxrss"], 32 * 1024 * 1024)
        self.assertLess(result["raw_maxrss"], 1024 * 1024 * 1024)

    def test_wrong_output_and_signal_are_explicit_failures(self):
        mismatch = self.run_child("print('wrong')", expected={"exit_status": 0, "stdout_sha256": hashlib.sha256(b"right\n").hexdigest()})
        self.assertEqual(mismatch["invalid_reasons"], ["stdout-mismatch"])
        signalled = self.run_child("import os, signal; os.kill(os.getpid(), signal.SIGTERM)")
        self.assertEqual(signalled["signal"], signal.SIGTERM)
        self.assertIsNone(signalled["exit_status"])
        self.assertIn("process-signal", signalled["invalid_reasons"])

    def test_timeout_kills_and_reaps_child(self):
        result = self.run_child("import time; time.sleep(10)", timeout=0.05)
        self.assertIn("timeout", result["invalid_reasons"])
        self.assertEqual(result["signal"], signal.SIGKILL)
        with self.assertRaises(ChildProcessError):
            os.waitpid(result["pid"], os.WNOHANG)

    def test_exec_failure_is_distinct_from_expected_nonzero_exit(self):
        result = measurement.run_process([str(self.directory / "missing")], self.directory, 1, self.directory / "absent", {"exit_status": 127}, os.environ.copy())
        self.assertEqual(result["status"], "failed")
        self.assertIn("harness-exec-failure", result["invalid_reasons"])

    def test_stdin_is_closed_by_default_and_frozen_bytes_can_be_supplied(self):
        result = self.run_child("import sys; assert sys.stdin.buffer.read() == b''")
        self.assertEqual(result["status"], "passed")
        stdin = self.directory / "input"
        stdin.write_bytes(b"frozen\x00input")
        result = measurement.run_process([sys.executable, "-c", "import sys; sys.stdout.buffer.write(sys.stdin.buffer.read())"], self.directory, 2, self.directory / "stdin-capture", {"exit_status": 0, "stdout_sha256": measurement.sha256_file(stdin)}, os.environ.copy(), stdin)
        self.assertEqual(result["status"], "passed")

    def test_observation_constraints_validate_native_counts_and_diagnostics(self):
        result = self.run_child("print('passed 1 in 23 ms'); import sys; print('type.dynamic', file=sys.stderr)", expected={"exit_status": 0, "stdout_pattern": r"^passed 1 in \d+ ms$", "stderr_contains": ["type.dynamic"]})
        self.assertEqual(result["status"], "passed")
        result = self.run_child("print('passed 0 in 3 ms')", expected={"exit_status": 0, "stdout_contains": ["passed 1"]})
        self.assertIn("stdout-mismatch", result["invalid_reasons"])

    def measure_report(self, contents, write=True):
        capture = self.directory / "phase-capture"
        report = capture.with_suffix(".json")
        source = "from pathlib import Path; Path(" + repr(str(report)) + ").write_text(" + repr(contents) + ")" if write else "pass"
        return measurement.run_process([sys.executable, "-c", source], self.directory, 2, capture, {"exit_status": 0}, os.environ.copy(), product_measurement={"report": True, "metric_names": ["preparation_ms", "execution_ms", "total_ms"]})

    def test_product_phase_report_preserves_unknown_fields_and_fingerprint(self):
        contents = json.dumps({"schema_version": 1, "status": 0, "preparation_ms": 1, "execution_ms": 3, "total_ms": 4, "scope": "one execution"})
        result = self.measure_report(contents)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["phase_metrics_ms"], {"preparation_ms": 1, "execution_ms": 3, "total_ms": 4})
        self.assertEqual(result["product_report"]["native_report"]["scope"], "one execution")
        self.assertEqual(result["product_report"]["sha256"], hashlib.sha256(contents.encode()).hexdigest())

    def test_missing_and_malformed_phase_reports_are_collection_failures(self):
        for contents, write in [("", False), ("not JSON", True)]:
            with self.subTest(contents=contents):
                result = self.measure_report(contents, write)
                self.assertEqual(result["status"], "failed")
                self.assertIn("harness-collection-failure", result["invalid_reasons"])
                self.assertIsNone(result["phase_metrics_ms"])

    def test_phase_report_requires_matching_status_and_finite_nonnegative_metrics(self):
        for replacement in [{"status": 7}, {"execution_ms": -1}, {"execution_ms": float("nan")}, {"execution_ms": float("inf")}, {"execution_ms": None}, {"execution_ms": True}]:
            with self.subTest(replacement=replacement):
                contents = json.dumps({"schema_version": 1, "status": 0, "preparation_ms": 1, "execution_ms": 3, "total_ms": 4, **replacement})
                result = self.measure_report(contents)
                self.assertEqual(result["status"], "failed")
                self.assertIn("harness-collection-failure", result["invalid_reasons"])

    def test_stale_report_is_removed_before_process_and_cannot_supply_metrics(self):
        report = (self.directory / "phase-capture").with_suffix(".json")
        report.write_text(json.dumps({"schema_version": 1, "status": 0, "preparation_ms": 1, "execution_ms": 3, "total_ms": 4}))
        result = self.measure_report("", write=False)
        self.assertEqual(result["status"], "failed")
        self.assertFalse(report.exists())

    def test_ordinary_process_route_ignores_sidecars(self):
        report = (self.directory / "capture").with_suffix(".json")
        report.write_text("unrelated sidecar")
        result = self.run_child("pass")
        self.assertEqual(result["status"], "passed")
        self.assertIsNone(result["phase_metrics_ms"])
        self.assertEqual(report.read_text(), "unrelated sidecar")


class ProvenanceTests(unittest.TestCase):
    def test_baseline_manifest_commit_and_build_source_must_agree_with_clean_checkout(self):
        cases = [
            (BASELINE_REVISION, BASELINE_REVISION, HISTORICAL_REVISION, []),
            (HISTORICAL_REVISION, BASELINE_REVISION, HISTORICAL_REVISION, []),
            (HISTORICAL_REVISION[:8], HISTORICAL_REVISION[:8], HISTORICAL_REVISION, []),
            (HISTORICAL_REVISION, HISTORICAL_REVISION, HISTORICAL_REVISION[:-1] + "b", []),
            (None, HISTORICAL_REVISION, HISTORICAL_REVISION, []),
            (HISTORICAL_REVISION, None, HISTORICAL_REVISION, []),
            (HISTORICAL_REVISION, HISTORICAL_REVISION, HISTORICAL_REVISION, [" M src/runner.rs"]),
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source.xsh"
            source.write_text("frozen input")
            binary = root / "xsh"
            binary.write_text("#!/bin/sh\nexit 0\n")
            binary.chmod(0o755)
            manifest = root / "manifest.json"
            output = root / "results.jsonl"
            arguments = ["--manifest", str(manifest), "--baseline", str(binary), "--baseline-source", str(root), "--mode", "observe", "--out", str(output)]
            for claimed, build, actual, changes in cases:
                with self.subTest(claimed=claimed, build=build, actual=actual, changes=changes):
                    content = {"schema_version": 1, "files": [{"path": source.name, "sha256": measurement.sha256_file(source)}], "workloads": [{"id": "isolated", "commandTemplate": ["{xsh}"], "expected": {"exit_status": 0}}], "build_settings": {}}
                    if claimed is not None:
                        content["baseline_commit"] = claimed
                    if build is not None:
                        content["build_settings"]["source_revision"] = build
                    manifest.write_text(json.dumps(content))
                    output.unlink(missing_ok=True)
                    identity = {"revision": actual, "tracked_changes": changes, "path": str(root)}
                    with patch.object(measurement, "git_identity", return_value=identity), patch.object(measurement, "run_process", return_value={"status": "passed"}) as execute, contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit) as error:
                        measurement.main(arguments)
                    self.assertEqual(error.exception.code, 2)
                    execute.assert_not_called()
                    self.assertFalse(output.exists())

    def test_environment_product_paths_follow_each_build(self):
        overrides = {"CARGO_BIN_EXE_xsh": "{xsh}", "CARGO_BIN_EXE_xsht": "{xsht}", "XSH_MODULE_PATH": "{root}/cohort/stabilized:{root}/cohort/stabilized/dev"}
        for side in ["baseline", "candidate"]:
            products = {name: {"path": f"/products/{side}/{name}"} for name in ["xsh", "xsht"]}
            expanded = measurement.expand_environment(overrides, products, Path("/frozen"))
            self.assertEqual(expanded["CARGO_BIN_EXE_xsh"], f"/products/{side}/xsh")
            self.assertEqual(expanded["CARGO_BIN_EXE_xsht"], f"/products/{side}/xsht")
            self.assertEqual(expanded["XSH_MODULE_PATH"], "/frozen/cohort/stabilized:/frozen/cohort/stabilized/dev")

    def test_observation_only_selection_keeps_native_oracles_out_of_timing_rows(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "xsh"
            binary.write_text("#!/bin/sh\nprintf 'witness\\n'\n")
            binary.chmod(0o755)
            source = root / "source.xsh"
            source.write_text("frozen input")
            manifest = root / "manifest.json"
            manifest.write_text(json.dumps({"schema_version": 1, "baseline_commit": BASELINE_REVISION, "build_settings": {"source_revision": BASELINE_REVISION}, "files": [{"path": source.name, "sha256": measurement.sha256_file(source)}], "workloads": [{"id": "performance", "commandTemplate": ["{xsh}"], "expected": {"exit_status": 7}}], "observation_workloads": [{"id": "native-oracle", "commandTemplate": ["{xsh}"], "expected": {"exit_status": 0, "stdout_utf8": "witness\n"}}]}))
            output = root / "results.jsonl"
            identity = {"revision": BASELINE_REVISION, "tracked_changes": [], "path": str(root)}
            arguments = ["--manifest", str(manifest), "--baseline", str(binary), "--baseline-source", str(root), "--mode", "observe", "--out", str(output), "--observations-only"]
            with patch.object(measurement, "git_identity", return_value=identity), patch.object(measurement, "host_conditions", return_value={"build_process_observation": []}), contextlib.redirect_stdout(io.StringIO()):
                status = measurement.main(arguments)
            self.assertEqual(status, 0)
            rows = [json.loads(line) for line in output.read_text().splitlines()]
            processes = [row for row in rows if row["record_type"] == "process"]
            self.assertEqual([row["workload_id"] for row in processes], ["native-oracle"])
            self.assertTrue(all(row["phase"] == "observation" for row in processes))
            self.assertFalse(any(row["record_type"] == "summary" for row in rows))

    def test_observations_only_refuses_timing_mode(self):
        arguments = ["--manifest", "/unused", "--baseline", "/unused", "--baseline-source", "/unused", "--mode", "final", "--out", "/unused", "--observations-only"]
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            measurement.main(arguments)
        self.assertEqual(error.exception.code, 2)

    def test_lightweight_sampling_preserves_provenance_without_quiet_machine_requirement(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "xsh"
            binary.write_text("#!/bin/sh\nprintf 'observed\\n'\n")
            binary.chmod(0o755)
            source = root / "source.xsh"
            source.write_text("frozen input")
            manifest = root / "manifest.json"
            manifest.write_text(json.dumps({"schema_version": 1, "baseline_commit": BASELINE_REVISION, "files": [{"path": source.name, "sha256": measurement.sha256_file(source)}], "build_settings": {"source_revision": BASELINE_REVISION, "profile": "release", "features": "default"}, "workloads": [{"id": "isolated", "source": source.name, "commandTemplate": ["{xsh}", "{source}"], "kind": "runtime", "expected": {"exit_status": 0, "stdout_utf8": "observed\n", "stderr_utf8": ""}}]}))
            identities = {"revision": BASELINE_REVISION, "tracked_changes": [], "path": str(root)}
            cases = [(mode, conditions, False) for mode in ["pilot", "final"] for conditions in [["234 cargo"], None]] + [("final", ["234 cargo"], True)]
            for index, (mode, observed_builds, paired) in enumerate(cases):
                with self.subTest(mode=mode, observed_builds=observed_builds, paired=paired):
                    output = root / f"results-{index}.jsonl"
                    arguments = ["--manifest", str(manifest), "--baseline", str(binary), "--baseline-source", str(root), "--mode", mode, "--out", str(output)]
                    if paired:
                        arguments += ["--candidate", str(binary), "--candidate-source", str(root)]
                    with patch.object(measurement.platform, "system", return_value="Darwin"), patch.object(measurement, "git_identity", return_value=identities), patch.object(measurement, "host_conditions", return_value={"build_process_observation": observed_builds}), contextlib.redirect_stdout(io.StringIO()):
                        status = measurement.main(arguments)
                    self.assertEqual(status, 0)
                    rows = [json.loads(line) for line in output.read_text().splitlines()]
                    samples = [row for row in rows if row.get("phase") == "sample"]
                    warmups = [row for row in rows if row.get("phase") == "warmup"]
                    for side in ["baseline", "candidate"] if paired else ["baseline"]:
                        self.assertEqual(len([row for row in samples if row["side"] == side]), 5)
                        self.assertEqual(len([row for row in warmups if row["side"] == side]), 1)
                    self.assertTrue(all(row["pair_valid"] for row in samples))
                    self.assertEqual(rows[0]["provenance"]["products"]["baseline"]["xsh"]["sha256"], measurement.sha256_file(binary))
                    self.assertEqual(rows[0]["provenance"]["conditions"]["build_process_observation"], observed_builds)
                    summary = next(row for row in rows if row["record_type"] == "summary")
                    self.assertEqual(summary["builds"]["baseline"]["wall_ms"]["sample_count"], 5)
                    self.assertIsNone(summary["builds"]["baseline"]["wall_ms"]["p95"])
                    self.assertEqual(summary["builds"]["baseline"]["wall_ms"]["p95_status"], "not-claimed")
                    self.assertEqual(rows[-1]["status"], "passed")

    def test_busy_machine_waiver_does_not_allow_source_or_binary_changes(self):
        for mutation in ["source", "binary"]:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                binary = root / "xsh"
                binary.write_text("#!/bin/sh\nprintf 'observed\\n'\n")
                binary.chmod(0o755)
                source = root / "source.xsh"
                source.write_text("frozen input")
                manifest = root / "manifest.json"
                manifest.write_text(json.dumps({"schema_version": 1, "baseline_commit": BASELINE_REVISION, "build_settings": {"source_revision": BASELINE_REVISION}, "files": [{"path": source.name, "sha256": measurement.sha256_file(source)}], "workloads": [{"id": "isolated", "commandTemplate": ["{xsh}"], "expected": {"exit_status": 0, "stdout_utf8": "observed\n", "stderr_utf8": ""}}]}))
                output = root / "results.jsonl"
                identity = {"revision": BASELINE_REVISION, "tracked_changes": [], "path": str(root)}
                runner = measurement.run_process
                def contaminated(*args, **kwargs):
                    row = runner(*args, **kwargs)
                    affected = source if mutation == "source" else binary
                    affected.write_bytes(affected.read_bytes() + b"changed")
                    return row
                with patch.object(measurement, "git_identity", return_value=identity), patch.object(measurement, "host_conditions", return_value={"build_process_observation": ["234 cargo"]}), patch.object(measurement, "run_process", side_effect=contaminated), contextlib.redirect_stdout(io.StringIO()):
                    status = measurement.main(["--manifest", str(manifest), "--baseline", str(binary), "--baseline-source", str(root), "--mode", "observe", "--out", str(output)])
                self.assertEqual(status, 1)
                rows = [json.loads(line) for line in output.read_text().splitlines()]
                process = next(row for row in rows if row["record_type"] == "process")
                self.assertEqual(process["status"], "passed")
                provenance = next(row for row in rows if row["record_type"] == "provenance_check")
                self.assertEqual(provenance["invalid_reasons"], ["provenance-changed"])
                self.assertEqual(rows[-1]["status"], "failed")

    def test_frozen_input_mutation_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source.xsh"
            source.write_bytes(b"original")
            files = [{"path": "source.xsh", "sha256": measurement.sha256_file(source)}]
            measurement.verify_files(root, files)
            source.write_bytes(b"changed")
            with self.assertRaises(ValueError):
                measurement.verify_files(root, files)

    def test_profiling_absence_does_not_create_zero_counts_or_pass(self):
        result = measurement.unavailable_profile("instrumentation absent")
        self.assertEqual(result["status"], "not-run")
        self.assertIsNone(result["native_report"])
        self.assertTrue(all(value is None for value in result["work_counters"].values()))
        self.assertTrue(all(phase["wall_ms"] is None for phase in result["phases"]))

    def test_paired_ratios_handle_zero_baseline_without_inventing_ratio(self):
        pairs = [{"baseline": {"wall_ms": 0, "peak_rss_bytes": 10}, "candidate": {"wall_ms": 2, "peak_rss_bytes": 12}}]
        summary = measurement.summarize_pairs(pairs)
        self.assertEqual(summary["paired_wall_differences_ms"], [2])
        self.assertEqual(summary["paired_wall_ratios"], [None])

    def test_lightweight_phase_summary_preserves_raw_pairs_without_percentile_claim(self):
        pairs = [{"baseline": {"wall_ms": 1, "peak_rss_bytes": 1, "phase_metrics_ms": {"execution_ms": value}}, "candidate": {"wall_ms": 1, "peak_rss_bytes": 1, "phase_metrics_ms": {"execution_ms": value + 2}}} for value in [8, 3, 5, 1, 12]]
        summary = measurement.summarize_pairs(pairs)
        phases = summary["builds"]["baseline"]["product_phase_metrics"]
        self.assertEqual(phases["execution_ms"]["median"], 5)
        self.assertEqual(phases["execution_ms"]["min"], 1)
        self.assertEqual(phases["execution_ms"]["max"], 12)
        self.assertIsNone(phases["execution_ms"]["p95"])
        self.assertEqual(len(phases["execution_ms"]["raw_samples"]), 5)
        self.assertEqual(summary["paired_phase_differences_ms"]["execution_ms"], [2] * 5)


if __name__ == "__main__":
    unittest.main()
