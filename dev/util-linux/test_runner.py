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


if __name__ == "__main__":
    unittest.main()
