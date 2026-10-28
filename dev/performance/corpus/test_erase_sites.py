#!/usr/bin/env python3
import io
from pathlib import Path
import stat
import tempfile
import unittest

import erase_sites

FAKE_XSHT = """#!/usr/bin/env python3
import sys
text = open(sys.argv[2]).read()
if ": Str" not in text:
    sys.stderr.write("err[check.type-mismatch]: type mismatch\\n")
    sys.exit(2)
if "BROKEN" in text:
    sys.stderr.write("err[check.parse]: broken\\n")
    sys.exit(2)
"""
SOURCE = "pure a(x: Int) -> Int { x }\npure b(y: Str) -> Str { y }\n"
HEADER = "site\tproject\tpath\tkind\tdeclaration_kind\tvisibility\tedit_start\tedit_end\n"


class EraseSitesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        base = Path(self.temp.name)
        self.root = base / "project"
        self.root.mkdir()
        (self.root / "main.xsh").write_text(SOURCE)
        self.xsht = base / "xsht"
        self.xsht.write_text(FAKE_XSHT)
        self.xsht.chmod(self.xsht.stat().st_mode | stat.S_IXUSR)
        self.scratch = base / "scratch"

    def inventory(self, spans):
        rows = [
            f"{index}\tp\tmain.xsh\tparameter\tpure\tprivate\t{start}\t{end}\n"
            for index, (start, end) in enumerate(spans)
        ]
        path = Path(self.temp.name) / "sites.tsv"
        path.write_text(HEADER + "".join(rows))
        return path

    def run_tool(self, spans):
        buffer = io.StringIO()
        sites = erase_sites.read_sites(self.inventory(spans))
        singles, joint = erase_sites.measure(
            sites, {"p": self.root}, str(self.xsht), self.scratch, jobs=2, timeout=30
        )
        erase_sites.summarize(singles, joint, buffer)
        return singles, joint, buffer.getvalue()

    def test_an_erasure_that_breaks_the_check_is_reported_with_its_first_error(self):
        x_start = SOURCE.index(": Int")
        y_start = SOURCE.index(": Str")
        singles, joint, summary = self.run_tool([(x_start, x_start + 5), (y_start, y_start + 5)])
        statuses = {site["site"]: (status, codes) for site, status, codes in singles}
        self.assertEqual(statuses["0"], ("pass", []))
        self.assertEqual(statuses["1"], ("fail", ["check.type-mismatch"]))
        self.assertIn("1/2 pass", summary)
        self.assertIn("1 parameter check.type-mismatch", summary)

    def test_joint_erasure_applies_only_sites_that_passed_alone_and_restores_the_tree(self):
        x_start = SOURCE.index(": Int")
        r_start = SOURCE.index("-> Int")
        singles, joint, summary = self.run_tool([(x_start, x_start + 5), (r_start, r_start + 7)])
        self.assertEqual(joint[("p", "main.xsh")][:2], (2, "pass"))
        self.assertEqual((self.root / "main.xsh").read_text(), SOURCE)
        self.assertIn("p/main.xsh: 2 sites, pass", summary)


if __name__ == "__main__":
    unittest.main()
