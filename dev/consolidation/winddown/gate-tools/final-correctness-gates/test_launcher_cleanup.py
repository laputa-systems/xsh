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
SOURCE_DIR = LAUNCHER_PATH.parent
PLAN_SOURCE_PATH = LAUNCHER_PATH.with_name("plan_source.py")
PLAN_SOURCE_SPEC = importlib.util.spec_from_file_location("final_correctness_plan_source", PLAN_SOURCE_PATH)
assert PLAN_SOURCE_SPEC is not None and PLAN_SOURCE_SPEC.loader is not None
PLAN_SOURCE = importlib.util.module_from_spec(PLAN_SOURCE_SPEC)
PLAN_SOURCE_SPEC.loader.exec_module(PLAN_SOURCE)
plan_data = PLAN_SOURCE.plan_data
SPEC = importlib.util.spec_from_file_location("final_correctness_launch", LAUNCHER_PATH)
assert SPEC is not None and SPEC.loader is not None
LAUNCHER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = LAUNCHER
SPEC.loader.exec_module(LAUNCHER)


def source_root(path):
    return next(
        parent
        for parent in pathlib.Path(path).resolve().parents
        if (parent / "Cargo.toml").is_file() and (parent / "dev").is_dir()
    )


SOURCE_ROOT = source_root(LAUNCHER_PATH)
SCRATCH_ROOT = SOURCE_ROOT / ".work/tmp"
SCRATCH_ROOT.mkdir(parents=True, exist_ok=True)
NATIVE_RUNNER = SOURCE_DIR / "run-native-final.sh"
CANONICAL_NATIVE_LOCK = LAUNCHER.ROOT / ".work/consolidation/infra/native-suite.lock"


class CoverageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.plan = plan_data()

    def test_batch_owns_the_broad_rust_native_and_docs_surfaces_once(self):
        workflow = (SOURCE_ROOT / "dev/test_workflows.xsh").read_text()
        gates = (SOURCE_ROOT / "dev/gates.xsh").read_text()
        rust_plan = re.search(r'export pure rust_plan\(.*?\n}\n', workflow, re.S)
        self.assertIsNotNone(rust_plan)
        target_loop = re.search(r'for target in \[(.*?)\]\s*\{', rust_plan.group(0), re.S)
        self.assertIsNotNone(target_loop)
        self.assertEqual(
            re.findall(r'"([^"]+)"', target_loop.group(1)),
            ["integration", "ambient_fs_policy", "symbol_plateau", "core_compat_boundaries", "core_compat_extra"],
        )
        self.assertRegex(
            rust_plan.group(0),
            r'\["cargo", "test", @profile_args, "-p", "xsh", "--test", target, "--",\s*'
            r'"--skip", "split::", "--skip", "runtime::coverage::"\]',
        )
        self.assertEqual(
            workflow.count('"--skip", "cp::privileged::test_cp_recursive_block_device_"'),
            1,
        )
        privileged_loop = re.search(r'if privileged \{\s*for target in \[(.*?)\]\s*\{', rust_plan.group(0), re.S)
        self.assertIsNotNone(privileged_loop)
        self.assertEqual(re.findall(r'"([^"]+)"', privileged_loop.group(1)), ["core_compat_extra", "linux_priv"])
        self.assertRegex(
            rust_plan.group(0),
            r'"--test", target,\s*"--", "--nocapture", "--skip", "split::",\s*'
            r'"--skip", "cp::privileged::test_cp_recursive_block_device_"',
        )
        integration = (SOURCE_ROOT / "tests/integration.rs").read_text()
        for module in (
            "cli",
            "core",
            "diagnostics",
            "grammar",
            "libc_hygiene",
            "libxsh_api",
            "runtime",
            "sema",
            "sema_bundle_view",
            "stdlib_port",
        ):
            self.assertRegex(integration, rf"(?m)^mod {module};$")
        self.assertIn("@sibling_syntax_exclusions", workflow)
        self.assertIn("@xsht_integration_exclusions", workflow)
        self.assertIn('["--skip", "syntax::formatter_is_idempotent_on_laputa_corpus"]', workflow)
        self.assertIn('"--skip", "lint_format_invariance::formatting_preserves_lints_on_laputa_corpus"', workflow)
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
        launcher = LAUNCHER_PATH.read_text()
        runner = NATIVE_RUNNER.read_text()
        self.assertNotIn("baseline-bin", launcher)
        self.assertNotIn("baseline-bin", runner)
        self.assertNotIn("XSH_GATE_BOOTSTRAP_DIR", launcher)
        self.assertNotIn("XSH_GATE_BOOTSTRAP_DIR", runner)
        self.assertIn('"xsh-verification",\n            "xsht-verification",\n            "xshi-verification",\n            "xsh-fuzz-verification",\n            "xsh-test-helper-verification",', launcher)

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

    def test_approved_plan_matches_current_product_and_lint_contract(self):
        self.assertEqual(self.plan["plan_status"], "approved")
        self.assertEqual(
            self.plan["products"][0],
            [
                "cargo", "build", "--locked", "--offline", "--profile", "verification",
                "--target", LAUNCHER.TARGET, "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht",
                "-p", "xshi", "--bin", "xshi", "-p", "xsh-fuzz", "--bins", "--message-format=json",
            ],
        )
        self.assertEqual(len(self.plan["products"]), 1)
        self.assertEqual(
            set(self.plan["product_variants"]["verification"]["binaries"]),
            {"xsh", "xsht", "xshi", "xsh-fuzz", "xsh-test-helper"},
        )
        self.assertEqual(set(self.plan["product_variants"]), {"verification"})
        self.assertEqual(
            self.plan["warning_deny"],
            [[
                "cargo", "clippy", "--locked", "--offline", "--workspace", "--target", LAUNCHER.TARGET,
                "--all-targets", "--all-features", "--quiet", "--", "-D", "warnings",
            ]],
        )

    def test_cleanup_self_test_uses_the_checkout_root_for_scratch(self):
        copied_test = LAUNCHER.ROOT / ".work/final-review-evidence/final-correctness-gates/test_launcher_cleanup.py"
        self.assertEqual(source_root(copied_test), LAUNCHER.ROOT)
        self.assertEqual(SCRATCH_ROOT, SOURCE_ROOT / ".work/tmp")


class ArtifactQualificationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=SCRATCH_ROOT)
        self.root = pathlib.Path(self.temporary.name)
        self.gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        self.gate.expected_commit = "a" * 40
        self.gate.expected_tree = "b" * 40
        self.gate.target_dir = self.root / "target-products"
        self.gate.harness_target_dir = self.root / "target-harness"
        self.gate.runtime_target_dir = self.root / "target-runtime"
        self.gate.linux_priv_target_dir = self.root / "target-linux-priv"
        for target_dir in (
            self.gate.target_dir,
            self.gate.harness_target_dir,
            self.gate.runtime_target_dir,
            self.gate.linux_priv_target_dir,
        ):
            target_dir.mkdir()
        self.gate.qualified_harness_xsh_variants = {}
        self.gate.receipt = {"source_checks": [], "hashes": {}}

    def tearDown(self):
        self.temporary.cleanup()

    def write_metadata(
        self,
        *,
        target_variant="harness",
        features=None,
        fresh=False,
        manifest_path=None,
        executable=None,
        build_success=True,
    ):
        target_dir = self.gate.target_dir_for(target_variant)
        binary_path = target_dir / LAUNCHER.TARGET / "verification" / "xsh"
        binary_path.parent.mkdir(parents=True, exist_ok=True)
        binary_path.write_bytes(b"default xsh" if target_variant == "harness" else b"privileged xsh")
        binary_path.chmod(0o755)
        artifact = {
            "reason": "compiler-artifact",
            "package_id": "path+file:///work#xsh@0.0.1",
            "manifest_path": str(LAUNCHER.ROOT / "Cargo.toml") if manifest_path is None else manifest_path,
            "target": {
                "name": "xsh",
                "kind": ["bin"],
                "crate_types": ["bin"],
                "test": False,
            },
            "profile": {
                "opt_level": "3",
                "debuginfo": 0,
                "debug_assertions": False,
                "overflow_checks": False,
                "test": False,
            },
            "features": ["default", "native-tests", "net", "tools"] if features is None else features,
            "executable": str(binary_path) if executable is None else executable,
            "fresh": fresh,
        }
        records = [artifact, {"reason": "build-finished", "success": build_success}]
        step_id = "build-harness-" + target_variant + "-" + str(len(self.gate.receipt["source_checks"]))
        self.gate.receipt["source_checks"].extend([
            {
                "where": f"before:{step_id}",
                "head": self.gate.expected_commit,
                "tree": self.gate.expected_tree,
                "clean": True,
                "status": "",
            },
            {
                "where": f"after:{step_id}",
                "head": self.gate.expected_commit,
                "tree": self.gate.expected_tree,
                "clean": True,
                "status": "",
            },
        ])
        log = self.root / f"{step_id}.jsonl"
        log.write_text("\n".join(json.dumps(record) for record in records) + "\n")
        step = {"id": step_id, "exit_code": 0}
        return log, step, artifact

    def qualify(self, *, target_variant="harness", required_features=None, **metadata):
        log, step, artifact = self.write_metadata(target_variant=target_variant, **metadata)
        result = self.gate.parse_harness_xsh_variant(
            log,
            step=step,
            target_variant=target_variant,
            profile="verification",
            requested_features=[] if required_features is None else required_features,
        )
        return result, artifact

    def test_rejects_harness_xsh_from_a_different_manifest(self):
        log, step, _ = self.write_metadata(manifest_path=str(self.root / "other" / "Cargo.toml"))
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "root manifest"):
            self.gate.parse_harness_xsh_variant(
                log, step=step, target_variant="harness", profile="verification", requested_features=[]
            )

    def test_rejects_harness_xsh_outside_its_target_profile(self):
        outside = self.root / "outside" / "xsh"
        outside.parent.mkdir()
        outside.write_bytes(b"outside xsh")
        outside.chmod(0o755)
        log, step, _ = self.write_metadata(executable=str(outside))
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "target-owned regular binary"):
            self.gate.parse_harness_xsh_variant(
                log, step=step, target_variant="harness", profile="verification", requested_features=[]
            )

    def test_rejects_harness_xsh_that_omits_the_selected_feature(self):
        log, step, _ = self.write_metadata(target_variant="linux-priv")
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "omitted requested features"):
            self.gate.parse_harness_xsh_variant(
                log,
                step=step,
                target_variant="linux-priv",
                profile="verification",
                requested_features=["linux-priv-tests"],
            )

    def test_rejects_privileged_feature_on_the_default_harness_variant(self):
        log, step, _ = self.write_metadata(features=["default", "linux-priv-tests"])
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "does not match the selected"):
            self.gate.parse_harness_xsh_variant(
                log, step=step, target_variant="harness", profile="verification", requested_features=[]
            )

    def test_rejects_fresh_harness_xsh_before_same_variant_was_qualified(self):
        log, step, _ = self.write_metadata(fresh=True)
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "before it was qualified"):
            self.gate.parse_harness_xsh_variant(
                log, step=step, target_variant="harness", profile="verification", requested_features=[]
            )

    def test_accepts_fresh_reuse_of_the_same_qualified_variant_and_hash(self):
        first, _ = self.qualify()
        second, _ = self.qualify(fresh=True)
        self.assertEqual(first["path"], second["path"])
        self.assertEqual(first["sha256"], second["sha256"])
        self.assertTrue(second["qualification"]["cargo_fresh"])

    def test_linux_privileged_xsh_variant_can_differ_from_default_product(self):
        default_product = self.gate.target_dir / LAUNCHER.TARGET / "verification" / "xsh"
        default_product.parent.mkdir(parents=True)
        default_product.write_bytes(b"default verification product")
        default_product.chmod(0o755)
        default_product_hash = LAUNCHER.sha256(default_product)
        self.gate.receipt["hashes"]["products_after_initial_build"] = {
            "xsh-verification": default_product_hash,
        }
        privileged, _ = self.qualify(
            target_variant="linux-priv",
            features=["default", "linux-priv-tests", "native-tests", "net", "tools"],
            required_features=["linux-priv-tests"],
        )
        self.assertNotEqual(default_product_hash, privileged["sha256"])
        self.assertIn("linux-priv-tests", privileged["qualification"]["features"])
        self.assertEqual(privileged["qualification"]["target_variant"], "linux-priv")

    def test_rejects_cargo_metadata_without_successful_build_completion(self):
        log, step, _ = self.write_metadata(build_success=False)
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "successful harness build"):
            self.gate.parse_harness_xsh_variant(
                log, step=step, target_variant="harness", profile="verification", requested_features=[]
            )

    def test_rejects_harness_xsh_after_the_frozen_source_tree_changes(self):
        log, step, _ = self.write_metadata()
        self.gate.receipt["source_checks"][-1]["tree"] = "c" * 40
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "source checks"):
            self.gate.parse_harness_xsh_variant(
                log, step=step, target_variant="harness", profile="verification", requested_features=[]
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
