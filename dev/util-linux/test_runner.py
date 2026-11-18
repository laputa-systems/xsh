from contextlib import nullcontext
from pathlib import Path
import json
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import check


class ContainerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(dir=check.ROOT / ".work/tmp")
        self.addCleanup(self.directory.cleanup)
        self.scratch = Path(self.directory.name) / "container"
        self.scratch.mkdir()
        self.stack = []
        for target, value in (("check.require_native_lease", None), ("check.mark_cleanup_unresolved", None)):
            handle = patch(target, return_value=value)
            mocked = handle.start()
            self.addCleanup(handle.stop)
            self.stack.append(mocked)
        handle = patch("check.tempfile.mkdtemp", return_value=str(self.scratch))
        handle.start()
        self.addCleanup(handle.stop)

    def test_container_uses_the_pinned_image_id(self):
        with patch("check.subprocess.run", side_effect=[
            subprocess.CompletedProcess([], 0, b"result", b""),
            subprocess.CompletedProcess([], 0, b"removed", b""),
        ]) as run:
            self.assertEqual(check.container("printf result"), b"result")
        self.assertEqual(run.call_args_list[0].args[0][-4:],
                         [check.IMAGE_ID, "sh", "-ec", "printf result"])

    def test_cleanup_failure_cannot_qualify_success(self):
        with patch("check.subprocess.run", side_effect=[
            subprocess.CompletedProcess([], 0, b"result", b""),
            subprocess.CompletedProcess([], 1, b"", b"daemon failed"),
        ]):
            with self.assertRaisesRegex(RuntimeError, "cleanup failed"):
                check.container("true")
        self.stack[1].assert_called_once()
        self.assertTrue(self.scratch.exists())

    def test_removal_already_in_progress_waits_for_the_container_to_vanish(self):
        with patch("check.subprocess.run", side_effect=[
            subprocess.CompletedProcess([], 0, b"result", b""),
            subprocess.CompletedProcess([], 1, b"", b"Error response from daemon: removal of container x is already in progress\n"),
            subprocess.CompletedProcess([], 0, b"[{}]", b""),
            subprocess.CompletedProcess([], 1, b"", b"Error: No such container: x\n"),
        ]), patch("check.time.sleep"):
            self.assertEqual(check.container("true"), b"result")
        self.stack[1].assert_not_called()
        self.assertFalse(self.scratch.exists())

    def test_removal_in_progress_that_never_finishes_is_unresolved(self):
        responses = [subprocess.CompletedProcess([], 0, b"result", b""),
                     subprocess.CompletedProcess([], 1, b"", b"removal of container x is already in progress")]
        responses += [subprocess.CompletedProcess([], 0, b"[{}]", b"")] * 200
        with patch("check.subprocess.run", side_effect=responses), patch("check.time.sleep"):
            with self.assertRaisesRegex(RuntimeError, "cleanup failed"):
                check.container("true")
        self.stack[1].assert_called_once()
        self.assertTrue(self.scratch.exists())

    def test_timeout_removes_a_container_the_daemon_creates_after_the_first_removal(self):
        with patch("check.subprocess.run", side_effect=[
            subprocess.TimeoutExpired(["docker", "run"], 30),
            subprocess.CompletedProcess([], 1, b"", b"Error: No such container: x\n"),
            subprocess.CompletedProcess([], 1, b"", b"Error: No such container: x\n"),
            subprocess.CompletedProcess([], 0, b"[{}]", b""),
            subprocess.CompletedProcess([], 0, b"x\n", b""),
        ]) as run, patch("check.time.sleep"):
            with self.assertRaises(subprocess.TimeoutExpired):
                check.container("true")
        commands = [call.args[0][:3] for call in run.call_args_list[1:]]
        self.assertEqual(commands, [
            ["docker", "rm", "--force"],
            ["docker", "container", "inspect"],
            ["docker", "container", "inspect"],
            ["docker", "rm", "--force"],
        ])
        self.stack[1].assert_not_called()
        self.assertFalse(self.scratch.exists())

    def test_timeout_without_a_late_container_still_cleans_up(self):
        responses = [subprocess.TimeoutExpired(["docker", "run"], 30),
                     subprocess.CompletedProcess([], 1, b"", b"Error: No such container: x\n")]
        responses += [subprocess.CompletedProcess([], 1, b"", b"Error: No such container: x\n")] * 200
        with patch("check.subprocess.run", side_effect=responses), patch("check.time.sleep"):
            with self.assertRaises(subprocess.TimeoutExpired):
                check.container("true")
        self.stack[1].assert_not_called()
        self.assertFalse(self.scratch.exists())

    def test_cancellation_runs_cleanup_before_returning(self):
        with patch("check.subprocess.run", side_effect=[
            check.Cancelled(signal.SIGTERM),
            subprocess.CompletedProcess([], 0, b"removed", b""),
        ]) as run:
            with self.assertRaises(check.Cancelled):
                check.container("true")
        self.assertEqual(run.call_args_list[1].args[0][:3], ["docker", "rm", "--force"])
        self.assertFalse(self.scratch.exists())

    def test_primary_and_marker_failure_details_survive(self):
        self.stack[1].side_effect = OSError("quarantine write failed")
        with patch("check.subprocess.run", side_effect=[
            RuntimeError("primary failure"),
            subprocess.TimeoutExpired(["docker", "rm"], 10),
        ]):
            with self.assertRaisesRegex(RuntimeError, "primary failure") as caught:
                check.container("true")
        notes = " ".join(caught.exception.__notes__)
        self.assertIn("cleanup also failed", notes)
        self.assertIn("quarantine write failed", notes)

    def test_term_handler_unwinds_and_restores_previous_handler(self):
        previous = signal.getsignal(signal.SIGTERM)
        with self.assertRaises(check.Cancelled):
            with check.cancellation_scope():
                signal.raise_signal(signal.SIGTERM)
        self.assertEqual(signal.getsignal(signal.SIGTERM), previous)


class LaputaCorpusCliTests(unittest.TestCase):
    def test_resource_and_close_cli_require_an_explicit_corpus_before_leasing(self):
        for mode in ("resources", "close"):
            with self.subTest(mode=mode):
                with patch.object(sys, "argv", ["check.py", mode, "--check"]), patch.object(check, "campaign_leases") as leases:
                    with self.assertRaises(SystemExit) as caught:
                        check.main()
                self.assertEqual(caught.exception.code, 2)
                leases.assert_not_called()

    def test_resource_and_close_dispatch_bind_the_supplied_corpus(self):
        import acceptance
        import resources
        with tempfile.TemporaryDirectory(dir=check.ROOT / ".work/tmp") as directory:
            root = Path(directory)
            (root / "pm").mkdir()
            (root / "pm/generation.xsh").write_text('print "generation"\n')
            subprocess.run(["git", "init", "--quiet", str(root)], check=True)
            subprocess.run(["git", "-C", str(root), "add", "pm/generation.xsh"], check=True)
            subprocess.run(["git", "-C", str(root), "-c", "user.name=G1 fixture", "-c", "user.email=g1-fixture@example.invalid", "commit", "--quiet", "-m", "test(util-linux): freeze Laputa corpus fixture"], check=True)
            for mode in ("resources", "close"):
                with self.subTest(mode=mode):
                    observed = []

                    def dispatch(runtime):
                        self.assertIs(runtime, check)
                        observed.append(resources._selected_laputa_corpus())

                    with patch.object(sys, "argv", ["check.py", mode, "--check", "--laputa-corpus", str(root), "--baseline-products", str(root)]), patch.object(check, "campaign_leases", return_value=nullcontext()), patch.object(check, "baseline_scope", return_value=nullcontext()), patch.object(acceptance, mode, side_effect=dispatch):
                        self.assertEqual(check.main(), 0)
                    self.assertEqual(observed, [root])

    def test_harness_and_parity_remain_corpus_independent(self):
        with patch.object(sys, "argv", ["check.py", "harness", "--self-test"]), patch.object(check, "campaign_leases", return_value=nullcontext()), patch.object(check, "harness") as harness:
            self.assertEqual(check.main(), 0)
        harness.assert_called_once_with()
        with patch.object(sys, "argv", ["check.py", "parity", "--command", "lslocks"]), patch.object(check, "campaign_leases", return_value=nullcontext()), patch.object(check, "parity") as parity:
            self.assertEqual(check.main(), 0)
        self.assertEqual(parity.call_args.args[0].command, "lslocks")


class BaselineScopeTests(unittest.TestCase):
    def test_receipt_and_products_are_checked_for_the_scope(self):
        import resources
        with tempfile.TemporaryDirectory(dir=check.ROOT / ".work/tmp") as temporary:
            directory = Path(temporary)
            for name in resources.PRODUCTS:
                path = directory / name
                path.write_bytes(name.encode())
                path.chmod(0o755)
            source = check.source_identity()
            signature = resources._product_signature(directory)
            receipt = {"schema": 1, "commit": source.commit, "tree": source.tree, "image": check.IMAGE,
                       "profile": {"name": "release", "opt_level": 3, "lto": "thin", "codegen_units": 16},
                       "target": resources.TARGET,
                       "products": {row[0]: row[-1].hex() for row in signature}}
            (directory / "receipt.json").write_text(json.dumps(receipt))
            receipt["profile"]["opt_level"] = 2
            (directory / "receipt.json").write_text(json.dumps(receipt))
            with self.assertRaisesRegex(ValueError, "release opt3"):
                with check.baseline_scope(directory):
                    pass
            receipt["profile"]["opt_level"] = 3
            (directory / "receipt.json").write_text(json.dumps(receipt))
            with check.baseline_scope(directory):
                self.assertEqual(check.baseline_products()[0], directory)
                (directory / "xsh").write_bytes(b"changed")
                with self.assertRaisesRegex(ValueError, "changed"):
                    check.baseline_products()
            with self.assertRaisesRegex(ValueError, "hashes differ"):
                with check.baseline_scope(directory):
                    pass
            receipt["tree"] = "wrong"
            (directory / "receipt.json").write_text(json.dumps(receipt))
            with self.assertRaisesRegex(ValueError, "source or image"):
                with check.baseline_scope(directory):
                    pass


class NativeCaseDispatchTests(unittest.TestCase):
    def test_standard_module_case_preserves_remaining_budget_and_exact_identity(self):
        selector = "tests/xsh/stdlib/linux_ipc.xsh::test_linux_ipc_names_reject_nul"
        output = f"running 1 tests\n{selector} ... ok 1ms\ntest result: ok. 1 passed; 0 failed; 0 skipped\n".encode()
        with patch("check.protected_products", return_value=Path("/qualified")), patch("check.container", return_value=output) as launch:
            self.assertEqual(check.native_case(selector, timeout=2.5), output)
        self.assertEqual(launch.call_args.kwargs["timeout"], 2.5)

    def test_failed_native_child_still_verifies_products(self):
        with patch("check.protected_products", return_value=Path("/qualified")) as products, patch("check.container", side_effect=RuntimeError("native failure")):
            with self.assertRaisesRegex(RuntimeError, "native failure"):
                check.native_case("core/tests/test-lslocks.xsh::test_wanted")
        self.assertEqual(products.call_count, 2)

    def test_product_failure_preserves_primary_native_failure(self):
        with patch("check.protected_products", side_effect=[Path("/qualified"), ValueError("changed products")]), patch("check.container", side_effect=RuntimeError("native failure")):
            with self.assertRaisesRegex(RuntimeError, "native failure") as caught:
                check.native_case("core/tests/test-lslocks.xsh::test_wanted")
        self.assertIn("changed products", " ".join(caught.exception.__notes__))

    def test_passing_count_cannot_hide_wrong_exact_selector(self):
        output = b"running 1 tests\ncore/tests/test-other.xsh::test_other ... ok 1ms\ntest result: ok. 1 passed; 0 failed; 0 skipped\n"
        with patch("check.protected_products", return_value=Path("/qualified")), patch("check.container", return_value=output):
            with self.assertRaisesRegex(ValueError, "exactly one passing behavior"):
                check.native_case("core/tests/test-lslocks.xsh::test_wanted")

    def test_source_check_rejects_traversal_before_launch(self):
        with patch("check.container") as launch:
            for paths in ([], ["core/../Cargo.toml.xsh"], ["/core/lsipc.xsh"]):
                with self.assertRaises(ValueError):
                    check.native_source(paths)
        launch.assert_not_called()

    def test_unbounded_or_traversing_dispatch_does_not_launch(self):
        with patch("check.container") as launch:
            for timeout in (0, -1, 31, float("nan")):
                with self.assertRaises(ValueError):
                    check.native_case("core/tests/test-lslocks.xsh::test_wanted", timeout=timeout)
            with self.assertRaises(ValueError):
                check.native_case("tests/xsh/../../core/lslocks.xsh::test_wanted")
        launch.assert_not_called()


class PrivateBatchDispatchTests(unittest.TestCase):
    def setUp(self):
        import acceptance
        self.source = acceptance.SourceIdentity("commit", "tree", True)

    def test_private_batch_binds_declared_cohort_and_rechecks_source_and_products(self):
        selector = "core/tests/test-lsmem.xsh::test_lsmem_util_linux_2_42_4_version_identity"
        with patch("check.source_identity", return_value=self.source) as source, patch("check.protected_products", return_value=Path("/qualified")) as products, patch("batches.run_private_exact_batch", return_value=()) as dispatch:
            self.assertEqual(check.native_private_batch((selector,)), ())
        arguments = dispatch.call_args.kwargs
        self.assertIn(selector, arguments["known_selectors"])
        self.assertEqual(arguments["private_namespace"], check.PRIVATE_NAMESPACE)
        self.assertTrue(all(name.startswith("core/tests/test-") for name in arguments["known_selectors"]))
        self.assertEqual(source.call_count, 2)
        self.assertEqual(products.call_count, 2)

    def test_failed_private_batch_still_checks_product_and_source(self):
        with patch("check.source_identity", return_value=self.source) as source, patch("check.protected_products", return_value=Path("/qualified")) as products, patch("batches.run_private_exact_batch", side_effect=RuntimeError("batch failure")):
            with self.assertRaisesRegex(RuntimeError, "batch failure"):
                check.native_private_batch(("core/tests/test-lsmem.xsh::test_wanted",))
        self.assertEqual(source.call_count, 2)
        self.assertEqual(products.call_count, 2)

    def test_dirty_or_changed_private_source_fails(self):
        import acceptance
        dirty = acceptance.SourceIdentity("commit", "tree", False)
        changed = acceptance.SourceIdentity("other", "tree", True)
        with patch("check.source_identity", return_value=dirty), patch("batches.run_private_exact_batch") as dispatch:
            with self.assertRaisesRegex(ValueError, "clean frozen source"):
                check.native_private_batch(())
        dispatch.assert_not_called()
        with patch("check.source_identity", side_effect=[self.source, changed]), patch("check.protected_products", return_value=Path("/qualified")), patch("batches.run_private_exact_batch", return_value=()):
            with self.assertRaisesRegex(ValueError, "source changed"):
                check.native_private_batch(())


class ReleaseProviderDispatchTests(unittest.TestCase):
    def test_resource_products_require_an_active_release_scope(self):
        with patch("release.active_products", return_value=None) as active, patch("check.source_identity") as source, patch("check.protected_products") as protected:
            with self.assertRaisesRegex(ValueError, "active release scope"):
                check.candidate_products()
        active.assert_called_once_with(check)
        source.assert_not_called()
        protected.assert_not_called()

    def test_native_case_uses_active_release_products_and_rechecks_them(self):
        import acceptance
        source = acceptance.SourceIdentity("commit", "tree", True)
        context = acceptance.BuildContext("commit:tree", "release", "target", "image", "mimalloc", "1cpu/1GiB")
        selector = "core/tests/test-lslocks.xsh::test_wanted"
        output = f"running 1 tests\n{selector} ... ok 1ms\ntest result: ok. 1 passed; 0 failed; 0 skipped\n".encode()
        with patch("release.active_products", return_value=(Path("/final-release"), context)) as active, patch("check.protected_products") as verification, patch("check.container", return_value=output) as launch:
            self.assertEqual(check.native_case(selector), output)
        verification.assert_not_called()
        self.assertEqual(active.call_count, 2)
        self.assertIn("source=/final-release,target=/products,readonly", launch.call_args.kwargs["mounts"][1])

    def test_each_resource_entry_uses_the_actual_release_producer(self):
        import acceptance
        source = acceptance.SourceIdentity("commit", "tree", True)
        calls = []
        def scoped(runtime, observed_source, operation):
            self.assertIs(runtime, check)
            self.assertEqual(observed_source, source)
            calls.append(observed_source)
            return operation()
        with patch("check.source_identity", return_value=source), patch("release.with_release_products", side_effect=scoped), patch("resources.measure", return_value="resources"), patch("resources.measure_timing", return_value="timing"), patch("resources.close", return_value="close"), patch("check.require_cargo_lease"):
            self.assertEqual(check.measure_resources(), "resources")
            self.assertEqual(check.measure_timing(source), "timing")
            self.assertEqual(check.close_gates(), "close")
        self.assertEqual(len(calls), 3)


class IndependentReviewTests(unittest.TestCase):
    def setUp(self):
        import acceptance
        self.source = acceptance.SourceIdentity("commit", "tree", True)
        self.result = acceptance.GateResult("adversarial-review", self.source, 0, 1, 1, 0, (), (), ("independent-reviewer",), ())

    def test_absent_review_fails_without_fabricating_a_result(self):
        with self.assertRaisesRegex(ValueError, "for this invocation"):
            check.adversarial_review(self.source)

    def test_actual_review_is_consumed_once_and_does_not_escape_scope(self):
        with patch("check.source_identity", return_value=self.source):
            with check.independent_review(self.result):
                self.assertIs(check.adversarial_review(self.source), self.result)
                with self.assertRaisesRegex(ValueError, "for this invocation"):
                    check.adversarial_review(self.source)
        with self.assertRaisesRegex(ValueError, "for this invocation"):
            check.adversarial_review(self.source)

    def test_nested_scopes_cannot_revive_a_consumed_review(self):
        with patch("check.source_identity", return_value=self.source):
            with check.independent_review(self.result):
                with self.assertRaisesRegex(ValueError, "cannot be nested"):
                    with check.independent_review(self.result):
                        self.fail("nested review admitted")
                self.assertIs(check.adversarial_review(self.source), self.result)
                with self.assertRaisesRegex(ValueError, "cannot be nested"):
                    with check.independent_review(self.result):
                        self.fail("consumed review revived")
                with self.assertRaisesRegex(ValueError, "for this invocation"):
                    check.adversarial_review(self.source)

    def test_stale_review_is_rejected_before_and_during_use(self):
        import acceptance
        stale = acceptance.SourceIdentity("different", "tree", True)
        with patch("check.source_identity", return_value=stale):
            with self.assertRaisesRegex(ValueError, "current clean source"):
                with check.independent_review(self.result):
                    self.fail("stale review admitted")
        with patch("check.source_identity", side_effect=[self.source, stale]):
            with check.independent_review(self.result):
                with self.assertRaisesRegex(ValueError, "current clean source"):
                    check.adversarial_review(self.source)


class OwnedCleanupTests(unittest.TestCase):
    def setUp(self):
        import acceptance
        self.source = acceptance.SourceIdentity("commit", "tree", True)
        self.directory = tempfile.TemporaryDirectory(dir=check.ROOT / ".work/tmp")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.worktree = self.root / "util-linux-cleanup-fixture"
        self.worktree.mkdir()
        self.infra = self.root / ".work/consolidation/infra"
        self.infra.mkdir(parents=True)
        for name in ("final-performance.lock", "native-suite.lock", "cargo.lock"):
            (self.infra / name).write_bytes(b"")
        root_patch = patch("check.ROOT", new=self.worktree)
        root_patch.start()
        self.addCleanup(root_patch.stop)
        for target, value in (
            ("check.require_native_lease", None),
            ("check.source_identity", self.source),
            ("check.original_root", self.root),
        ):
            handle = patch(target, return_value=value)
            handle.start()
            self.addCleanup(handle.stop)
        self.results = [b"", "999999 ps ps -eo pid=,comm=,args=\n", b"worktree " + bytes(check.ROOT) + b"\0\0", b""]

    def test_success_requires_observed_empty_ownership_and_preserved_work(self):
        with patch("check.subprocess.check_output", side_effect=self.results):
            result = check.owned_cleanup(self.source)
        self.assertEqual(result.executed, 3)
        self.assertEqual(set(result.selectors), {"remaining-owned-processes:0", "unresolved-markers:0", "lost-work:false"})

    def test_owned_container_prevents_cleanup_acceptance(self):
        with patch("check.subprocess.check_output", return_value=b"xsh-util-linux-owned\n"):
            with self.assertRaisesRegex(ValueError, "owned containers remain"):
                check.owned_cleanup(self.source)

    def test_unrelated_container_name_is_not_owned(self):
        self.results[0] = b"other-xsh-util-linux-container\n"
        with patch("check.subprocess.check_output", side_effect=self.results):
            result = check.owned_cleanup(self.source)
        self.assertEqual(result.failed, 0)

    def test_dangling_quarantine_marker_prevents_cleanup_acceptance(self):
        (self.infra / "native-suite.cleanup-unresolved").symlink_to(self.root / "missing")
        with patch("check.subprocess.check_output", side_effect=self.results):
            with self.assertRaisesRegex(ValueError, "unresolved cleanup"):
                check.owned_cleanup(self.source)

    def test_missing_worktree_observation_cannot_mean_no_lost_work(self):
        self.results[2] = b""
        with patch("check.subprocess.check_output", side_effect=self.results):
            with self.assertRaisesRegex(ValueError, "omitted the current source"):
                check.owned_cleanup(self.source)

    def test_uncommitted_work_prevents_cleanup_acceptance(self):
        self.results[3] = b" M core/lsipc.xsh\n"
        with patch("check.subprocess.check_output", side_effect=self.results):
            with self.assertRaisesRegex(ValueError, "uncommitted campaign work"):
                check.owned_cleanup(self.source)


class RuntimeProductTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(dir=check.ROOT / ".work/tmp")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name) / "checkout"
        (self.root / "src").mkdir(parents=True)
        (self.root / "core").mkdir()
        self.git("init", "-q")
        (self.root / "src/lib.rs").write_text("fn a() {}\n")
        self.rust_source = self.commit("rust")
        (self.root / "core/tool.xsh").write_text("print 1\n")
        self.commit("core")
        self.built = Path(self.directory.name) / "built"
        self.built.mkdir()
        for name in check.RUNTIME_PRODUCTS:
            (self.built / name).write_bytes(f"{name} product\n".encode())
            (self.built / name).chmod(0o755)
        root_patch = patch("check.ROOT", new=self.root)
        root_patch.start()
        self.addCleanup(root_patch.stop)
        self.destination = self.root / ".work/protected-products" / f"util-linux-runtime-{self.rust_source}"

    def git(self, *arguments):
        return subprocess.run(["git", "-C", str(self.root), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", *arguments],
                              check=True, capture_output=True, text=True).stdout.strip()

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", message)
        return self.git("rev-parse", "HEAD")

    def test_qualified_products_bind_the_last_rust_input_commit_and_their_hashes(self):
        self.assertEqual(check.runtime_source(), self.rust_source)
        self.assertEqual(check.qualify_runtime(self.built), self.destination)
        receipt = json.loads((self.destination / "receipt.json").read_text())
        self.assertEqual(receipt["source"], self.rust_source)
        self.assertEqual(set(receipt["products"]), set(check.RUNTIME_PRODUCTS))
        self.assertEqual(self.destination.stat().st_mode & 0o777, 0o755)
        self.assertEqual(check.protected_products(), self.destination)
        self.assertEqual(check.qualify_runtime(self.built), self.destination)

    def test_changed_or_untracked_rust_inputs_refuse_qualified_products(self):
        check.qualify_runtime(self.built)
        (self.root / "src/lib.rs").write_text("fn b() {}\n")
        with self.assertRaisesRegex(ValueError, "changed Rust inputs"):
            check.protected_products()
        with self.assertRaisesRegex(ValueError, "changed Rust inputs"):
            check.qualify_runtime(self.built)
        self.git("checkout", "--", "src/lib.rs")
        (self.root / "src/new.rs").write_text("fn c() {}\n")
        with self.assertRaisesRegex(ValueError, "changed Rust inputs"):
            check.protected_products()

    def test_a_new_rust_commit_requires_new_products(self):
        check.qualify_runtime(self.built)
        (self.root / "src/lib.rs").write_text("fn b() {}\n")
        self.commit("rust again")
        with self.assertRaisesRegex(ValueError, "no qualified runtime products"):
            check.protected_products()

    def test_changed_product_or_receipt_cannot_qualify(self):
        check.qualify_runtime(self.built)
        receipt_path = self.destination / "receipt.json"
        receipt = json.loads(receipt_path.read_text())
        (self.destination / "xsh").write_bytes(b"changed\n")
        with self.assertRaisesRegex(ValueError, "qualified product changed: xsh"):
            check.protected_products()
        for changed in ({**receipt, "source": "0" * 40}, {**receipt, "products": {"xsh": receipt["products"]["xsh"]}}, {**receipt, "schema": True}):
            receipt_path.write_text(json.dumps(changed))
            with self.assertRaisesRegex(ValueError, "does not qualify"):
                check.protected_products()
        receipt_path.unlink()
        with self.assertRaisesRegex(ValueError, "no qualified runtime products"):
            check.protected_products()

    def test_qualification_refuses_unsafe_products_and_replacement(self):
        (self.built / "xsht").unlink()
        (self.built / "xsht").symlink_to(self.built / "xsh")
        with self.assertRaisesRegex(ValueError, "regular executable"):
            check.qualify_runtime(self.built)
        self.assertFalse(self.destination.exists())
        (self.built / "xsht").unlink()
        (self.built / "xsht").write_bytes(b"xsht product\n")
        (self.built / "xsht").chmod(0o755)
        check.qualify_runtime(self.built)
        (self.built / "xsh").write_bytes(b"rebuilt\n")
        with self.assertRaisesRegex(ValueError, "already qualifies other products"):
            check.qualify_runtime(self.built)

    def test_an_existing_product_directory_is_qualified_in_place(self):
        self.destination.mkdir(parents=True)
        for name in check.RUNTIME_PRODUCTS:
            (self.destination / name).write_bytes((self.built / name).read_bytes())
            (self.destination / name).chmod(0o755)
        with self.assertRaisesRegex(ValueError, "exists without a receipt"):
            check.qualify_runtime(self.built)
        self.assertEqual(check.qualify_runtime(self.destination), self.destination)
        self.assertEqual(check.protected_products(), self.destination)


if __name__ == "__main__":
    unittest.main()
