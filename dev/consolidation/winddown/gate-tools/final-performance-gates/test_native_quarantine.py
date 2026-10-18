from argparse import Namespace
from contextlib import nullcontext
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent / "native-scheduler-comparison"))


def load(name, path):
    spec = spec_from_file_location(name, path)
    module = module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


profile = load("quarantine_profile", HERE / "profile-cost.py")
forced = load("quarantine_forced", HERE / "forced-frames.py")
observer = load("quarantine_observer", HERE / "final-observers.py")
canonical = load("quarantine_canonical", ROOT / "dev/consolidation/final_canonical_replay.py")
scheduler = load("quarantine_scheduler", HERE.parent / "native-scheduler-comparison/compare.py")
comparative = load("quarantine_comparative", HERE / "final-comparative.py")


class WorkEntered(Exception):
    pass


class NativeQuarantineTests(unittest.TestCase):
    def setUp(self):
        scratch = ROOT / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.native = self.root / "native-suite.lock"
        self.native.write_text("")
        self.performance = self.root / "final-performance.lock"
        self.performance.write_text("")
        self.marker = self.native.with_suffix(".cleanup-unresolved")

    def read(self, reader):
        if reader == "profile":
            with profile.exclusive_lock(self.native):
                pass
        elif reader == "forced":
            with patch.multiple(forced, ROOT=self.root, NATIVE_LOCK=self.native,
                                NATIVE_CLEANUP_MARKER=self.marker):
                with forced.native_suite_lock():
                    pass
        elif reader == "observer":
            with patch.multiple(observer, NATIVE_SUITE_LOCK=self.native,
                                NATIVE_CLEANUP_MARKER=self.marker):
                observer.ensure_native_marker_clear()
        elif reader == "scheduler":
            scheduler.acquire_lock(self.native).close()
        elif reader == "comparative":
            comparative.acquire_lock(self.native).close()
        elif reader == "profile-artifacts":
            laputa = self.root / ".work/laputa-integration"
            laputa.mkdir(parents=True, exist_ok=True)
            args = Namespace(tree=self.root, output=self.root / ".work/consolidation/output",
                             commit="a" * 40, functional_close=self.root / "close.json",
                             laputa_corpus=laputa, artifacts_only=True)
            with patch.multiple(profile, ROOT=self.root, INFRA=self.root,
                                PROFILE_LOCK=self.performance), \
                 patch.object(profile, "source_tree_path", return_value=self.root), \
                 patch.object(profile, "check_source", return_value={}), \
                 patch.object(profile, "laputa_corpus_source", return_value=(laputa, {"status": "clean"})), \
                 patch.object(profile, "check_close", return_value={}), \
                 patch.object(profile, "image_identity", return_value={"image_id": "test"}), \
                 patch.object(profile, "repository_tmp_dir", return_value=nullcontext(self.root)), \
                 patch.object(profile, "no_active_native_containers", side_effect=WorkEntered):
                profile.run(args)
        else:
            validated = {"root": self.root, "output_root": self.root / "output",
                         "args": object(), "lock_path": self.native,
                         "target_mount": self.root / "target"}
            with patch.object(canonical, "source_check", side_effect=WorkEntered):
                canonical.run_batch(validated)

    def test_clear_lease_reaches_work_without_changing_identity(self):
        identity = self.native.stat().st_ino
        for reader in ("profile", "profile-artifacts", "forced", "observer", "scheduler", "comparative", "canonical"):
            with self.subTest(reader=reader):
                if reader in ("canonical", "profile-artifacts"):
                    with self.assertRaises(WorkEntered):
                        self.read(reader)
                else:
                    self.read(reader)
                self.assertEqual(self.native.stat().st_ino, identity)
                self.assertEqual(self.native.read_text(), "")

    def test_every_reader_rejects_quarantine_before_starting_work(self):
        for kind in ("prefix", "marker", "dangling-marker"):
            if kind == "prefix":
                self.native.write_text("cleanup-unresolved:owned-container\n")
            elif kind == "marker":
                self.native.write_text("")
                self.marker.write_text("cleanup-unresolved:owned-container\n")
            else:
                self.marker.unlink()
                self.marker.symlink_to(self.root / "missing")
            identity = self.native.stat().st_ino
            contents = self.native.read_bytes()
            for reader in ("profile", "profile-artifacts", "forced", "observer", "scheduler", "comparative", "canonical"):
                with self.subTest(kind=kind, reader=reader):
                    with self.assertRaisesRegex(Exception, "cleanup"):
                        self.read(reader)
                    self.assertEqual(self.native.stat().st_ino, identity)
                    self.assertEqual(self.native.read_bytes(), contents)


if __name__ == "__main__":
    unittest.main()
