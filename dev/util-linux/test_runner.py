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

    def test_passing_count_cannot_hide_wrong_exact_selector(self):
        output = b"running 1 tests\ncore/tests/test-other.xsh::test_other ... ok 1ms\ntest result: ok. 1 passed; 0 failed; 0 skipped\n"
        with patch("check.protected_products", return_value=Path("/qualified")), patch("check.container", return_value=output):
            with self.assertRaisesRegex(ValueError, "exactly one passing behavior"):
                check.native_case("core/tests/test-lslocks.xsh::test_wanted")

    def test_unbounded_or_traversing_dispatch_does_not_launch(self):
        with patch("check.container") as launch:
            for timeout in (0, -1, 31, float("nan")):
                with self.assertRaises(ValueError):
                    check.native_case("core/tests/test-lslocks.xsh::test_wanted", timeout=timeout)
            with self.assertRaises(ValueError):
                check.native_case("tests/xsh/../../core/lslocks.xsh::test_wanted")
        launch.assert_not_called()


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
