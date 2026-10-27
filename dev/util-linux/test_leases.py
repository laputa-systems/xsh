import fcntl
import importlib.util
import multiprocessing
import os
from pathlib import Path
import queue
import signal
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

from leases import _cargo_held, _cargo_lease, require_cargo_lease, acquire, LeaseBusy, _native_descriptor, mark_cleanup_unresolved, original_root, prepare_cleanup_unresolved, resolve_cleanup_unresolved


def hold(path, mode, channel):
    with acquire([(Path(path), mode, None)]):
        channel.send("held")
        channel.recv()
    channel.send("released")


def interrupt_worker(signum, frame):
    raise KeyboardInterrupt


def cargo_worker(path, root, target, channel):
    signal.signal(signal.SIGUSR1, interrupt_worker)
    try:
        with _cargo_lease(Path(root), Path(path), None, Path(target)):
            channel.send("held")
            action = channel.recv()
            if action == "raise":
                raise RuntimeError("worker failure")
            if action == "signal":
                channel.send("signal-ready")
                signal.pause()
        channel.send("released")
    except LeaseBusy:
        channel.send("busy")
    except RuntimeError:
        channel.send("exception")
    except KeyboardInterrupt:
        channel.send("interrupted")


class LeaseTests(unittest.TestCase):
    def setUp(self):
        scratch = Path(__file__).resolve().parents[2] / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "lease"
        self.path.write_bytes(b"preserve")
        self.roots = []
        for name in ("source-a", "source-b", "source-c", "source-d"):
            root = Path(self.directory.name) / name
            (root / ".work/target").mkdir(parents=True)
            self.roots.append(root)

    def start_cargo_worker(self, root_index):
        parent, child = multiprocessing.Pipe()
        worker = multiprocessing.get_context("spawn").Process(
            target=cargo_worker,
            args=(str(self.path), str(self.roots[root_index]), str(self.roots[root_index] / ".work/target"), child),
        )
        worker.start()
        child.close()

        def cleanup():
            if worker.is_alive():
                worker.kill()
            worker.join(5)
            parent.close()

        self.addCleanup(cleanup)
        return worker, parent

    def cargo_identity(self):
        info = self.path.stat()
        return info.st_dev, info.st_ino

    def attempt_cargo(self, root_index, target=True):
        path = self.roots[root_index] / ".work/target" if target else None
        return _cargo_lease(self.roots[root_index], self.path, self.cargo_identity(), path)

    def join_worker(self, worker):
        worker.join(5)
        self.assertFalse(worker.is_alive(), "cargo worker did not exit")
        self.assertEqual(worker.exitcode, 0)

    def receive(self, channel, expected):
        self.assertTrue(channel.poll(5), f"worker did not send {expected}")
        self.assertEqual(channel.recv(), expected)

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

    def test_prepared_quarantine_clears_only_after_explicit_resolution(self):
        identity = self.path.stat().st_ino
        with acquire([(self.path, fcntl.LOCK_EX, None)]) as descriptors:
            token = _native_descriptor.set((descriptors[0], self.path))
            try:
                prepare_cleanup_unresolved("focused container launch")
                sentinel = self.path.read_bytes()
                self.assertTrue(sentinel.startswith(b"cleanup-unresolved:"))
                self.assertEqual(self.path.with_suffix(".cleanup-unresolved").read_bytes(), sentinel)
                self.assertEqual(self.path.stat().st_ino, identity)
                resolve_cleanup_unresolved()
            finally:
                _native_descriptor.reset(token)
        self.assertEqual(self.path.read_bytes(), b"")
        self.assertFalse(self.path.with_suffix(".cleanup-unresolved").exists())
        with acquire([(self.path, fcntl.LOCK_EX, None)]):
            pass

    def test_quarantine_marker_fsync_failure_leaves_dispatch_fence(self):
        with acquire([(self.path, fcntl.LOCK_EX, None)]) as descriptors:
            token = _native_descriptor.set((descriptors[0], self.path))
            try:
                with patch("leases.os.fsync", side_effect=OSError("marker fsync failed")):
                    with self.assertRaisesRegex(OSError, "marker fsync failed"):
                        prepare_cleanup_unresolved("focused container launch")
                self.assertTrue(self.path.with_suffix(".cleanup-unresolved").exists())
            finally:
                _native_descriptor.reset(token)
        with self.assertRaises(ValueError):
            with acquire([(self.path, fcntl.LOCK_SH, None)]):
                self.fail("partially prepared quarantine accepted")

    def test_lease_sentinel_write_failure_leaves_durable_marker_fence(self):
        with acquire([(self.path, fcntl.LOCK_EX, None)]) as descriptors:
            token = _native_descriptor.set((descriptors[0], self.path))
            try:
                with patch("leases.os.pwrite", side_effect=OSError("lease sentinel write failed")):
                    with self.assertRaisesRegex(OSError, "lease sentinel write failed"):
                        prepare_cleanup_unresolved("focused container launch")
                self.assertTrue(self.path.with_suffix(".cleanup-unresolved").exists())
            finally:
                _native_descriptor.reset(token)
        with self.assertRaises(ValueError):
            with acquire([(self.path, fcntl.LOCK_SH, None)]):
                self.fail("marker-only quarantine accepted")

    def test_quarantine_removal_failure_restores_dispatch_fence(self):
        with acquire([(self.path, fcntl.LOCK_EX, None)]) as descriptors:
            token = _native_descriptor.set((descriptors[0], self.path))
            try:
                prepare_cleanup_unresolved("focused cleanup verified")
                marker = self.path.with_suffix(".cleanup-unresolved")
                with patch("leases.os.unlink", side_effect=OSError("marker removal failed")):
                    with self.assertRaisesRegex(OSError, "marker removal failed"):
                        resolve_cleanup_unresolved()
                self.assertTrue(self.path.read_bytes().startswith(b"cleanup-unresolved:"))
                self.assertEqual(marker.read_bytes(), self.path.read_bytes())
            finally:
                _native_descriptor.reset(token)
        with self.assertRaises(ValueError):
            with acquire([(self.path, fcntl.LOCK_SH, None)]):
                self.fail("restored quarantine accepted")

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

    def test_parallel_cargo_admits_three_disjoint_outputs_and_rejects_a_fourth(self):
        workers = []
        for index in range(3):
            worker, channel = self.start_cargo_worker(index)
            self.receive(channel, "held")
            workers.append((worker, channel))
        with self.assertRaises(LeaseBusy):
            with self.attempt_cargo(3):
                self.fail("fourth disjoint output entered")
        first, first_pipe = workers.pop(0)
        first_pipe.send("release")
        self.receive(first_pipe, "released")
        self.join_worker(first)
        with self.attempt_cargo(3):
            pass
        for worker, channel in workers:
            channel.send("release")
            self.receive(channel, "released")
            self.join_worker(worker)

    def test_parallel_cargo_rejects_same_output_without_consuming_free_slots(self):
        first, first_pipe = self.start_cargo_worker(0)
        self.receive(first_pipe, "held")
        with self.assertRaises(LeaseBusy):
            with self.attempt_cargo(0):
                self.fail("same output entered twice")
        others = []
        for index in (1, 2):
            worker, channel = self.start_cargo_worker(index)
            self.receive(channel, "held")
            others.append((worker, channel))
        for worker, channel in [(first, first_pipe), *others]:
            channel.send("release")
            self.receive(channel, "released")
            self.join_worker(worker)

    def test_failed_nested_exclusive_cargo_lease_preserves_shared_output_range(self):
        with self.attempt_cargo(0):
            with self.assertRaises(LeaseBusy):
                with self.attempt_cargo(1, target=False):
                    self.fail("nested exclusive cargo lease entered")
            results = queue.Queue()

            def concurrent_exclusive_attempt():
                try:
                    with self.attempt_cargo(1, target=False):
                        results.put("entered")
                except LeaseBusy:
                    results.put("busy")

            thread = threading.Thread(target=concurrent_exclusive_attempt)
            thread.start()
            thread.join(5)
            self.assertFalse(thread.is_alive(), "same-process Cargo attempt did not return")
            self.assertEqual(results.get_nowait(), "busy")
            probe, probe_pipe = self.start_cargo_worker(0)
            self.receive(probe_pipe, "busy")
            self.join_worker(probe)
        with self.attempt_cargo(0):
            pass

    def test_failed_exclusive_cargo_alias_preserves_shared_output_range(self):
        alias_name = "leases_import_alias"
        spec = importlib.util.spec_from_file_location(alias_name, Path(__file__).with_name("leases.py"))
        alias = importlib.util.module_from_spec(spec)
        sys.modules[alias_name] = alias
        try:
            spec.loader.exec_module(alias)
            with self.attempt_cargo(0):
                with self.assertRaises(alias.LeaseBusy):
                    with alias._cargo_lease(self.roots[1], self.path, self.cargo_identity(), None):
                        self.fail("aliased exclusive Cargo lease entered")
                probe, probe_pipe = self.start_cargo_worker(0)
                self.receive(probe_pipe, "busy")
                self.join_worker(probe)
        finally:
            del sys.modules[alias_name]

    def test_parallel_cargo_excludes_exclusive_final_lease(self):
        workers = []
        for index in range(3):
            worker, channel = self.start_cargo_worker(index)
            self.receive(channel, "held")
            workers.append((worker, channel))
        with self.assertRaises(LeaseBusy):
            with self.attempt_cargo(3, target=False):
                self.fail("exclusive final lease entered during parallel builds")
        for worker, channel in workers:
            channel.send("release")
            self.receive(channel, "released")
            self.join_worker(worker)
        with self.attempt_cargo(3, target=False):
            worker, channel = self.start_cargo_worker(0)
            self.receive(channel, "busy")
            self.join_worker(worker)

    def test_parallel_cargo_releases_ranges_after_worker_exception(self):
        others = []
        for index in (1, 2):
            other, other_pipe = self.start_cargo_worker(index)
            self.receive(other_pipe, "held")
            others.append((other, other_pipe))
        worker, channel = self.start_cargo_worker(0)
        self.receive(channel, "held")
        channel.send("raise")
        self.receive(channel, "exception")
        self.join_worker(worker)
        with self.attempt_cargo(0):
            pass
        for other, other_pipe in others:
            other_pipe.send("release")
            self.receive(other_pipe, "released")
            self.join_worker(other)

    def test_parallel_cargo_releases_ranges_after_worker_signal(self):
        others = []
        for index in (1, 2):
            other, other_pipe = self.start_cargo_worker(index)
            self.receive(other_pipe, "held")
            others.append((other, other_pipe))
        worker, channel = self.start_cargo_worker(0)
        self.receive(channel, "held")
        channel.send("signal")
        self.receive(channel, "signal-ready")
        os.kill(worker.pid, signal.SIGUSR1)
        self.receive(channel, "interrupted")
        self.join_worker(worker)
        with self.attempt_cargo(0):
            pass
        for other, other_pipe in others:
            other_pipe.send("release")
            self.receive(other_pipe, "released")
            self.join_worker(other)

    def test_parallel_cargo_rejects_output_replacement_during_lease(self):
        target = self.roots[0] / ".work/target"
        replacement = target.with_name("target-original")
        with self.assertRaisesRegex(ValueError, "changed during lease"):
            with self.attempt_cargo(0):
                target.rename(replacement)
                target.mkdir()

    def test_parallel_cargo_rejects_symlinked_output(self):
        target = self.roots[0] / ".work/target"
        target.rmdir()
        target.symlink_to(self.roots[1] / ".work/target", target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "source-local"):
            with self.attempt_cargo(0):
                self.fail("symlinked Cargo output entered")


if __name__ == "__main__":
    unittest.main()
