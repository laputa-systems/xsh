import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from dev.performance import results
from dev.performance.test_results import encode, native_rows
import sys
sys.modules['results'] = results
from dev.performance import rank


class RankTests(unittest.TestCase):
    def test_native_schedule_and_rankings(self):
        rows = native_rows()
        rows[0]['selected'].append('tests/b.xsh::slow')
        rows[0]['workers'] = 3
        rows.insert(2, rows[1] | {'case_id': 'tests/b.xsh::slow', 'worker': 1, 'start_offset_ns': 2})
        rows.insert(4, rows[3] | {'case_id': 'tests/b.xsh::slow', 'duration_ns': 9})
        rows[-1].update(selected=2, completed=2, counts={name: 2 if name == 'passed' else 0 for name in results.OUTCOMES})
        output = rank.render_native(rank.native_summary(encode(rows), 10))
        self.assertIn('case sum 0.000000s', output)
        self.assertLess(output.index('tests/b.xsh::slow'), output.index('tests/a.xsh::case'))
        self.assertIn('Last to finish: tests/b.xsh::slow', output)
        self.assertIn('  1: ', output)
        self.assertIn('  2: 0.000000s 0.000000s', output)

    def test_cargo_runner_records_report_time_output_and_preserves_exit_status(self):
        root = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            directory = Path(directory)
            harness = directory / 'harness'
            harness.write_text("#!/bin/sh\nprintf '%s\\n' \"$*\"\nexit 7\n")
            harness.chmod(0o755)
            lock = directory / 'lock'
            env = os.environ | {'XSH_CARGO_TEST_LOCK_FILE': str(lock), 'XSH_CARGO_TIMINGS_DIR': str(directory)}
            result = subprocess.run(['flock', '-x', str(lock), str(root / 'dev/cargo_test_runner.sh'), str(harness)],
                                    env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 7)
            self.assertEqual(result.stdout, '-Z unstable-options --report-time\n')
            self.assertEqual(next(directory.glob('cargo-harness.*')).read_text(), result.stdout)
            self.assertEqual(list(directory.glob('*.status')), [])

    def test_cargo_duration_and_missing_duration(self):
        text = (
            'running 3 tests\n'
            'test one ... ok <0.250s>\n'
            'test two ... FAILED <1.500s>\n'
            'test three ... ignored, manual\n'
            '\n'
            'test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.75s\n'
        )
        output = rank.render_cargo(rank.cargo_summary(text, 10))
        self.assertIn('Cargo cases: 2, case sum 1.750000s', output)
        self.assertLess(output.index('1.500000s two'), output.index('0.250000s one'))
        with self.assertRaises(results.ResultError):
            rank.cargo_summary(text.replace(' <0.250s>', ''), 10)
        with self.assertRaises(results.ResultError):
            rank.cargo_summary(text.replace('test one ... ok <0.250s>\n', ''), 10)

    def test_json_record_combines_native_and_cargo_summaries(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            directory = Path(directory)
            native = directory / 'native-events.jsonl'
            native.write_text(encode(native_rows()))
            cargo = directory / 'cargo-integration-0123456789abcdef.AbCdEf'
            cargo.write_text('test one ... ok <0.250s>\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s\n')
            out = directory / 'health.json'
            result = subprocess.run([sys.executable, str(Path(rank.__file__)), '--json', str(out), '--label', 'abc', str(native), str(cargo)],
                                    env=os.environ | {'PYTHONPATH': str(Path(rank.__file__).parent)}, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            record = json.loads(out.read_text())
            self.assertEqual(record['label'], 'abc')
            self.assertEqual(record['cargo'][0]['binary'], 'integration')
            self.assertEqual(record['cargo'][0]['cases'], [{'case': 'one', 'duration_ns': 250000000}])
            self.assertEqual(record['native'][0]['source'], 'native-events.jsonl')
