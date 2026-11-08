from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import outcomes

OK = "core/tests/a.xsh::test_ok"
FLAKY = "core/tests/b.xsh::test_flaky"
SKIP = "core/tests/c.xsh::test_skip"
IDS = sorted([OK, FLAKY, SKIP])


def clean_log(skip_scratch="1111"):
    return (
        "running 3 tests\n"
        f"{OK} ... ok 1ms\n"
        "egrep: warning: egrep is obsolescent; using grep -E\n"
        f"cp: overwrite '/tmp/x/dest'? {FLAKY} ... ok 2ms\n"
        f"{SKIP} ... skipped: fixture unavailable: /tmp/xsh-test-23-{skip_scratch}-2900-test_skip/cap.txt 1ms\n"
        "test result: ok. 2 passed; 0 failed; 1 skipped\n"
    )


def failed_log(status="FAILED", skip_scratch="2222"):
    return (
        "running 3 tests\n"
        f"{OK} ... ok 1ms\n"
        f"{FLAKY} ... {status} 1m3s\n"
        f"{SKIP} ... skipped: fixture unavailable: /tmp/xsh-test-23-{skip_scratch}-2900-test_skip/cap.txt 1ms\n"
        "\nfailures:\n\n"
        f"---- {FLAKY} ----\n"
        f"{OK} ... ok 9ms\n"
        "test result: ok. 9 passed; 0 failed; 0 skipped\n"
        "assertion failed: header minute changed\n\n"
        "test result: FAILED. 1 passed; 1 failed; 1 skipped\n"
    )


class PairAccountingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def run_log(self, name, text):
        path = self.root / f"{name}.log"
        path.write_text(text, encoding="utf-8")
        return outcomes.validate_log(path, IDS)

    def test_clean_pair_with_stray_output_and_run_scoped_skip_paths_is_accepted(self):
        pair = outcomes.compare_pair(
            self.run_log("reference", clean_log("1111")),
            self.run_log("candidate", clean_log("3333")),
            0,
            0,
        )
        self.assertEqual(pair["issues"], [])
        self.assertTrue(pair["accepted"])
        self.assertEqual(pair["test_failures"], [])

    def test_one_failed_test_with_full_accounting_is_reported_without_rejecting_the_pair(self):
        for status, outcome in (("FAILED", "failed"), ("TIMEOUT", "timeout")):
            with self.subTest(status=status):
                pair = outcomes.compare_pair(
                    self.run_log("reference", clean_log()),
                    self.run_log("candidate", failed_log(status)),
                    0,
                    1,
                )
                self.assertEqual(pair["issues"], [])
                self.assertTrue(pair["accepted"])
                self.assertEqual(pair["test_failures"], [{"arm": "candidate", "id": FLAKY, "status": outcome}])

    def test_a_failed_test_still_fails_correctness_acceptance_of_its_run(self):
        run = self.run_log("candidate", failed_log())
        self.assertTrue(run.complete)
        self.assertFalse(run.accepted)

    def test_incomplete_or_inconsistent_runs_still_reject_the_pair(self):
        reference = self.run_log("reference", clean_log())
        cases = {
            "missing result": (clean_log().replace(f"{OK} ... ok 1ms\n", ""), 0),
            "summary count mismatch": (clean_log().replace("2 passed", "3 passed"), 0),
            "killed before summary": (clean_log().rsplit("test result", 1)[0], 137),
            "cancelled case": (clean_log().replace(f"{OK} ... ok 1ms", f"{OK} ... CANCELLED 1ms"), 130),
            "runner failure without a failed case": (clean_log(), 1),
            "runner error line": (failed_log() + "xsht: worker cleanup failed\n", 1),
            "failed row without failure detail": (failed_log().replace(f"---- {FLAKY} ----\n", ""), 1),
            "failure exit code hidden": (failed_log(), 0),
            "out-of-manifest row": (clean_log().replace(OK, "core/tests/z.xsh::test_other"), 0),
            "different skip reason": (clean_log().replace("fixture unavailable", "other reason"), 0),
        }
        for name, (text, exit_code) in cases.items():
            with self.subTest(name):
                pair = outcomes.compare_pair(reference, self.run_log("candidate", text), 0, exit_code)
                self.assertFalse(pair["accepted"])
                self.assertTrue(pair["issues"])


class CountedRunTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def counted(self, text):
        path = self.root / "run.log"
        path.write_text(text, encoding="utf-8")
        return outcomes.validate_counted_log(path)

    def test_counted_run_accounts_for_every_header_case(self):
        run = self.counted(failed_log())
        self.assertTrue(run.complete)
        self.assertEqual(run.test_failures, (FLAKY,))
        self.assertEqual(run.expected_exit_code, 1)

    def test_counted_run_rejects_a_shortfall_or_missing_header(self):
        for name, text in {
            "shortfall": clean_log().replace("running 3 tests", "running 4 tests"),
            "no header": clean_log().replace("running 3 tests\n", ""),
            "no cases": "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 skipped\n",
        }.items():
            with self.subTest(name):
                self.assertFalse(self.counted(text).complete)


if __name__ == "__main__":
    unittest.main()
