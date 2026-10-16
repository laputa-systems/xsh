from __future__ import annotations

import pathlib
import shutil
import subprocess
import tempfile
import unittest

import w9_program_snapshot as program_snapshot


ROOT = pathlib.Path(__file__).resolve().parents[2]


class ProgramSnapshotTests(unittest.TestCase):
    def setUp(self):
        scratch = ROOT / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="w9-program-snapshot-", dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name) / "program"
        (self.root / "dev").mkdir(parents=True)
        (self.root / program_snapshot.PROGRAM_CONFIG).write_text("module_path = .\n  dev\n", encoding="utf-8")
        self.entry = self.root / "dev/main.xsh"
        self.entry.write_bytes(b"print \"pinned\"\n")
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "config", "user.name", "program snapshot test"], check=True)
        subprocess.run(
            ["git", "-C", str(self.root), "config", "user.email", "program-snapshot@example.invalid"],
            check=True,
        )
        subprocess.run(["git", "-C", str(self.root), "add", program_snapshot.PROGRAM_CONFIG, "dev/main.xsh"], check=True)
        subprocess.run(["git", "-C", str(self.root), "commit", "--quiet", "-m", "program"], check=True)
        self.commit = subprocess.run(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"], check=True, text=True, capture_output=True
        ).stdout.strip()
        self.tree = subprocess.run(
            ["git", "-C", str(self.root), "rev-parse", "HEAD^{tree}"], check=True, text=True, capture_output=True
        ).stdout.strip()
        self.snapshot = program_snapshot._validate_program_snapshot(self.root, self.commit, self.tree)

    def test_entry_and_module_roots_stay_in_the_pinned_tree(self):
        self.assertEqual(
            program_snapshot.program_entry_path(self.snapshot, "dev/main.xsh"),
            self.entry,
        )
        self.assertEqual(
            program_snapshot.validate_module_roots(self.snapshot, (self.root, self.root / "dev")),
            (self.root, self.root / "dev"),
        )
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.program_entry_path(self.snapshot, "../outside.xsh")
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.program_entry_path(self.snapshot, "untracked.xsh")
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.validate_module_roots(self.snapshot, (self.root.parent,))
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.validate_module_roots(self.snapshot, ())

    def test_nearest_config_falls_back_and_pins_nested_config(self):
        self.assertEqual(
            program_snapshot.program_config_path(self.snapshot, self.entry),
            self.root / program_snapshot.PROGRAM_CONFIG,
        )
        nested_config = self.root / "dev" / program_snapshot.PROGRAM_CONFIG
        nested_config.write_bytes(b"module_path = .\n")
        subprocess.run(["git", "-C", str(self.root), "add", "dev/xsht-config.ini"], check=True)
        subprocess.run(["git", "-C", str(self.root), "commit", "--quiet", "-m", "nested config"], check=True)
        commit = subprocess.run(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"], check=True, text=True, capture_output=True
        ).stdout.strip()
        tree = subprocess.run(
            ["git", "-C", str(self.root), "rev-parse", "HEAD^{tree}"], check=True, text=True, capture_output=True
        ).stdout.strip()
        nested_snapshot = program_snapshot._validate_program_snapshot(self.root, commit, tree)
        self.assertEqual(
            program_snapshot.program_config_path(nested_snapshot, self.entry),
            nested_config,
        )
        nested_config.write_bytes(b"module_path = changed\n")
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.program_config_path(nested_snapshot, self.entry)
        nested_config.unlink()
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.program_config_path(nested_snapshot, self.entry)
        nested_config.symlink_to(self.root / "dev", target_is_directory=True)
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.program_config_path(nested_snapshot, self.entry)

    def test_untracked_nearer_config_is_rejected(self):
        nested_config = self.root / "dev" / program_snapshot.PROGRAM_CONFIG
        nested_config.write_bytes(b"module_path = .\n")
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.program_config_path(self.snapshot, self.entry)

    def test_rejects_symlink_module_roots_and_symlink_source_paths(self):
        outside = pathlib.Path(self.temporary.name) / "outside"
        outside.mkdir()
        (self.root / "link").symlink_to(outside, target_is_directory=True)
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.validate_module_roots(self.snapshot, (self.root / "link",))
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.validate_loaded_program_sources(
                self.snapshot,
                (program_snapshot.LoadedProgramSource(self.root / "link/main.xsh", 1, "a" * 64),),
            )

    def test_loaded_source_rows_match_pinned_git_bytes_and_length(self):
        source_bytes = self.entry.read_bytes()
        record = program_snapshot.LoadedProgramSource(self.entry, len(source_bytes), "a" * 64)
        program_snapshot.validate_loaded_program_sources(self.snapshot, (record,))
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.validate_loaded_program_sources(
                self.snapshot,
                (program_snapshot.LoadedProgramSource(self.entry, len(source_bytes) + 1, "a" * 64),),
            )
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.validate_loaded_program_sources(
                self.snapshot,
                (program_snapshot.LoadedProgramSource(self.entry, len(source_bytes), "not-a-blake3"),),
            )
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.validate_loaded_program_sources(
                self.snapshot,
                (program_snapshot.LoadedProgramSource(pathlib.Path(self.temporary.name) / "outside.xsh", 0, "a" * 64),),
            )

    def test_source_bytes_changed_after_snapshot_fail_against_git_blob(self):
        self.entry.write_bytes(b"print \"changed\"\n")
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.validate_loaded_program_sources(
                self.snapshot,
                (program_snapshot.LoadedProgramSource(self.entry, len(self.entry.read_bytes()), "a" * 64),),
            )
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.program_entry_path(self.snapshot, "dev/main.xsh")

    def test_snapshot_identity_requires_clean_worktree_and_pinned_config(self):
        dirty = program_snapshot._validate_program_snapshot(self.root, self.commit, self.tree)
        self.assertEqual(dirty.commit, self.commit)
        self.entry.write_bytes(b"changed\n")
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot._validate_program_snapshot(self.root, self.commit, self.tree)
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot.validate_program_snapshot(self.root)

    def test_missing_config_is_rejected(self):
        shutil.rmtree(self.root / "dev")
        (self.root / "xsht-config.ini").unlink()
        subprocess.run(["git", "-C", str(self.root), "add", "-A"], check=True)
        subprocess.run(["git", "-C", str(self.root), "commit", "--quiet", "-m", "remove config"], check=True)
        commit = subprocess.run(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"], check=True, text=True, capture_output=True
        ).stdout.strip()
        tree = subprocess.run(
            ["git", "-C", str(self.root), "rev-parse", "HEAD^{tree}"], check=True, text=True, capture_output=True
        ).stdout.strip()
        with self.assertRaises(program_snapshot.ProgramSnapshotError):
            program_snapshot._validate_program_snapshot(self.root, commit, tree)


if __name__ == "__main__":
    unittest.main()
