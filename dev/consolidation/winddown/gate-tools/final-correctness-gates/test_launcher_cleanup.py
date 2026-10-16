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
GENERATOR_PATH = SOURCE_DIR / "generate_manifest.py"
GENERATOR_SPEC = importlib.util.spec_from_file_location("final_correctness_manifest_generator", GENERATOR_PATH)
assert GENERATOR_SPEC is not None and GENERATOR_SPEC.loader is not None
MANIFEST_GENERATOR = importlib.util.module_from_spec(GENERATOR_SPEC)
GENERATOR_SPEC.loader.exec_module(MANIFEST_GENERATOR)
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
FROZEN_E048_SOURCE_ROOT = LAUNCHER.CONTROLLER_ROOT / ".work/wt/consolidation-final-e048"
SCRATCH_ROOT = SOURCE_ROOT / ".work/tmp"
SCRATCH_ROOT.mkdir(parents=True, exist_ok=True)
NATIVE_RUNNER = SOURCE_DIR / "run-native-final.sh"
CANONICAL_NATIVE_LOCK = LAUNCHER.CONTROLLER_ROOT / ".work/consolidation/infra/native-suite.lock"


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
        self.assertIn('f"TMPDIR={scratch}"', launcher)
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
                "--target", LAUNCHER.TARGET, "-p", "xsh", "-p", "xsht", "-p", "xshi", "-p", "xsh-fuzz",
                "--bins", "--message-format=json",
            ],
        )
        packages = [
            self.plan["products"][0][index + 1]
            for index, value in enumerate(self.plan["products"][0][:-1])
            if value == "-p"
        ]
        self.assertEqual(packages, ["xsh", "xsht", "xshi", "xsh-fuzz"])
        self.assertEqual(self.plan["products"][0].count("--bins"), 1)
        self.assertNotIn("--bin", self.plan["products"][0])
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
        self.assertEqual(source_root(LAUNCHER.TOOL_DIR / "test_launcher_cleanup.py"), SOURCE_ROOT)
        self.assertEqual(SCRATCH_ROOT, SOURCE_ROOT / ".work/tmp")


class PreflightTests(unittest.TestCase):
    def test_rendered_approved_plan_passes_full_launcher_preflight(self):
        plan = MANIFEST_GENERATOR.render_manifest()
        self.assertIs(type(plan["environment"]["container_cpus"]), int)
        gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        gate.receipt = {"environment": {}}
        gate.save = mock.Mock()
        with mock.patch.multiple(
            LAUNCHER,
            TOOL_DIR=SOURCE_DIR,
            RUNNER=SOURCE_DIR / "run-native-final.sh",
            PLAN_SOURCE_PATH=PLAN_SOURCE_PATH,
            MANIFEST_GENERATOR_PATH=GENERATOR_PATH,
            NATIVE_LOCK=LAUNCHER.CONTROLLER_ROOT / ".work/consolidation/infra/native-suite.lock",
            NATIVE_CLEANUP_MARKER=LAUNCHER.CONTROLLER_ROOT / ".work/consolidation/infra/native-suite.cleanup-unresolved",
            RUNTIME_CARGO_FORWARDER=SOURCE_DIR / "cargo-forwarder.sh",
            RUNTIME_RUSTC_WRAPPER=SOURCE_DIR / "rustc-jemalloc.sh",
        ):
            with mock.patch.object(LAUNCHER.os, "getuid", return_value=1000):
                with mock.patch.object(LAUNCHER.os, "getgid", return_value=1000):
                    gate.verify_toolchain_inputs(plan)
        gate.save.assert_called_once_with()
        self.assertEqual(gate.receipt["environment"]["container_cpus"], LAUNCHER.CONTAINER_CPUS)


class SourceAdmissionTests(unittest.TestCase):
    def test_resolves_the_requested_worktree_and_preserves_its_source(self):
        before = LAUNCHER.git_snapshot(SOURCE_ROOT)
        source_root, git_common_dir = LAUNCHER.resolve_source_root(SOURCE_ROOT)
        after = LAUNCHER.git_snapshot(source_root)

        self.assertEqual(source_root, SOURCE_ROOT)
        self.assertEqual(git_common_dir, (LAUNCHER.CONTROLLER_ROOT / ".git").resolve())
        self.assertEqual(after, before)

    def test_accepts_the_controller_checkout_as_an_explicit_source_root(self):
        source_root, git_common_dir = LAUNCHER.resolve_source_root(LAUNCHER.CONTROLLER_ROOT)

        self.assertEqual(source_root, LAUNCHER.CONTROLLER_ROOT)
        self.assertEqual(git_common_dir, (LAUNCHER.CONTROLLER_ROOT / ".git").resolve())

    def test_accepts_the_frozen_e048_source_worktree(self):
        before = LAUNCHER.git_snapshot(FROZEN_E048_SOURCE_ROOT)
        source_root, git_common_dir = LAUNCHER.resolve_source_root(FROZEN_E048_SOURCE_ROOT)
        after = LAUNCHER.git_snapshot(source_root)

        self.assertEqual(source_root, FROZEN_E048_SOURCE_ROOT)
        self.assertEqual(git_common_dir, (LAUNCHER.CONTROLLER_ROOT / ".git").resolve())
        self.assertEqual(after, before)

    def test_rejects_a_symlinked_source_root(self):
        link = SCRATCH_ROOT / "source-root-link"
        link.symlink_to(SOURCE_ROOT, target_is_directory=True)
        try:
            with self.assertRaisesRegex(LAUNCHER.GateFailure, "canonical path"):
                LAUNCHER.resolve_source_root(link)
        finally:
            link.unlink()

    def test_rejects_a_subdirectory_instead_of_the_checkout_root(self):
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "controller checkout"):
            LAUNCHER.resolve_source_root(SOURCE_ROOT / "dev")

    def test_rejects_a_source_checkout_with_a_different_shared_git_directory(self):
        with tempfile.TemporaryDirectory(dir=LAUNCHER.WORKTREE_ROOT) as temporary:
            unrelated_root = pathlib.Path(temporary)
            (unrelated_root / "Cargo.toml").write_text("[workspace]\n")
            (unrelated_root / "dev").mkdir()
            subprocess.run(["git", "init", "--quiet", str(unrelated_root)], check=True)

            with self.assertRaisesRegex(LAUNCHER.GateFailure, "share the controller's Git directory"):
                LAUNCHER.resolve_source_root(unrelated_root)

    def test_rejects_source_commit_and_tree_mismatches_before_build(self):
        source = LAUNCHER.git_snapshot(FROZEN_E048_SOURCE_ROOT)
        gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        gate.source_root = FROZEN_E048_SOURCE_ROOT
        gate.expected_commit = "f" * 40
        gate.expected_tree = source["tree"]
        gate.receipt = {"source_checks": []}
        gate.save = mock.Mock()

        with self.assertRaisesRegex(LAUNCHER.GateFailure, "source changed or is dirty"):
            gate.verify_source("before-build")

        gate.expected_commit = source["head"]
        gate.expected_tree = "f" * 40
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "source changed or is dirty"):
            gate.verify_source("before-build")

    def test_rejects_a_different_checkout_when_the_frozen_commit_is_expected(self):
        frozen = LAUNCHER.git_snapshot(FROZEN_E048_SOURCE_ROOT)
        gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        gate.source_root = LAUNCHER.CONTROLLER_ROOT
        gate.expected_commit = frozen["head"]
        gate.expected_tree = frozen["tree"]
        gate.receipt = {"source_checks": []}
        gate.save = mock.Mock()

        with self.assertRaisesRegex(LAUNCHER.GateFailure, "source changed or is dirty"):
            gate.verify_source("before-build")

    def test_rejects_a_changed_tool_checkout(self):
        gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        gate.tool_root = FROZEN_E048_SOURCE_ROOT
        gate.tool_expected = {"commit": "f" * 40, "tree": "e" * 40}
        gate.receipt = {"tool_checks": []}
        gate.save = mock.Mock()

        with self.assertRaisesRegex(LAUNCHER.GateFailure, "gate tools changed or are dirty"):
            gate.verify_tool("before-build")

    def test_execute_requires_the_source_root_argument(self):
        arguments = [
            str(LAUNCHER.TOOL_DIR / "launch.py"),
            "--execute",
            "--expected-commit", "a" * 40,
            "--expected-tree", "b" * 40,
            "--out", str(LAUNCHER.OUTPUT_ROOT / "missing-source-root-self-test"),
        ]
        with mock.patch.object(sys, "argv", arguments):
            with mock.patch.object(LAUNCHER, "load_plan", return_value=plan_data()):
                with mock.patch.object(
                    LAUNCHER.argparse.ArgumentParser,
                    "error",
                    side_effect=ValueError("--source-root is required"),
                ):
                    with self.assertRaisesRegex(ValueError, "--source-root"):
                        LAUNCHER.main()

    def test_native_build_uses_the_requested_source_worktree(self):
        gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        gate.source_root = FROZEN_E048_SOURCE_ROOT
        gate.target_dir = SCRATCH_ROOT / "source-admission-target"
        gate.native_lock_file = None
        gate.cid_dir = SCRATCH_ROOT
        gate.step_number = 0
        gate.run = mock.Mock(return_value={})

        gate.run_native("source-root", "build", ["cargo", "build"])

        command = gate.run.call_args.args[1]
        self.assertEqual(command[:2], [str(LAUNCHER.RUNNER), str(FROZEN_E048_SOURCE_ROOT)])

    def test_supplemental_context_mounts_the_selected_source_and_scratch(self):
        _, git_common_dir = LAUNCHER.resolve_source_root(FROZEN_E048_SOURCE_ROOT)
        gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        gate.source_root = FROZEN_E048_SOURCE_ROOT
        gate.git_common_dir = git_common_dir
        gate.cid_dir = SCRATCH_ROOT
        gate.step_number = 0
        gate.target_dir = LAUNCHER.OUTPUT_ROOT / "source-admission-target"
        gate.run = mock.Mock(return_value={})

        gate.run_special_container(
            "source-root-special",
            ["/bin/true"],
            target_dir=gate.target_dir,
            privileged=False,
            memory="1g",
        )

        command = gate.run.call_args.args[1]
        volumes = [command[index + 1] for index, value in enumerate(command[:-1]) if value == "-v"]
        environment = [command[index + 1] for index, value in enumerate(command[:-1]) if value == "-e"]
        scratch = FROZEN_E048_SOURCE_ROOT / ".work/tmp"

        self.assertIn(f"{FROZEN_E048_SOURCE_ROOT}:{FROZEN_E048_SOURCE_ROOT}:ro", volumes)
        self.assertIn(f"{git_common_dir}:{git_common_dir}:ro", volumes)
        self.assertIn(f"{scratch}:{scratch}:rw", volumes)
        self.assertIn(f"TMPDIR={scratch}", environment)
        self.assertIn("--tmpfs", command)
        self.assertEqual(command[command.index("--tmpfs") + 1], "/tmp:rw,nodev,exec,nosuid,mode=1777")
        self.assertEqual(command[command.index("-w") + 1], str(FROZEN_E048_SOURCE_ROOT))


class ArtifactQualificationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=SCRATCH_ROOT)
        self.root = pathlib.Path(self.temporary.name)
        self.gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        self.gate.source_root = SOURCE_ROOT
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
            "manifest_path": str(SOURCE_ROOT / "Cargo.toml") if manifest_path is None else manifest_path,
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
        self.previous_infra = LAUNCHER.INFRA
        self.previous_native_lock = LAUNCHER.NATIVE_LOCK
        self.previous_cleanup_marker = LAUNCHER.NATIVE_CLEANUP_MARKER
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
        LAUNCHER.INFRA = self.previous_infra
        LAUNCHER.NATIVE_LOCK = self.previous_native_lock
        LAUNCHER.NATIVE_CLEANUP_MARKER = self.previous_cleanup_marker
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
        target = LAUNCHER.OUTPUT_ROOT / "lock-rejection-self-test" / "target" / "verification"
        cidfile = SCRATCH_ROOT / "lock-rejection-self-test.cid"
        env = dict(os.environ)
        env.pop("XSH_GATE_NATIVE_LOCK_FD", None)
        env["XSH_GATE_CIDFILE"] = str(cidfile)
        env["XSH_GATE_TARGET_DIR"] = str(target)
        env["XSH_GATE_TOOL_DIR"] = str(LAUNCHER.CONTROLLER_ROOT / "dev/consolidation/winddown/gate-tools/final-correctness-gates")
        with CANONICAL_NATIVE_LOCK.open("a+") as holder:
            try:
                fcntl.flock(holder.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                self.fail(f"canonical native-suite lock is already held: {error}")
            result = subprocess.run(
                ["bash", str(NATIVE_RUNNER), str(LAUNCHER.CONTROLLER_ROOT), "build", "cargo", "build", "--bin", "xsh"],
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


class CommandOutputTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=SCRATCH_ROOT)
        self.out = pathlib.Path(self.temporary.name) / "run"
        self.out.mkdir()
        self.logs = self.out / "logs"
        self.logs.mkdir()
        self.gate = LAUNCHER.Launch.__new__(LAUNCHER.Launch)
        self.gate.source_root = SOURCE_ROOT
        self.gate.out = self.out
        self.gate.logs = self.logs
        self.gate.receipt = {"steps": []}
        self.gate.step_number = 0
        self.gate.interrupt_started = False

    def tearDown(self):
        self.temporary.cleanup()

    def test_run_returns_child_streams_separately_from_command_metadata(self):
        stdout = f"{LAUNCHER.IMAGE}\n"
        stderr = "diagnostic from child\n"
        command = [
            sys.executable,
            "-c",
            "import sys; sys.stdout.write(" + repr(stdout) + "); sys.stderr.write(" + repr(stderr) + ")",
        ]
        step = self.gate.run("stream-contract", command, check_source=False)

        stdout_path = pathlib.Path(step["stdout_log"])
        stderr_path = pathlib.Path(step["stderr_log"])
        human_log = pathlib.Path(step["log"]).read_text()
        self.assertEqual(stdout_path.read_text(), stdout)
        self.assertEqual(stderr_path.read_text(), stderr)
        self.assertEqual(step["stdout_sha256"], LAUNCHER.sha256(stdout_path))
        self.assertEqual(step["stderr_sha256"], LAUNCHER.sha256(stderr_path))
        self.assertIn("argv: ", human_log)
        self.assertIn("stdout:\n" + stdout, human_log)
        self.assertIn("stderr:\n" + stderr, human_log)
        LAUNCHER.Launch.validate_pinned_image_output(step)

    def test_image_output_requires_success_and_exact_stdout_framing(self):
        stdout_path = self.logs / "image.stdout"
        stderr_path = self.logs / "image.stderr"
        stderr_path.write_text("diagnostic is retained but does not alter stdout identity\n")
        step = {"exit_code": 0, "stdout_log": str(stdout_path), "stderr_log": str(stderr_path)}

        for output in (
            f"{LAUNCHER.IMAGE}\nextra\n",
            f"{LAUNCHER.IMAGE} extra\n",
            f"{LAUNCHER.IMAGE}",
            "malformed\n",
        ):
            with self.subTest(output=output):
                stdout_path.write_text(output)
                with self.assertRaisesRegex(LAUNCHER.GateFailure, "mismatched ID"):
                    LAUNCHER.Launch.validate_pinned_image_output(step)

        stdout_path.write_text(f"{LAUNCHER.IMAGE}\n")
        step["exit_code"] = 1
        with self.assertRaisesRegex(LAUNCHER.GateFailure, "mismatched ID"):
            LAUNCHER.Launch.validate_pinned_image_output(step)


class RunnerInvocationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=LAUNCHER.OUTPUT_ROOT)
        self.out = pathlib.Path(self.temporary.name)
        self.bin_dir = self.out / "bin"
        self.bin_dir.mkdir()
        self.argv_path = self.out / "docker-argv.json"
        docker = self.bin_dir / "docker"
        docker.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "with open(os.environ['XSH_FAKE_DOCKER_ARGS'], 'w') as output:\n"
            "    json.dump(sys.argv[1:], output)\n"
        )
        docker.chmod(0o755)
        flock = self.bin_dir / "flock"
        flock.write_text("#!/bin/sh\nexit 0\n")
        flock.chmod(0o755)
        self.target_dir = self.out / "target" / "products"
        self.cargo_home = self.out / "runtime-cargo-home"
        self.xdg_cache = self.out / "runtime-xdg-cache"
        self.cargo_home.mkdir()
        self.xdg_cache.mkdir()

    def tearDown(self):
        self.temporary.cleanup()

    def invoke(self, mode, command, *, tree=None, tool_dir=None, target_dir=None, extra_path=None, extra_env=None):
        selected_tree = LAUNCHER.CONTROLLER_ROOT if tree is None else pathlib.Path(tree)
        selected_tool_dir = (
            pathlib.Path(tool_dir)
            if tool_dir is not None
            else selected_tree / "dev/consolidation/winddown/gate-tools/final-correctness-gates"
        )
        self.argv_path.unlink(missing_ok=True)
        env = dict(os.environ)
        env["PATH"] = str(self.bin_dir) + os.pathsep + env["PATH"]
        if extra_path is not None:
            env["PATH"] = str(extra_path) + os.pathsep + env["PATH"]
        env["XSH_FAKE_DOCKER_ARGS"] = str(self.argv_path)
        env["XSH_GATE_CIDFILE"] = str(self.out / "container.cid")
        env["XSH_GATE_TARGET_DIR"] = str(self.target_dir if target_dir is None else target_dir)
        env["XSH_GATE_CARGO_HOME"] = str(self.cargo_home)
        env["XSH_GATE_XDG_CACHE_HOME"] = str(self.xdg_cache)
        env["XSH_GATE_TOOL_DIR"] = str(selected_tool_dir)
        env["XSH_GATE_CARGO_FORWARDER"] = str(selected_tool_dir / "cargo-forwarder.sh")
        env["XSH_GATE_RUSTC_WRAPPER"] = str(selected_tool_dir / "rustc-jemalloc.sh")
        env.update(extra_env or {})
        result = subprocess.run(
            ["bash", str(NATIVE_RUNNER), str(selected_tree), mode, *command],
            env=env,
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        arguments = json.loads(self.argv_path.read_text()) if self.argv_path.exists() else None
        return result, arguments

    def test_all_modes_mount_source_read_only_and_scratch_explicitly(self):
        cases = {
            "build": ["cargo", "build", "--locked"],
            "lint": ["cargo", "clippy", "--locked"],
            "run": ["/bin/true"],
            "prepare-cargo": [],
            "run-cargo": ["cargo", "metadata", "--no-deps"],
        }
        tree = FROZEN_E048_SOURCE_ROOT
        scratch = tree / ".work/tmp"
        ordinary_read_only_mounts = {
            f"{tree}:{tree}:ro",
            f"{LAUNCHER.CONTROLLER_ROOT / '.git'}:{LAUNCHER.CONTROLLER_ROOT / '.git'}:ro",
            "/home/josh/d/laputa-systems/laputa:/home/josh/d/laputa-systems/laputa:ro",
            "/home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro",
            "/usr/local/bin/mold:/usr/local/bin/mold:ro",
            f"{LAUNCHER.INFRA / 'mold-linker.sh'}:/usr/local/bin/xsh-mold-linker:ro",
        }

        for mode, command in cases.items():
            with self.subTest(mode=mode):
                result, arguments = self.invoke(mode, command, tree=tree, tool_dir=SOURCE_DIR)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIsNotNone(arguments)
                volumes = [
                    arguments[index + 1]
                    for index, value in enumerate(arguments[:-1])
                    if value == "-v"
                ]
                environment = {
                    arguments[index + 1]
                    for index, value in enumerate(arguments[:-1])
                    if value == "-e"
                }
                self.assertTrue(ordinary_read_only_mounts.issubset(volumes), volumes)
                expected_tmpdir_mount = f"{scratch}:{scratch}:rw"
                expected_target_mount = f"{self.target_dir}:{self.target_dir}" + (":ro" if mode == "run" else ":rw")
                self.assertEqual([item for item in volumes if item.startswith(f"{tree}:{tree}")], [f"{tree}:{tree}:ro"])
                self.assertEqual([item for item in volumes if item.startswith(f"{scratch}:{scratch}")], [expected_tmpdir_mount])
                self.assertEqual([item for item in volumes if item.startswith(f"{self.target_dir}:{self.target_dir}")], [expected_target_mount])
                self.assertEqual({item for item in environment if item.startswith("TMPDIR=")}, {f"TMPDIR={scratch}"})
                for option, expected in (
                    ("--platform", "linux/amd64"),
                    ("--tmpfs", "/tmp:rw,exec,nosuid,mode=1777"),
                ):
                    self.assertIn(option, arguments)
                    self.assertEqual(arguments[arguments.index(option) + 1], expected)
                self.assertTrue({"--cpus=12", "--memory=24g", "--memory-swap=24g", "--network=none"}.issubset(arguments))
                self.assertTrue({
                    "LC_ALL=C",
                    "TZ=UTC",
                    f"CARGO_TARGET_DIR={self.target_dir}",
                    "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker",
                    "CARGO_BUILD_JOBS=12",
                    "XSH_LAPUTA_CORPUS=/home/josh/d/laputa-systems/laputa",
                    "PATH=/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin",
                    "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=-Wl,--defsym=__isoc23_sscanf=sscanf -C link-arg=-Wl,--defsym=__isoc23_strtol=strtol",
                }.issubset(environment), environment)
                self.assertIn("-w", arguments)
                self.assertEqual(arguments[arguments.index("-w") + 1], str(tree))
                self.assertIn("sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c", arguments)
                if mode in {"prepare-cargo", "run-cargo"}:
                    self.assertIn(f"{self.cargo_home}:{self.cargo_home}", volumes)
                    self.assertIn(f"{self.xdg_cache}:{self.xdg_cache}", volumes)
                if mode in {"build", "lint"}:
                    self.assertTrue({
                        "RUSTC_WRAPPER=/usr/local/bin/kache",
                        "LD_PRELOAD=/usr/lib/libjemalloc.so.2",
                    }.issubset(environment), environment)
                if mode in {"run", "prepare-cargo", "run-cargo"}:
                    self.assertTrue({"HOME=/home/compat", "RUSTUP_HOME=/root/.rustup"}.issubset(environment), environment)
                if mode == "prepare-cargo":
                    self.assertIn("--user", arguments)
                    self.assertEqual(arguments[arguments.index("--user") + 1], "0:0")
                if mode == "run":
                    self.assertIn("--user", arguments)
                    self.assertEqual(arguments[arguments.index("--user") + 1], f"{os.getuid()}:{os.getgid()}")
                if mode == "run-cargo":
                    self.assertTrue({
                        f"CARGO_HOME={self.cargo_home}",
                        f"XDG_CACHE_HOME={self.xdg_cache}",
                        "XSH_GATE_RUSTC_WRAPPER=/usr/local/bin/xsh-rustc-jemalloc",
                    }.issubset(environment), environment)
                    self.assertIn("--user", arguments)
                    self.assertEqual(arguments[arguments.index("--user") + 1], f"{os.getuid()}:{os.getgid()}")

    def test_runner_rejects_target_symlink_and_disjoint_target_before_docker(self):
        outside_target = LAUNCHER.CONTROLLER_ROOT / ".work/tmp/final-runner-outside/target"
        result, arguments = self.invoke("run", ["/bin/true"], target_dir=outside_target)
        self.assertEqual(result.returncode, 2)
        self.assertIn("isolated under this gate's ignored output directory", result.stderr)
        self.assertIsNone(arguments)

        target_parent = self.out / "target"
        target_parent.symlink_to(self.out / "elsewhere", target_is_directory=True)
        result, arguments = self.invoke("run", ["/bin/true"])
        self.assertEqual(result.returncode, 2)
        self.assertIn("must not contain symlinks", result.stderr)
        self.assertIsNone(arguments)

    def test_runner_rejects_a_symlinked_worktree_before_docker(self):
        tree_link = LAUNCHER.CONTROLLER_ROOT / ".work/wt/final-runner-tree-symlink"
        self.assertFalse(tree_link.exists())
        tree_link.symlink_to(SOURCE_ROOT, target_is_directory=True)
        try:
            result, arguments = self.invoke("build", ["cargo", "build", "--locked"], tree=tree_link)
        finally:
            tree_link.unlink()
        self.assertEqual(result.returncode, 2)
        self.assertIn("canonical repository root", result.stderr)
        self.assertIsNone(arguments)

    def test_runner_checks_worktree_exclusion_before_creating_target(self):
        fake_path = self.out / "fake-git"
        fake_path.mkdir()
        fake_git = fake_path / "git"
        fake_git.write_text(
            "#!/bin/sh\n"
            "if [ \"$3\" = rev-parse ] && [ \"$4\" = --show-toplevel ]; then\n"
            "    /usr/bin/git -C \"$2\" rev-parse --show-toplevel\n"
            "    exit 0\n"
            "fi\n"
            "if [ \"$3\" = rev-parse ] && [ \"$4\" = --path-format=absolute ]; then\n"
            "    printf '%s\\n' \"$XSH_FAKE_GIT_COMMON_DIR\"\n"
            "    exit 0\n"
            "fi\n"
            "exit 0\n"
        )
        fake_git.chmod(0o755)
        common_dir = self.out / "git-common"
        exclude = common_dir / "info" / "exclude"
        exclude.parent.mkdir(parents=True)
        exclude.write_text("")

        result, arguments = self.invoke(
            "run",
            ["/bin/true"],
            extra_path=fake_path,
            extra_env={"XSH_FAKE_GIT_COMMON_DIR": str(common_dir)},
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn(".work/tmp must be excluded", result.stderr)
        self.assertFalse(self.target_dir.exists())
        self.assertIsNone(arguments)


if __name__ == "__main__":
    unittest.main()
