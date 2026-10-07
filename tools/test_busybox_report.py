import unittest

from dev.compat.busybox_report import parse_log


class BusyboxReportTests(unittest.TestCase):
    def test_counts_pass_failure_and_unsupported_feature_cases(self):
        report = parse_log(
            """PASS: cat prints a file
FAIL: cat numbers blank lines
SKIPPED: cat color mode
UNTESTED: cat legacy mode
""",
            "cat",
            exit_status=1,
        )

        self.assertEqual(
            report,
            {
                "pass": 1,
                "fail": 1,
                "skip": 2,
                "excluded": 0,
                "failing": ["cat numbers blank lines"],
                "applet": True,
            },
        )

    def test_nonzero_suite_without_case_output_is_reported_as_failure(self):
        report = parse_log("compile helper failed\n", "cat", exit_status=2)

        self.assertEqual(report["fail"], 1)
        self.assertEqual(report["failing"], ["runtest exited with status 2 before reporting cases"])

    def test_empty_successful_run_fails_loudly(self):
        with self.assertRaisesRegex(ValueError, "did not report any cases"):
            parse_log("", "cat", exit_status=0)


if __name__ == "__main__":
    unittest.main()
