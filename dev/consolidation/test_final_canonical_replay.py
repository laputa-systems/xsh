import argparse
import importlib.util
import json
import pathlib
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRATCH_ROOT = ROOT / ".work/tmp"
RUNNER_PATH = ROOT / "dev/consolidation/final_canonical_replay.py"
RUNNER_SPEC = importlib.util.spec_from_file_location("final_canonical_replay", RUNNER_PATH)
runner = importlib.util.module_from_spec(RUNNER_SPEC)
RUNNER_SPEC.loader.exec_module(runner)


class CanonicalReplayPreflightTests(unittest.TestCase):
    def setUp(self):
        SCRATCH_ROOT.mkdir(parents=True, exist_ok=True)
        self.scratch = pathlib.Path(tempfile.mkdtemp(prefix="canonical-replay-preflight-", dir=SCRATCH_ROOT))
        self.repo = self.scratch / "repo"
        self.repo.mkdir()
        subprocess.run(["git", "init", "--quiet", str(self.repo)], check=True)
        subprocess.run(["git", "-C", str(self.repo), "config", "user.name", "Test"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "config", "user.email", "test@example.invalid"], check=True)
        (self.repo / ".git/info/exclude").write_text("/.work/\n/target\n", encoding="utf-8")
        (self.repo / "source.txt").write_text("source\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(self.repo), "add", "source.txt"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "commit", "--quiet", "-m", "source"], check=True)
        self.head = self.git("rev-parse", "HEAD")
        self.tree = self.git("rev-parse", "HEAD^{tree}")
        self.output = self.repo / ".work/tmp/canonical-run"
        self.target_dir = self.repo / ".work/target-final-canonical"
        self.bootstrap = self.target_dir / runner.TARGET / "verification" / "xsh"
        self.infra = self.repo / ".work/consolidation/infra"
        self.metrics = self.repo / ".work/consolidation"
        self.native_lock = self.infra / "native-suite.lock"
        self.cargo_lock = self.infra / "cargo.lock"
        self.scratch_dir = self.repo / ".work/tmp"
        self.receipt_path = self.scratch_dir / "bootstrap-build-receipt.json"
        self.build_json_path = self.scratch_dir / "bootstrap-build.jsonl"
        self.scratch_dir.mkdir(parents=True)
        self.infra.mkdir(parents=True)
        self.native_lock.write_text("", encoding="utf-8")
        self.cargo_lock.write_text("", encoding="utf-8")
        self.create_bootstrap()
        self.create_build_receipt()

    def tearDown(self):
        shutil.rmtree(self.scratch)

    def git(self, *argv):
        return subprocess.run(
            ["git", "-C", str(self.repo), *argv],
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()

    def create_bootstrap(self):
        self.bootstrap.parent.mkdir(parents=True, exist_ok=True)
        self.bootstrap.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        self.bootstrap.chmod(0o755)

    def build_records(self):
        return [
            {
                "reason": "compiler-artifact",
                "package_id": "path+file:///work#xsh@0.1.0",
                "target": {"name": "xsh", "kind": ["bin"], "crate_types": ["bin"]},
                "profile": {"test": False},
                "executable": f"/work/target/{runner.TARGET}/verification/xsh",
            },
            {"reason": "build-finished", "success": True},
        ]

    def create_build_receipt(self, receipt_updates=None):
        records = self.build_records()
        self.build_json_path.write_text("".join(json.dumps(record) + "\n" for record in records), encoding="utf-8")
        receipt = {
            "schema_version": 1,
            "status": "passed",
            "source": {"commit": self.head, "tree": self.tree, "clean": True},
            "image": runner.IMAGE,
            "target": runner.TARGET,
            "profile": "verification",
            "rustflags": runner.RUSTFLAGS,
            "cargo_argv": runner.BOOTSTRAP_CARGO_ARGV,
            "cargo_target_dir": runner.BOOTSTRAP_CONTAINER_TARGET_DIR,
            "build_json": {
                "path": str(self.build_json_path),
                "sha256": runner.sha256_file(self.build_json_path),
                "exit_code": 0,
            },
            "artifact": {
                "path": str(self.bootstrap),
                "size_bytes": self.bootstrap.stat().st_size,
                "sha256": runner.sha256_file(self.bootstrap),
            },
        }
        if receipt_updates:
            receipt.update(receipt_updates)
        self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        return receipt

    def arguments(self, receipt=None):
        return argparse.Namespace(
            repo=self.repo,
            head=self.head,
            tree=self.tree,
            output=pathlib.Path(".work/tmp/canonical-run"),
            run_name="canonical-run",
            timeout_seconds=60,
            bootstrap_receipt=self.receipt_path if receipt is None else receipt,
        )

    def with_mocked_tools(self):
        real_host_file = runner.host_file

        def host_file(path, executable=False):
            path = pathlib.Path(path)
            if path == self.bootstrap:
                return real_host_file(path, executable=executable)
            return {"path": str(path), "size_bytes": 1, "sha256": "0" * 64}

        return mock.patch.object(runner, "host_file", side_effect=host_file)

    def validate(self, args=None):
        with self.with_mocked_tools():
            return runner.validate_inputs(args or self.arguments())

    def test_rejects_missing_bootstrap_binary(self):
        self.bootstrap.unlink()
        with self.assertRaisesRegex(runner.ReplayError, "required host file is missing"):
            self.validate()

    def test_rejects_wrong_bootstrap_artifact_path(self):
        receipt = self.create_build_receipt()
        receipt["artifact"]["path"] = str(self.target_dir / "xsh")
        self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        with self.assertRaisesRegex(runner.ReplayError, "artifact path must be"):
            self.validate()

    def test_rejects_bootstrap_artifact_hash_mismatch(self):
        receipt = self.create_build_receipt()
        receipt["artifact"]["sha256"] = "0" * 64
        self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        with self.assertRaisesRegex(runner.ReplayError, "artifact hash or size"):
            self.validate()

    def test_rejects_bootstrap_receipt_from_another_source(self):
        receipt = self.create_build_receipt()
        receipt["source"]["commit"] = "0" * 40
        self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        with self.assertRaisesRegex(runner.ReplayError, "source does not match"):
            self.validate()

    def test_rejects_non_boolean_bootstrap_clean_flag(self):
        receipt = self.create_build_receipt()
        receipt["source"]["clean"] = 1
        self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        with self.assertRaisesRegex(runner.ReplayError, "source does not match"):
            self.validate()

    def test_rejects_bootstrap_receipt_build_configuration_mismatch(self):
        for field, value in (
            ("image", "sha256:" + "0" * 64),
            ("target", "aarch64-unknown-linux-musl"),
            ("profile", "release"),
            ("rustflags", ""),
            ("cargo_argv", ["cargo", "build"]),
            ("cargo_target_dir", "/work/target-wrong"),
        ):
            with self.subTest(field=field):
                receipt = self.create_build_receipt()
                receipt[field] = value
                self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
                with self.assertRaisesRegex(runner.ReplayError, "configuration does not match"):
                    self.validate()

    def test_rejects_missing_bootstrap_receipt(self):
        self.receipt_path.unlink()
        with self.assertRaisesRegex(runner.ReplayError, "bootstrap receipt file is missing"):
            self.validate()

    def test_rejects_bootstrap_receipt_outside_scratch(self):
        with self.assertRaisesRegex(runner.ReplayError, "bootstrap receipt must be under"):
            self.validate(self.arguments(receipt=self.repo / "source.txt"))

    def test_rejects_failed_or_unmatched_cargo_build_json(self):
        records = self.build_records()
        records[-1]["success"] = False
        self.build_json_path.write_text("".join(json.dumps(record) + "\n" for record in records), encoding="utf-8")
        receipt = json.loads(self.receipt_path.read_text(encoding="utf-8"))
        receipt["build_json"]["sha256"] = runner.sha256_file(self.build_json_path)
        self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        with self.assertRaisesRegex(runner.ReplayError, "successful build-finished"):
            self.validate()

    def test_rejects_cargo_json_for_a_different_executable(self):
        records = self.build_records()
        records[0]["executable"] = "/work/target/wrong/xsh"
        self.build_json_path.write_text("".join(json.dumps(record) + "\n" for record in records), encoding="utf-8")
        receipt = json.loads(self.receipt_path.read_text(encoding="utf-8"))
        receipt["build_json"]["sha256"] = runner.sha256_file(self.build_json_path)
        self.receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        with self.assertRaisesRegex(runner.ReplayError, "does not identify the expected xsh"):
            self.validate()

    def test_rejects_missing_shared_cargo_lock(self):
        self.cargo_lock.unlink()
        with self.assertRaisesRegex(runner.ReplayError, "shared Cargo lock is missing"):
            self.validate()

    def test_rejects_missing_native_suite_lock(self):
        self.native_lock.unlink()
        with self.assertRaisesRegex(runner.ReplayError, "shared native-suite lock is missing"):
            self.validate()

    def test_rejects_missing_metrics_mount(self):
        shutil.rmtree(self.metrics)
        with self.assertRaisesRegex(runner.ReplayError, "writable consolidation metrics mount is missing"):
            self.validate()

    def test_rejects_symlinked_target_mount(self):
        target_mount = self.repo / "target"
        target_mount.symlink_to(self.target_dir)
        with self.assertRaisesRegex(runner.ReplayError, "container target mountpoint must be a real directory"):
            self.validate()

    def test_mounts_source_read_only_and_writable_paths_explicitly(self):
        validated = self.validate()
        command = runner.docker_command(validated, "canonical-run", self.output / "container.cid")
        volumes = [command[index + 1] for index, value in enumerate(command[:-1]) if value == "--volume"]
        self.assertIn(f"{self.repo}:/work:ro", volumes)
        self.assertIn(f"{self.target_dir}:/work/target:rw", volumes)
        self.assertIn(f"{self.cargo_lock}:{runner.CARGO_LOCK_CONTAINER_PATH}:rw", volumes)
        self.assertIn(f"{self.scratch_dir}:/scratch:rw", volumes)
        self.assertIn(f"{self.scratch_dir}:{runner.SCRATCH_CONTAINER_PATH}:rw", volumes)
        self.assertIn(f"{self.metrics}:{runner.METRICS_CONTAINER_PATH}:rw", volumes)
        self.assertFalse(any(value.endswith(":/work:rw") for value in volumes))
        hostname_index = command.index("--hostname")
        self.assertEqual(command[hostname_index + 1], runner.CONTAINER_HOSTNAME)
        hosts_index = command.index("--add-host")
        self.assertEqual(
            command[hosts_index + 1],
            f"{runner.CONTAINER_HOSTNAME}:{runner.CONTAINER_HOST_ADDRESS}",
        )
        self.assertEqual(command[command.index("--network") + 1], "none")
        self.assertIn(f"--cpus={runner.CPU_LIMIT}", command)
        self.assertIn(f"--memory={runner.MEMORY_LIMIT_BYTES // (1024 * 1024 * 1024)}g", command)
        self.assertIn(
            f"--memory-swap={(runner.MEMORY_LIMIT_BYTES + runner.SWAP_LIMIT_BYTES) // (1024 * 1024 * 1024)}g",
            command,
        )
        self.assertEqual(command[-len(runner.batch_command()):], runner.batch_command())
        self.assertEqual(runner.batch_command()[0], f"/work/target/{runner.TARGET}/verification/xsh")
        self.assertNotIn("cargo", runner.batch_command())
        self.assertIn("/work/dev/consolidation/final_canonical_replay.py", command)

    def test_run_batch_creates_validated_target_mount_before_docker_launch(self):
        target_mount = self.repo / "target"
        self.assertFalse(target_mount.exists())
        validated = self.validate()
        self.assertEqual(validated["target_mount"], target_mount)
        process = mock.MagicMock()
        process.poll.return_value = 0
        process.returncode = 0

        def start_process(command, **kwargs):
            self.assertTrue(target_mount.is_dir())
            self.assertIn(f"{self.target_dir}:/work/target:rw", command)
            self.assertEqual(kwargs["cwd"], self.repo)
            return process

        with (
            mock.patch.object(runner, "source_check", return_value={"where": "before"}),
            mock.patch.object(runner, "run_text", return_value=runner.IMAGE) as run_text,
            mock.patch.object(
                runner,
                "git_snapshot",
                return_value={"head": self.head, "tree": self.tree, "clean": True, "status": ""},
            ),
            mock.patch.object(runner, "stop_container", return_value={"resolved": True}),
            mock.patch.object(runner.subprocess, "Popen", side_effect=start_process) as popen,
            mock.patch("builtins.print"),
        ):
            result = runner.run_batch(validated)

        self.assertEqual(result, 1)
        run_text.assert_called_once_with(
            ["docker", "image", "inspect", "--format", "{{.Id}}", runner.IMAGE], self.repo
        )
        popen.assert_called_once()
        receipt_path = self.output / "canonical-replay.json"
        self.assertTrue(receipt_path.is_file())
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        self.assertEqual(
            receipt["docker_argv"],
            runner.docker_command(
                validated,
                "xsh-canonical-canonical-run",
                self.output / "container.cid",
            ),
        )

    def test_resource_schema_accepts_measurement_even_when_batch_failed(self):
        runner.validate_resource_record(resource_record(exit_code=3), 3)
        with self.assertRaisesRegex(runner.ReplayError, "exit code does not match"):
            runner.validate_resource_record(resource_record(exit_code=3), 0)
        record = resource_record()
        record["command"] = ["cargo", "run"]
        with self.assertRaisesRegex(runner.ReplayError, "command does not match the batch command"):
            runner.validate_resource_record(record, 0, runner.batch_command())

    def test_resource_schema_rejects_missing_observed_limits_and_counters(self):
        record = resource_record()
        record["final"]["cgroup"]["memory_swap_max"] = None
        with self.assertRaisesRegex(runner.ReplayError, "memory_swap_max is missing"):
            runner.validate_resource_record(record)

        record = resource_record()
        del record["final"]["cgroup"]["memory_events"]["oom_kill"]
        with self.assertRaisesRegex(runner.ReplayError, "memory_events.oom_kill"):
            runner.validate_resource_record(record)

        record = resource_record()
        record["final"]["cgroup"]["cpu_max"] = None
        with self.assertRaisesRegex(runner.ReplayError, "CPU limit is missing"):
            runner.validate_resource_record(record)

        record = resource_record()
        record["final"]["cgroup"]["memory_max"] = None
        with self.assertRaisesRegex(runner.ReplayError, "memory_max is missing"):
            runner.validate_resource_record(record)

        record = resource_record()
        record["resource_samples"] = 1
        with self.assertRaisesRegex(runner.ReplayError, "resource_samples"):
            runner.validate_resource_record(record)

    def test_resource_schema_rejects_observed_oom(self):
        record = resource_record()
        record["final"]["cgroup"]["memory_events"]["oom"] = 1
        record["cgroup_limits_and_peaks"] = record["final"]["cgroup"]
        record["oom_events"] = 1
        with self.assertRaisesRegex(runner.ReplayError, "recorded an OOM event"):
            runner.validate_resource_record(record)

    def test_resource_schema_rejects_observed_swap_event(self):
        record = resource_record()
        record["final"]["cgroup"]["memory_swap_events"]["high"] = 1
        with self.assertRaisesRegex(runner.ReplayError, "recorded a swap event"):
            runner.validate_resource_record(record)

    def test_cleanup_requires_successful_final_absence_check(self):
        cid_file = self.scratch / "container.cid"
        cid_file.write_text("a" * 64, encoding="utf-8")
        responses = [
            subprocess.CompletedProcess([], 0, "id\n", ""),
            subprocess.CompletedProcess([], 0, "id\n", ""),
            subprocess.CompletedProcess([], 0, "a" * 64 + "\n", ""),
        ]
        with mock.patch.object(runner.subprocess, "run", side_effect=responses):
            self.assertFalse(runner.stop_container(cid_file)["resolved"])

        responses[-1] = subprocess.CompletedProcess([], 0, "", "")
        with mock.patch.object(runner.subprocess, "run", side_effect=responses):
            self.assertTrue(runner.stop_container(cid_file)["resolved"])

    def test_cleanup_rejects_failed_final_absence_check(self):
        cid_file = self.scratch / "container.cid"
        cid_file.write_text("a" * 64, encoding="utf-8")
        responses = [
            subprocess.CompletedProcess([], 0, "", ""),
            subprocess.CompletedProcess([], 0, "", ""),
            subprocess.CompletedProcess([], 1, "", "daemon unavailable"),
        ]
        with mock.patch.object(runner.subprocess, "run", side_effect=responses):
            self.assertFalse(runner.stop_container(cid_file)["resolved"])


def cgroup_snapshot(usage_usec, current_bytes, peak_bytes, pids_peak):
    return {
        "cpu_max": "1200000 100000",
        "cpu_stat": {
            "usage_usec": usage_usec,
            "user_usec": usage_usec // 2,
            "system_usec": usage_usec // 2,
            "nr_periods": 10,
            "nr_throttled": 1,
            "throttled_usec": 100,
        },
        "memory_current": str(current_bytes),
        "memory_peak": str(peak_bytes),
        "memory_max": "25769803776",
        "memory_events": {"low": 0, "high": 0, "max": 0, "oom": 0, "oom_kill": 0, "oom_group_kill": 0},
        "memory_swap_current": "0",
        "memory_swap_peak": "0",
        "memory_swap_max": "0",
        "memory_swap_events": {"fail": 0, "high": 0, "max": 0},
        "pids_current": "2",
        "pids_peak": str(pids_peak),
        "pids_max": "max",
    }


def resource_record(exit_code=0):
    initial = {
        "sampled_utc": "2026-10-16T01:19:59.098+00:00",
        "process_rss_sum_bytes": 19087360,
        "visible_process_count": 2,
        "cgroup": cgroup_snapshot(44475, 21553152, 21553152, 6),
    }
    final_cgroup = cgroup_snapshot(17929202070, 784908288, 5663592448, 204)
    return {
        "schema_version": 1,
        "command": runner.batch_command(),
        "started_utc": "2026-10-16T01:19:59+00:00",
        "finished_utc": "2026-10-16T01:48:28+00:00",
        "exit_code": exit_code,
        "spawn_error": None,
        "received_signal": None,
        "duration_seconds": 2830.562,
        "timing_claim": "none",
        "resource_sample_interval_ms": 100,
        "process_rss_peak_bytes": 4117807104,
        "process_rss_peak_method": "sum of resident pages for visible container processes, sampled at 100 ms",
        "resource_samples": 27946,
        "initial": initial,
        "final": {
            "sampled_utc": "2026-10-16T01:48:28.840+00:00",
            "process_rss_sum_bytes": 17969152,
            "visible_process_count": 2,
            "cgroup": final_cgroup,
        },
        "cgroup_limits_and_peaks": final_cgroup,
        "oom_events": 0,
        "oom_kill_events": 0,
        "swap_events": final_cgroup["memory_swap_events"],
    }


if __name__ == "__main__":
    unittest.main()
