import argparse
import importlib.util
import json
import os
import pathlib
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
import weakref
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
        self.observer_path = self.repo / "dev/consolidation_metrics_current.xsh"
        self.ratchet_path = self.repo / "dev/tests/test-consolidation-ratchets-current.xsh"
        self.attestation_path = self.repo / "dev/consolidation/metric-observer-successor.json"
        for path in (self.observer_path, self.ratchet_path, self.attestation_path):
            path.parent.mkdir(parents=True, exist_ok=True)
        self.observer_path.write_text("observer\n", encoding="utf-8")
        self.ratchet_path.write_text("ratchet\n", encoding="utf-8")
        self.attestation = {
            "schema": 1,
            "successor_scanner_path": str(self.observer_path.relative_to(self.repo)),
            "successor_scanner_sha256": runner.sha256_file(self.observer_path),
            "current_ratchet_tests_path": str(self.ratchet_path.relative_to(self.repo)),
            "current_ratchet_tests_sha256": runner.sha256_file(self.ratchet_path),
        }
        self.attestation_path.write_text(json.dumps(self.attestation), encoding="utf-8")
        subprocess.run(["git", "-C", str(self.repo), "add", "."], check=True)
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
        self.bootstrap_snapshot = self.receipt_path.parent / "xsh"
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
                "executable": f"{runner.BOOTSTRAP_CONTAINER_TARGET_DIR}/{runner.TARGET}/verification/xsh",
            },
            {"reason": "build-finished", "success": True},
        ]

    def create_build_receipt(self, receipt_updates=None):
        records = self.build_records()
        self.build_json_path.write_text("".join(json.dumps(record) + "\n" for record in records), encoding="utf-8")
        self.bootstrap_snapshot.write_bytes(self.bootstrap.read_bytes())
        self.bootstrap_snapshot.chmod(0o755)
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
                "path": str(self.bootstrap_snapshot),
                "size_bytes": self.bootstrap_snapshot.stat().st_size,
                "sha256": runner.sha256_file(self.bootstrap_snapshot),
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

    def bootstrap_arguments(self):
        return argparse.Namespace(
            repo=self.repo,
            head=self.head,
            tree=self.tree,
            output=pathlib.Path(".work/tmp/canonical-bootstrap"),
            run_name="canonical-bootstrap",
            timeout_seconds=60,
        )

    def with_mocked_tools(self):
        real_host_file = runner.host_file

        def host_file(path, executable=False):
            path = pathlib.Path(path)
            if path == self.bootstrap or (path.name == "xsh" and path.is_relative_to(self.scratch_dir)):
                return real_host_file(path, executable=executable)
            return {"path": str(path), "size_bytes": 1, "sha256": "0" * 64}

        return mock.patch.object(runner, "host_file", side_effect=host_file)

    def validate(self, args=None):
        with self.with_mocked_tools():
            return runner.validate_inputs(args or self.arguments())

    def commit_source(self):
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "change source")
        self.head = self.git("rev-parse", "HEAD")
        self.tree = self.git("rev-parse", "HEAD^{tree}")
        self.create_build_receipt()

    def assert_attestation_rejected_before_execution(self, detail):
        for mode in ("bootstrap", "execute"):
            with self.subTest(mode=mode):
                argv = [
                    str(RUNNER_PATH), mode, "--repo", str(self.repo),
                    "--head", self.head, "--tree", self.tree,
                    "--output", str(self.output), "--run-name", self.output.name,
                ]
                if mode == "execute":
                    argv += ["--bootstrap-receipt", str(self.receipt_path)]
                with (
                    mock.patch.object(sys, "argv", argv),
                    mock.patch.object(runner, "run_bootstrap") as bootstrap,
                    mock.patch.object(runner, "run_batch") as batch,
                    mock.patch("builtins.print") as print_error,
                    self.with_mocked_tools(),
                ):
                    self.assertEqual(runner.main(), 2)
                bootstrap.assert_not_called()
                batch.assert_not_called()
                self.assertIn(detail, print_error.call_args.args[0])
                self.assertFalse(self.output.exists())

    def test_rejects_committed_scanner_change_before_bootstrap_or_batch(self):
        self.observer_path.write_text("changed observer\n", encoding="utf-8")
        self.commit_source()
        self.assert_attestation_rejected_before_execution("scanner hash mismatch")

    def test_rejects_committed_ratchet_change_before_bootstrap_or_batch(self):
        self.ratchet_path.write_text("changed ratchet\n", encoding="utf-8")
        self.commit_source()
        self.assert_attestation_rejected_before_execution("ratchet fixture hash mismatch")

    def test_rejects_changed_attestation_before_bootstrap_or_batch(self):
        self.attestation["successor_scanner_sha256"] = "0" * 64
        self.attestation_path.write_text(json.dumps(self.attestation), encoding="utf-8")
        self.commit_source()
        self.assert_attestation_rejected_before_execution("scanner hash mismatch")

    def test_rejects_noncanonical_active_attestation_paths(self):
        for key, value in (
            ("successor_scanner_path", "dev/../dev/consolidation_metrics_current.xsh"),
            ("current_ratchet_tests_path", "../dev/tests/test-consolidation-ratchets-current.xsh"),
        ):
            with self.subTest(key=key):
                attestation = {**self.attestation, key: value}
                self.attestation_path.write_text(json.dumps(attestation), encoding="utf-8")
                self.commit_source()
                self.assert_attestation_rejected_before_execution("noncanonical")

    def test_rejects_missing_bootstrap_binary(self):
        self.bootstrap.unlink()
        with self.assertRaisesRegex(runner.ReplayError, "required host file is missing"):
            self.validate()

    def test_rejects_symlinked_mutable_bootstrap_binary(self):
        self.bootstrap.unlink()
        self.bootstrap.symlink_to(self.bootstrap_snapshot)
        with self.assertRaisesRegex(runner.ReplayError, "bootstrap product path must not contain symlinks"):
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

    def test_rejects_tampered_retained_bootstrap_artifact(self):
        self.bootstrap_snapshot.write_text("#!/bin/sh\nexit 3\n", encoding="utf-8")
        self.bootstrap_snapshot.chmod(0o755)
        with self.assertRaisesRegex(runner.ReplayError, "artifact hash or size"):
            self.validate()

    def test_rejects_missing_retained_bootstrap_artifact(self):
        self.bootstrap_snapshot.unlink()
        with self.assertRaisesRegex(runner.ReplayError, "retained bootstrap artifact file is missing"):
            self.validate()

    def test_rejects_symlinked_retained_bootstrap_artifact(self):
        self.bootstrap_snapshot.unlink()
        self.bootstrap_snapshot.symlink_to(self.bootstrap)
        with self.assertRaisesRegex(runner.ReplayError, "retained bootstrap artifact path contains a symlink"):
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
        records[0]["executable"] = f"{runner.BOOTSTRAP_CONTAINER_TARGET_DIR}/wrong/xsh"
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

    def test_rejects_symlinked_canonical_target_directory(self):
        shutil.rmtree(self.target_dir)
        self.target_dir.symlink_to(self.scratch_dir, target_is_directory=True)
        with self.assertRaisesRegex(runner.ReplayError, "root target directory must exist as a writable real directory"):
            self.validate()

    def test_mounts_source_read_only_and_writable_paths_explicitly(self):
        validated = self.validate()
        command = runner.docker_command(validated, "canonical-run", self.output / "container.cid")
        volumes = [command[index + 1] for index, value in enumerate(command[:-1]) if value == "--volume"]
        self.assertIn(f"{self.repo}:/work:ro", volumes)
        self.assertIn(f"{self.target_dir}:{runner.BOOTSTRAP_CONTAINER_TARGET_DIR}:rw", volumes)
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
        self.assertEqual(
            runner.batch_command()[0],
            f"{runner.BOOTSTRAP_CONTAINER_TARGET_DIR}/{runner.TARGET}/verification/xsh",
        )
        self.assertNotIn("cargo", runner.batch_command())
        self.assertIn("/work/dev/consolidation/final_canonical_replay.py", command)

    def test_bootstrap_command_uses_the_exact_pinned_build_and_target(self):
        with self.with_mocked_tools():
            validated = runner.validate_bootstrap_inputs(self.bootstrap_arguments())
        command = runner.bootstrap_docker_command(
            validated,
            "xsh-canonical-bootstrap-canonical-bootstrap",
            self.output / "container.cid",
        )
        volumes = [command[index + 1] for index, value in enumerate(command[:-1]) if value == "--volume"]
        self.assertEqual(command[-len(runner.BOOTSTRAP_CARGO_ARGV):], runner.BOOTSTRAP_CARGO_ARGV)
        self.assertIn(f"{self.target_dir}:{runner.BOOTSTRAP_CONTAINER_TARGET_DIR}:rw", volumes)
        self.assertIn(f"{self.repo}:/work:ro", volumes)
        self.assertIn(f"{self.scratch_dir}:/scratch:rw", volumes)
        environment = {
            item.split("=", 1)[0]: item.split("=", 1)[1]
            for index, value in enumerate(command[:-1])
            if value == "--env"
            for item in [command[index + 1]]
        }
        self.assertEqual(environment["CARGO_TARGET_DIR"], runner.BOOTSTRAP_CONTAINER_TARGET_DIR)
        self.assertEqual(environment["CARGO_BUILD_JOBS"], str(runner.CPU_LIMIT))
        self.assertEqual(
            environment["CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS"],
            runner.RUSTFLAGS,
        )
        self.assertEqual(environment["TMPDIR"], "/scratch/canonical-bootstrap/tmp")

    def test_bootstrap_producer_writes_a_receipt_accepted_by_the_canonical_validator(self):
        self.create_build_receipt()
        self.receipt_path.unlink()
        receipt_output = self.scratch_dir / "retained-bootstrap" / "bootstrap-receipt.json"
        receipt_output.parent.mkdir()
        source = runner.git_snapshot(self.repo)

        with self.with_mocked_tools():
            receipt = runner.create_bootstrap_receipt(
                receipt_output,
                self.build_json_path,
                self.bootstrap,
                source,
                0,
                self.repo,
                self.scratch_dir,
            )
            validated = runner.validate_bootstrap_receipt(
                receipt_output,
                self.repo,
                self.scratch_dir,
                source,
                receipt_output.parent / "xsh",
            )

        self.assertEqual(receipt["cargo_argv"], runner.BOOTSTRAP_CARGO_ARGV)
        self.assertEqual(receipt["cargo_target_dir"], runner.BOOTSTRAP_CONTAINER_TARGET_DIR)
        self.assertEqual(receipt["rustflags"], runner.RUSTFLAGS)
        self.assertEqual(receipt["build_json"]["sha256"], runner.sha256_file(self.build_json_path))
        retained_path = receipt_output.parent / "xsh"
        self.assertEqual(receipt["artifact"]["path"], str(retained_path))
        self.assertEqual(retained_path.read_bytes(), self.bootstrap.read_bytes())
        self.assertEqual(retained_path.stat().st_mode & 0o777, 0o755)
        self.assertEqual(validated["record"], receipt)

        self.bootstrap.write_text("#!/bin/sh\nexit 9\n", encoding="utf-8")
        self.bootstrap.chmod(0o755)
        with self.with_mocked_tools():
            retained = runner.validate_bootstrap_receipt(
                receipt_output,
                self.repo,
                self.scratch_dir,
                source,
                retained_path,
            )
        self.assertEqual(retained["artifact"]["sha256"], receipt["artifact"]["sha256"])
        with self.assertRaisesRegex(
            runner.ReplayError,
            "mutable target differs from the retained bootstrap artifact",
        ):
            self.validate(self.arguments(receipt_output))

    def test_retained_copy_failure_remains_primary_when_descriptor_close_fails(self):
        retained_path = self.scratch_dir / "copy-failure" / "xsh"
        retained_path.parent.mkdir()
        real_open = os.open
        real_close = os.close
        real_write = os.write
        opened = {}
        closed = []
        copy_error = OSError("snapshot write failed")
        source_close_error = OSError("source close failed")

        def track_open(path, *args):
            descriptor = real_open(path, *args)
            if pathlib.Path(path) == self.bootstrap:
                opened["source"] = descriptor
            elif pathlib.Path(path) == retained_path:
                opened["retained"] = descriptor
            return descriptor

        def fail_snapshot_write(descriptor, data):
            if descriptor == opened.get("retained"):
                raise copy_error
            return real_write(descriptor, data)

        def fail_source_close(descriptor):
            closed.append(descriptor)
            real_close(descriptor)
            if descriptor == opened.get("source"):
                raise source_close_error

        with (
            mock.patch.object(runner.os, "open", side_effect=track_open),
            mock.patch.object(runner.os, "write", side_effect=fail_snapshot_write),
            mock.patch.object(runner.os, "close", side_effect=fail_source_close),
        ):
            with self.assertRaises(OSError) as raised:
                runner.retain_bootstrap_artifact(self.bootstrap, retained_path)

        self.assertIs(raised.exception, copy_error)
        self.assertEqual(closed, [opened["source"], opened["retained"]])
        self.assertTrue(any("source bootstrap artifact descriptor" in note for note in copy_error.__notes__))

    def test_retained_copy_surfaces_close_failure_and_attempts_remaining_close(self):
        retained_path = self.scratch_dir / "close-failure" / "xsh"
        retained_path.parent.mkdir()
        real_open = os.open
        real_close = os.close
        opened = {}
        closed = []
        source_close_error = OSError("source close failed")
        retained_close_error = OSError("retained close failed")

        def track_open(path, *args):
            descriptor = real_open(path, *args)
            if pathlib.Path(path) == self.bootstrap:
                opened["source"] = descriptor
            elif pathlib.Path(path) == retained_path:
                opened["retained"] = descriptor
            return descriptor

        def fail_owned_closes(descriptor):
            closed.append(descriptor)
            real_close(descriptor)
            if descriptor == opened.get("source"):
                raise source_close_error
            if descriptor == opened.get("retained"):
                raise retained_close_error

        with (
            mock.patch.object(runner.os, "open", side_effect=track_open),
            mock.patch.object(runner.os, "close", side_effect=fail_owned_closes),
        ):
            with self.assertRaises(OSError) as raised:
                runner.retain_bootstrap_artifact(self.bootstrap, retained_path)

        self.assertIs(raised.exception, source_close_error)
        self.assertEqual(closed, [opened["source"], opened["retained"]])
        self.assertTrue(any("retained bootstrap artifact descriptor" in note for note in source_close_error.__notes__))

    def test_run_batch_rejects_stale_mutable_bootstrap_before_docker_launch(self):
        with self.with_mocked_tools():
            validated = self.validate()

        def replace_during_image_check(argv, cwd):
            self.bootstrap.write_text("#!/bin/sh\nexit 5\n", encoding="utf-8")
            self.bootstrap.chmod(0o755)
            return runner.IMAGE

        with (
            mock.patch.object(runner, "source_check", return_value={"where": "before"}),
            mock.patch.object(runner, "run_text", side_effect=replace_during_image_check) as run_text,
            mock.patch.object(runner, "stop_container", return_value={"resolved": True}),
            mock.patch.object(runner.subprocess, "Popen") as popen,
        ):
            with self.assertRaisesRegex(
                runner.ReplayError,
                "mutable target differs from the retained bootstrap artifact",
            ):
                runner.run_batch(validated)
        run_text.assert_called_once_with(["docker", "image", "inspect", "--format", "{{.Id}}", runner.IMAGE], self.repo)
        popen.assert_not_called()
        self.assertTrue(validated["output_root"].exists())

    def test_bootstrap_producer_rejects_failed_build_and_wrong_output_directory(self):
        self.create_build_receipt()
        self.receipt_path.unlink()
        receipt_output = self.output / "bootstrap-receipt.json"
        receipt_output.parent.mkdir()
        source = runner.git_snapshot(self.repo)

        with self.assertRaisesRegex(runner.ReplayError, "did not succeed"):
            runner.create_bootstrap_receipt(
                receipt_output,
                self.build_json_path,
                self.bootstrap,
                source,
                1,
                self.repo,
                self.scratch_dir,
            )
        with self.assertRaisesRegex(runner.ReplayError, "outside the pinned target directory"):
            runner.create_bootstrap_receipt(
                receipt_output,
                self.build_json_path,
                self.target_dir / "xsh",
                source,
                0,
                self.repo,
                self.scratch_dir,
            )

    def test_run_bootstrap_captures_cargo_json_and_writes_the_validated_receipt(self):
        shutil.rmtree(self.target_dir)
        source = runner.git_snapshot(self.repo)
        records = self.build_records()
        encoded_records = "".join(json.dumps(record) + "\n" for record in records)
        process = mock.MagicMock()
        process.poll.return_value = 0
        process.returncode = 0
        run_text = runner.run_text

        def run_host_text(command, cwd):
            if command[:3] == ["docker", "image", "inspect"]:
                return runner.IMAGE
            return run_text(command, cwd)

        def start_process(command, **kwargs):
            kwargs["stdout"].write(encoded_records.encode())
            kwargs["stdout"].flush()
            self.assertTrue(self.target_dir.is_dir())
            self.assertTrue((self.target_dir / runner.TARGET).is_dir())
            self.assertTrue(self.bootstrap.parent.is_dir())
            self.bootstrap.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            self.bootstrap.chmod(0o755)
            self.assertEqual(command[-len(runner.BOOTSTRAP_CARGO_ARGV):], runner.BOOTSTRAP_CARGO_ARGV)
            self.assertEqual(kwargs["cwd"], self.repo)
            self.assertEqual(kwargs["env"]["TMPDIR"], str(self.scratch_dir / "canonical-bootstrap" / "tmp"))
            return process

        with self.with_mocked_tools():
            validated = runner.validate_bootstrap_inputs(self.bootstrap_arguments())
        with (
            mock.patch.object(runner, "run_text", side_effect=run_host_text),
            mock.patch.object(runner, "source_check", side_effect=[source, source]),
            mock.patch.object(runner, "stop_container", return_value={"resolved": True}) as stop_container,
            mock.patch.object(runner.subprocess, "Popen", side_effect=start_process),
            mock.patch("builtins.print"),
        ):
            result = runner.run_bootstrap(validated)

        self.assertEqual(result, 0)
        stop_container.assert_called_once()
        receipt_path = self.scratch_dir / "canonical-bootstrap" / "bootstrap-receipt.json"
        cargo_json_path = self.scratch_dir / "canonical-bootstrap" / "cargo-build.jsonl"
        self.assertEqual(cargo_json_path.read_text(encoding="utf-8"), encoded_records)
        with self.with_mocked_tools():
            receipt = runner.validate_bootstrap_receipt(
                receipt_path,
                self.repo,
                self.scratch_dir,
                source,
                receipt_path.parent / "xsh",
            )
        self.assertEqual(receipt["record"]["cargo_argv"], runner.BOOTSTRAP_CARGO_ARGV)

    def completed_child_factory(self, script, events, signal_after_exit=None):
        real_popen = subprocess.Popen
        child_holder = {}

        class CompletedChild:
            def __init__(self, child):
                self.child = child
                self.pid = child.pid
                self.returncode = child.returncode

            def poll(self):
                self.returncode = self.child.poll()
                return self.returncode

            def wait(self, timeout=None):
                self.returncode = self.child.wait(timeout=timeout)
                return self.returncode

        def start_process(_command, **kwargs):
            child = real_popen(
                [sys.executable, "-c", script],
                cwd=kwargs["cwd"],
                env=kwargs["env"],
                stdin=kwargs["stdin"],
                stdout=kwargs["stdout"],
                stderr=kwargs["stderr"],
                start_new_session=kwargs["start_new_session"],
            )
            status = child.wait(timeout=5)
            events.append(("child-exited", status))
            if signal_after_exit is not None:
                signal.raise_signal(signal_after_exit)
            process = CompletedChild(child)
            child_holder["process"] = weakref.ref(process)
            return process

        return start_process, child_holder

    def bootstrap_success_script(self):
        records = "".join(json.dumps(record) + "\n" for record in self.build_records())
        return (
            "import pathlib,sys; "
            f"sys.stdout.write({records!r}); "
            f"artifact=pathlib.Path({str(self.bootstrap)!r}); "
            "artifact.write_text('#!/bin/sh\\nexit 0\\n'); "
            "artifact.chmod(0o755)"
        )

    def batch_script(self, exit_code):
        record = json.dumps(resource_record(exit_code))
        resource_path = self.output / "container-resources.json"
        return (
            "import pathlib; "
            f"pathlib.Path({str(resource_path)!r}).write_text({record!r}); "
            f"raise SystemExit({exit_code})"
        )

    def run_batch_with_completed_child(self, exit_code, signal_after_exit=None):
        validated = self.validate()
        events = []
        start_process, child_holder = self.completed_child_factory(
            self.batch_script(exit_code), events, signal_after_exit=signal_after_exit
        )
        run_text = runner.run_text

        def run_host_text(command, cwd):
            if command[:3] == ["docker", "image", "inspect"]:
                return runner.IMAGE
            return run_text(command, cwd)

        def cleanup(*_args):
            events.append("cleanup")
            process = child_holder["process"]()
            self.assertIsNotNone(process)
            self.assertEqual(process.returncode, exit_code)
            receipt = json.loads((self.output / "canonical-replay.json").read_text(encoding="utf-8"))
            self.assertEqual(receipt["status"], "running")
            return {"resolved": True}

        with (
            mock.patch.object(runner, "source_check", return_value={"where": "before"}),
            mock.patch.object(
                runner,
                "git_snapshot",
                return_value={"head": self.head, "tree": self.tree, "clean": True, "status": ""},
            ),
            mock.patch.object(runner, "run_text", side_effect=run_host_text),
            mock.patch.object(runner, "stop_container", side_effect=cleanup),
            mock.patch.object(runner.subprocess, "Popen", side_effect=start_process),
            mock.patch("builtins.print"),
        ):
            result = runner.run_batch(validated)

        receipt = json.loads((self.output / "canonical-replay.json").read_text(encoding="utf-8"))
        return result, events, receipt

    def test_run_bootstrap_consumes_child_that_exited_before_first_poll(self):
        shutil.rmtree(self.target_dir)
        source = runner.git_snapshot(self.repo)
        events = []
        script = self.bootstrap_success_script()
        start_process, child_holder = self.completed_child_factory(script, events)

        def cleanup(*_args):
            events.append("cleanup")
            process = child_holder["process"]()
            self.assertIsNotNone(process)
            self.assertIsNotNone(process.returncode)
            self.assertFalse((self.scratch_dir / "canonical-bootstrap" / "bootstrap-receipt.json").exists())
            return {"resolved": True}

        with self.with_mocked_tools():
            validated = runner.validate_bootstrap_inputs(self.bootstrap_arguments())
        with (
            self.with_mocked_tools(),
            mock.patch.object(runner, "run_text", return_value=runner.IMAGE),
            mock.patch.object(runner, "source_check", side_effect=[source, source]),
            mock.patch.object(runner, "stop_container", side_effect=cleanup),
            mock.patch.object(runner.subprocess, "Popen", side_effect=start_process),
            mock.patch("builtins.print"),
        ):
            result = runner.run_bootstrap(validated)

        receipt_path = self.scratch_dir / "canonical-bootstrap" / "bootstrap-receipt.json"
        self.assertEqual(result, 0)
        self.assertEqual(events, [("child-exited", 0), "cleanup"])
        self.assertTrue(receipt_path.is_file())
        with self.with_mocked_tools():
            execute = self.validate(args=self.arguments(receipt=receipt_path))
        self.assertEqual(execute["bootstrap_receipt"]["record"]["status"], "passed")

    def test_run_batch_records_quick_success_after_observing_and_cleaning_child(self):
        result, events, receipt = self.run_batch_with_completed_child(0)

        self.assertEqual(result, 0)
        self.assertEqual(events, [("child-exited", 0), "cleanup"])
        self.assertEqual(receipt["exit_code"], 0)
        self.assertEqual(receipt["status"], "passed")
        self.assertTrue(receipt["cleanup"]["resolved"])

    def test_run_batch_records_quick_failure_as_terminal(self):
        result, events, receipt = self.run_batch_with_completed_child(7)

        self.assertEqual(result, 1)
        self.assertEqual(events, [("child-exited", 7), "cleanup"])
        self.assertEqual(receipt["exit_code"], 7)
        self.assertEqual(receipt["status"], "failed-or-incomplete")

    def test_run_batch_records_interruption_after_quick_success_as_terminal(self):
        result, events, receipt = self.run_batch_with_completed_child(0, signal.SIGTERM)

        self.assertEqual(result, 1)
        self.assertEqual(events, [("child-exited", 0), "cleanup"])
        self.assertEqual(receipt["exit_code"], 0)
        self.assertEqual(receipt["received_signal"], "SIGTERM")
        self.assertEqual(receipt["status"], "failed-or-incomplete")

    def test_run_bootstrap_failure_blocks_dependent_execution_after_quick_exit(self):
        shutil.rmtree(self.target_dir)
        source = runner.git_snapshot(self.repo)
        events = []
        start_process, child_holder = self.completed_child_factory("import sys; sys.exit(7)", events)

        def cleanup(*_args):
            events.append("cleanup")
            process = child_holder["process"]()
            self.assertIsNotNone(process)
            self.assertEqual(process.returncode, 7)
            return {"resolved": True}

        with self.with_mocked_tools():
            validated = runner.validate_bootstrap_inputs(self.bootstrap_arguments())
        with (
            self.with_mocked_tools(),
            mock.patch.object(runner, "run_text", return_value=runner.IMAGE),
            mock.patch.object(runner, "source_check", side_effect=[source, source]),
            mock.patch.object(runner, "stop_container", side_effect=cleanup),
            mock.patch.object(runner.subprocess, "Popen", side_effect=start_process),
            mock.patch("builtins.print"),
        ):
            with self.assertRaisesRegex(runner.ReplayError, "build exited 7"):
                runner.run_bootstrap(validated)

        receipt_path = self.scratch_dir / "canonical-bootstrap" / "bootstrap-receipt.json"
        self.assertFalse(receipt_path.exists())
        self.assertEqual(events, [("child-exited", 7), "cleanup"])
        with self.assertRaisesRegex(runner.ReplayError, "bootstrap receipt file is missing"):
            self.validate(args=self.arguments(receipt=receipt_path))

    def test_run_bootstrap_interruption_after_quick_success_blocks_receipt(self):
        shutil.rmtree(self.target_dir)
        source = runner.git_snapshot(self.repo)
        events = []
        start_process, child_holder = self.completed_child_factory(
            self.bootstrap_success_script(), events, signal_after_exit=signal.SIGTERM
        )

        def cleanup(*_args):
            events.append("cleanup")
            process = child_holder["process"]()
            self.assertIsNotNone(process)
            self.assertEqual(process.returncode, 0)
            return {"resolved": True}

        with self.with_mocked_tools():
            validated = runner.validate_bootstrap_inputs(self.bootstrap_arguments())
        with (
            self.with_mocked_tools(),
            mock.patch.object(runner, "run_text", return_value=runner.IMAGE),
            mock.patch.object(runner, "source_check", side_effect=[source, source]),
            mock.patch.object(runner, "stop_container", side_effect=cleanup),
            mock.patch.object(runner.subprocess, "Popen", side_effect=start_process),
            mock.patch("builtins.print"),
        ):
            with self.assertRaisesRegex(runner.ReplayError, "build interrupted by SIGTERM"):
                runner.run_bootstrap(validated)

        self.assertEqual(events, [("child-exited", 0), "cleanup"])
        self.assertFalse((self.scratch_dir / "canonical-bootstrap" / "bootstrap-receipt.json").exists())

    def test_run_bootstrap_quarantines_unresolved_cleanup_after_runner_exception(self):
        with self.with_mocked_tools():
            validated = runner.validate_bootstrap_inputs(self.bootstrap_arguments())
        process = mock.MagicMock()
        process.poll.side_effect = RuntimeError("runner poll failed")
        with (
            self.with_mocked_tools(),
            mock.patch.object(runner, "run_text", return_value=runner.IMAGE),
            mock.patch.object(runner, "source_check", return_value=runner.git_snapshot(self.repo)),
            mock.patch.object(runner, "stop_container", return_value={"resolved": False, "container_id": "a" * 64}) as stop_container,
            mock.patch.object(runner.subprocess, "Popen", return_value=process) as popen,
            mock.patch("builtins.print"),
        ):
            with self.assertRaisesRegex(runner.ReplayError, "cleanup is unresolved"):
                runner.run_bootstrap(validated)
            with self.assertRaisesRegex(runner.ReplayError, "cleanup remains unresolved"):
                runner.run_bootstrap(validated)

        stop_container.assert_called_once()
        popen.assert_called_once()
        marker_path = self.native_lock.with_suffix(".cleanup-unresolved")
        lock_record = self.native_lock.read_text(encoding="utf-8")
        self.assertTrue(lock_record.startswith("cleanup-unresolved:"))
        self.assertIn("runner poll failed", lock_record)
        self.assertEqual(marker_path.read_text(encoding="utf-8"), lock_record)
        self.assertFalse((self.scratch_dir / "canonical-bootstrap" / "bootstrap-receipt.json").exists())

    def test_run_bootstrap_reaps_runner_after_oserror_and_preserves_child_stderr(self):
        with self.with_mocked_tools():
            validated = runner.validate_bootstrap_inputs(self.bootstrap_arguments())
        real_popen = subprocess.Popen
        child_holder = {}
        stderr_path = self.scratch_dir / "canonical-bootstrap" / "cargo-build.stderr.log"

        class PollFailure:
            def __init__(self, child):
                self.child = child
                self.pid = child.pid

            @property
            def returncode(self):
                return self.child.returncode

            def poll(self):
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    if stderr_path.is_file() and "child diagnostic" in stderr_path.read_text(encoding="utf-8"):
                        raise OSError("runner poll failed after launch")
                    time.sleep(0.01)
                raise AssertionError("child did not write its diagnostic")

            def wait(self, timeout=None):
                return self.child.wait(timeout=timeout)

        def start_process(_command, **kwargs):
            child = real_popen(
                [sys.executable, "-c", "import sys,time; print('child diagnostic', file=sys.stderr, flush=True); time.sleep(60)"],
                cwd=kwargs["cwd"],
                env=kwargs["env"],
                stdin=kwargs["stdin"],
                stdout=kwargs["stdout"],
                stderr=kwargs["stderr"],
                start_new_session=kwargs["start_new_session"],
            )
            child_holder["child"] = child
            return PollFailure(child)

        try:
            with (
                self.with_mocked_tools(),
                mock.patch.object(runner, "run_text", return_value=runner.IMAGE),
                mock.patch.object(runner, "source_check", return_value=runner.git_snapshot(self.repo)),
                mock.patch.object(runner, "stop_container", return_value={"resolved": True}) as stop_container,
                mock.patch.object(runner.subprocess, "Popen", side_effect=start_process),
                mock.patch.object(runner, "TERM_GRACE_SECONDS", 0.01),
                mock.patch("builtins.print"),
            ):
                with self.assertRaisesRegex(OSError, "runner poll failed after launch"):
                    runner.run_bootstrap(validated)

            stop_container.assert_called_once()
            self.assertEqual(child_holder["child"].returncode, -signal.SIGKILL)
            stderr_log = stderr_path.read_text(encoding="utf-8")
            self.assertIn("child diagnostic", stderr_log)
            self.assertNotIn("bootstrap launcher error", stderr_log)
        finally:
            child = child_holder.get("child")
            if child is not None and child.returncode is None:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                child.wait()

    def test_canonical_target_mount_uses_existing_worktree_path(self):
        self.assertEqual(runner.BOOTSTRAP_CONTAINER_TARGET_DIR, "/work/.work/target-final-canonical")
        validated = self.validate()
        with self.with_mocked_tools():
            bootstrap_validated = runner.validate_bootstrap_inputs(self.bootstrap_arguments())
        commands = (
            runner.docker_command(validated, "canonical-run", self.output / "container.cid"),
            runner.bootstrap_docker_command(
                bootstrap_validated,
                "xsh-canonical-bootstrap-canonical-run",
                self.output / "bootstrap.cid",
            ),
        )
        for command in commands:
            volumes = [command[index + 1] for index, value in enumerate(command[:-1]) if value == "--volume"]
            self.assertIn(f"{self.target_dir}:{runner.BOOTSTRAP_CONTAINER_TARGET_DIR}:rw", volumes)
            self.assertNotIn(f"{self.target_dir}:/work/target:rw", volumes)

    def test_run_batch_does_not_create_a_root_target_side_path(self):
        target_mount = self.repo / "target"
        self.assertFalse(target_mount.exists())
        validated = self.validate()
        process = mock.MagicMock()
        process.poll.return_value = 0
        process.returncode = 0

        def start_process(command, **kwargs):
            self.assertFalse(target_mount.exists())
            self.assertIn(f"{self.target_dir}:{runner.BOOTSTRAP_CONTAINER_TARGET_DIR}:rw", command)
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
