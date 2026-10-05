import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]


class CompatibilityLaneBriefTests(unittest.TestCase):
    def test_brief_uses_checkout_paths_and_revision_from_another_directory(self):
        env = os.environ.copy()
        for name in ("LANES_ROOT", "LANES_TARGETS", "XSH_SHARED_BIN"):
            env.pop(name, None)
        revision = subprocess.check_output(
            ["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"], text=True,
        ).strip()
        with tempfile.TemporaryDirectory() as scratch:
            result = subprocess.run(
                [sys.executable, str(REPO / "dev/compat/lanes.py"), "brief", "cp", "native-fs"],
                cwd=scratch, env=env, capture_output=True, text=True, check=True,
            )
        self.assertEqual(result.stderr, "")
        self.assertIn("Agent: gpt-6.1-sol (medium reasoning effort)", result.stdout)
        self.assertIn(f"Worktree: {REPO}-lanes/cp", result.stdout)
        self.assertIn(f"from master @ {revision}", result.stdout)
        self.assertIn(f"XSH_BIN={REPO}/target/release/xsh", result.stdout)
        self.assertIn(f"CARGO_TARGET_DIR={REPO.parent}/xsh-lane-targets/native-fs", result.stdout)


if __name__ == "__main__":
    unittest.main()
