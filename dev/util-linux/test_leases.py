import fcntl
import multiprocessing
import os
from pathlib import Path
import tempfile
import unittest

from leases import _cargo_held, require_cargo_lease, acquire, LeaseBusy, _native_descriptor, mark_cleanup_unresolved, original_root


def hold(path, mode, channel):
    with acquire([(Path(path), mode, None)]):
        channel.send("held")
        channel.recv()
    channel.send("released")


class LeaseTests(unittest.TestCase):
    def setUp(self):
        scratch = Path(__file__).resolve().parents[2] / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "lease"
        self.path.write_bytes(b"preserve")

    def assert_exclusion(self, held, rejected):
        parent, child = multiprocessing.Pipe()
        worker = multiprocessing.get_context("spawn").Process(target=hold, args=(str(self.path), held, child))
        worker.start()
        try:
            self.assertTrue(parent.poll(5), "holder did not acquire lease")
            self.assertEqual(parent.recv(), "held")
            with self.assertRaises(LeaseBusy):
                with acquire([(self.path, rejected, None)]):
                    self.fail("contending worker entered")
            parent.send("release")
            self.assertTrue(parent.poll(5), "holder did not release lease")
            self.assertEqual(parent.recv(), "released")
            worker.join(5)
            self.assertEqual(worker.exitcode, 0)
            with acquire([(self.path, rejected, None)]):
                self.assertEqual(self.path.read_bytes(), b"preserve")
        finally:
            if worker.is_alive():
                worker.kill()
                worker.join(5)
            parent.close()
            child.close()

    def test_ordinary_worker_cannot_enter_exclusive_measurement(self):
        self.assert_exclusion(fcntl.LOCK_EX, fcntl.LOCK_SH)

    def test_measurement_cannot_enter_ordinary_worker(self):
        self.assert_exclusion(fcntl.LOCK_SH, fcntl.LOCK_EX)

    def test_native_and_cargo_workers_serialize(self):
        self.assert_exclusion(fcntl.LOCK_EX, fcntl.LOCK_EX)

    def test_partial_acquisition_releases_earlier_lease(self):
        missing = self.path.with_name("missing")
        with self.assertRaises(FileNotFoundError):
            with acquire([(self.path, fcntl.LOCK_EX, None), (missing, fcntl.LOCK_EX, None)]):
                self.fail("missing lease accepted")
        with acquire([(self.path, fcntl.LOCK_EX, None)]):
            pass

    def test_symlink_and_replaced_identity_are_rejected(self):
        link = self.path.with_name("link")
        link.symlink_to(self.path)
        with self.assertRaises(OSError):
            with acquire([(link, fcntl.LOCK_SH, None)]):
                self.fail("symlink accepted")
        with self.assertRaises(ValueError):
            with acquire([(self.path, fcntl.LOCK_SH, (0, 0))]):
                self.fail("wrong identity accepted")

    def test_unresolved_cleanup_prevents_dispatch(self):
        self.path.write_bytes(b"cleanup-unresolved: owned-container")
        with self.assertRaises(ValueError):
            with acquire([(self.path, fcntl.LOCK_SH, None)]):
                self.fail("unresolved cleanup accepted")

    def test_quarantine_preserves_lease_inode_and_blocks_next_worker(self):
        identity = self.path.stat().st_ino
        with acquire([(self.path, fcntl.LOCK_EX, None)]) as descriptors:
            token = _native_descriptor.set((descriptors[0], self.path))
            try:
                mark_cleanup_unresolved("container=owned")
            finally:
                _native_descriptor.reset(token)
        self.assertEqual(self.path.stat().st_ino, identity)
        self.assertTrue(self.path.read_bytes().startswith(b"cleanup-unresolved: util-linux container=owned"))
        self.assertEqual(self.path.with_suffix(".cleanup-unresolved").read_bytes(), self.path.read_bytes())
        with self.assertRaises(ValueError):
            with acquire([(self.path, fcntl.LOCK_SH, None)]):
                self.fail("quarantined lease accepted")

    def test_build_dispatch_requires_held_cargo_guard(self):
        with self.assertRaisesRegex(RuntimeError, "original cargo lease"):
            require_cargo_lease()
        token = _cargo_held.set(True)
        try:
            require_cargo_lease()
        finally:
            _cargo_held.reset(token)
        with self.assertRaises(RuntimeError):
            require_cargo_lease()

    def test_worktree_resolves_original_repository(self):
        root = Path(__file__).resolve().parents[2]
        self.assertEqual(original_root(root), Path("/home/josh/d/laputa-systems/xsh"))

    def test_dangling_cleanup_marker_prevents_dispatch(self):
        self.path.with_suffix(".cleanup-unresolved").symlink_to(self.path.with_name("missing"))
        with self.assertRaises(ValueError):
            with acquire([(self.path, fcntl.LOCK_SH, None)]):
                self.fail("dangling cleanup marker accepted")


if __name__ == "__main__":
    unittest.main()
