import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import check_port


class CheckPortTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp.name)
        self.port = self.repo / "dev/compat/port"
        self.port.mkdir(parents=True)
        (self.repo / "core/tests").mkdir(parents=True)
        (self.repo / "tests/xsh").mkdir(parents=True)
        self.freeze = {
            "uutils": {
                "basename": ["test_basename::test_file"],
            },
        }
        self.write_json(self.port / "freeze.json", self.freeze)

    def tearDown(self):
        self.temp.cleanup()

    @staticmethod
    def write_json(path, value):
        path.write_text(json.dumps(value))

    def run_checker(self, *args):
        output = io.StringIO()
        with patch.object(check_port, "REPO", self.repo), patch(
            "sys.argv", ["check_port.py", "--strict", *args]
        ), contextlib.redirect_stdout(output):
            status = check_port.main()
        return status, output.getvalue()

    def test_origin_must_immediately_precede_a_test_declaration(self):
        source = self.repo / "core/tests/test-basename.xsh"
        source.write_text(
            "# origin: uutils test_basename::test_file\n"
            "\n"
            "test test_basename_file { |ctx|\n"
            "}\n"
        )

        status, output = self.run_checker()

        self.assertEqual(status, 1)
        self.assertIn("not immediately above a test declaration", output)

    def test_duplicate_origin_claims_fail_strict_mode(self):
        source = self.repo / "core/tests/test-basename.xsh"
        source.write_text(
            "# origin: uutils test_basename::test_file\n"
            "test test_basename_file_a { |ctx|\n"
            "}\n"
            "# origin: uutils test_basename::test_file\n"
            "test test_basename_file_b { |ctx|\n"
            "}\n"
        )

        status, output = self.run_checker()

        self.assertEqual(status, 1)
        self.assertIn("claimed twice", output)

    def test_exception_key_must_name_a_frozen_test(self):
        self.write_json(
            self.port / "exceptions.json",
            {"uutils test_basename::test_file": "retained boundary", "uutils typo": "stale"},
        )

        status, output = self.run_checker()

        self.assertEqual(status, 1)
        self.assertIn("exception keys name no frozen test", output)

    def test_exception_reason_must_not_be_empty(self):
        self.write_json(
            self.port / "exceptions.json",
            {"uutils test_basename::test_file": "  \t"},
        )

        status, output = self.run_checker()

        self.assertEqual(status, 1)
        self.assertIn("exception entries need a non-empty reason", output)

    def test_nonempty_reason_keeps_a_retained_boundary_exception_valid(self):
        self.write_json(
            self.port / "exceptions.json",
            {"uutils test_basename::test_file": "requires a PTY"},
        )

        status, output = self.run_checker()

        self.assertEqual(status, 0)
        self.assertNotIn("exception", output)


if __name__ == "__main__":
    unittest.main()
