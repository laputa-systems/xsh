from importlib.util import module_from_spec, spec_from_file_location
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
CONSOLIDATION = ROOT / "dev/consolidation"
PERFORMANCE = CONSOLIDATION / "winddown/gate-tools/final-performance-gates"
CORRECTNESS = CONSOLIDATION / "winddown/gate-tools/final-correctness-gates"
SCHEDULER = CONSOLIDATION / "winddown/gate-tools/native-scheduler-comparison"
UTIL_LINUX = ROOT / "dev/util-linux"
sys.path[:0] = [str(CONSOLIDATION), str(PERFORMANCE), str(CORRECTNESS), str(SCHEDULER), str(UTIL_LINUX)]


def load_module(name, path):
    spec = spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


class NativeImagePinTests(unittest.TestCase):
    def test_active_linux_qualification_controllers_share_the_versions_lock_image(self):
        from native_image import IMAGE_ID

        expected = subprocess.check_output(
            [sys.executable, str(CONSOLIDATION / "native_image.py")],
            cwd=ROOT,
            text=True,
        ).strip()
        self.assertEqual(IMAGE_ID, expected)
        controllers = (
            (load_module("canonical_replay_pin", CONSOLIDATION / "final_canonical_replay.py"), "IMAGE", expected),
            (load_module("scheduler_bridge_pin", CONSOLIDATION / "final_scheduler_receipt_bridge.py"), "IMAGE_ID", expected),
            (load_module("route_replay_pin", CONSOLIDATION / "route_replay_driver.py"), "IMAGE", expected),
            (load_module("correctness_plan_pin", CORRECTNESS / "plan_source.py"), "IMAGE", expected),
            (load_module("correctness_launcher_pin", CORRECTNESS / "launch.py"), "IMAGE", expected),
            (load_module("scheduler_comparator_pin", SCHEDULER / "compare.py"), "PINNED_IMAGE", "xsh-test@" + expected),
            (load_module("profile_cost_pin", PERFORMANCE / "profile-cost.py"), "NATIVE_IMAGE_ID", expected),
            (load_module("final_observers_pin", PERFORMANCE / "final-observers.py"), "NATIVE_IMAGE_ID", expected),
            (load_module("forced_frames_pin", PERFORMANCE / "forced-frames.py"), "IMAGE", expected),
            (load_module("final_comparative_pin", PERFORMANCE / "final-comparative.py"), "IMAGE_ID", expected),
            (load_module("docker_observer_shim_pin", PERFORMANCE / "docker-observer-shim.py"), "NATIVE_IMAGE_ID", expected),
        )
        for controller, field, value in controllers:
            self.assertEqual(getattr(controller, field), value)

        util_linux = load_module("util_linux_pin", UTIL_LINUX / "check.py")
        self.assertEqual(util_linux.IMAGE, "xsh-test@" + expected)

        for runner in (
            CONSOLIDATION / "winddown/gate-tools/infra/run-native.sh",
            CORRECTNESS / "run-native-final.sh",
        ):
            self.assertIn('TASK_IMAGE_ID=$(python3 "$TASK_SCRIPT_DIR/../../../native_image.py")', runner.read_text())

    def test_util_linux_reference_uses_the_fresh_ef7_artifacts(self):
        from native_image import IMAGE_ID

        util_linux = load_module("util_linux_fresh_products", UTIL_LINUX / "check.py")
        import resources

        observed = {}

        def verify(directory, hashes):
            observed["directory"] = directory
            observed["hashes"] = hashes
            return directory

        with tempfile.TemporaryDirectory(dir=ROOT / ".work/tmp") as temporary:
            root = Path(temporary)
            output = root / util_linux.PRODUCT_OUTPUT
            directory = output / "opt3/target" / resources.TARGET / "release"
            expected_tree = subprocess.check_output(
                ["git", "-C", str(ROOT), "rev-parse", f"{util_linux.PRODUCT_SOURCE}^{{tree}}"], text=True
            ).strip()
            binary_hashes = {
                name: {"path": str(directory / name), "sha256": digest}
                for name, digest in util_linux.PRODUCT_HASHES.items()
            }
            receipt = {
                "schema": 1,
                "state": "artifact-qualified",
                "qualification_mode": "artifacts-only",
                "source_commit": util_linux.PRODUCT_SOURCE,
                "source_status": "clean",
                "source_identity": {
                    "commit": util_linux.PRODUCT_SOURCE,
                    "git_tree": expected_tree,
                    "status": "clean",
                },
                "profile": {"name": "release", "lto": "thin", "codegen_units": 16, "opt_levels": [3]},
                "target": resources.TARGET,
                "native_image": {"image_id": IMAGE_ID, "architecture": "amd64"},
                "results": [{
                    "name": "release-opt3",
                    "profile": "release",
                    "opt_level": 3,
                    "lto": "thin",
                    "codegen_units": 16,
                    "build": {
                        "status": 0,
                        "stage_validation_error": None,
                        "container_cleanup": {"resolved": True},
                        "stage_validation": {"binary_hashes": binary_hashes},
                    },
                }],
            }
            output.mkdir(parents=True)
            (output / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            with patch.object(util_linux, "original_root", return_value=root), patch.object(
                util_linux, "verify_product_hashes", side_effect=verify
            ):
                products, context = util_linux.baseline_products()

        self.assertEqual(products, directory)
        self.assertEqual(observed["directory"], directory)
        self.assertEqual(observed["hashes"], util_linux.PRODUCT_HASHES)
        self.assertEqual(context.source, util_linux.PRODUCT_SOURCE)
        self.assertEqual(context.image, "xsh-test@" + IMAGE_ID)
        self.assertEqual(context.profile, "release")
        self.assertEqual(context.target, resources.TARGET)

    def test_util_linux_rejects_wrong_source_and_resource_pair_rejects_old_image(self):
        from native_image import IMAGE_ID

        import acceptance
        import resources

        util_linux = load_module("util_linux_profile_receipt_binding", UTIL_LINUX / "check.py")
        directory = ROOT / ".work/wt/requalify-57a-ef7/.work/consolidation/final-performance-gates/profile-cost-57a-ef7/opt3/target" / resources.TARGET / "release"
        expected_tree = subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", f"{util_linux.PRODUCT_SOURCE}^{{tree}}"], text=True
        ).strip()
        receipt = {
            "schema": 1,
            "state": "artifact-qualified",
            "qualification_mode": "artifacts-only",
            "source_commit": util_linux.PRODUCT_SOURCE,
            "source_status": "clean",
            "source_identity": {
                "commit": util_linux.PRODUCT_SOURCE,
                "git_tree": expected_tree,
                "status": "clean",
            },
            "profile": {"name": "release", "lto": "thin", "codegen_units": 16, "opt_levels": [3]},
            "target": resources.TARGET,
            "native_image": {"image_id": "sha256:" + "3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c", "architecture": "amd64"},
            "results": [{
                "name": "release-opt3",
                "profile": "release",
                "opt_level": 3,
                "lto": "thin",
                "codegen_units": 16,
                "build": {
                    "status": 0,
                    "stage_validation_error": None,
                    "container_cleanup": {"resolved": True},
                    "stage_validation": {"binary_hashes": {
                        name: {"path": str(directory / name), "sha256": digest}
                        for name, digest in util_linux.PRODUCT_HASHES.items()
                    }},
                },
            }],
        }
        bad_source = json.loads(json.dumps(receipt))
        bad_source["source_commit"] = "0" * 40
        with self.assertRaisesRegex(ValueError, "qualified clean source"):
            util_linux.profile_build_context(bad_source, directory, resources.TARGET)

        baseline_context = util_linux.profile_build_context(receipt, directory, resources.TARGET)
        candidate_context = acceptance.BuildContext(
            source="a" * 40 + ":" + "b" * 40,
            profile="release",
            target=resources.TARGET,
            image="xsh-test@" + IMAGE_ID,
            allocator="mimalloc",
            allocation="1cpu/1GiB",
        )

        class Runtime:
            PRODUCT_SOURCE = util_linux.PRODUCT_SOURCE
            IMAGE = "xsh-test@" + IMAGE_ID

            def baseline_products(self):
                return directory, baseline_context

            def candidate_products(self):
                return directory, candidate_context

        with patch.object(resources, "_product_signature", side_effect=AssertionError("product read")):
            with self.assertRaisesRegex(ValueError, "pinned acceptance image"):
                resources._product_pair(Runtime(), acceptance.SourceIdentity("a" * 40, "b" * 40, True))

    def test_profile_identity_inspects_the_locked_immutable_id(self):
        from native_image import IMAGE_ID

        profile = load_module("profile_cost_image_identity", PERFORMANCE / "profile-cost.py")
        calls = []

        def check_output(argv, **kwargs):
            calls.append(argv)
            if argv == ["docker", "image", "inspect", IMAGE_ID]:
                return json.dumps([{
                    "Id": IMAGE_ID,
                    "Architecture": "amd64",
                    "Config": {"Env": [
                        "CC=/usr/bin/clang-23",
                        "AR=/usr/local/bin/llvm-ar",
                        "CFLAGS_x86_64_unknown_linux_musl=--target=x86_64-unknown-linux-musl -B/usr/lib/xsh-crt/x86_64-unknown-linux-musl -I/usr/include",
                    ]},
                }])
            if argv[-1:] == ["--version"] and argv[0].endswith("kache"):
                return "kache 1.0.0\n"
            if argv[-1:] == ["--version"] and argv[0].endswith("mold"):
                return "mold 3.0.0 test\n"
            raise AssertionError(argv)

        with patch.multiple(
            profile,
            KACHE=Path("/tools/kache"),
            KACHE_SHA256="hash",
            MOLD=Path("/tools/mold"),
            MOLD_SHA256="hash",
            LINKER=Path("/tools/linker"),
            LINKER_SHA256="hash",
        ), patch.object(profile, "runner_image_id", return_value=IMAGE_ID) as selected_image, patch.object(
            profile, "digest", return_value="hash"
        ), patch.object(profile.subprocess, "check_output", side_effect=check_output):
            identity = profile.image_identity()

        self.assertEqual(calls[0], ["docker", "image", "inspect", IMAGE_ID])
        selected_image.assert_called_once_with(profile.RUNNER)
        self.assertEqual(identity["image_id"], IMAGE_ID)
        self.assertEqual(identity["architecture"], "amd64")

    def test_comparative_identity_inspects_the_locked_immutable_id(self):
        from native_image import IMAGE_ID

        comparative = load_module("final_comparative_image_identity", PERFORMANCE / "final-comparative.py")
        calls = []
        with patch.object(comparative, "runner_image_id", return_value=IMAGE_ID) as selected_image, patch.object(
            comparative.subprocess,
            "check_output",
            side_effect=lambda argv: calls.append(argv) or json.dumps([{"Id": IMAGE_ID, "Architecture": "amd64"}]),
        ), patch.object(comparative, "digest", return_value="wrapper-hash"):
            identity = comparative.native_image()

        self.assertEqual(calls, [["docker", "image", "inspect", IMAGE_ID]])
        selected_image.assert_called_once_with(comparative.RUNNER)
        self.assertEqual(identity, {"image_id": IMAGE_ID, "architecture": "amd64", "wrapper_sha256": "wrapper-hash"})

    def test_checked_in_native_runner_selects_only_the_locked_image(self):
        from native_image import IMAGE_ID, runner_image_id

        runner = CONSOLIDATION / "winddown/gate-tools/infra/run-native.sh"
        self.assertEqual(runner_image_id(runner), IMAGE_ID)

        with tempfile.TemporaryDirectory(dir=ROOT / ".work/tmp") as temporary:
            wrong_runner = Path(temporary) / "run-native.sh"
            wrong_runner.write_text("#!/bin/sh\nprintf '%s\\n' sha256:" + "3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c\n")
            wrong_runner.chmod(0o755)
            with self.assertRaisesRegex(ValueError, "runner selected image differs"):
                runner_image_id(wrong_runner)


if __name__ == "__main__":
    unittest.main()
