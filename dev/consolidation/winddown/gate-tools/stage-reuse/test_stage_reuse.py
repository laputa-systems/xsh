from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
SCRATCH = HERE.parents[4] / ".work/tmp"
sys.path.insert(0, str(HERE))
import stage_reuse


class StageReuseTests(unittest.TestCase):
    def setUp(self):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=SCRATCH)
        self.addCleanup(self.temporary.cleanup)
        self.repo = Path(self.temporary.name)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "gate@example.invalid")
        self.git("config", "user.name", "gate")
        self.base = self.commit({"TODO.md": "a\n", "src/lib.rs": "fn a() {}\n"})

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), *args], check=True,
                              capture_output=True, text=True).stdout.strip()

    def commit(self, files, removed=()):
        for name, text in files.items():
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        for name in removed:
            self.git("rm", "-q", name)
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "change")
        return self.git("rev-parse", "HEAD")

    def test_a_planning_record_only_diff_carries_every_stage_forward(self):
        target = self.commit({"TODO.md": "b\n", "docs/ROADMAP.md": "r\n"})
        record = stage_reuse.acceptance_record(self.repo, target, {stage: self.base for stage in stage_reuse.STAGES})
        self.assertTrue(record["accepted"])
        replay = next(row for row in record["stages"] if row["stage"] == "replay")
        self.assertEqual(replay["receipt_commit"], self.base)
        self.assertEqual(replay["reuse"], "carried-forward")
        self.assertEqual(replay["diff_check"]["changed_paths"], ["TODO.md", "docs/ROADMAP.md"])
        self.assertEqual(replay["diff_check"]["touched_inputs"], [])

    def test_a_diff_touching_any_input_invalidates_reuse(self):
        for name in ("src/lib.rs", "Cargo.lock", "Dockerfile.test", "core/tests/test-pr.xsh",
                     "dev/consolidation/winddown/gate-tools/native-scheduler-comparison/compare.py",
                     "docs/SPEC.md", "dev/consolidation/evidence/run/README.md"):
            with self.subTest(name):
                target = self.commit({name: f"{name} changed\n"})
                row = stage_reuse.stage_reuse(self.repo, "scheduler", self.base, target)
                self.assertFalse(row["reusable"])
                self.assertIn(name, row["diff_check"]["touched_inputs"])

    def test_moving_a_planning_record_into_an_input_path_invalidates_reuse(self):
        self.git("mv", "TODO.md", "src/TODO.md")
        self.git("commit", "-q", "-m", "move")
        row = stage_reuse.stage_reuse(self.repo, "replay", self.base, self.git("rev-parse", "HEAD"))
        self.assertEqual(row["diff_check"]["touched_inputs"], ["src/TODO.md"])
        self.assertFalse(row["reusable"])

    def test_the_receipt_commit_must_precede_the_target(self):
        later = self.commit({"TODO.md": "b\n"})
        with self.assertRaises(stage_reuse.ReuseError):
            stage_reuse.stage_reuse(self.repo, "replay", later, self.base)

    def test_one_rejected_stage_rejects_the_acceptance_record(self):
        middle = self.commit({"src/lib.rs": "fn b() {}\n"})
        target = self.commit({"TODO.md": "c\n"})
        commits = {stage: middle for stage in stage_reuse.STAGES}
        commits["comparative"] = self.base
        record = stage_reuse.acceptance_record(self.repo, target, commits)
        self.assertFalse(record["accepted"])
        rejected = [row["stage"] for row in record["stages"] if not row["reusable"]]
        self.assertEqual(rejected, ["comparative"])

    def test_every_stage_needs_a_receipt_commit(self):
        with self.assertRaises(stage_reuse.ReuseError):
            stage_reuse.acceptance_record(self.repo, self.base, {"replay": self.base})


if __name__ == "__main__":
    unittest.main()
