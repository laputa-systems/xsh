#!/usr/bin/env python3
"""Exercise final correctness cleanup decisions without Docker."""

import fcntl
import importlib.util
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True

LAUNCHER_PATH = pathlib.Path(__file__).with_name("launch.py")
SPEC = importlib.util.spec_from_file_location("final_correctness_launch", LAUNCHER_PATH)
assert SPEC is not None and SPEC.loader is not None
LAUNCHER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = LAUNCHER
SPEC.loader.exec_module(LAUNCHER)
SCRATCH_ROOT = LAUNCHER.ROOT / ".work/tmp"
SCRATCH_ROOT.mkdir(parents=True, exist_ok=True)
NATIVE_RUNNER = LAUNCHER.HERE / "run-native-final.sh"
CANONICAL_NATIVE_LOCK = LAUNCHER.ROOT / ".work/consolidation/infra/native-suite.lock"
MANIFEST_PATH = LAUNCHER.HERE / "manifest.json"


class CoverageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.plan = json.loads(MANIFEST_PATH.read_text(), object_pairs_hook=LAUNCHER.unique_json_object)

    def test_batch_owns_the_broad_rust_native_and_docs_surfaces_once(self):
        workflow = (LAUNCHER.ROOT / "dev/test_workflows.xsh").read_text()
        gates = (LAUNCHER.ROOT / "dev/gates.xsh").read_text()
        root_tests = re.search(
            r'\["cargo", "test", @profile_args, "-p", "xsh", (.*?)\],\s*ctx\.root, environment\)',
            workflow,
            re.S,
        )
        self.assertIsNotNone(root_tests)
        self.assertEqual(
            " ".join(root_tests.group(1).split()),
            '"--test", "integration", "--test", "ambient_fs_policy", "--test", "symbol_plateau", '
            '"--test", "core_compat_boundaries", "--test", "core_compat_extra", "--", "--skip", "split::", '
            '"--skip", "runtime::coverage::", "--skip", "runtime::examples::example_corpus_is_formatted", '
            '"--skip", "runtime::examples::example_corpus_lints_without_warnings", @sibling_syntax_exclusions',
        )
        self.assertEqual(
            workflow.count('"--skip", "cp::privileged::test_cp_recursive_block_device_"'),
            1,
        )
        self.assertRegex(
            workflow,
            r'"--test", "linux_priv", "--", "--nocapture", "--skip", "split::",\s*'
            r'"--skip", "cp::privileged::test_cp_recursive_block_device_"',
        )
        integration = (LAUNCHER.ROOT / "tests/integration.rs").read_text()
        for module in (
            "cli",
            "core",
            "diagnostics",
            "libc_hygiene",
            "libxsh_api",
            "runtime",
            "sema",
            "sema_bundle_view",
            "stdlib_port",
            "syntax",
        ):
            self.assertRegex(integration, rf"(?m)^mod {module};$")
        self.assertIn("@sibling_syntax_exclusions", workflow)
        self.assertIn("@sibling_xsht_exclusions", workflow)
        self.assertIn('["--skip", "syntax::formatter_is_idempotent_on_laputa_corpus"]', workflow)
        self.assertIn('["--skip", "lint_format_invariance::formatting_preserves_lints_on_laputa_corpus"]', workflow)
        batch = re.search(r"export proc batch\(.*?\n}\n", gates, re.S)
        full = re.search(r"export proc full\(.*?\n}\n", workflow, re.S)
        self.assertIsNotNone(batch)
        self.assertIsNotNone(full)
        self.assertEqual(batch.group(0).count('tests.full(ctx, profile, privileged: ctx.target.os == "linux")'), 1)
        self.assertEqual(batch.group(0).count("docs_gate(ctx, profile)"), 1)
        self.assertEqual(full.group(0).count("native(ctx, profile, [])"), 1)
        self.assertEqual(self.plan["rust_harness_runs"], [])
        self.assertEqual(self.plan["focused_native_files"], [])
        self.assertEqual(self.plan["native_extra"], [])

    def test_privileged_cp_cases_use_the_feature_enabled_target_once_as_root(self):
        builds = {item["id"]: item for item in self.plan["rust_harness_builds"]}
        build = builds["core-compat-extra-privileged"]
        self.assertEqual(
            build["args"],
            ["-p", "xsh", "--test", "core_compat_extra_privileged", "--features", "linux-priv-tests"],
        )
        self.assertEqual(build["targets"], ["core_compat_extra_privileged"])
        self.assertEqual(build["target_variant"], "linux-priv")
        runs = [
            run for run in self.plan["linux_context_runs"]
            if run.get("harness") == "core-compat-extra-privileged"
        ]
        self.assertEqual(len(runs), 1)
        self.assertEqual(
            runs[0],
            {
                "harness": "core-compat-extra-privileged",
                "filter": "cp::privileged::test_cp_recursive_block_device_",
                "network": "none",
                "privileged": True,
                "memory": "1g",
            },
        )
        launcher = LAUNCHER_PATH.read_text()
        self.assertIn('"--tmpfs", "/tmp:rw,nodev,exec,nosuid,mode=1777"', launcher)

    def test_only_uid_1000_traceroute_is_supplemental(self):
        runs = [
            run for run in self.plan["linux_context_runs"]
            if run.get("id", "").startswith("traceroute-")
        ]
        self.assertEqual(
            runs,
            [{
                "id": "traceroute-unprivileged",
                "binary": "xsht",
                "argv": ["test", "core/tests/test-traceroute.xsh", "-j", "1"],
                "network": "none",
                "privileged": False,
                "user": "1000:1000",
                "memory": "1g",
            }],
        )

    def test_core_compatibility_runs_only_in_distinct_linux_contexts(self):
        ordinary = [
            run for run in self.plan["linux_context_runs"]
            if run.get("harness") in {"core-compat-boundaries", "core-compat-extra"}
        ]
        self.assertEqual(
            [(run["harness"], run["filter"], run.get("skip_filter")) for run in ordinary],
            [
                ("core-compat-boundaries", "", "split::"),
                ("core-compat-extra", "", "split::"),
                ("core-compat-extra", "split::", None),
            ],
        )
        self.assertEqual(
            {build["id"] for build in self.plan["rust_harness_builds"]},
            {"core-compat-boundaries", "core-compat-extra", "core-compat-extra-privileged"},
        )


class CleanupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=SCRATCH_ROOT)
        self.root = pathlib.Path(self.temporary.name)
        self.infra = self.root / "infra"
        self.infra.mkdir()
        self.out = self.root / "run"
        self.out.mkdir()
        self.cid_dir = self.out / "container-ids"
        self.cid_dir.mkdir()
        self.cidfile = self.cid_dir / "step.cid"
        self.log = self.out / "step.log"
        self.log.write_text("")
        self.gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        self.gate.out = self.out
        self.gate.receipt = {"steps": []}
        LAUNCHER.INFRA = self.infra
        LAUNCHER.NATIVE_LOCK = self.infra / "native-suite.lock"
        LAUNCHER.NATIVE_CLEANUP_MARKER = self.infra / "native-suite.cleanup-unresolved"

    def tearDown(self):
        self.temporary.cleanup()

    @staticmethod
    def cleanup_result(argv, *, exit_code=0, stdout=""):
        return {
            "argv": argv,
            "resolved": exit_code == 0,
            "timed_out": False,
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": "",
        }

    def test_removed_container_is_confirmed_without_docker(self):
        container_id = "a" * 64
        self.cidfile.write_text(container_id + "\n")
        calls = []

        def fake_command(_log, argv):
            calls.append(argv)
            return self.cleanup_result(argv, stdout="" if argv[1] == "ps" else "")

        with mock.patch.object(self.gate, "append_cleanup", side_effect=fake_command):
            result = self.gate.stop_owned_container(self.cidfile, self.log)

        self.assertTrue(result["resolved"])
        self.assertEqual(result["container_id"], container_id)
        self.assertEqual([call[1] for call in calls], ["stop", "rm", "ps"])
        self.assertTrue(self.cidfile.exists())
        self.assertFalse(LAUNCHER.NATIVE_CLEANUP_MARKER.exists())

    def test_remaining_container_persists_cidfile_marker_and_blocks_next_launch(self):
        container_id = "b" * 64
        self.cidfile.write_text(container_id + "\n")
        calls = []

        def fake_command(_log, argv):
            calls.append(argv)
            return self.cleanup_result(argv, stdout=container_id + "\n" if argv[1] == "ps" else "")

        with mock.patch.object(self.gate, "append_cleanup", side_effect=fake_command):
            result = self.gate.stop_owned_container(self.cidfile, self.log)
        self.assertFalse(result["resolved"])
        self.gate.retain_unresolved_cleanup(
            step_id="timed-step",
            cidfile=self.cidfile,
            container_cleanup=result,
            process_cleanup={"resolved": True, "actions": []},
        )

        self.assertTrue(self.cidfile.exists())
        self.assertTrue(LAUNCHER.NATIVE_CLEANUP_MARKER.exists())
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "cleanup is unresolved"):
            self.gate.require_native_lock()

    def test_held_canonical_lock_rejects_another_gate(self):
        with LAUNCHER.NATIVE_LOCK.open("a+") as holder:
            fcntl.flock(holder.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(LAUNCHER.GateFailure, "shared native-suite lock is held"):
                self.gate.require_native_lock()
            fcntl.flock(holder.fileno(), fcntl.LOCK_UN)

    def test_final_runner_rejects_held_lock_before_docker(self):
        target = LAUNCHER.HERE / "lock-rejection-self-test" / "target" / "verification"
        cidfile = SCRATCH_ROOT / "lock-rejection-self-test.cid"
        env = dict(os.environ)
        env.pop("XSH_GATE_NATIVE_LOCK_FD", None)
        env["XSH_GATE_CIDFILE"] = str(cidfile)
        env["XSH_GATE_TARGET_DIR"] = str(target)
        with CANONICAL_NATIVE_LOCK.open("a+") as holder:
            try:
                fcntl.flock(holder.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                self.fail(f"canonical native-suite lock is already held: {error}")
            result = subprocess.run(
                ["bash", str(NATIVE_RUNNER), str(LAUNCHER.ROOT), "build", "cargo", "build", "--bin", "xsh"],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            fcntl.flock(holder.fileno(), fcntl.LOCK_UN)

        self.assertEqual(result.returncode, 75, result.stderr)
        self.assertIn("canonical native-suite lock is held", result.stderr)
        self.assertFalse(target.parent.exists())
        self.assertFalse(cidfile.exists())


if __name__ == "__main__":
    unittest.main()
