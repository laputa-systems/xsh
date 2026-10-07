#!/usr/bin/env python3
"""Exercise the GNU runner's host process and report publication boundary."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
COMMIT = json.loads((REPO / "dev/compat/upstream.lock.json").read_text())["uutils"]["commit"]


class GnuRunnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="xsh-gnu-harness-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.root.chmod(0o755)
        for name in ("uutils", "uutils/target", "uutils/target/debug", "gnu", "gnu/src", "gnu/tests", "gnu/tests/ls", "tools", "results"):
            (self.root / name).mkdir(mode=0o777)
            (self.root / name).chmod(0o777)
        binary = self.root / "uutils/target/debug/coreutils"
        binary.write_text("#!/bin/sh\nexit 0\n")
        binary.chmod(0o755)
        for name in ("Makefile", "Makefile.in", "tests/local.mk"):
            (self.root / "gnu" / name).write_text("  PATH=unused\n")
            (self.root / "gnu" / name).chmod(0o666)
        git = self.root / "tools/git"
        git.write_text(f"#!/bin/sh\nprintf '%s\\n' '{COMMIT}'\n")
        git.chmod(0o755)
        make = self.root / "tools/make"
        make.write_text("""#!/usr/bin/env python3
import os, pathlib, subprocess, sys
if 'compat-list-tests' in sys.argv:
    print('tests/ls/pass.log\\ntests/ls/fail.log\\ntests/ls/skip.log')
    sys.exit(0)
assert not pathlib.Path('/proc/self/fd/9').exists(), 'suite lock inherited by child'
assert subprocess.run(['flock', '-n', os.environ['UUTILS_SUITE_LOCK'], 'true']).returncode != 0
scenario = os.environ['SCENARIO']
if scenario == 'no-report':
    sys.exit(2)
statuses = {'pass': 'PASS', 'fail': 'FAIL', 'skip': 'SKIP'}
if scenario == 'all-pass-error':
    statuses['fail'] = 'PASS'
for name, status in statuses.items():
    if scenario == 'partial' and name == 'skip':
        continue
    prefix = '001012012301234' if scenario == 'unterminated-output' else ''
    pathlib.Path(f'tests/ls/{name}.log').write_text(prefix + f'{status} tests/ls/{name}.sh (exit status: 0)\\n')
    pathlib.Path(f'tests/ls/{name}.trs').write_text(f':test-result: {status}\\n')
if scenario == 'mismatch':
    pathlib.Path('tests/ls/fail.trs').write_text(':test-result: PASS\\n')
counts = {status: list(statuses.values()).count(status) for status in ('PASS', 'FAIL', 'SKIP', 'ERROR', 'XFAIL', 'XPASS')}
summary = '# TOTAL: 3\\n' + ''.join(f'# {status}: {count}\\n' for status, count in counts.items())
if scenario == 'bad-summary':
    summary = summary.replace('# TOTAL: 3', '# TOTAL: 2')
if scenario != 'no-summary':
    pathlib.Path('tests/test-suite.log').write_text(summary)
sys.exit(124 if scenario == 'timeout' else 2)
""")
        make.chmod(0o755)
        self.output = self.root / "results/gnu-uutils.json"
        self.output.write_text('{"previous": true}\n')
        self.env = {
            **os.environ,
            "PATH": f"{self.root}/tools:{os.environ['PATH']}",
            "UUTILS_ROOT": str(self.root / "uutils"),
            "GNU_ROOT": str(self.root / "gnu"),
            "COMPAT_RESULTS_DIR": str(self.root / "results"),
            "UUTILS_SUITE_LOCK": str(self.root / "suite.lock"),
            "CARGO_TARGET_DIR": str(self.root / "uutils/target"),
            "GNU_RUN_UID": "1000",
            "GNU_RUN_GID": "1000",
        }

    def run_runner(self, scenario):
        return subprocess.run(
            ["bash", str(REPO / "dev/compat/run-gnu.sh"), "uutils", "tests/ls/pass.sh", "tests/ls/fail.sh", "tests/ls/skip.sh"],
            env={**self.env, "SCENARIO": scenario},
            capture_output=True,
            text=True,
            timeout=15,
        )

    def test_complete_report_preserves_failures_and_skips_in_scratch_results(self):
        unrelated = self.root / "gnu/tests/ls/unrelated.log"
        unrelated.write_text("PASS unrelated (exit status: 0)\n")
        result = self.run_runner("unterminated-output")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(self.output.read_text()), {
            "uutils_commit": COMMIT,
            "results": {"ls": {"pass.log": "PASS", "fail.log": "FAIL", "skip.log": "SKIP"}},
        })
        self.assertTrue(unrelated.exists())
        self.assertEqual(list((self.root / "results").glob(".gnu-run.*")), [])
        for name in ("Makefile", "tests/local.mk"):
            contents = (self.root / "gnu" / name).read_text()
            self.assertIn(
                f"{self.root}/uutils/target/debug$(PATH_SEPARATOR){self.root}/gnu/src$(PATH_SEPARATOR)",
                contents,
            )

    def test_harness_failures_preserve_previous_report(self):
        for scenario in ("no-report", "partial", "mismatch", "bad-summary", "no-summary", "timeout", "all-pass-error"):
            with self.subTest(scenario=scenario):
                # Earlier evidence must not fill holes in this run.
                for name in ("pass", "fail", "skip"):
                    (self.root / f"gnu/tests/ls/{name}.log").write_text(f"PASS {name} (exit status: 0)\n")
                    (self.root / f"gnu/tests/ls/{name}.trs").write_text(":test-result: PASS\n")
                (self.root / "gnu/tests/test-suite.log").write_text("# TOTAL: 3\n")
                result = self.run_runner(scenario)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("not updating results", result.stderr)
                self.assertEqual(json.loads(self.output.read_text()), {"previous": True})

    def test_diff_rejects_different_test_selections(self):
        self.output.write_text(json.dumps({"uutils_commit": COMMIT, "results": {"ls": {"pass.log": "PASS"}}}))
        (self.root / "results/gnu-xsh.json").write_text(json.dumps({"ls": {"other.log": "PASS"}}))
        output = self.root / "results/gnu-differential.json"
        output.write_text('{"previous": true}\n')
        result = subprocess.run(
            ["bash", str(REPO / "dev/compat/run-gnu.sh"), "diff"],
            env=self.env, capture_output=True, text=True, timeout=15,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("test selections differ", result.stderr)
        self.assertEqual(json.loads(output.read_text()), {"previous": True})

    @unittest.skipUnless(os.getuid() == 0, "unprivileged account ownership requires root")
    def test_unwritable_source_fails_before_running_tests(self):
        (self.root / "gnu/Makefile.in").chmod(0o644)
        result = self.run_runner("complete")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fix tree ownership", result.stderr)
        self.assertEqual(json.loads(self.output.read_text()), {"previous": True})


if __name__ == "__main__":
    unittest.main()
