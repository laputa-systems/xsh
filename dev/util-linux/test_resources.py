import ast
from contextlib import contextmanager
import io
import json
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import acceptance
import resources
from acceptance import (
    BuildContext,
    GateResult,
    MeasurementKind,
    MemorySample,
    PairOrder,
    Readiness,
    SourceIdentity,
)


ROOT = Path(__file__).resolve().parents[2]
SCRATCH = ROOT / ".work" / "tmp"
COMMIT = "a" * 40
TREE = "b" * 40
SOURCE = SourceIdentity(COMMIT, TREE, True)
COHORT = ("lslocks", "lsipc", "lsmem", "mountpoint", "namei")
REGRESSIONS = (
    "core/tests/test-util-linux-process-regressions.xsh::test_process_case",
    "core/tests/test-util-linux-storage-regressions.xsh::test_storage_case",
)


@contextmanager
def laputa_corpus_scope(prefix="resource-laputa-"):
    with tempfile.TemporaryDirectory(dir=SCRATCH, prefix=prefix) as directory:
        root = Path(directory)
        subprocess.run(["git", "init", "--quiet", str(root)], check=True)
        (root / "pm").mkdir()
        (root / "pm/generation.xsh").write_text('print "generation"\n')
        subprocess.run(["git", "-C", str(root), "add", "pm/generation.xsh"], check=True)
        subprocess.run(["git", "-C", str(root), "-c", "user.name=G1 fixture", "-c", "user.email=g1-fixture@example.invalid", "commit", "--quiet", "-m", "test(util-linux): freeze Laputa corpus fixture"], check=True)
        with resources._laputa_corpus_scope(root):
            yield root


class LaputaCorpusScopeTests(unittest.TestCase):
    def test_selection_is_required_and_restored_after_scope(self):
        with self.assertRaisesRegex(RuntimeError, "explicit Laputa corpus selection"):
            resources._selected_laputa_corpus()
        with laputa_corpus_scope() as root:
            self.assertEqual(resources._selected_laputa_corpus(), root)
        with self.assertRaisesRegex(RuntimeError, "explicit Laputa corpus selection"):
            resources._selected_laputa_corpus()

    def test_selection_requires_canonical_git_root_and_pm_directory(self):
        with tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-laputa-invalid-") as directory:
            root = Path(directory)
            (root / "pm").mkdir()
            with self.assertRaisesRegex(ValueError, "Git repository root"):
                with resources._laputa_corpus_scope(root):
                    self.fail("non-Git corpus was selected")
            subprocess.run(["git", "init", "--quiet", str(root)], check=True)
            alias = root.parent / (root.name + "-alias")
            alias.symlink_to(root, target_is_directory=True)
            self.addCleanup(alias.unlink)
            with self.assertRaisesRegex(ValueError, "canonical non-symlink"):
                with resources._laputa_corpus_scope(alias):
                    self.fail("symlink corpus was selected")
            (root / "pm").rmdir()
            (root / "pm").symlink_to(root, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "pm directory"):
                with resources._laputa_corpus_scope(root):
                    self.fail("symlink pm directory was selected")

    def test_scope_requires_a_clean_corpus_before_selection(self):
        with tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-laputa-dirty-") as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "--quiet", str(root)], check=True)
            (root / "pm").mkdir()
            (root / "pm/generation.xsh").write_text('print "generation"\n')
            subprocess.run(["git", "-C", str(root), "add", "pm/generation.xsh"], check=True)
            subprocess.run(["git", "-C", str(root), "-c", "user.name=G1 fixture", "-c", "user.email=g1-fixture@example.invalid", "commit", "--quiet", "-m", "test(util-linux): freeze Laputa corpus fixture"], check=True)
            (root / "untracked.xsh").write_text('print "dirty"\n')
            with self.assertRaisesRegex(ValueError, "Laputa corpus must be clean"):
                with resources._laputa_corpus_scope(root):
                    self.fail("dirty corpus was selected")
            with self.assertRaisesRegex(RuntimeError, "explicit Laputa corpus selection"):
                resources._selected_laputa_corpus()

    def test_scope_detects_changes_and_preserves_the_primary_exception(self):
        with self.assertRaisesRegex(ValueError, "Laputa corpus changed or became dirty"):
            with laputa_corpus_scope() as root:
                (root / "pm/generation.xsh").write_text('print "changed"\n')

        with self.assertRaisesRegex(ValueError, "Laputa corpus changed or became dirty"):
            with laputa_corpus_scope() as root:
                (root / "pm/generation.xsh").write_text('print "changed"\n')
                subprocess.run(["git", "-C", str(root), "add", "pm/generation.xsh"], check=True)
                subprocess.run(["git", "-C", str(root), "-c", "user.name=G1 fixture", "-c", "user.email=g1-fixture@example.invalid", "commit", "--quiet", "-m", "test(util-linux): change Laputa corpus fixture"], check=True)

        with self.assertRaisesRegex(RuntimeError, "primary failure"):
            with laputa_corpus_scope() as root:
                (root / "untracked.xsh").write_text('print "dirty"\n')
                raise RuntimeError("primary failure")

    def test_scope_notes_postcheck_failure_on_the_original_exception(self):
        primary = RuntimeError("primary failure")
        with self.assertRaises(RuntimeError) as raised:
            with laputa_corpus_scope() as root:
                (root / "untracked.xsh").write_text('print "dirty"\n')
                raise primary

        self.assertIs(raised.exception, primary)
        self.assertTrue(any("Laputa corpus changed or became dirty" in note for note in raised.exception.__notes__))


class SourceIdentityTests(unittest.TestCase):
    def test_clean_identity_binds_commit_tree_and_worktree_status(self):
        outputs = iter((COMMIT + "\n", TREE + "\n", ""))
        calls = []

        def run(argv, **kwargs):
            calls.append((argv, kwargs))
            return subprocess.CompletedProcess(argv, 0, next(outputs), "")

        identity = resources.source_identity(ROOT, run=run)

        self.assertEqual(identity, SOURCE)
        self.assertEqual(len(calls), 3)
        self.assertTrue(all(call[1]["cwd"] == ROOT for call in calls))
        self.assertEqual(calls[-1][0], ["git", "status", "--porcelain=v1", "--untracked-files=all"])

    def test_dirty_worktree_is_reported_as_dirty(self):
        outputs = iter((COMMIT, TREE, " M core/lslocks.xsh\n"))

        def run(argv, **_kwargs):
            return subprocess.CompletedProcess(argv, 0, next(outputs), "")

        self.assertFalse(resources.source_identity(ROOT, run=run).clean)

    def test_git_failure_is_not_replaced_with_a_source_default(self):
        def run(argv, **_kwargs):
            return subprocess.CompletedProcess(argv, 128, "", "fatal: no HEAD")

        with self.assertRaisesRegex(RuntimeError, "rev-parse.*failed"):
            resources.source_identity(ROOT, run=run)


class RegressionSelectorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-selectors-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.files = (
            "core/tests/test-util-linux-process-regressions.xsh",
            "core/tests/test-util-linux-storage-regressions.xsh",
        )
        for relative, name in zip(self.files, ("test_process_case", "test_storage_case"), strict=True):
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"test {name} {{\n}}\n", encoding="utf-8")

    def test_clean_source_returns_actual_declared_selectors_in_stable_order(self):
        identities = iter((SOURCE, SOURCE))
        with patch.object(acceptance, "REGRESSION_FILES", self.files):
            selectors = resources.regression_selectors(
                self.root,
                SOURCE,
                identify=lambda _root: next(identities),
            )

        self.assertEqual(selectors, tuple(sorted(REGRESSIONS)))

    def test_resource_denominator_is_the_frozen_source_declaration_set(self):
        runtime = type("Runtime", (), {
            "ROOT": self.root,
            "source_identity": lambda _self: SOURCE,
            "regression_selectors": lambda _self, _source: tuple(sorted(REGRESSIONS)),
        })()
        with patch.object(acceptance, "REGRESSION_FILES", self.files):
            self.assertEqual(resources._selectors(runtime, SOURCE), tuple(sorted(REGRESSIONS)))

    def test_dirty_or_stale_source_fails_before_reading_files(self):
        dirty = SourceIdentity(COMMIT, TREE, False)
        stale = SourceIdentity("c" * 40, TREE, True)
        with patch.object(acceptance, "REGRESSION_FILES", ("missing.xsh",)):
            for source, observed in ((SOURCE, dirty), (stale, SOURCE)):
                with self.subTest(source=source), self.assertRaisesRegex(ValueError, "current clean source"):
                    resources.regression_selectors(
                        self.root,
                        source,
                        identify=lambda _root, value=observed: value,
                    )

    def test_missing_empty_changed_and_duplicate_declarations_fail(self):
        with patch.object(acceptance, "REGRESSION_FILES", ("missing.xsh",)):
            with self.assertRaisesRegex(ValueError, "missing required regression source"):
                acceptance.declared_regression_selectors(self.root)
        empty = self.root / self.files[0]
        empty.write_text("let no_test = 1\n", encoding="utf-8")
        with patch.object(acceptance, "REGRESSION_FILES", (self.files[0],)):
            with self.assertRaisesRegex(ValueError, "declares no behaviors"):
                acceptance.declared_regression_selectors(self.root)
        empty.write_text("test test_duplicate {\n}\ntest test_duplicate {\n}\n", encoding="utf-8")
        with patch.object(acceptance, "REGRESSION_FILES", (self.files[0],)):
            with self.assertRaisesRegex(ValueError, "duplicates behavior selectors"):
                acceptance.declared_regression_selectors(self.root)

    def test_source_change_while_selectors_are_read_fails(self):
        changed = SourceIdentity(COMMIT, "c" * 40, True)
        identities = iter((SOURCE, changed))
        with patch.object(acceptance, "REGRESSION_FILES", self.files):
            with self.assertRaisesRegex(ValueError, "source changed while reading"):
                resources.regression_selectors(
                    self.root,
                    SOURCE,
                    identify=lambda _root: next(identities),
                )


class LsmemFixtureTests(unittest.TestCase):
    def test_copy_preserves_relative_node_links_and_only_fixture_contents(self):
        source = ROOT / "core/tests/fixtures/util-linux/memory/sparse"
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-lsmem-")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name) / "lsmem"
        tree = ast.parse(resources._driver_source(REGRESSIONS, COHORT))
        helper = next(
            node for node in tree.body
            if isinstance(node, ast.FunctionDef) and node.name == "copy_lsmem_fixture"
        )
        snapshot = next(
            node for node in tree.body
            if isinstance(node, ast.FunctionDef) and node.name == "fixture_tree"
        )
        namespace = {
            "hashlib": resources.hashlib,
            "os": os,
            "Path": Path,
            "shutil": __import__("shutil"),
        }
        module = ast.fix_missing_locations(ast.Module(body=[snapshot, helper], type_ignores=[]))
        exec(compile(module, "lsmem-copy-control", "exec"), namespace)

        namespace["copy_lsmem_fixture"](source, root)
        source_tree = namespace["fixture_tree"](source)
        copied_tree = namespace["fixture_tree"](root)
        source_links = tuple(row for row in source_tree if row[1] == "link")
        copied_links = tuple(row for row in copied_tree if row[1] == "link")
        self.assertTrue(source_links)
        self.assertEqual(copied_tree, source_tree)
        self.assertEqual(copied_links, source_links)


class BuildScratchTests(unittest.TestCase):
    def test_cleanup_preserves_link_targets_and_removes_only_owned_contents(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-build-cleanup-")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name) / "owned"
        root.mkdir()
        outside = Path(self.temp.name) / "outside"
        outside.mkdir()
        sentinel = outside / "sentinel"
        sentinel.write_text("keep", encoding="utf-8")
        nested = root / "nested"
        nested.mkdir()
        (nested / "artifact").write_text("remove", encoding="utf-8")
        (root / "outside-link").symlink_to(outside, target_is_directory=True)
        tree = ast.parse(resources._BUILD_CLEANUP_DRIVER)
        cleanup = next(
            node for node in tree.body
            if isinstance(node, ast.FunctionDef) and node.name == "clear_build_directory"
        )
        namespace = {
            "Path": Path,
            "shutil": shutil,
            "stat": __import__("stat"),
        }
        module = ast.fix_missing_locations(ast.Module(body=[cleanup], type_ignores=[]))
        exec(compile(module, "build-cleanup-control", "exec"), namespace)

        namespace["clear_build_directory"](root)

        self.assertEqual(tuple(root.iterdir()), ())
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")

    def test_build_source_scratch_cleanup_does_not_mask_primary_failure(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-build-source-")
        self.addCleanup(self.temp.cleanup)
        with patch.object(resources.shutil, "rmtree", side_effect=PermissionError("cleanup failed")):
            with self.assertRaisesRegex(RuntimeError, "build failed") as raised:
                with resources._build_source_scratch(Path(self.temp.name)):
                    raise RuntimeError("build failed")

        self.assertEqual(len(raised.exception.__notes__), 1)
        self.assertIn("build-source scratch retained at", raised.exception.__notes__[0])

    def test_source_archives_replace_the_tree_at_one_path_without_following_links(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-build-stage-")
        self.addCleanup(self.temp.cleanup)
        scratch = Path(self.temp.name)
        archives = []
        for name, payload in (("baseline", "old"), ("candidate", "new")):
            root = scratch / name
            root.mkdir()
            (root / "manifest").write_text(payload, encoding="utf-8")
            archive = scratch / f"{name}.tar"
            with tarfile.open(archive, "w") as tar:
                tar.add(root / "manifest", arcname="manifest")
            archives.append(archive)
        work = scratch / "work"
        work.mkdir()
        outside = scratch / "outside"
        outside.mkdir()
        sentinel = outside / "sentinel"
        sentinel.write_text("keep", encoding="utf-8")
        (work / "outside-link").symlink_to(outside, target_is_directory=True)

        with laputa_corpus_scope():
            tree = ast.parse(resources._build_driver_source({"baseline": (), "candidate": ()}, PairOrder.BASELINE_FIRST, 0))
        functions = [
            node for node in tree.body
            if isinstance(node, ast.FunctionDef) and node.name in ("clear_build_directory", "stage_source")
        ]
        namespace = {
            "Path": Path,
            "shutil": shutil,
            "stat": __import__("stat"),
            "subprocess": subprocess,
        }
        module = ast.fix_missing_locations(ast.Module(body=functions, type_ignores=[]))
        exec(compile(module, "build-source-stage-control", "exec"), namespace)
        work_inode = work.stat().st_ino

        for archive, payload in zip(archives, ("old", "new"), strict=True):
            namespace["stage_source"](work, archive)
            self.assertEqual(work.stat().st_ino, work_inode)
            self.assertEqual((work / "manifest").read_text(encoding="utf-8"), payload)
            self.assertFalse((work / "outside-link").is_symlink())
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")


class ProductSignatureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-products-")
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        for name in resources.PRODUCTS:
            product = self.directory / name
            product.write_bytes((name + "\n").encode())
            product.chmod(0o755)

    def test_signature_tracks_only_regular_executable_product_contents(self):
        before = resources._product_signature(self.directory)
        self.assertEqual(tuple(row[0] for row in before), resources.PRODUCTS)
        self.assertEqual(len(before[0][-1]), 32)
        (self.directory / "xsh").write_bytes(b"different\n")
        after = resources._product_signature(self.directory)
        self.assertNotEqual(before, after)

    def test_product_mutation_while_hashing_is_rejected(self):
        target = self.directory / "xsh"

        def mutate_during_digest(stream, algorithm):
            data = stream.read()
            target.write_bytes(data + b"changed")
            return resources.hashlib.sha256(data)

        with patch.object(resources.hashlib, "file_digest", side_effect=mutate_during_digest):
            with self.assertRaisesRegex(ValueError, "changed while hashing"):
                resources._product_signature(self.directory)

    def test_symlink_nonexecutable_and_non_directory_products_fail(self):
        product = self.directory / "xsht"
        product.unlink()
        product.symlink_to(self.directory / "xsh")
        with self.assertRaises((OSError, ValueError)):
            resources._product_signature(self.directory)
        product.unlink()
        product.write_bytes(b"not executable")
        product.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "regular executable"):
            resources._product_signature(self.directory)
        with self.assertRaises((NotADirectoryError, OSError)):
            resources._product_signature(self.directory / "xsh")

    def test_verification_profile_is_rejected_before_product_reads(self):
        class Runtime:
            PRODUCT_SOURCE = "protected-source"
            IMAGE = "pinned-image"

            def baseline_products(self):
                return Path("/missing/baseline"), BuildContext(
                    "protected-source", "release", "target", "pinned-image", "mimalloc", "1cpu/1GiB"
                )

            def candidate_products(self):
                return Path("/missing/candidate"), BuildContext(
                    SOURCE.key, "verification", "target", "pinned-image", "mimalloc", "1cpu/1GiB"
                )

        with patch.object(resources, "_product_signature", side_effect=AssertionError("product read")):
            with self.assertRaisesRegex(ValueError, "final release products"):
                resources._product_pair(Runtime(), SOURCE)


class ClosePositiveBoundaryTests(unittest.TestCase):
    def test_close_parses_actual_positive_selector_and_accounts_private_gate(self):
        positive_name = acceptance.POSITIVE_BOUNDARIES["lsipc"]
        positive_selector = f"core/tests/test-lsipc.xsh::{positive_name}"
        regression_selectors = (
            "core/tests/test-util-linux-process-regressions.xsh::test_process_case",
            "core/tests/test-util-linux-storage-regressions.xsh::test_storage_case",
        )
        baseline = BuildContext("baseline", "release", resources.TARGET, "image", "mimalloc", "1cpu/1GiB")
        candidate = BuildContext(SOURCE.key, "release", resources.TARGET, "image", "mimalloc", "1cpu/1GiB")
        native_calls = []

        class Runtime:
            ROOT = ROOT
            COHORT = ("lsipc",)

            def source_identity(self):
                return SOURCE

            def native_case(self, selector, *, private=False, timeout=30):
                native_calls.append((selector, private, timeout))
                return (
                    f"running 1 tests\n{selector} ... ok 1ms\n"
                    "test result: ok. 1 passed; 0 failed; 0 skipped\n"
                ).encode()

            def inventory(self):
                pass

            def harness(self):
                pass

            def baseline(self):
                pass

            def gate(self, name):
                return GateResult(name, SOURCE, 0, 1, 1, 0, (), (), (), ())

            def run_full_native(self, source):
                return self.gate("full-native")

            def run_host_boundaries(self, source):
                return self.gate("host-boundaries")

            def run_repository_contract(self, source):
                return self.gate("repository-contract")

            def run_docs(self, source):
                return self.gate("docs")

            def measure_resources(self):
                report = acceptance.ResourceReport(
                    SOURCE,
                    (acceptance.BinarySize("xsh", 1, 1, baseline, candidate),),
                    (),
                    (),
                    acceptance.MeasuredTiming((), (), acceptance.BuildCost(baseline, candidate, ())),
                )
                return report

            def adversarial_review(self, source):
                return self.gate("adversarial-review")

            def owned_cleanup(self, source):
                observations = (
                    "remaining-owned-processes:0",
                    "unresolved-markers:0",
                    "lost-work:false",
                )
                return GateResult("cleanup", SOURCE, 0, 3, 3, 0, (), (), observations, ())

        with (
            patch.object(resources, "_product_pair"),
            patch.object(resources, "_selectors", return_value=regression_selectors),
            patch.object(acceptance, "_case_names", return_value=(positive_name,)),
            patch.object(acceptance, "_validate_resource_report"),
        ):
            report = resources.close(Runtime())

        gates = {gate.name: gate for gate in report.gates}
        self.assertEqual(gates["parity"].selectors, (positive_selector,))
        self.assertEqual(gates["parity"].private_selectors, (positive_selector,))
        positive = gates["positive-boundaries"]
        self.assertEqual(positive.executed, 1)
        self.assertEqual(positive.selectors, (positive_selector,))
        self.assertEqual(positive.private_selectors, (positive_selector,))
        self.assertEqual(
            tuple(sorted(gates["focused-regressions"].selectors)),
            tuple(sorted(regression_selectors)),
        )
        self.assertEqual(
            tuple(sorted(gates["focused-regressions"].private_selectors)),
            tuple(sorted(regression_selectors)),
        )
        self.assertEqual(
            tuple(selector for selector, private, _timeout in native_calls if private),
            (positive_selector, positive_selector, *sorted(regression_selectors)),
        )


class CloseCommandDispatchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="close-command-dispatch-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.context = BuildContext(
            SOURCE.key,
            "release",
            resources.TARGET,
            "pinned-image",
            "mimalloc",
            "1cpu/4GiB",
        )

    def test_api_snippet_check_runs_from_worktree_root(self):
        private = ("core/tests/test-host-boundary.xsh::test_boundary",)
        api = ("tests/xsh/api-tool.xsh::test_api",)
        calls = []

        class Runtime:
            ROOT = self.root

            def source_identity(self):
                return SOURCE

            def container(self, script, **kwargs):
                calls.append((script, kwargs))
                return b""

        with (
            laputa_corpus_scope(),
            patch.object(resources, "_qualified_candidate", return_value=(Path("/candidate"), self.context, ())),
            patch.object(resources, "_run_private_native", return_value=private),
            patch.object(resources, "_recheck_candidate"),
            patch.object(resources, "_cargo_mounts", return_value=[]),
            patch.object(resources, "_native_file", return_value=acceptance.NativeResult(1, 0, 0, api)),
            patch.object(resources, "_parse_rust_gate", return_value=(1, ("rust::test",), ())),
            patch.object(acceptance, "declared_native_selectors", return_value=private),
        ):
            resources.run_host_boundaries(Runtime(), SOURCE)

        script = next(script for script, _kwargs in calls if "docs/snippets/api" in script)
        self.assertTrue(script.startswith("cd /work; "))

    def test_repository_contract_runs_from_worktree_root(self):
        selector = "dev/tests/test-repository-contracts.xsh::test_contract"
        source = self.root / "dev/tests/test-repository-contracts.xsh"
        source.parent.mkdir(parents=True)
        source.write_text("test test_contract {\n}\n", encoding="utf-8")
        calls = []

        class Runtime:
            ROOT = self.root

            def source_identity(self):
                return SOURCE

            def container(self, script, **kwargs):
                calls.append((script, kwargs))
                return (
                    f"running 1 tests\n{selector} ... ok 1ms\n"
                    "test result: ok. 1 passed; 0 failed; 0 skipped\n"
                ).encode()

        with (
            patch.object(resources, "_qualified_candidate", return_value=(Path("/candidate"), self.context, ())),
            patch.object(resources, "_recheck_candidate"),
        ):
            resources.run_repository_contract(Runtime(), SOURCE)

        self.assertTrue(calls[0][0].startswith("cd /work; "))


class IdentityKeeperDeadlineTests(unittest.TestCase):
    def test_outer_wait_leaves_the_deadline_to_its_container_owner(self):
        path = ROOT / "core/tests/fixtures/util-linux/contexts/identity_keeper.py"
        namespace = {"__name__": "identity_keeper_test"}
        exec(compile(path.read_text(encoding="utf-8"), str(path), "exec"), namespace)

        class Child:
            pid = 123

            def __init__(self):
                self.returncode = None

            def poll(self):
                return self.returncode

            def wait(self, *args, **kwargs):
                timeout = kwargs.get("timeout", args[0] if args else None)
                if timeout is not None and timeout < 300 and self.returncode is None:
                    raise namespace["subprocess"].TimeoutExpired(["true"], timeout)
                if self.returncode is None:
                    self.returncode = 0
                return self.returncode

        child = Child()
        file_info = type("FileInfo", (), {"st_dev": 1, "st_ino": 2})()

        def terminate_group(_pid, number):
            child.returncode = -number

        with (
            patch.object(namespace["tempfile"], "mkstemp", return_value=(11, "/t/keeper")),
            patch.object(namespace["os"], "fstat", return_value=file_info),
            patch.object(namespace["os"], "close"),
            patch.object(namespace["os"], "unlink"),
            patch.object(namespace["os"], "killpg", side_effect=terminate_group),
            patch.object(namespace["fcntl"], "flock"),
            patch.object(namespace["signal"], "pthread_sigmask", return_value=set()),
            patch.object(namespace["signal"], "signal"),
            patch.object(namespace["subprocess"], "Popen", return_value=child),
        ):
            self.assertEqual(namespace["outer"](["true"]), 0)


class FullNativeProducerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="full-native-producer-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.products = self.root / "candidate"
        self.products.mkdir()
        for name in resources.PRODUCTS:
            product = self.products / name
            product.write_bytes(name.encode())
            product.chmod(0o755)
        self.context = BuildContext(
            SOURCE.key,
            "release",
            resources.TARGET,
            "pinned-image",
            "mimalloc",
            "1cpu/4GiB",
        )
        self.cohort = COHORT
        self.selectors = []
        for command in COHORT:
            self.add_source(f"core/tests/test-{command}.xsh", f"test_cohort_{command}")
        for relative in acceptance.HOST_BOUNDARY_FILES:
            self.add_source(relative, "test_host_boundary")
        for relative, name in zip(acceptance.REGRESSION_FILES, ("test_storage_regression", "test_process_regression"), strict=True):
            self.add_source(relative, name)
        self.add_source("dev/tests/test-optional.xsh", "test_optional_skip", skip=True)
        self.mandatory = tuple(sorted(self.selectors[:-1]))
        self.expected = tuple(sorted((*self.mandatory, self.selectors[-1])))

    def add_source(self, relative, name, *, skip=False):
        source = self.root / relative
        source.parent.mkdir(parents=True, exist_ok=True)
        body = f'test {name} {{\n  test.skip("fixture unavailable")\n}}\n' if skip else f"test {name} {{\n}}\n"
        source.write_text(body, encoding="utf-8")
        self.selectors.append(f"{relative}::{name}")

    def native_output(self, *, skip_selector=None):
        lines = [f"running {len(self.expected)} tests"]
        skipped = 0
        for selector in self.expected:
            if selector == skip_selector:
                lines.append(f"{selector} ... skipped: fixture unavailable 1ms")
                skipped += 1
            else:
                lines.append(f"{selector} ... ok 1ms")
        lines.append(
            f"test result: ok. {len(self.expected) - skipped} passed; 0 failed; {skipped} skipped"
        )
        return "\n".join(lines).encode()

    def framed_output(self, *, skip_selector=None):
        def frame(label, content):
            return b"@@xsh-util-linux:" + label.encode() + b":" + str(len(content)).encode() + b"@@\n" + content + b"\n"

        listed = ("\n".join(self.expected) + "\n").encode()
        return frame("list", listed) + frame("result", self.native_output(skip_selector=skip_selector))

    def runtime(self, *, skip_selector=None, mutate_products=False):
        case = self

        class Runtime:
            ROOT = case.root
            COHORT = case.cohort
            IMAGE = "pinned-image"
            PRIVATE_NAMESPACE = "exec private-namespace"

            def source_identity(self):
                return SOURCE

            def candidate_products(self):
                return case.products, case.context

            def regression_selectors(self, source):
                return acceptance.declared_regression_selectors(case.root)

            def container(self, script, **kwargs):
                case.dispatch = (script, kwargs)
                if mutate_products:
                    (case.products / "xsh").write_bytes(b"changed after dispatch")
                return case.framed_output(skip_selector=skip_selector)

        return Runtime()

    def test_full_native_uses_private_single_job_container_and_accounts_real_skips(self):
        result = resources.run_full_native(self.runtime(skip_selector=self.selectors[-1]), SOURCE)

        self.assertEqual(result.executed, len(self.expected))
        self.assertEqual(result.passed, len(self.expected) - 1)
        self.assertEqual(result.skipped, (f"{self.selectors[-1]}: fixture unavailable",))
        self.assertEqual(result.accounted_skips, result.skipped)
        script, options = self.dispatch
        self.assertIn("private-namespace", script)
        self.assertIn('"/products/xsht", "test", "--jobs", "1"', script)
        self.assertEqual((options["cpus"], options["memory_gib"]), (1, 4))
        self.assertEqual(options["timeout"], resources.FULL_NATIVE_TIMEOUT_SECONDS)

    def test_full_native_rejects_mandatory_skips_and_product_mutation(self):
        for runtime, reason in (
            (self.runtime(skip_selector=self.mandatory[0]), "skipped mandatory behavior"),
            (self.runtime(mutate_products=True), "candidate products changed"),
        ):
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                resources.run_full_native(runtime, SOURCE)


class PrivateHostBoundaryTests(unittest.TestCase):
    def test_each_declared_boundary_case_uses_a_private_exact_dispatch(self):
        selector = "core/tests/test-host-boundary.xsh::test_boundary"
        output = (
            f"running 1 tests\n{selector} ... ok 1ms\n"
            "test result: ok. 1 passed; 0 failed; 0 skipped\n"
        ).encode()
        calls = []
        runtime = type("Runtime", (), {
            "source_identity": lambda _self: SOURCE,
            "native_case": lambda _self, actual, **kwargs: calls.append((actual, kwargs)) or output,
        })()

        result = resources._run_private_native(
            runtime,
            SOURCE,
            (selector,),
            monotonic=lambda: 0,
        )

        self.assertEqual(result, (selector,))
        self.assertEqual(calls, [(selector, {"private": True, "timeout": 30.0})])

    def test_private_boundary_rejects_zero_unknown_duplicate_and_skipped_cases(self):
        runtime = type("Runtime", (), {"source_identity": lambda _self: SOURCE})()
        for selectors in ((), ("x::test_one", "x::test_one")):
            with self.subTest(selectors=selectors), self.assertRaisesRegex(ValueError, "nonempty and distinct"):
                resources._run_private_native(runtime, SOURCE, selectors)

        selector = "tests/xsh/a.xsh::test_boundary"
        skipped = (
            f"running 1 tests\n{selector} ... skipped: condition 1ms\n"
            "test result: ok. 0 passed; 0 failed; 1 skipped\n"
        ).encode()
        runtime.native_case = lambda *_args, **_kwargs: skipped
        with self.assertRaisesRegex(ValueError, "mandatory behavior"):
            resources._run_private_native(runtime, SOURCE, (selector,), monotonic=lambda: 0)


class BuildCostAdapterTests(unittest.TestCase):
    def test_build_driver_stages_both_archives_at_the_same_work_path(self):
        with laputa_corpus_scope():
            driver = resources._build_driver_source(
                {"baseline": (), "candidate": ()},
                PairOrder.BASELINE_FIRST,
                0,
            )

        self.assertIn('Path(f"/cargo-target/{side}-{sample}")', driver)
        self.assertIn('commands = ((Path("/baseline.tar"), "baseline"), (Path("/candidate.tar"), "candidate"))', driver)
        self.assertIn('stage_source(Path("/work"), source_archive)', driver)
        self.assertIn('results[side] = build(Path("/work"), side, sample)', driver)
        self.assertIn('"TMPDIR": str(temporary)', driver)
        self.assertIn('"TARGET": TARGET', driver)
        self.assertIn('temporary = target / "tmp"', driver)
        self.assertNotIn("/baseline-source", driver)
        self.assertNotIn("/t/targets/", driver)

    def test_build_archives_pin_baseline_and_clean_candidate_identity(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-build-archives-")
        self.addCleanup(self.temp.cleanup)
        scratch = Path(self.temp.name)
        source_checks = []

        class Runtime:
            ROOT = ROOT
            PRODUCT_SOURCE = "c" * 40

            def source_identity(self):
                source_checks.append(True)
                return SOURCE

        runtime = Runtime()
        archives = {"baseline": scratch / "baseline.tar", "candidate": scratch / "candidate.tar"}
        calls = []

        def archive(root, commit, destination):
            calls.append((root, commit, destination))
            return archives["baseline" if commit == runtime.PRODUCT_SOURCE else "candidate"]

        with patch.object(resources, "_git_archive", side_effect=archive):
            baseline, candidate = resources._build_archives(runtime, SOURCE, scratch)

        self.assertEqual((baseline, candidate), (archives["baseline"], archives["candidate"]))
        self.assertEqual(
            calls,
            [
                (ROOT, runtime.PRODUCT_SOURCE, scratch / "baseline.tar"),
                (ROOT, SOURCE.commit, scratch / "candidate.tar"),
            ],
        )
        self.assertEqual(len(source_checks), 2)

    def test_build_archives_reject_a_candidate_changed_while_archiving(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-build-archive-change-")
        self.addCleanup(self.temp.cleanup)
        scratch = Path(self.temp.name)
        source_checks = iter((SOURCE, SourceIdentity(COMMIT, TREE, False)))

        class Runtime:
            ROOT = ROOT
            PRODUCT_SOURCE = "c" * 40

            def source_identity(self):
                return next(source_checks)

        with (
            patch.object(resources, "_git_archive", side_effect=lambda _root, _commit, destination: destination),
            self.assertRaisesRegex(ValueError, "source changed during build-cost source archives"),
        ):
            resources._build_archives(Runtime(), SOURCE, scratch)

    def test_cargo_mounts_pin_the_mold_binary_used_by_the_container(self):
        runtime = type("Runtime", (), {"ROOT": ROOT})()
        calls = []

        def which(tool):
            calls.append(tool)
            return {
                "kache": "/usr/local/bin/kache",
                "mold": "/home/josh/.local/bin/mold",
            }[tool]

        with (
            patch.object(resources.shutil, "which", side_effect=which),
            patch.object(Path, "is_file", return_value=True),
            patch.object(Path, "is_dir", return_value=True),
            patch.object(resources.os, "access", return_value=True),
        ):
            with laputa_corpus_scope() as corpus:
                mounts = resources._cargo_mounts(runtime, Path("/build-target"))

        self.assertEqual(calls, ["kache"])
        self.assertIn(
            f"type=bind,source={resources.MOLD_PATH},target=/usr/local/bin/mold,readonly",
            mounts,
        )
        self.assertIn(f"type=bind,source={ROOT},target=/work,readonly", mounts)
        self.assertIn(f"type=bind,source={corpus},target={corpus},readonly", mounts)
        self.assertNotIn(
            "type=bind,source=/home/josh/.local/bin/mold,target=/usr/local/bin/mold,readonly",
            mounts,
        )

    def test_cancelled_build_cleans_exact_mounted_scratch_and_preserves_primary(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-build-pair-")
        self.addCleanup(self.temp.cleanup)
        scratch = Path(self.temp.name)
        baseline_archive = scratch / "baseline.tar"
        candidate_archive = scratch / "candidate.tar"
        baseline_archive.write_bytes(b"baseline archive")
        candidate_archive.write_bytes(b"candidate archive")
        outside = scratch / "outside"
        outside.mkdir()
        sentinel = outside / "sentinel"
        sentinel.write_text("keep", encoding="utf-8")
        tree = ast.parse(resources._BUILD_CLEANUP_DRIVER)
        cleanup = next(
            node for node in tree.body
            if isinstance(node, ast.FunctionDef) and node.name == "clear_build_directory"
        )
        cleanup_namespace = {
            "Path": Path,
            "shutil": shutil,
            "stat": __import__("stat"),
        }
        module = ast.fix_missing_locations(ast.Module(body=[cleanup], type_ignores=[]))
        exec(compile(module, "build-pair-cleanup-control", "exec"), cleanup_namespace)
        build_calls = []
        cleanup_calls = []

        class Runtime:
            ROOT = ROOT

            def source_identity(self):
                return SOURCE

            def container(self, script, *, mounts, capabilities, timeout, cpus=1, memory_gib=1):
                cleanup_calls.append((script, mounts, capabilities, timeout, cpus, memory_gib))
                roots = {
                    mount.rsplit(",target=", 1)[1]: Path(mount.split("source=", 1)[1].split(",target=", 1)[0])
                    for mount in mounts
                }
                for target in ("/work", "/cargo-target"):
                    cleanup_namespace["clear_build_directory"](roots[target])
                return b""

        def interrupted_build(runtime, program, script, *, mounts, **_kwargs):
            build_calls.append((script, mounts))
            roots = {
                mount.rsplit(",target=", 1)[1]: Path(mount.split("source=", 1)[1].split(",target=", 1)[0])
                for mount in mounts
            }
            self.assertTrue(script.startswith("cd /work; python3 -c "))
            self.assertIn(
                f"type=bind,source={resources.MOLD_PATH},target=/usr/local/bin/mold,readonly",
                mounts,
            )
            self.assertIn(
                f"type=bind,source={baseline_archive},target=/baseline.tar,readonly",
                mounts,
            )
            self.assertIn(
                f"type=bind,source={candidate_archive},target=/candidate.tar,readonly",
                mounts,
            )
            worktree_mount = next(mount for mount in mounts if mount.endswith(",target=/work"))
            self.assertTrue(worktree_mount.startswith("type=bind,source="))
            self.assertNotIn("readonly", worktree_mount)
            for target in ("/work", "/cargo-target"):
                root = roots[target]
                (root / "artifact-dir").mkdir()
                (root / "artifact-dir" / "build-output").write_text("remove", encoding="utf-8")
                (root / "outside-link").symlink_to(outside, target_is_directory=True)
            raise TimeoutError("build timed out")

        with (
            laputa_corpus_scope(),
            patch.object(resources.shutil, "which", side_effect=lambda tool: "/usr/local/bin/kache"),
            patch.object(Path, "is_file", return_value=True),
            patch.object(Path, "is_dir", return_value=True),
            patch.object(resources.os, "access", return_value=True),
            patch.object(resources, "_readied_container", side_effect=interrupted_build),
        ):
            with self.assertRaisesRegex(TimeoutError, "build timed out"):
                resources._readied_build_pair(
                    Runtime(),
                    Path("/tools/perf-gate-ready"),
                    SOURCE,
                    baseline_archive,
                    candidate_archive,
                    {"baseline": (), "candidate": ()},
                    PairOrder.BASELINE_FIRST,
                    0,
                )

        self.assertEqual(len(build_calls), 1)
        self.assertEqual(len(cleanup_calls), 1)
        cleanup_script, cleanup_mounts, _, _, _, _ = cleanup_calls[0]
        self.assertEqual(cleanup_script, "python3 -c " + resources.shlex.quote(resources._BUILD_CLEANUP_DRIVER))
        self.assertEqual(
            tuple(mount.rsplit(",target=", 1)[1] for mount in cleanup_mounts),
            ("/work", "/cargo-target"),
        )
        self.assertEqual(set(scratch.iterdir()), {baseline_archive, candidate_archive, outside})
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")

    def test_build_cleanup_error_is_attached_to_the_primary_timeout(self):
        self.temp = tempfile.TemporaryDirectory(dir=SCRATCH, prefix="resource-build-cleanup-error-")
        self.addCleanup(self.temp.cleanup)
        scratch = Path(self.temp.name)
        baseline_archive = scratch / "baseline.tar"
        candidate_archive = scratch / "candidate.tar"
        baseline_archive.write_bytes(b"baseline archive")
        candidate_archive.write_bytes(b"candidate archive")

        class Runtime:
            ROOT = ROOT

            def source_identity(self):
                return SOURCE

            def container(self, *_args, **_kwargs):
                raise PermissionError("cleanup denied")

        def interrupted_build(_runtime, _program, _script, *, mounts, **_kwargs):
            root = next(
                Path(mount.split("source=", 1)[1].split(",target=", 1)[0])
                for mount in mounts
                if mount.endswith(",target=/work")
            )
            (root / "build-output").write_text("remove", encoding="utf-8")
            raise TimeoutError("build timed out")

        with (
            laputa_corpus_scope(),
            patch.object(resources.shutil, "which", side_effect=lambda _tool: "/usr/local/bin/kache"),
            patch.object(Path, "is_file", return_value=True),
            patch.object(Path, "is_dir", return_value=True),
            patch.object(resources.os, "access", return_value=True),
            patch.object(resources, "_readied_container", side_effect=interrupted_build),
        ):
            with self.assertRaisesRegex(TimeoutError, "build timed out") as raised:
                resources._readied_build_pair(
                    Runtime(),
                    Path("/tools/perf-gate-ready"),
                    SOURCE,
                    baseline_archive,
                    candidate_archive,
                    {"baseline": (), "candidate": ()},
                    PairOrder.BASELINE_FIRST,
                    0,
                )

        self.assertEqual(len(raised.exception.__notes__), 1)
        self.assertIn("build-cost scratch cleanup failed", raised.exception.__notes__[0])


class MeasurementProtocolTests(unittest.TestCase):
    def rows(self, regressions=REGRESSIONS):
        rows = [
            "\t".join(("positive", command, resources.POSITIVE_SELECTORS[command], "1024", "2048", "0", "0", "1", "1"))
            for command in COHORT
        ]
        rows.extend("\t".join(("regression", selector, "3072", "4096", "0", "0", "1", "1")) for selector in regressions)
        return rows

    def parse(self, rows, regressions=REGRESSIONS, cohort=COHORT):
        return resources._parse_measurements("\n".join(rows).encode(), cohort, regressions)

    def test_exact_positive_and_regression_denominators_are_required(self):
        parsed = self.parse(self.rows())
        self.assertEqual(len(parsed), len(COHORT) + len(REGRESSIONS))
        missing_positive = self.rows()
        missing_positive.pop(0)
        with self.assertRaisesRegex(ValueError, "positive boundary exactly once"):
            self.parse(missing_positive)
        with self.assertRaisesRegex(ValueError, "frozen regression selectors exactly once"):
            self.parse(self.rows(), regressions=REGRESSIONS + ("unexpected::test_extra",))

    def test_cargo_gates_inherit_pinned_musl_c_compiler_flags(self):
        with laputa_corpus_scope() as corpus:
            environment = resources._cargo_environment()
        self.assertNotIn("CFLAGS_x86_64_unknown_linux_musl=", environment)
        self.assertIn(f"XSH_LAPUTA_CORPUS={corpus}", environment)

    def test_cargo_environment_roundtrips_a_corpus_path_with_spaces(self):
        with laputa_corpus_scope(prefix="resource laputa spaced-") as corpus:
            result = subprocess.run(
                ["sh", "-c", resources._cargo_environment() + '; printf "%s" "$XSH_LAPUTA_CORPUS"'],
                check=True,
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.stdout, str(corpus))

    def test_zero_rows_duplicate_selectors_and_unknown_rows_fail(self):
        with self.assertRaisesRegex(ValueError, "positive boundary exactly once"):
            self.parse(())
        rows = self.rows()
        rows.append(rows[0])
        with self.assertRaisesRegex(ValueError, "repeated positive fixture"):
            self.parse(rows)
        rows = self.rows()
        rows.append("\t".join(("unknown", "unexpected", "fixture")))
        with self.assertRaisesRegex(ValueError, "unrecognized measurement"):
            self.parse(rows)

    def test_native_observation_frames_are_exact_and_complete(self):
        output = b"@@xsh-util-linux:first:3@@\nabc\n@@xsh-util-linux:second:0@@\n\n"
        self.assertEqual(resources._parse_frames(output, ("first", "second")), {
            "first": b"abc",
            "second": b"",
        })
        for malformed in (
            output + b"unaccounted",
            output.replace(b"first:3", b"first:4"),
            output.replace(b"first:3", b"first:x"),
            output.replace(b"second:0", b"other:0"),
        ):
            with self.subTest(malformed=malformed), self.assertRaises(ValueError):
                resources._parse_frames(malformed, ("first", "second"))

    def test_cargo_results_reconcile_lists_summaries_and_actual_skip_reasons(self):
        listed = ("api::active", "api::ignored")
        output = (
            "running 2 tests\n"
            "test api::active ... ok\n"
            "test api::ignored ... ignored, needs root\n"
            "test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s\n"
        ).encode()
        result = resources._rust_test_results("api", listed, output)
        self.assertEqual(result, (
            1,
            2,
            ("cargo:api::api::active", "cargo:api::api::ignored"),
            ("cargo:api::api::ignored: needs root",),
        ))

        filtered = (
            "running 1 tests\n"
            "test api::active ... ok\n"
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s\n"
        ).encode()
        self.assertEqual(
            resources._rust_test_results(
                "api",
                ("api::active", "split::excluded"),
                filtered,
                filtered=("split::excluded",),
            )[3],
            ("cargo:api::split::excluded: filtered by --skip split::",),
        )

    def test_cargo_results_reject_zero_missing_extra_duplicate_and_unaccounted_skips(self):
        listed = ("api::active", "split::filtered")
        valid = (
            "running 1 tests\n"
            "test api::active ... ok\n"
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s\n"
        ).encode()
        invalid = (
            (
                (),
                b"test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
                (),
                "completed Rust tests",
            ),
            (listed, valid, (), "exactly once"),
            (listed, valid + b"test api::active ... ok\n", ("split::filtered",), "exactly once"),
            (listed, valid.replace(b"1 filtered out", b"0 filtered out"), ("split::filtered",), "did not account"),
            (listed, valid, ("not-listed",), "invalid conditional skip set"),
        )
        for observed, output, filtered, reason in invalid:
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                resources._rust_test_results("api", observed, output, filtered=filtered)

        ignored_without_reason = (
            "running 1 tests\n"
            "test api::ignored ... ignored\n"
            "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
        ).encode()
        with self.assertRaisesRegex(ValueError, "omitted its reason"):
            resources._rust_test_results("api", ("api::ignored",), ignored_without_reason)

    def test_failed_skipped_zero_rss_mutated_output_and_state_fail(self):
        mutations = (
            ("1024\t2048\t0\t0\t1\t1", "0\t2048\t0\t0\t1\t1", "positive resource sample did not complete"),
            ("1024\t2048\t0\t0\t1\t1", "1024\t2048\t2\t0\t1\t1", "positive resource sample did not complete"),
            ("1024\t2048\t0\t0\t1\t1", "1024\t2048\t0\t0\t0\t1", "changed output or fixture state"),
            ("1024\t2048\t0\t0\t1\t1", "1024\t2048\t0\t0\t1\t0", "changed output or fixture state"),
        )
        for original, mutated, error in mutations:
            with self.subTest(mutated=mutated):
                rows = self.rows()
                rows[0] = rows[0].replace(original, mutated)
                with self.assertRaisesRegex(ValueError, error):
                    self.parse(rows)

    def test_malformed_numeric_observation_fails(self):
        rows = self.rows()
        rows[0] = rows[0].replace("1024", "1KiB", 1)
        with self.assertRaisesRegex(ValueError, "nonnumeric measurement"):
            self.parse(rows)

    def test_startup_targets_bind_each_command_to_the_actual_regression_set(self):
        expected = set(acceptance.STARTUP_TARGETS)
        regression_selectors = tuple(sorted({selector for _, selector in expected}))
        acceptance._validate_startup_targets(expected, regression_selectors)

        with self.assertRaisesRegex(ValueError, "fixed command targets exactly"):
            acceptance._validate_startup_targets(expected - {next(iter(expected))}, regression_selectors)
        with self.assertRaisesRegex(ValueError, "actual regression declarations"):
            acceptance._validate_startup_targets(expected, regression_selectors + ("unexpected::test_extra",))

    def test_new_reference_allows_distinct_allocator_but_existing_baseline_does_not(self):
        reference = BuildContext(
            "package",
            acceptance.REFERENCE_PROFILE,
            "target",
            "image",
            acceptance.REFERENCE_ALLOCATOR,
            "1cpu/1GiB",
        )
        candidate = BuildContext(SOURCE.key, "release", "target", "image", "mimalloc", "1cpu/1GiB")
        acceptance._validate_contexts(
            acceptance.MeasurementKind.NEW_REFERENCE,
            reference,
            candidate,
            SOURCE,
            "package",
            "product",
        )
        for invalid, reason in (
            (
                BuildContext("package", "unqualified", "target", "image", acceptance.REFERENCE_ALLOCATOR, "1cpu/1GiB"),
                "qualified util-linux profile",
            ),
            (
                BuildContext("package", acceptance.REFERENCE_PROFILE, "target", "image", "unknown", "1cpu/1GiB"),
                "util-linux allocator",
            ),
        ):
            with self.subTest(reference=invalid), self.assertRaisesRegex(ValueError, reason):
                acceptance._validate_contexts(
                    acceptance.MeasurementKind.NEW_REFERENCE,
                    invalid,
                    candidate,
                    SOURCE,
                    "package",
                    "product",
                )

        baseline = BuildContext("product", "release", "target", "image", "jemalloc", "1cpu/1GiB")
        with self.assertRaisesRegex(ValueError, "different allocators"):
            acceptance._validate_contexts(
                acceptance.MeasurementKind.EXISTING_BASELINE,
                baseline,
                candidate,
                SOURCE,
                "package",
                "product",
            )

    def test_peak_rss_cap_applies_to_new_reference_and_existing_commands(self):
        context = BuildContext("source", "release", "target", "image", "allocator", "1cpu/1GiB")
        baseline = 100 * 1024 * 1024
        allowance = baseline // 10
        for kind, comparison in (
            (MeasurementKind.NEW_REFERENCE, "new-command"),
            (MeasurementKind.EXISTING_BASELINE, "existing-command"),
        ):
            with self.subTest(kind=kind):
                at_limit = MemorySample(
                    "command", "selector", kind, baseline, baseline + allowance, context, context
                )
                acceptance._require_peak_rss_threshold(at_limit, comparison)
                over_limit = MemorySample(
                    "command", "selector", kind, baseline, baseline + allowance + 1, context, context
                )
                with self.assertRaisesRegex(ValueError, f"{comparison} limit"):
                    acceptance._require_peak_rss_threshold(over_limit, comparison)

    def test_timed_pair_requires_exact_target_completion_and_readiness(self):
        before = Readiness(0, b"", b"", True)
        after = Readiness(0, b"", b"", True)
        output = b"timing\tlslocks\ttest_lslocks_blocker_flock\t100\t110\t0\t0\t1\t1\n"
        pair = resources._parse_timed_pair(
            output,
            "lslocks",
            "test_lslocks_blocker_flock",
            PairOrder.BASELINE_FIRST,
            before,
            after,
        )
        self.assertEqual((pair.baseline_cpu_ns, pair.candidate_cpu_ns), (100, 110))
        self.assertIs(pair.before, before)
        self.assertIs(pair.after, after)
        self.assertIs(pair.order, PairOrder.BASELINE_FIRST)

        invalid = (
            (output.replace(b"lslocks", b"unknown", 1), "unexpected measurement target"),
            (output.replace(b"\t0\t0\t1\t1", b"\t1\t0\t1\t1"), "failed process"),
            (output.replace(b"\t1\t1\n", b"\t0\t1\n"), "changed output or state"),
            (output + output, "exactly one paired sample"),
            (b"timing\tlslocks\ttest_lslocks_blocker_flock\t0\t1\t0\t0\t1\t1\n", "no positive CPU sample"),
        )
        for observed, reason in invalid:
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                resources._parse_timed_pair(
                    observed,
                    "lslocks",
                    "test_lslocks_blocker_flock",
                    PairOrder.BASELINE_FIRST,
                    before,
                    after,
                )


class EffectBoundaryTests(unittest.TestCase):
    def test_readiness_uses_the_named_program_and_preserves_quiet_result(self):
        calls = []

        def run(argv, **kwargs):
            calls.append((argv, kwargs))
            return subprocess.CompletedProcess(argv, 0, b"", b"")

        ready = resources._readiness(Path("/tools/perf-gate-ready"), ("-1",), run=run)

        self.assertTrue(ready.host_quiet)
        self.assertEqual(calls[0][0], ["/tools/perf-gate-ready", "-1"])
        self.assertEqual(calls[0][1]["capture_output"], True)
        self.assertEqual(calls[0][1]["check"], False)
        self.assertEqual(calls[0][1]["timeout"], 30)
        self.assertEqual(calls[0][1]["env"]["PERF_GATE_ATTEMPTS"], "1")
        self.assertEqual(calls[0][1]["env"]["PERF_GATE_WAIT"], "0")

    def test_busy_pre_readiness_skips_child_and_post_readiness(self):
        calls = []

        def readiness(program, arguments):
            calls.append([str(program), *arguments])
            return Readiness(75, b"", b"host busy", False)

        class Runtime:
            def container(self, *args, **kwargs):
                raise AssertionError("measurement ran while host was busy")

        with patch.object(resources, "_readiness", side_effect=readiness):
            result = resources._readied_container(
                Runtime(),
                Path("/tools/perf-gate-ready"),
                "measurement",
                mounts=[],
                capabilities=[],
                timeout=1,
            )

        self.assertIsNone(result)
        self.assertEqual(calls, [["/tools/perf-gate-ready"]])

    def test_unreadable_or_malformed_readiness_fails_instead_of_deferring(self):
        class Runtime:
            def container(self, *args, **kwargs):
                raise AssertionError("measurement must not run without a qualified readiness result")

        for readiness in (
            Readiness(1, b"", b"cannot read load", False),
            Readiness(0, b"unexpected", b"", False),
            Readiness(0, b"", b"", False),
        ):
            with self.subTest(readiness=readiness), patch.object(
                resources,
                "_readiness",
                return_value=readiness,
            ), self.assertRaisesRegex(RuntimeError, "perf-gate-ready before measurement failed"):
                resources._readied_container(
                    Runtime(),
                    Path("/tools/perf-gate-ready"),
                    "measurement",
                    mounts=[],
                    capabilities=[],
                    timeout=1,
                )

    def test_post_readiness_unreadable_fails_instead_of_defer(self):
        results = iter((Readiness(0, b"", b"", True), Readiness(1, b"", b"cannot read load", False)))

        class Runtime:
            def container(self, *args, **kwargs):
                return b"measured"

        with patch.object(resources, "_readiness", side_effect=lambda *_args: next(results)):
            with self.assertRaisesRegex(RuntimeError, "perf-gate-ready after measurement failed"):
                resources._readied_container(
                    Runtime(),
                    Path("/tools/perf-gate-ready"),
                    "measurement",
                    mounts=[],
                    capabilities=[],
                    timeout=1,
                )

    def test_readied_container_checks_the_host_immediately_after_child(self):
        calls = []

        def readiness(program, arguments):
            calls.append([str(program), *arguments])
            return Readiness(0, b"", b"", True)

        class Runtime:
            def container(self, script, **kwargs):
                calls.append((script, kwargs))
                return b"measured"

        with patch.object(resources, "_readiness", side_effect=readiness):
            result = resources._readied_container(
                Runtime(),
                Path("/tools/perf-gate-ready"),
                "measurement",
                mounts=["mount"],
                capabilities=["SYS_ADMIN"],
                timeout=9,
                cpus=2,
                memory_gib=8,
            )

        self.assertEqual(result[0], b"measured")
        self.assertEqual(calls[0], ["/tools/perf-gate-ready"])
        self.assertEqual(calls[1], (
            "measurement",
            {
                "mounts": ["mount"],
                "capabilities": ["SYS_ADMIN"],
                "timeout": 9,
                "cpus": 2,
                "memory_gib": 8,
            },
        ))
        self.assertEqual(calls[2], ["/tools/perf-gate-ready", "-1"])

    def test_readied_container_runs_post_gate_even_when_child_fails(self):
        calls = []

        def readiness(program, arguments):
            calls.append([str(program), *arguments])
            return Readiness(0, b"", b"", True)

        class Runtime:
            def container(self, *args, **kwargs):
                raise RuntimeError("measurement failed")

        with patch.object(resources, "_readiness", side_effect=readiness):
            with self.assertRaisesRegex(RuntimeError, "measurement failed"):
                resources._readied_container(
                    Runtime(),
                    Path("/tools/perf-gate-ready"),
                    "measurement",
                    mounts=[],
                    capabilities=[],
                    timeout=1,
                )
        self.assertEqual(calls, [["/tools/perf-gate-ready"], ["/tools/perf-gate-ready", "-1"]])

    def test_post_readiness_error_does_not_hide_measurement_failure(self):
        calls = []

        def readiness(program, arguments):
            calls.append(tuple(arguments))
            if arguments:
                raise OSError("readiness command failed")
            return Readiness(0, b"", b"", True)

        class Runtime:
            def container(self, *args, **kwargs):
                raise RuntimeError("measurement failed")

        with patch.object(resources, "_readiness", side_effect=readiness):
            with self.assertRaisesRegex(RuntimeError, "measurement failed") as raised:
                resources._readied_container(
                    Runtime(),
                    Path("/tools/perf-gate-ready"),
                    "measurement",
                    mounts=[],
                    capabilities=[],
                    timeout=1,
                )
        self.assertEqual(calls, [(), ("-1",)])
        self.assertTrue(any("post-measurement readiness failed" in note for note in raised.exception.__notes__))

    def test_generated_container_programs_are_valid_python(self):
        driver = resources._driver_source(REGRESSIONS, COHORT)
        compile(driver, "resource-driver", "exec")
        with laputa_corpus_scope():
            build = resources._build_driver_source(
                {"baseline": (("xsh", "a" * 64),), "candidate": (("xsh", "b" * 64),)},
                PairOrder.BASELINE_FIRST,
                0,
            )
        compile(build, "build-driver", "exec")
        compile(resources._BUILD_CLEANUP_DRIVER, "build-cleanup-driver", "exec")

    def test_keeper_startup_failure_reaps_child_and_closes_pipes(self):
        tree = ast.parse(resources._driver_source(REGRESSIONS, COHORT))
        start_keeper = next(
            node for node in tree.body
            if isinstance(node, ast.FunctionDef) and node.name == "start_keeper"
        )
        namespace = {
            "selectors": resources.selectors if hasattr(resources, "selectors") else __import__("selectors"),
            "os": os,
            "time": __import__("time"),
            "subprocess": subprocess,
            "signal": __import__("signal"),
            "json": json,
            "ENV": os.environ.copy(),
        }
        function = ast.fix_missing_locations(ast.Module(body=[start_keeper], type_ignores=[]))
        exec(compile(function, "start-keeper-control", "exec"), namespace)
        original_popen = subprocess.Popen
        children = []

        def spawn(*args, **kwargs):
            child = original_popen(*args, **kwargs)
            children.append(child)
            return child

        with patch.object(subprocess, "Popen", side_effect=spawn):
            with self.assertRaises(json.JSONDecodeError):
                namespace["start_keeper"]([sys.executable, "-c", "print('invalid', flush=True)"])

        self.assertEqual(len(children), 1)
        child = children[0]
        self.assertIsNotNone(child.poll())
        self.assertTrue(child.stdin.closed)
        self.assertTrue(child.stdout.closed)
        self.assertTrue(child.stderr.closed)

    def test_keeper_request_failure_terminates_reaps_and_closes_pipes(self):
        tree = ast.parse(resources._driver_source(REGRESSIONS, COHORT))
        finish_keeper = next(
            node for node in tree.body
            if isinstance(node, ast.FunctionDef) and node.name == "finish_keeper"
        )
        namespace = {
            "os": os,
            "subprocess": subprocess,
            "signal": signal,
        }
        module = ast.fix_missing_locations(ast.Module(body=[finish_keeper], type_ignores=[]))
        exec(compile(module, "finish-keeper-control", "exec"), namespace)
        child = subprocess.Popen(
            [sys.executable, "-c", "import os,time; os.close(0); print('ready', flush=True); time.sleep(20)"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
            start_new_session=True,
        )
        try:
            ready, _, _ = select.select([child.stdout], [], [], 2)
            self.assertTrue(ready, "keeper child did not announce its closed-input state")
            self.assertEqual(child.stdout.readline(), b"ready\n")
            with self.assertRaises(BrokenPipeError):
                namespace["finish_keeper"](child, "DONE")
            self.assertIsNotNone(child.poll())
            self.assertTrue(child.stdin.closed)
            self.assertTrue(child.stdout.closed)
            self.assertTrue(child.stderr.closed)
        finally:
            if child.poll() is None:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                child.wait(timeout=2)
            for pipe in (child.stdin, child.stdout, child.stderr):
                if pipe is not None and not pipe.closed:
                    pipe.close()

    def test_keeper_that_exited_without_release_is_not_accepted_as_cleaned(self):
        tree = ast.parse(resources._driver_source(REGRESSIONS, COHORT))
        finish_keeper = next(
            node for node in tree.body
            if isinstance(node, ast.FunctionDef) and node.name == "finish_keeper"
        )
        namespace = {"os": os, "subprocess": subprocess, "signal": signal}
        module = ast.fix_missing_locations(ast.Module(body=[finish_keeper], type_ignores=[]))
        exec(compile(module, "finish-keeper-exit-control", "exec"), namespace)

        class Child:
            pid = 2_000_000_000
            returncode = 0
            stdin = io.BytesIO()
            stdout = io.BytesIO()
            stderr = io.BytesIO()

            def poll(self):
                return self.returncode

            def communicate(self, timeout):
                return b"", b""

        child = Child()
        with self.assertRaisesRegex(RuntimeError, "exited before cleanup was requested"):
            namespace["finish_keeper"](child, "DONE")
        self.assertTrue(child.stdin.closed)
        self.assertTrue(child.stdout.closed)
        self.assertTrue(child.stderr.closed)

    def test_dirty_source_stops_resource_and_close_effects(self):
        class Runtime:
            COHORT = COHORT

            def source_identity(self):
                return SourceIdentity(COMMIT, TREE, False)

            def __getattr__(self, name):
                raise AssertionError(f"effect ran before source validation: {name}")

        runtime = Runtime()
        with self.assertRaisesRegex(ValueError, "clean final source"):
            resources.measure(runtime)
        with self.assertRaisesRegex(ValueError, "clean final source"):
            resources.close(runtime)

    def test_close_rejects_verification_candidate_before_any_gate(self):
        class Runtime:
            COHORT = COHORT
            IMAGE = "image"
            PRODUCT_SOURCE = "baseline"

            def source_identity(self):
                return SOURCE

            def baseline_products(self):
                return Path("/missing/baseline"), BuildContext(
                    "baseline", "release", resources.TARGET, "image", "mimalloc", "1cpu/1GiB"
                )

            def candidate_products(self):
                return Path("/missing/candidate"), BuildContext(
                    SOURCE.key, "verification", resources.TARGET, "image", "mimalloc", "1cpu/1GiB"
                )

        with patch.object(resources, "_product_signature", side_effect=AssertionError("product read")):
            with self.assertRaisesRegex(ValueError, "final release products"):
                resources.close(Runtime())

    def test_returned_close_gate_must_be_current_and_passed(self):
        stale = SourceIdentity("old", "old-tree", True)
        gate = GateResult("docs", stale, 0, 1, 1, 0, (), (), (), ())
        with self.assertRaisesRegex(ValueError, "mismatched gate evidence"):
            resources._validate_returned_gate(gate, "docs", SOURCE)
        failed = GateResult("docs", SOURCE, 1, 1, 0, 1, (), (), (), ())
        with self.assertRaisesRegex(ValueError, "did not pass"):
            resources._validate_returned_gate(failed, "docs", SOURCE)

    def test_cleanup_state_requires_actual_zero_process_marker_and_handoff_observations(self):
        selectors = (
            "remaining-owned-processes:0",
            "unresolved-markers:0",
            "lost-work:false",
        )
        gate = GateResult("cleanup", SOURCE, 0, 3, 3, 0, (), (), selectors, ())
        self.assertEqual(resources._cleanup_state(gate, SOURCE), (0, False))

        invalid = (
            selectors[:2],
            ("remaining-owned-processes:1", *selectors[1:]),
            (selectors[0], "unresolved-markers:1", selectors[2]),
            (*selectors[:2], "lost-work:true"),
            (*selectors, "unknown:observation"),
        )
        for observations in invalid:
            with self.subTest(observations=observations):
                result = GateResult("cleanup", SOURCE, 0, 3, 3, 0, (), (), observations, ())
                with self.assertRaisesRegex(ValueError, "cleanup gate"):
                    resources._cleanup_state(result, SOURCE)

        undercounted = GateResult("cleanup", SOURCE, 0, 2, 2, 0, (), (), selectors, ())
        with self.assertRaisesRegex(ValueError, "exactly once"):
            resources._cleanup_state(undercounted, SOURCE)


if __name__ == "__main__":
    unittest.main()
