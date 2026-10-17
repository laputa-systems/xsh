from __future__ import annotations

import importlib.util
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]


def load_script(name: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / "formal/scripts" / f"{name}.py")
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {name}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CHECK = load_script("check-fresh")


class ParanoidCheckTests(unittest.TestCase):
    def test_failed_build_stops_check_and_removes_its_scratch(self):
        workspaces = []

        def fail_build(command, cwd, env, label):
            workspaces.append(cwd)
            scratch = Path(env["TMPDIR"]) / "interrupted-export"
            self.assertTrue(scratch.is_relative_to(cwd))
            scratch.write_text("incomplete export")
            return 17

        with patch.object(CHECK, "run_guarded", side_effect=fail_build) as run:
            with patch.object(CHECK.shutil, "which", return_value="/usr/bin/false"):
                self.assertEqual(CHECK.main(), 17)
        self.assertEqual(run.call_count, 1)
        self.assertEqual(len(workspaces), 1)
        self.assertFalse(workspaces[0].exists())

    def test_workspace_keeps_proof_sources_and_targets_inside_sandbox(self):
        formal = ROOT / "formal"
        with tempfile.TemporaryDirectory(dir=ROOT / ".work/tmp") as directory:
            workspace = Path(directory)
            CHECK.prepare_workspace(formal, workspace)
            original = tomllib.loads((formal / "lakefile.toml").read_text())
            staged = tomllib.loads((workspace / "lakefile.toml").read_text())
            for field in ("name", "defaultTargets", "moreLeanArgs", "lean_lib"):
                self.assertEqual(original[field], staged[field])
            for field in ("srcDir", "buildDir", "packagesDir"):
                self.assertTrue((workspace / staged[field]).resolve().is_relative_to(workspace))
            expected = {
                source.relative_to(formal): source.read_bytes()
                for source in formal.rglob("*.lean")
                if source.relative_to(formal).parts[0] != "scripts"
            }
            actual = {
                source.relative_to(workspace / staged["srcDir"]): source.read_bytes()
                for source in (workspace / staged["srcDir"]).rglob("*.lean")
            }
            self.assertEqual(expected, actual)
            self.assertEqual((formal / "lean-toolchain").read_bytes(),
                             (workspace / "lean-toolchain").read_bytes())
            self.assertEqual((formal / "scripts/check-sandbox.py").read_bytes(),
                             (workspace / "check-sandbox.py").read_bytes())
            self.assertFalse((workspace / "check-sandbox.py").samefile(
                formal / "scripts/check-sandbox.py"))

    def test_real_sandbox_preserves_read_only_sources_and_hides_home(self):
        bwrap = shutil.which("bwrap")
        if bwrap is None:
            self.skipTest("bubblewrap is unavailable")
        with tempfile.TemporaryDirectory(dir=ROOT / ".work/tmp") as directory:
            workspace = Path(directory)
            source = workspace / "source"
            source.write_text("proof")
            probe = (
                "import os; from pathlib import Path; "
                f"p=Path({str(source)!r}); assert p.read_text() == 'proof'; "
                f"assert not Path({str(ROOT / 'AGENTS.md')!r}).exists(); "
                "assert Path('/run/user').is_dir(); "
                "assert os.environ['LEAN_NUM_THREADS'] == '3'; "
                "assert os.environ['LEAN_STACK_SIZE_KB'] == '32768'; "
                "assert not os.access(p, os.W_OK)"
            )
            args = [
                "--ro-bind", "/", "/", "--tmpfs", "/home", "--tmpfs", "/root",
                "--tmpfs", "/run/user", "--tmpfs", "/tmp", "--dev", "/dev",
                "--proc", "/proc", "--clearenv", "--ro-bind", str(workspace), str(workspace),
                "--unshare-all", "--die-with-parent", "--chdir", str(workspace),
                "--", sys.executable, "-c", probe,
            ]
            command = [sys.executable, str(ROOT / "formal/scripts/check-sandbox.py"),
                       bwrap, "3", "32768", *args]
            result = subprocess.run(command,
                                    capture_output=True, text=True, timeout=10, env={})
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
