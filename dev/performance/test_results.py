from copy import deepcopy
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from dev.performance import results


def native_rows(outcome='passed', status=0):
    kinds = {'failed': 'test_failure', 'skipped': 'test_skip', 'timed_out': 'timeout',
             'cancelled': 'cancellation', 'not_executed': 'not_admitted'}
    reason = None if outcome == 'passed' else {'kind': kinds[outcome], 'detail': 'fixture outcome'}
    if outcome == 'timed_out':
        reason['limit'] = '1s'
    common = {'schema': 1, 'run_id': 'native-fixture'}
    return [common | {'event': 'run_started', 'selected': ['tests/a.xsh::case']},
            common | {'event': 'case_started', 'case_id': 'tests/a.xsh::case'},
            common | {'event': 'case_finished', 'case_id': 'tests/a.xsh::case', 'outcome': outcome,
                      'reason': reason, 'duration_ns': 7},
            common | {'event': 'run_finished', 'status': status, 'reason': None, 'selected': 1, 'completed': 1,
                      'counts': {name: int(name == outcome) for name in results.OUTCOMES}, 'duration_ns': 11}]


def encode(rows):
    return ''.join(json.dumps(row) + '\n' for row in rows)


class ResultTests(unittest.TestCase):
    def test_trace_records_stream_large_complete_jsonl_and_reject_bad_inputs(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            path = Path(directory) / 'trace.jsonl'
            line = b'{"kind":"ordinary","payload":"' + b'x' * 1000 + b'"}\n'
            with path.open('wb') as stream:
                for _ in range(17000):
                    stream.write(line)
                stream.write(b'{"type":"syscall.summary","syscall_count":1}\n')
            self.assertGreater(path.stat().st_size, 16 * 1024 * 1024)
            count = 0
            for row in results.trace_records(path):
                count += 1
            self.assertEqual(count, 17001)
            self.assertEqual(row['type'], 'syscall.summary')
            for data in (b'', b'{"a":1}', b'{"a":1,"a":2}\n', b'[]\n', b'\xff\n', b'{"x":"' + b'x' * (1024 * 1024) + b'"}\n'):
                path.write_bytes(data)
                with self.subTest(prefix=data[:20]), self.assertRaises(results.ResultError):
                    list(results.trace_records(path))
            with path.open('wb') as stream:
                stream.truncate(129 * 1024 * 1024)
            with self.assertRaisesRegex(results.ResultError, 'trace file exceeds'):
                list(results.trace_records(path))
            path.unlink()
            os.mkfifo(path)
            with self.assertRaisesRegex(results.ResultError, 'regular file'):
                list(results.trace_records(path))
            path.unlink()
            (Path(directory) / 'target').write_text('{}\n')
            path.symlink_to('target')
            with self.assertRaises(OSError):
                list(results.trace_records(path))

    def test_trace_stream_rejects_changed_or_replaced_file(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            path = Path(directory) / 'trace.jsonl'
            for replace in (False, True):
                with self.subTest(replace=replace):
                    path.write_text('{"event":1}\n{"event":2}\n')
                    records = results.trace_records(path)
                    self.assertEqual(next(records), {'event': 1})
                    if replace:
                        path.rename(Path(directory) / 'old')
                    path.write_text('{"event":3}\n{"event":4}\n')
                    with self.assertRaisesRegex(results.ResultError, 'trace file changed'):
                        list(records)

    def test_cargo_diagnostics_reject_nonregular_input_without_blocking(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            fifo = Path(directory) / 'cargo.fifo'
            os.mkfifo(fifo)
            command = [sys.executable, '-B', str(Path(results.__file__)), '--runner', 'cargo',
                       '--events', str(fifo), '--cargo-diagnostics']
            outcome = subprocess.run(command, capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 2)
            self.assertIn('regular file', outcome.stderr)

    def test_cargo_diagnostics_caps_input_and_output(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            path = Path(directory) / 'cargo.jsonl'
            row = json.dumps({'reason': 'compiler-message', 'message': {'rendered': 'error: ' + 'x' * 6000}}) + '\n'
            path.write_text(row * 17)
            output = StringIO()
            with redirect_stdout(output):
                results.cargo_diagnostics(path)
            self.assertEqual(output.getvalue().count('error: '), 16)
            self.assertIn('Cargo diagnostics limited to 16 messages', output.getvalue())
            self.assertLess(len(output.getvalue()), 16 * 4100)
            path.write_bytes(b'{' + b'x' * (3 * 1024 * 1024))
            output = StringIO()
            with redirect_stdout(output):
                results.cargo_diagnostics(path)
            self.assertEqual(output.getvalue(), 'Cargo diagnostic input limited to 2 MiB\n')

    def test_cargo_discovery_requires_complete_unique_selected_tests(self):
        rows = [{'type': 'suite', 'event': 'discovery'},
                {'type': 'test', 'event': 'discovered', 'name': 'one', 'ignore': False},
                {'type': 'test', 'event': 'discovered', 'name': 'two', 'ignore': True},
                {'type': 'suite', 'event': 'completed', 'tests': 2, 'benchmarks': 0, 'total': 2, 'ignored': 1}]
        self.assertEqual(results.discover_cargo(encode(rows)), ('one', 'two'))
        for changed in (rows[:-1], rows[:2] + [rows[1]] + rows[2:],
                        rows[:1] + [rows[-1] | {'tests': 0, 'total': 0, 'ignored': 0}]):
            with self.subTest(changed=changed), self.assertRaises(results.ResultError):
                results.discover_cargo(encode(changed))

    def test_cargo_timeout_progress_requires_a_running_test_and_terminal_result(self):
        rows = [{'type': 'suite', 'event': 'started', 'test_count': 1},
                {'type': 'test', 'event': 'started', 'name': 'one'},
                {'type': 'test', 'event': 'timeout', 'name': 'one'},
                {'type': 'test', 'event': 'ok', 'name': 'one'},
                {'type': 'suite', 'event': 'ok', 'passed': 1, 'failed': 0, 'ignored': 0}]
        self.assertEqual(results.reconcile_cargo(encode(rows), ['one'], child_status=0)['status'], 0)
        for changed in ([rows[0], rows[2], rows[1], *rows[3:]], rows[:3] + rows[-1:]):
            with self.subTest(changed=changed), self.assertRaises(results.ResultError):
                results.reconcile_cargo(encode(changed), ['one'], child_status=0)

    def test_native_nonzero_status_requires_a_terminal_or_run_reason(self):
        rows = native_rows(status=1)
        result = results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=1)
        self.assertEqual(result['status'], 1)
        self.assertFalse(result['coverage_complete'])

    def test_failed_native_invocation_retains_failure_without_events(self):
        for raw in (1, 17, -9):
            result = results.reconcile_xsht('{', ['tests/a.xsh::case'], child_status=raw)
            self.assertEqual(result['status'], 1)
            self.assertFalse(result['coverage_complete'])
            self.assertEqual(result['unreconciled_selected'], ['tests/a.xsh::case'])
        stdout = StringIO()
        argv = ['results', '--runner', 'xsht', '--events', '/missing', '--case', 'one', '--child-status', '1']
        with patch('sys.argv', argv), patch.object(results, '_event_file', side_effect=FileNotFoundError('missing events')), redirect_stdout(stdout):
            self.assertEqual(results.main(), 1)
        self.assertIn('missing events', stdout.getvalue())

    def test_failed_cargo_invocation_retains_failure_without_events(self):
        result = results.reconcile_cargo('{', ['one'], child_status=101)
        self.assertEqual(result['status'], 1)
        self.assertFalse(result['coverage_complete'])
        self.assertEqual(result['unreconciled_selected'], ['one'])
        stdout = StringIO()
        argv = ['results', '--runner', 'cargo', '--events', '/missing', '--case', 'one', '--child-status', '101']
        with patch('sys.argv', argv), patch.object(results, '_event_file', side_effect=FileNotFoundError('missing events')), redirect_stdout(stdout):
            self.assertEqual(results.main(), 1)
        self.assertIn('missing events', stdout.getvalue())

    def test_native_failed_case_takes_precedence_while_reporting_cleanup_failure(self):
        for status in (1, 2):
            rows = native_rows('failed', status)
            rows[-1]['reason'] = {'kind': 'cleanup_failure', 'detail': 'cleanup failed'}
            result = results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=status)
            self.assertEqual(result['status'], 1)
            self.assertEqual(result['child_status'], status)
            self.assertIn('cleanup_failure', results.render_report(result))
            self.assertEqual(results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=status,
                                                    cancelled=True)['status'], 130)
        rows = native_rows('failed', 2)
        rows[-1]['reason'] = None
        with self.assertRaises(results.ResultError):
            results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=2)
        coverage = native_rows('passed', 1)
        coverage[-1]['reason'] = {'kind': 'coverage_failure', 'detail': 'coverage failed; cleanup failure: fixture'}
        result = results.reconcile_xsht(encode(coverage), ['tests/a.xsh::case'], child_status=1)
        self.assertEqual(result['status'], 1)
        self.assertIn('cleanup failure', results.render_report(result))

    def test_native_not_executed_cannot_follow_start(self):
        rows = native_rows('not_executed', 2)
        with self.assertRaises(results.ResultError):
            results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=2)

    def test_native_run_reason_must_match_status(self):
        rows = native_rows(status=2)
        rows[-1]['reason'] = {'kind': 'coverage_failure', 'detail': 'incomplete'}
        with self.assertRaises(results.ResultError):
            results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=2)

    def test_cancelled_case_cannot_turn_failed_run_into_cancellation(self):
        rows = native_rows('cancelled', 1)
        rows[-1]['reason'] = {'kind': 'coverage_failure', 'detail': 'coverage failed'}
        result = results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=1)
        self.assertEqual(result['status'], 1)
        self.assertFalse(result['coverage_complete'])

    def test_temporary_event_report_has_no_expired_link(self):
        stdout = StringIO()
        argv = ['results', '--runner', 'xsht', '--events', '/temporary/events.jsonl',
                '--case', 'tests/a.xsh::case', '--child-status', '0', '--no-artifact-link']
        with patch('sys.argv', argv), patch.object(results, '_event_file', return_value=encode(native_rows())), redirect_stdout(stdout):
            self.assertEqual(results.main(), 0)
        self.assertNotIn('Details:', stdout.getvalue())

    def test_cli_reads_selected_ids_from_json_without_argument_expansion(self):
        stdout = StringIO()
        argv = ['results', '--runner', 'xsht', '--events', '/events', '--cases-json', '/cases', '--child-status', '0']
        with patch('sys.argv', argv), patch.object(results, '_case_file', return_value=['tests/a.xsh::case']), \
                patch.object(results, '_event_file', return_value=encode(native_rows())), redirect_stdout(stdout):
            self.assertEqual(results.main(), 0)
        self.assertIn('Selected scope: passed', stdout.getvalue())

    def test_event_file_rejects_nonregular_and_oversized_input(self):
        scratch = Path(__file__).resolve().parents[2] / '.work/tmp'
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            root = Path(directory)
            fifo = root / 'events.fifo'
            os.mkfifo(fifo)
            command = [sys.executable, '-B', str(Path(results.__file__)), '--runner', 'xsht',
                       '--events', str(fifo), '--case', 'tests/a.xsh::case', '--child-status', '0']
            outcome = subprocess.run(command, capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 2)
            self.assertIn('regular file', outcome.stderr)
            oversized = root / 'large.jsonl'
            with oversized.open('wb') as stream:
                stream.truncate(16 * 1024 * 1024 + 1)
            command[command.index(str(fifo))] = str(oversized)
            outcome = subprocess.run(command, capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 2)
            self.assertIn('exceeds 16 MiB', outcome.stderr)
            command[-1] = '1'
            outcome = subprocess.run(command, capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 1)
            self.assertIn('events_unavailable', outcome.stdout)
            event_file = root / 'events.jsonl'
            event_file.write_text(encode(native_rows()))
            linked = root / 'linked.jsonl'
            linked.symlink_to(event_file)
            command[command.index(str(oversized))] = str(linked)
            command[-1] = '0'
            outcome = subprocess.run(command, capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 2)

    def test_unavailable_selection_preserves_observed_failure_and_cancellation(self):
        scratch = Path(__file__).resolve().parents[2] / '.work/tmp'
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            root = Path(directory)
            events = root / 'events.jsonl'
            events.write_text(encode(native_rows()))
            command = [sys.executable, '-B', str(Path(results.__file__)), '--runner', 'xsht',
                       '--events', str(events), '--cases-json', str(root / 'missing'), '--child-status']
            for raw, expected in (('0', 2), ('1', 1), ('2', 2), ('130', 130)):
                with self.subTest(raw=raw):
                    outcome = subprocess.run([*command, raw], capture_output=True, text=True, timeout=2)
                    self.assertEqual(outcome.returncode, expected)
                    self.assertIn('missing', outcome.stderr)
            outcome = subprocess.run([*command, '2', '--cancelled'], capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 130)
            command[command.index('xsht')] = 'cargo'
            outcome = subprocess.run([*command, '2'], capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 1)

    def test_selected_id_file_rejects_nonregular_input_without_blocking(self):
        scratch = Path(__file__).resolve().parents[2] / '.work/tmp'
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            root = Path(directory)
            fifo = root / 'ids.fifo'
            os.mkfifo(fifo)
            events = root / 'events.jsonl'
            events.write_text(encode(native_rows()))
            command = [sys.executable, '-B', str(Path(results.__file__)), '--runner', 'xsht',
                       '--events', str(events), '--cases-json', str(fifo), '--child-status', '0']
            outcome = subprocess.run(command, capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 2)
            self.assertIn('regular file', outcome.stderr)
            oversized = root / 'large.json'
            with oversized.open('wb') as stream:
                stream.truncate(16 * 1024 * 1024 + 1)
            command[command.index(str(fifo))] = str(oversized)
            outcome = subprocess.run(command, capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 2)
            self.assertIn('exceeds 16 MiB', outcome.stderr)
            selected = root / 'cases.json'
            selected.write_text(json.dumps(['tests/a.xsh::case']))
            command[command.index(str(oversized))] = str(selected)
            outcome = subprocess.run(command, capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 0, outcome.stderr)
            selected.write_bytes(b'\xff')
            outcome = subprocess.run(command, capture_output=True, text=True, timeout=2)
            self.assertEqual(outcome.returncode, 2)
            self.assertIn('utf-8', outcome.stderr)

    def test_cli_preserves_case_reasons_and_links_full_truncated_detail(self):
        rows = native_rows('failed', 1)
        rows[2]['reason'] = {'kind': 'test_failure', 'detail': 'expected 7\nactual 9'}
        stdout = StringIO()
        argv = ['results', '--runner', 'xsht', '--events', '/owned/events.jsonl',
                '--case', 'tests/a.xsh::case', '--child-status', '1']
        with patch('sys.argv', argv), patch.object(results, '_event_file', return_value=encode(rows)), redirect_stdout(stdout):
            self.assertEqual(results.main(), 1)
        self.assertEqual(stdout.getvalue(), 'Selected scope: failed (1 cases; xsht)\nResults: failed=1\nChild status: 1\nCoverage: complete\nUnresolved: "tests/a.xsh::case": failed {"detail": "expected 7\\nactual 9", "kind": "test_failure"}; runner exited 1\nFull: not run\nDetails: [events](/owned/events.jsonl)\n')
        cargo = [{'type': 'suite', 'event': 'started', 'test_count': 1},
                 {'type': 'test', 'event': 'started', 'name': 'one'},
                 {'type': 'test', 'event': 'failed', 'name': 'one', 'stdout': 'assertion failed\n' + 'x' * 500},
                 {'type': 'suite', 'event': 'failed', 'passed': 0, 'failed': 1, 'ignored': 0}]
        result = results.reconcile_cargo(encode(cargo), ['one'], child_status=101)
        report = results.render_report(result, artifact='/owned/cargo.jsonl')
        self.assertIn('assertion failed\\n', report)
        self.assertIn('...', report)
        self.assertIn('Details: [events](/owned/cargo.jsonl)', report)
        self.assertIn('"kind": "failed"', report)
        self.assertNotIn('x' * 500, report)
        self.assertLessEqual(len(report.splitlines()), 8)

    def test_native_run_reason_remains_visible_when_all_cases_passed(self):
        rows = native_rows(status=2)
        rows[-1]['reason'] = {'kind': 'cleanup_failure', 'detail': 'owned worker cleanup unresolved\nretained'}
        result = results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=2)
        report = results.render_report(result)
        self.assertEqual(result['status'], 2)
        self.assertIn('cleanup_failure', report)
        self.assertIn('owned worker cleanup unresolved', report)
        self.assertNotIn('Unresolved: none', report)
        self.assertLessEqual(len(report.splitlines()), 8)

    def test_explicit_cancellation_preserves_incomplete_coverage_and_raw_status(self):
        cargo = encode([{'type': 'suite', 'event': 'started', 'test_count': 1},
                        {'type': 'test', 'event': 'started', 'name': 'one'}])
        for reconcile, text, selected in ((results.reconcile_cargo, cargo, ['one']),
                                           (results.reconcile_xsht, encode(native_rows()[:2]), ['tests/a.xsh::case'])):
            for raw, cancelled in ((130, False), (-2, True)):
                with self.subTest(runner=reconcile.__name__, raw=raw):
                    result = reconcile(text, selected, child_status=raw, cancelled=cancelled)
                    self.assertEqual(result['status'], 130)
                    self.assertEqual(result['child_status'], raw)
                    self.assertFalse(result['coverage_complete'])
                    self.assertEqual(result['unreconciled_selected'], selected)
                    self.assertEqual(result['cases'], {})
                    self.assertIn('Coverage: incomplete', results.render_report(result))
            self.assertEqual(reconcile(text, selected, child_status=1)['status'], 1)
        result = results.reconcile_xsht(encode(native_rows()), ['tests/a.xsh::case'], child_status=0, cancelled=True)
        self.assertEqual(result['status'], 130)
        self.assertTrue(result['coverage_complete'])
        self.assertNotIn('Unresolved: none', results.render_report(result))

    def test_cli_cancellation_precedes_truncated_stream_but_does_not_pass_it(self):
        for raw, cancelled, expected in ((130, False, 130), (-2, True, 130), (1, False, 1)):
            stdout, stderr = StringIO(), StringIO()
            argv = ['results', '--runner', 'cargo', '--events', 'unused', '--case', 'one', '--child-status', str(raw)]
            if cancelled:
                argv.append('--cancelled')
            with patch('sys.argv', argv), patch.object(results, '_event_file', return_value='{'), \
                    redirect_stdout(stdout), redirect_stderr(stderr):
                status = results.main()
            self.assertEqual(status, expected)
            report = stdout.getvalue() + stderr.getvalue()
            self.assertNotIn('scope: passed', report)
            self.assertIn('incomplete', report)
            self.assertLessEqual(len(report.splitlines()), 8)

    def test_native_required_scope_and_compact_report(self):
        result = results.reconcile_xsht(encode(native_rows()), ['tests/a.xsh::case'], child_status=0)
        self.assertEqual(result['status'], 0)
        self.assertEqual(result['cases']['tests/a.xsh::case']['duration_ns'], 7)
        report = results.render_report(result)
        self.assertLessEqual(len(report.splitlines()), 8)
        self.assertIn('Selected scope: passed (1 cases; xsht)', report)
        self.assertIn('Full: not run', report)

    def test_native_terminal_variants_never_make_required_skip_green(self):
        for outcome, raw, expected in (('failed', 1, 1), ('timed_out', 1, 1), ('skipped', 2, 2),
                                       ('cancelled', 130, 130), ('not_executed', 2, 2)):
            with self.subTest(outcome=outcome):
                rows = native_rows(outcome, raw)
                if outcome == 'not_executed':
                    del rows[1]
                result = results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=raw)
                self.assertEqual(result['status'], expected)
                self.assertEqual(result['child_status'], raw)
                report = results.render_report(result, artifact='/owned/native.jsonl')
                self.assertIn('fixture outcome', report)
                self.assertIn(rows[-2]['reason']['kind'], report)
                self.assertNotIn('Unresolved: none', report)

    def test_native_rejects_schema_scope_lifecycle_counts_and_truncation(self):
        original = native_rows()
        variants = [original[:-1], original + original[-1:], [original[1], original[0], *original[2:]],
                    [original[0], original[2], original[1], original[3]], original[:2] + original[1:]]
        for index, key, value in ((0, 'schema', 2), (2, 'duration_ns', True), (2, 'reason', {}),
                                  (3, 'completed', 0), (2, 'case_id', 'other'), (1, 'run_id', 'other'),
                                  (0, 'selected', ['tests/a.xsh::case', 'tests/a.xsh::case'])):
            changed = deepcopy(original)
            changed[index][key] = value
            variants.append(changed)
        for index, key in ((2, 'reason'), (3, 'reason'), (3, 'counts')):
            changed = deepcopy(original)
            del changed[index][key]
            variants.append(changed)
        for rows in variants:
            with self.subTest(rows=rows), self.assertRaises(results.ResultError):
                results.reconcile_xsht(encode(rows), ['tests/a.xsh::case'], child_status=0)
        for text in (encode(original).rstrip(), encode(original).replace('"schema": 1', '"schema": 1, "schema": 1'), '{}\n{'):
            with self.subTest(text=text), self.assertRaises(results.ResultError):
                results.reconcile_xsht(text, ['tests/a.xsh::case'], child_status=0)
        contradiction = results.reconcile_xsht(encode(original), ['tests/a.xsh::case'], child_status=1)
        self.assertEqual(contradiction['status'], 1)
        self.assertFalse(contradiction['coverage_complete'])

    def test_cargo_actual_libtest_shape_reconciles_raw_failure_status(self):
        rows = [{'type': 'suite', 'event': 'started', 'test_count': 2},
                {'type': 'test', 'event': 'started', 'name': 'one'},
                {'type': 'test', 'event': 'ok', 'name': 'one'},
                {'type': 'test', 'event': 'started', 'name': 'two'},
                {'type': 'test', 'event': 'failed', 'name': 'two', 'stdout': 'assertion failed\n'},
                {'type': 'suite', 'event': 'failed', 'passed': 1, 'failed': 1, 'ignored': 0,
                 'measured': 0, 'filtered_out': 17, 'exec_time': 0.01}]
        result = results.reconcile_cargo(encode(rows), ['two', 'one'], child_status=101)
        self.assertEqual(result['status'], 1)
        self.assertEqual(result['child_status'], 101)
        self.assertEqual(result['selected'], ['one', 'two'])
        self.assertEqual(result['cases']['two']['reason']['detail'], 'assertion failed\n')
        for changed in (rows[:-1], rows + rows[-1:], rows[:2] + rows[1:], rows[:1] + rows[2:]):
            with self.subTest(changed=changed):
                incomplete = results.reconcile_cargo(encode(changed), ['one', 'two'], child_status=101)
                self.assertEqual(incomplete['status'], 1)
                self.assertFalse(incomplete['coverage_complete'])
                self.assertEqual(incomplete['unreconciled_selected'], ['one', 'two'])


if __name__ == '__main__':
    unittest.main()
