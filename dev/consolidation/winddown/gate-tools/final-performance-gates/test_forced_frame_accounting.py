from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
spec = spec_from_file_location("accounting_forced", HERE / "forced-frames.py")
forced = module_from_spec(spec)
spec.loader.exec_module(forced)

FLAKY = "core/tests/b.xsh::test_flaky"
ONE_FAILURE = (
    "running 2 tests\n"
    "core/tests/a.xsh::test_ok ... ok 1ms\n"
    f"{FLAKY} ... FAILED 2ms\n"
    "\nfailures:\n\n"
    f"---- {FLAKY} ----\n"
    "assertion failed\n\n"
    "test result: FAILED. 1 passed; 1 failed; 0 skipped\n"
)


class ForcedFrameAccountingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.log = Path(self.temporary.name) / "native-suite.log"

    def accounting(self, text, status):
        self.log.write_text(text, encoding="utf-8")
        return forced.suite_accounting(status, self.log)

    def test_one_failed_test_in_a_fully_accounted_run_is_reported_and_accepted(self):
        result = self.accounting(ONE_FAILURE, 1)
        self.assertEqual(result["issues"], [])
        self.assertTrue(result["accepted"])
        self.assertEqual(result["test_failures"], [{"id": FLAKY, "status": "failed"}])

    def test_incomplete_or_inconsistent_runs_are_rejected(self):
        for name, text, status in (
            ("killed before summary", ONE_FAILURE.rsplit("test result", 1)[0], 137),
            ("missing result", ONE_FAILURE.replace("running 2 tests", "running 3 tests"), 1),
            ("runner failure without a failed case", ONE_FAILURE.replace("FAILED 2ms", "ok 2ms").replace("FAILED. 1 passed; 1 failed", "ok. 2 passed; 0 failed"), 1),
            ("exit code hides the failure", ONE_FAILURE, 0),
            ("no log", None, 1),
        ):
            with self.subTest(name):
                if text is None:
                    self.log.unlink(missing_ok=True)
                    result = forced.suite_accounting(status, self.log)
                else:
                    result = self.accounting(text, status)
                self.assertFalse(result["accepted"])
                self.assertTrue(result["issues"])


if __name__ == "__main__":
    unittest.main()
