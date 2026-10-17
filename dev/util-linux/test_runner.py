from pathlib import Path
import signal
import subprocess
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

    def test_cleanup_failure_cannot_qualify_success(self):
        with patch("check.subprocess.run", side_effect=[
            subprocess.CompletedProcess([], 0, b"result", b""),
            subprocess.CompletedProcess([], 1, b"", b"daemon failed"),
        ]):
            with self.assertRaisesRegex(RuntimeError, "cleanup failed"):
                check.container("true")
        self.stack[1].assert_called_once()
        self.assertTrue(self.scratch.exists())

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
        self.infra = self.root / ".work/consolidation/infra"
        self.infra.mkdir(parents=True)
        for name in ("final-performance.lock", "native-suite.lock", "cargo.lock"):
            (self.infra / name).write_bytes(b"")
        for target, value in (("check.require_native_lease", None), ("check.source_identity", self.source), ("check.original_root", self.root)):
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


if __name__ == "__main__":
    unittest.main()
