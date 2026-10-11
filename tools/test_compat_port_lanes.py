import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from dev.compat.port import check_port, lanes


class PortLaneGenerationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.repo = self.root / "integration"
        self.repo.mkdir()
        self.lanes_root = self.root / "integration-lanes"
        self.scratch_root = self.lanes_root / "_scratch"
        self.created = []

        self.git("init", "--initial-branch=codex/compat-native")
        self.git("config", "user.name", "Compatibility Lane Test")
        self.git("config", "user.email", "compat-lane-test@example.invalid")
        (self.repo / "README").write_text("integration head\n")
        (self.repo / "core/tests").mkdir(parents=True)
        (self.repo / "dev/compat/port").mkdir(parents=True)
        (self.repo / "core/tests/already.xsh").write_text(
            "# origin: uutils test_cp::already\ntest test_uu_cp_already { |ctx|\n",
        )
        (self.repo / "dev/compat/port/freeze.json").write_text(json.dumps({
            "uutils": {
                "cp": [
                    "test_cp::already",
                    "test_cp::one",
                    "test_cp::two",
                    "test_cp::three",
                    "test_cp::four",
                    "test_cp::five",
                ],
            },
            "gnu": {"cp": ["cp/one.log"]},
            "busybox": {"cp": ["cp/one case"]},
        }))
        self.git("add", ".")
        self.git("commit", "-m", "integration base")
        self.head = self.git("rev-parse", "HEAD")

        self.patches = [
            patch.object(lanes, "REPO", self.repo),
            patch.object(lanes, "LANES_ROOT", self.lanes_root),
            patch.object(lanes, "SCRATCH_ROOT", self.scratch_root),
            patch.object(lanes, "SHARED_XSH_BIN", Path(
                "/home/josh/d/laputa-systems/xsh/target/x86_64-unknown-linux-musl/release/xsh",
            )),
            patch.object(lanes, "reference_paths", self.reference_paths),
            patch.object(lanes, "test_file", self.resolve_test_file),
            patch.object(check_port, "REPO", self.repo),
        ]
        for context in self.patches:
            context.start()

    def tearDown(self):
        for name in self.created:
            worktree = self.lanes_root / name
            subprocess.run(
                ["git", "-C", str(self.repo), "worktree", "remove", "--force", str(worktree)],
                capture_output=True, text=True,
            )
            subprocess.run(
                ["git", "-C", str(self.repo), "branch", "-D", f"lane/{name}"],
                capture_output=True, text=True,
            )
        for context in reversed(self.patches):
            context.stop()
        self.temp.cleanup()

    def git(self, *args):
        result = subprocess.run(
            ["git", "-C", str(self.repo), *args],
            capture_output=True, text=True, check=True,
        )
        return result.stdout.strip()

    def reference_paths(self, suite, util):
        source = self.root / "ref" / suite
        paths = {"source": source, "tests": source / "tests" / util}
        if suite == "uutils":
            paths["fixtures"] = source / "tests/fixtures" / util
        return paths

    def resolve_test_file(self, suite, util, test_id):
        if suite == "uutils":
            return f"tests/by-util/test_{util}.rs"
        if suite == "gnu":
            return f"tests/{util}/one.sh"
        return f"testsuite/{util}.tests"

    def test_chunks_use_integration_head_and_disjoint_ownership(self):
        names = lanes.create("uutils", "cp", chunk=2)
        self.created.extend(names)

        self.assertEqual(names, [
            "port-uutils-cp-1",
            "port-uutils-cp-2",
            "port-uutils-cp-3",
        ])
        self.assertEqual(self.git("branch", "--show-current"), "codex/compat-native")

        for index, name in enumerate(names):
            worktree = self.lanes_root / name
            scratch = self.scratch_root / name
            brief = (scratch / "brief.md").read_text()
            self.assertEqual(
                subprocess.check_output(["git", "-C", str(worktree), "rev-parse", "HEAD"], text=True).strip(),
                self.head,
            )
            self.assertIn(f"from integration HEAD {self.head}", brief)
            self.assertNotIn("master", brief)
            self.assertIn(f"Worktree: {worktree}", brief)
            self.assertIn(f"Scratch: {scratch}", brief)
            self.assertIn("dev/compat/docker-xsht.sh -- test -j 1", brief)
            self.assertIn(
                "XSH_BIN=/home/josh/d/laputa-systems/xsh/target/x86_64-unknown-linux-musl/release/xsh",
                brief,
            )
            self.assertNotRegex(brief, r"(?i)gpt-|haiku|Agent:")
            self.assertEqual(json.loads((scratch / "exceptions.json").read_text()), {})
            self.assertIn("uu.invoke_from_path", brief)
            self.assertIn("uu.invoke_paths", brief)
            self.assertIn("uu.at_bytes", brief)
            self.assertIn("uu.stdout_only_bytes", brief)
            self.assertIn("native-testable, not automatic", brief)

            test_ids = (scratch / "tests.txt").read_text().splitlines()
            self.assertEqual(len(test_ids), 2 if index < 2 else 1)
            fixture_path = "core/tests/data/uutils/cp/"
            owns_fixture = f"You own exactly: core/tests/test-uu-cp-{index + 1}.xsh, {fixture_path}" in brief
            self.assertEqual(owns_fixture, index == 0)
            self.assertIn(f"# origin: uutils", brief or "")

        all_ids = [
            line
            for name in names
            for line in (self.scratch_root / name / "tests.txt").read_text().splitlines()
        ]
        self.assertEqual(all_ids, [
            "test_cp::one",
            "test_cp::two",
            "test_cp::three",
            "test_cp::four",
            "test_cp::five",
        ])

    def test_gnu_and_busybox_get_separate_briefs_and_test_files(self):
        gnu = lanes.create("gnu", "cp", chunk=80, head=self.head)
        busybox = lanes.create("busybox", "cp", chunk=80, head=self.head)
        self.created.extend(gnu + busybox)

        gnu_brief = (self.scratch_root / "port-gnu-cp/brief.md").read_text()
        busybox_brief = (self.scratch_root / "port-busybox-cp/brief.md").read_text()
        self.assertEqual(gnu, ["port-gnu-cp"])
        self.assertEqual(busybox, ["port-busybox-cp"])
        self.assertIn("core/tests/test-gnu-cp.xsh", gnu_brief)
        self.assertIn("# origin: gnu <full frozen id>", gnu_brief)
        self.assertIn("cp/one.log", gnu_brief)
        self.assertIn("test_gnu_cp_one_log", gnu_brief)
        self.assertIn("core/tests/test-bb-cp.xsh", busybox_brief)
        self.assertIn("# origin: busybox <full frozen id>", busybox_brief)
        self.assertIn("cp/one case", busybox_brief)
        self.assertIn("test_bb_cp_one_case", busybox_brief)
        self.assertIn("do not copy test scripts", gnu_brief)
        self.assertIn("do not copy test scripts", busybox_brief)

    def test_fully_mapped_utility_creates_no_lane(self):
        (self.repo / "core/tests/all.xsh").write_text(
            "\n".join(
                line
                for test_id in (
                    "test_cp::already", "test_cp::one", "test_cp::two",
                    "test_cp::three", "test_cp::four", "test_cp::five",
                )
                for line in (f"# origin: uutils {test_id}", "test test_uu_mapped { |ctx|")
            ) + "\n",
        )
        self.assertEqual(lanes.create("uutils", "cp", chunk=2), [])
        self.assertFalse(self.lanes_root.exists())

    def test_test_names_preserve_uutils_modules_and_disambiguate_busybox(self):
        cksum_ids = [
            "test_cksum::cksum_base64_untagged_encoding::blake2b_216::check_length_guess",
            "test_cksum::cksum_base64_untagged_encoding::blake2b_224::check_length_guess",
        ]
        cksum_names = lanes.native_test_names("uutils", "cksum", cksum_ids)
        self.assertIn("blake2b_216_check_length_guess", cksum_names[cksum_ids[0]])
        self.assertIn("blake2b_224_check_length_guess", cksum_names[cksum_ids[1]])
        self.assertNotEqual(cksum_names[cksum_ids[0]], cksum_names[cksum_ids[1]])

        busybox_ids = ["awk/awk if operator !=", "awk/awk if operator <"]
        busybox_names = lanes.native_test_names("busybox", "awk", busybox_ids)
        self.assertEqual(len(set(busybox_names.values())), len(busybox_ids))
        for name in busybox_names.values():
            self.assertRegex(name, r"_[0-9a-f]{8}$")

        freeze_path = Path(__file__).resolve().parents[1] / "dev/compat/port/freeze.json"
        frozen = json.loads(freeze_path.read_text())
        for suite, utilities in frozen.items():
            for util, ids in utilities.items():
                names = lanes.native_test_names(suite, util, ids)
                self.assertEqual(len(set(names.values())), len(ids), f"{suite} {util}")

    def test_tagged_ignores_detached_origin_comments(self):
        (self.repo / "core/tests/detached.xsh").write_text(
            "# origin: uutils test_cp::detached\nprint \"not a test\"\n",
        )
        self.assertNotIn("uutils test_cp::detached", lanes.tagged())


if __name__ == "__main__":
    unittest.main()
