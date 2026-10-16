import argparse
import importlib.util
import os
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
        (self.repo / ".git/info/exclude").write_text("/.work/\n", encoding="utf-8")
        (self.repo / "source.txt").write_text("source\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(self.repo), "add", "source.txt"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "commit", "--quiet", "-m", "source"], check=True)
        self.head = self.git("rev-parse", "HEAD")
        self.tree = self.git("rev-parse", "HEAD^{tree}")
        self.output = self.repo / ".work/tmp/canonical-run"
        self.target_dir = self.repo / ".work/target-final-canonical"
        self.bootstrap = self.target_dir / runner.TARGET / "verification" / "xsh"
        self.infra = self.repo / ".work/consolidation/infra"
        self.native_lock = self.infra / "native-suite.lock"
        self.cargo_lock = self.infra / "cargo.lock"
        (self.repo / ".work/tmp").mkdir(parents=True)
        self.infra.mkdir(parents=True)
        self.native_lock.write_text("", encoding="utf-8")
        self.cargo_lock.write_text("", encoding="utf-8")
        self.create_bootstrap()

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

    def arguments(self, bootstrap=None):
        return argparse.Namespace(
            repo=self.repo,
            head=self.head,
            tree=self.tree,
            output=pathlib.Path(".work/tmp/canonical-run"),
            run_name="canonical-run",
            timeout_seconds=60,
            bootstrap_xsh=self.bootstrap if bootstrap is None else bootstrap,
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

    def test_rejects_bootstrap_outside_the_pinned_target_profile(self):
        wrong_bootstrap = self.target_dir / "xsh"
        with self.assertRaisesRegex(runner.ReplayError, "must be the verification product"):
            self.validate(self.arguments(bootstrap=wrong_bootstrap))

    def test_rejects_missing_shared_cargo_lock(self):
        self.cargo_lock.unlink()
        with self.assertRaisesRegex(runner.ReplayError, "shared Cargo lock is missing"):
            self.validate()

    def test_rejects_missing_native_suite_lock(self):
        self.native_lock.unlink()
        with self.assertRaisesRegex(runner.ReplayError, "shared native-suite lock is missing"):
            self.validate()

    def test_direct_bootstrap_command_mounts_the_shared_host_lock(self):
        validated = self.validate()
        command = runner.docker_command(validated, "canonical-run", self.output / "container.cid")
        volumes = [command[index + 1] for index, value in enumerate(command[:-1]) if value == "--volume"]
        self.assertEqual(runner.CARGO_LOCK_CONTAINER_PATH, "/work/.work/consolidation/infra/cargo.lock")
        expected_mount = f"{self.cargo_lock}:{runner.CARGO_LOCK_CONTAINER_PATH}:rw"
        self.assertIn(expected_mount, volumes)
        self.assertEqual(command[-len(runner.batch_command()):], runner.batch_command())
        self.assertEqual(
            runner.batch_command()[0],
            f"/work/target/{runner.TARGET}/verification/xsh",
        )
        self.assertNotIn("cargo", runner.batch_command())
        self.assertIn("/work/dev/consolidation/final_canonical_replay.py", command)


if __name__ == "__main__":
    unittest.main()
