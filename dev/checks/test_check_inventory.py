#!/usr/bin/env python3
"""Focused tests for command-to-native-test inventory discovery."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

CHECKER_PATH = Path(__file__).with_name("check_inventory.py")
SPEC = importlib.util.spec_from_file_location("check_inventory", CHECKER_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load the inventory generator")
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
SCRATCH = CHECKER.REPO / ".work" / "tmp"


class NativeTestDiscoveryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        SCRATCH.mkdir(parents=True, exist_ok=True)

    def test_command_invocation_beats_unrelated_module_mentions(self) -> None:
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            core = Path(directory)
            tests = core / "tests"
            tests.mkdir()
            (tests / "test-df.xsh").write_text(
                "let mounts = fs.mounts()?\n"
                "for mount in mounts {\n"
                "  assert mount.mounted_on == mount.mounted_on\n"
                "}\n"
                "assert \"mount\" in \"mount\"\n"
            )
            (tests / "test-storage.xsh").write_text(
                'let mounted = run_storage(ctx, "mount", ["/owned"], log)\n'
            )

            with patch.object(CHECKER, "CORE", core):
                selected = CHECKER.native_tests("mount", None)

        self.assertEqual(selected, "core/tests/test-storage.xsh")

    def test_invocation_ties_remain_sorted_deterministically(self) -> None:
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            core = Path(directory)
            tests = core / "tests"
            tests.mkdir()
            (tests / "test-alpha.xsh").write_text(
                'let result = run_storage(ctx, "mount", [], log)\n'
            )
            (tests / "test-zeta.xsh").write_text(
                'let result = run_storage(ctx, "mount", [], log)\n'
            )

            with patch.object(CHECKER, "CORE", core):
                selected = CHECKER.native_tests("mount", None)

        self.assertEqual(selected, "core/tests/test-alpha.xsh")

    def test_family_helper_arguments_are_recognized_as_invocations(self) -> None:
        source = '''
for family in [{encode: "gzip", decode: "gunzip"}] {
  let result = invoke(ctx, family.encode, [])
}
'''
        self.assertTrue(CHECKER.invokes_native_command("gzip", source))

    def test_mention_fallback_remains_deterministic(self) -> None:
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            core = Path(directory)
            tests = core / "tests"
            tests.mkdir()
            (tests / "test-alpha.xsh").write_text('let words = "mount mount"\n')
            (tests / "test-zeta.xsh").write_text('let words = "mount mount mount"\n')

            with patch.object(CHECKER, "CORE", core):
                selected = CHECKER.native_tests("mount", None)

        self.assertEqual(selected, "core/tests/test-zeta.xsh")


if __name__ == "__main__":
    unittest.main()
