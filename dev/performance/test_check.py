import base64
from contextlib import contextmanager, nullcontext
import importlib.util
import json
import os
from pathlib import Path
import signal
import sys
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('performance_check', Path(__file__).with_name('check.py'))
check = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = check
spec.loader.exec_module(check)
sys.path.insert(0, str(check.ADAPTER_ROOT))
from dev.performance import workloads
from bench.consolidation import fixtures as fixture_owner


class CohortTests(unittest.TestCase):
    def setUp(self):
        (check.ADAPTER_ROOT / '.work/tmp').mkdir(parents=True, exist_ok=True)

    def test_demanding_fixtures_preserve_row_and_tree_outputs(self):
        with tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
            root = Path(directory) / '16384'
            fixture_owner.generate(root, package_rows=16384)
            self.assertEqual(len(json.loads((root / 'index.json').read_bytes())), 16385)
            self.assertEqual(len(json.loads((root / 'expected/json').read_bytes())), 16386)
            self.assertEqual(len((root / 'expected/directory').read_text().splitlines()),
                             len(fixture_owner.snapshot(root / 'tree')))
            manifest = json.loads((root / 'manifest.json').read_bytes())
            actual = fixture_owner.snapshot(root)
            del actual['manifest.json']
            self.assertEqual(manifest, actual)

    def test_demanding_cases_append_after_the_exact_r13_cohort(self):
        original_names = (
            'startup-trivial', 'startup-template',
            'stream-count-n', 'stream-map-where-n',
            'stream-count-2n', 'stream-map-where-2n',
            'stream-count-4n', 'stream-map-where-4n',
            'json-n', 'directory-n', 'json-2n', 'directory-2n',
            'json-4n', 'directory-4n', 'spawn-128',
        )
        with tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
            fixtures = Path(directory)
            for count in fixture_owner.PACKAGE_ROW_SHAPES + fixture_owner.DEMANDING_PACKAGE_ROW_SHAPES:
                expected = fixtures / str(count) / 'expected'
                expected.mkdir(parents=True)
                (expected / 'json').write_bytes(b'[]\n')
                (expected / 'directory').write_bytes(b'')
            cases = workloads.frozen_workloads(fixtures)
        self.assertEqual(tuple(case.name for case in cases[:15]), original_names)
        self.assertEqual(tuple(case.name for case in cases[15:]), tuple(
            f'{script}-package-range-{count}'
            for count in fixture_owner.DEMANDING_PACKAGE_ROW_SHAPES
            for script in ('json', 'directory')
        ))
        self.assertEqual(tuple(case.shape for case in cases[15:]), tuple(
            f'package-range-{count}'
            for count in fixture_owner.DEMANDING_PACKAGE_ROW_SHAPES
            for _ in ('json', 'directory')
        ))
        self.assertEqual(tuple(case.family for case in cases[15:]),
                         ('algorithms', 'filesystem-process') * 3)
        self.assertEqual(tuple(case.arguments for case in cases[15:]), tuple(
            (str(fixtures / str(count)),)
            for count in fixture_owner.DEMANDING_PACKAGE_ROW_SHAPES
            for _ in ('json', 'directory')
        ))

    def test_syscall_summary_and_complete_helper_lifecycles_are_required(self):
        rows = [{'type': 'syscall.summary', 'syscall_count': 17},
                {'kind': 'run.start', 'event_id': 1, 'payload': {'type': 'run.start', 'target': {'display': 'x', 'hex': '78'}, 'argv': [], 'cwd': {'display': 'x', 'hex': '78'}, 'env': []}},
                {'kind': 'run.end', 'event_id': 2, 'parent_event_id': 1,
                 'payload': {'type': 'run.end', 'pid': 19, 'status': {'kind': 'exit', 'code': 0, 'success': True}, 'error': None}}]
        self.assertEqual(workloads.trace_counts(iter(rows)), {'syscalls': 17, 'subprocesses': 1})
        for changed in (rows[1:], rows[:2], rows + rows[:1], rows[:2] + [{'payload': {'type': 'run.end', 'pid': None, 'error': 'failed'}}],
                        [rows[0] | {'syscall_count': -1}, *rows[1:]]):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, 'attribution unavailable'):
                workloads.trace_counts(iter(changed))

    def test_syscall_summary_cannot_double_as_a_process_event(self):
        argument = {'display': 'x', 'hex': '78'}
        rows = [{'kind': 'run.start', 'event_id': 1,
                 'payload': {'type': 'run.start', 'target': argument, 'argv': [], 'cwd': argument, 'env': []}},
                {'type': 'syscall.summary', 'syscall_count': 1, 'kind': 'run.end', 'event_id': 2, 'parent_event_id': 1,
                 'payload': {'type': 'run.end', 'pid': 19, 'status': {'kind': 'exit', 'code': 0, 'success': True}, 'error': None}}]
        with self.assertRaisesRegex(ValueError, 'attribution unavailable'):
            workloads.trace_counts(iter(rows))

    def test_non_object_trace_payload_is_incomplete(self):
        for payload in (None, 42, 'bad', []):
            with self.subTest(payload=payload), self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
                workloads.trace_counts(iter([{'kind': 'trace.event', 'payload': payload},
                                              {'type': 'syscall.summary', 'syscall_count': 1}]))

    def test_pipeline_segments_are_not_reported_as_zero_helpers(self):
        rows = [{'kind': 'pipeline.enter', 'payload': {'type': 'none'}},
                {'kind': 'pipeline.segment.end', 'payload': {'type': 'pipeline.segment.end',
                                                              'index': 0, 'pid': 19, 'error': None}},
                {'type': 'syscall.summary', 'syscall_count': 17}]
        with self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
            workloads.trace_counts(iter(rows))

    def test_run_helper_completions_match_distinct_starts(self):
        rows = [{'kind': 'run.start', 'event_id': 3, 'payload': {'type': 'run.start', 'target': {'display': 'x', 'hex': '78'}, 'argv': [], 'cwd': {'display': 'x', 'hex': '78'}, 'env': []}},
                {'kind': 'run.start', 'event_id': 4, 'payload': {'type': 'run.start', 'target': {'display': 'x', 'hex': '78'}, 'argv': [], 'cwd': {'display': 'x', 'hex': '78'}, 'env': []}},
                {'kind': 'run.end', 'event_id': 5, 'parent_event_id': 3,
                 'payload': {'type': 'run.end', 'pid': 19, 'status': {'kind': 'exit', 'code': 0, 'success': True}, 'error': None}},
                {'kind': 'run.end', 'event_id': 6, 'parent_event_id': 4,
                 'payload': {'type': 'run.end', 'pid': 20, 'status': {'kind': 'exit', 'code': 0, 'success': True}, 'error': None}},
                {'type': 'syscall.summary', 'syscall_count': 17}]
        self.assertEqual(workloads.trace_counts(iter(rows)), {'syscalls': 17, 'subprocesses': 2})
        for changed in ([*rows[:3], rows[2], rows[-1]], [*rows[:3], rows[3] | {'parent_event_id': 9}, rows[-1]]):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
                workloads.trace_counts(iter(changed))
        malformed_end = rows[2] | {'payload': rows[2]['payload'] | {'status': {}}}
        missing_error = rows[2] | {'payload': {key: value for key, value in rows[2]['payload'].items() if key != 'error'}}
        malformed_start = rows[0] | {'payload': rows[0]['payload'] | {'target': {'display': 'x', 'hex': 'zz'}}}
        for changed in ([*rows[:2], malformed_end, *rows[3:]],
                        [*rows[:2], missing_error, *rows[3:]],
                        [malformed_start, *rows[1:]],
                        [*rows[:2], {key: value for key, value in rows[2].items() if key != 'event_id'}, *rows[3:]],
                        [*rows[:3], rows[3] | {'event_id': 5}, rows[-1]],
                        [*rows[:2], rows[2] | {'payload': rows[2]['payload'] | {'status': {'kind': 'exit', 'code': 1, 'success': True}}}, *rows[3:]],
                        [*rows[:2], rows[2] | {'payload': rows[2]['payload'] | {'status': {'kind': 'signal', 'code': 9, 'success': True}}}, *rows[3:]]):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
                workloads.trace_counts(iter(changed))
        with self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
            workloads.trace_counts(iter([{'kind': 'spawn.start', 'payload': {'type': 'spawn.start'}}, rows[-1]]))

    def test_spawned_helpers_require_matched_ready_and_wait_events(self):
        argument = {'display': 'x', 'hex': '78'}
        span = {'file': 'script.xsh', 'start_offset': 0, 'end_offset': 1, 'start_line': 1, 'start_column': 1, 'end_line': 1, 'end_column': 2}
        status = {'kind': 'exit', 'code': 0, 'success': True}
        rows = [
            {'kind': 'spawn.start', 'event_id': 2, 'parent_event_id': 1, 'source_span': span,
             'payload': {'type': 'spawn.start', 'handle_id': None, 'target': argument, 'argv': [], 'cwd': argument, 'env': [], 'detached': False}},
            {'kind': 'spawn.ready', 'event_id': 3, 'parent_event_id': 1, 'source_span': span,
             'payload': {'type': 'spawn.ready', 'handle_id': 1, 'pid': 21}},
            {'kind': 'wait.start', 'event_id': 4, 'parent_event_id': 1, 'source_span': span,
             'payload': {'type': 'wait.start', 'handle_ids': [1]}},
            {'kind': 'wait.end', 'event_id': 5, 'parent_event_id': 1, 'source_span': span,
             'payload': {'type': 'wait.end', 'handle_id': 1, 'pid': 21, 'status': status, 'error': None}},
            {'type': 'syscall.summary', 'syscall_count': 10},
        ]
        self.assertEqual(workloads.trace_counts(iter(rows)), {'syscalls': 10, 'subprocesses': 1})
        cancel = {'kind': 'spawn.cancel', 'event_id': 6, 'parent_event_id': 1, 'source_span': span,
                  'payload': {'type': 'spawn.cancel', 'handle_id': 1, 'pid': None, 'signal': 'TERM', 'kill_after_ms': 2000, 'error': None}}
        self.assertEqual(workloads.trace_counts(iter([*rows[:-1], cancel, rows[-1]])), {'syscalls': 10, 'subprocesses': 1})
        for changed in (rows[:1] + rows[2:], rows[:3] + rows[4:],
                        [rows[0], rows[1] | {'source_span': {'file': 'other'}}, *rows[2:]],
                        [*rows[:3], rows[3] | {'payload': rows[3]['payload'] | {'pid': 22}}, rows[4]],
                        [rows[0] | {'payload': rows[0]['payload'] | {'detached': True}}, *rows[1:]],
                        [rows[0], rows[1], rows[2] | {'payload': {'type': 'wait.start', 'handle_ids': [1, 1]}}, *rows[3:]],
                        [*rows[:3], rows[3] | {'event_id': 3}, rows[4]],
                        [*rows[:2], rows[2] | {'event_id': 9}, *rows[3:]],
                        [*[row | {'source_span': {'file': 'bad'}} for row in rows[:-1]], rows[-1]],
                        [*rows[:2], cancel | {'event_id': 4, 'payload': cancel['payload'] | {'pid': 21}}, *rows[2:]]):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
                workloads.trace_counts(iter(changed))

    def test_empty_wait_is_a_completed_noop(self):
        span = {'file': 'script.xsh', 'start_offset': 0, 'end_offset': 1,
                'start_line': 1, 'start_column': 1, 'end_line': 1, 'end_column': 2}
        rows = [{'kind': 'wait.start', 'event_id': 2, 'parent_event_id': 1, 'source_span': span,
                 'payload': {'type': 'wait.start', 'handle_ids': []}},
                {'type': 'syscall.summary', 'syscall_count': 1}]
        self.assertEqual(workloads.trace_counts(iter(rows)), {'syscalls': 1, 'subprocesses': 0})

    def test_pipeline_completions_require_exact_declared_segments(self):
        status = {'kind': 'exit', 'code': 0, 'success': True}
        rows = [
            {'kind': 'pipeline.enter', 'event_id': 3, 'payload': {'type': 'none'}},
            {'kind': 'pipeline.segment.end', 'event_id': 4, 'parent_event_id': 3,
             'payload': {'type': 'pipeline.segment.end', 'index': 0, 'pid': 19, 'error': None, 'status': status}},
            {'kind': 'pipeline.segment.end', 'event_id': 5, 'parent_event_id': 3,
             'payload': {'type': 'pipeline.segment.end', 'index': 1, 'pid': 20, 'error': None, 'status': status}},
            {'kind': 'pipeline.exit', 'event_id': 6, 'parent_event_id': 3,
             'payload': {'type': 'pipeline.end', 'error': None, 'status': status}},
            {'type': 'syscall.summary', 'syscall_count': 17},
        ]
        self.assertEqual(workloads.trace_counts(iter(rows), expected_pipeline_segments=(2,)),
                         {'syscalls': 17, 'subprocesses': 2})
        duplicate_pid = rows[2] | {'payload': rows[2]['payload'] | {'pid': 19}}
        failed = rows[2] | {'payload': rows[2]['payload'] | {'status': status | {'code': 1, 'success': False}}}
        forged = rows[0] | {'payload': {'type': 'run.start', 'target': {'display': 'x', 'hex': '78'}, 'argv': [], 'cwd': {'display': 'x', 'hex': '78'}, 'env': []}}
        for changed in (rows[:2] + rows[3:], rows[:3] + rows[4:], rows[:3] + [rows[2]] + rows[3:],
                        rows[:2] + [duplicate_pid] + rows[3:], rows[:2] + [failed] + rows[3:],
                        [rows[0], {key: value for key, value in rows[1].items() if key != 'event_id'}, *rows[2:]],
                        [*rows[:2], rows[2] | {'event_id': 4}, *rows[3:]],
                        [*rows[:3], {key: value for key, value in rows[3].items() if key != 'event_id'}, rows[4]],
                        [rows[0], rows[1] | {'payload': {key: value for key, value in rows[1]['payload'].items() if key != 'error'}}, *rows[2:]],
                        [forged] + rows[1:], [*rows[:-1], {'payload': {'type': 'run.start', 'target': {'display': 'x', 'hex': '78'}, 'argv': [], 'cwd': {'display': 'x', 'hex': '78'}, 'env': []}},
                                             {'payload': {'type': 'run.end', 'pid': 21, 'error': None}}, rows[-1]]):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
                workloads.trace_counts(iter(changed), expected_pipeline_segments=(2,))
        with self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
            workloads.trace_counts(iter(rows), expected_pipeline_segments=(1, 1))
        for kind in ('wait.start', 'wait.end', 'spawn.cancel'):
            with self.subTest(kind=kind), self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
                workloads.trace_counts(iter([*rows[:-1], {'payload': {'type': kind}}, rows[-1]]),
                                       expected_pipeline_segments=(2,))
                workloads.trace_counts(iter([*rows[:-1], {'kind': kind, 'payload': {'type': 'none'}}, rows[-1]]),
                                       expected_pipeline_segments=(2,))
        with self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
            workloads.trace_counts(iter([*rows[:-1], {'kind': 'pipeline.segment.start', 'payload': {'type': 'none'}}, rows[-1]]),
                                   expected_pipeline_segments=(2,))

    def run_fake_cohort(self, root, *, failure=None):
        cases = tuple(workloads.Workload(name, family, 'common', root / 'script.xsh', (), b'ready\n', 0)
                      for name, family in (('cold', 'startup'), ('algorithm', 'algorithms')))
        def invoke(argv, directory, environment, timeout, *, measured=False, useful_prefix=None):
            row = {'wall_ns': 10 if measured else None, 'first_useful_stdout_ns': 3 if measured else None,
                   'useful_stdout_observed': True, 'cpu_ns': 7, 'max_rss_bytes': 4096, 'status': 0,
                   'timed_out': False, 'stdout_base64': base64.b64encode(b'ready\n').decode(),
                   'stderr_base64': '', 'effects': {}}
            if failure and measured:
                row.update(failure)
            return row
        sampler = SimpleNamespace(invoke=invoke, equivalent=lambda row, expected: row['stdout_base64'] == base64.b64encode(expected).decode(),
                                  summarize_samples=lambda *_: {'samples': 1})
        resources = SimpleNamespace(cgroup_snapshot=lambda: {'memory_peak': '8192'})
        with patch.object(workloads, 'frozen_workloads', return_value=cases):
            workloads.run_cohort(root, root / 'xsh', root / 'xsht', root / 'work', root / 'report.json', sampler, resources)
        return check.read_json(root / 'report.json')

    def test_frozen_sample_counts_and_completion_remain_distinct_from_attribution(self):
        writes = []
        real_write = Path.write_text
        def write(path, *args, **kwargs):
            writes.append(path)
            return real_write(path, *args, **kwargs)
        with tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
            with patch.object(Path, 'write_text', new=write):
                report = self.run_fake_cohort(Path(directory))
        self.assertEqual(len(report['samples']), 3 * (60 + 30))
        self.assertEqual(len(report['parity']), 4)
        self.assertTrue(report['timing_complete'])
        self.assertFalse(report['complete'])
        self.assertEqual(report['attribution'], {})
        self.assertTrue(all(row['first_useful_stdout_ns'] < row['wall_ns'] for row in report['samples']))
        self.assertTrue(all('stdout_base64' not in row and 'stdout_sha256' in row for row in report['samples']))
        self.assertLess(len(writes), len(report['samples']) // 10)

    def test_wrong_output_and_absent_cpu_retain_failed_sample_without_completion(self):
        for failure, message in (({'stdout_base64': base64.b64encode(b'wrong\n').decode()}, 'output/status/effects'),
                                 ({'cpu_ns': None}, 'CPU/RSS accounting unavailable')):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
                root = Path(directory)
                with self.assertRaisesRegex(ValueError, message):
                    self.run_fake_cohort(root, failure=failure)
                report = check.read_json(root / 'report.json')
                self.assertFalse(report['complete'])
                self.assertFalse(report['timing_complete'])
                self.assertEqual(len(report['samples']), 1)

    def test_failed_final_publication_does_not_replace_wrong_output(self):
        real_write = Path.write_text
        def write(path, *args, **kwargs):
            if json.loads(args[0])['samples']:
                raise OSError('publication failed')
            return real_write(path, *args, **kwargs)
        with tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
            with patch.object(Path, 'write_text', new=write), self.assertRaisesRegex(ValueError, 'output/status/effects') as caught:
                self.run_fake_cohort(Path(directory), failure={'stdout_base64': ''})
        self.assertIn('publication failed', caught.exception.__notes__[0])

    def run_diagnostic(self, root, name, *, syscall_count=17, failure=None, helper=True):
        case = workloads.Workload('helper', 'filesystem-process', 'common', root / 'script.xsh', ('argument with space',), b'ready\n', 1)
        calls = []
        def invoke(argv, directory, environment, timeout, *, measured=False, useful_prefix=None):
            calls.append((argv, measured, useful_prefix))
            phase_times = {'preparation_inclusive': 4, 'project_parse_load_exclusive': 1, 'check_solve_fact_build_exclusive': 1,
                           'declaration_publication_exclusive': 1, 'lower_verify_install_exclusive': 1,
                           'preparation_other_exclusive': 0, 'execution_finish_exclusive': 1, 'diagnostic_finish_exclusive': None}
            phase = {'tracking_active': True, 'status': 0, 'stdout_bytes': 6, 'stderr_bytes': 0,
                     **{key: {'live_bytes': 1, 'peak_bytes': 2, 'alloc_count': 3, 'alloc_bytes': 4} for key in ('construction', 'controller', 'execution')},
                     'workers': {'stage_count': 0, 'alloc_count': 0, 'alloc_bytes': 0, 'peak_thread_bytes': 0, 'attributions': []},
                     'cpu_time_ns': dict(phase_times, clock='process_cpu'), 'wall_time_ns': dict(phase_times, clock='monotonic_wall')}
            if argv[1] == 'runtime-stats':
                Path(argv[3]).write_text(json.dumps(phase))
            else:
                events = [{'kind': 'run.start', 'event_id': 2, 'parent_event_id': 1, 'payload': {'type': 'run.start', 'target': {'display': 'x', 'hex': '78'}, 'argv': [], 'cwd': {'display': 'x', 'hex': '78'}, 'env': []}},
                          {'kind': 'run.end', 'event_id': 3, 'parent_event_id': 2,
                           'payload': {'type': 'run.end', 'pid': 19, 'status': {'kind': 'exit', 'code': 0, 'success': True}, 'error': None}}] if helper else []
                summary = {'type': 'syscall.summary', 'syscall_count': syscall_count, 'syscall_seconds': 0,
                           'top_syscalls_by_count': [], 'per_program_top_syscalls': [], 'per_process_top_syscalls': []}
                root_event = {'kind': 'script.enter', 'event_id': 1, 'parent_event_id': None, 'depth': 0, 'payload': {'type': 'none'}}
                exit_event = {'kind': 'script.exit', 'event_id': len(events) + 2, 'parent_event_id': 1, 'depth': 0, 'payload': {'type': 'none'}}
                Path(argv[argv.index('--trace-file') + 1]).write_text(''.join(json.dumps(row) + '\n' for row in [root_event, *events, exit_event, summary]))
            row = {'wall_ns': None, 'first_useful_stdout_ns': None, 'useful_stdout_observed': True,
                   'status': 0, 'timed_out': False, 'stdout_base64': base64.b64encode(b'ready\n').decode(),
                   'stderr_base64': '', 'effects': {}}
            return row | (failure or {})
        owner = check.baseline_owner(SimpleNamespace(source_root=check.ADAPTER_ROOT), 'bench/consolidation/run.py', 'comparative_bench')
        sampler = SimpleNamespace(invoke=invoke, equivalent=owner.equivalent)
        resources = SimpleNamespace(cgroup_snapshot=lambda: {'memory_peak': '8192'})
        output = root / f'{name}.json'
        with patch.object(workloads, 'frozen_workloads', return_value=(case,)):
            workloads.run_cohort(root, root / 'xsh', root / 'xsht', root / name, output, sampler, resources, diagnostic=True)
        return check.read_json(output), calls

    def test_two_diagnostic_reports_in_one_parent_keep_distinct_sidecars(self):
        with tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
            root = Path(directory)
            first, calls = self.run_diagnostic(root, 'first')
            paths = [Path(first['attribution']['helper'][key]) for key in ('runtime_stats', 'syscall_trace')]
            before = [path.read_bytes() for path in paths]
            second, _ = self.run_diagnostic(root, 'second', syscall_count=23)
            for path, contents, key in zip(paths, before, ('runtime_stats', 'syscall_trace')):
                self.assertEqual(path.read_bytes(), contents)
                self.assertNotEqual(str(path), second['attribution']['helper'][key])
                self.assertTrue(path.is_relative_to(root / 'first'))
            self.assertTrue(first['attribution_complete'])
            self.assertFalse(first['timing_complete'])
            self.assertEqual(first['attribution']['helper']['subprocesses'], 1)
            self.assertEqual(first['attribution']['helper']['syscalls'], 17)
            self.assertTrue(all(not measured and prefix == b'ready\n' for _, measured, prefix in calls))
            self.assertEqual(calls[0][0][-3:], [str(root / 'script.xsh'), '--', 'argument with space'])
            self.assertEqual(calls[1][0][-3:], calls[0][0][-3:])

    def test_diagnostic_output_status_effects_and_helper_mismatch_reject(self):
        for failure, helper, message in (({'stdout_base64': ''}, True, 'output/status/effects'),
                                         ({'status': 7}, True, 'output/status/effects'),
                                         ({'effects': {'leak': {'after': 'file'}}}, True, 'output/status/effects'),
                                         (None, False, 'unexpected helper count')):
            with self.subTest(failure=failure, helper=helper), tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
                root = Path(directory)
                with self.assertRaisesRegex(ValueError, message):
                    self.run_diagnostic(root, 'failed', failure=failure, helper=helper)
                report = check.read_json(root / 'failed.json')
                self.assertFalse(report['complete'])
                self.assertFalse(report['attribution_complete'])
                self.assertTrue(report['attribution']['helper']['observations'])


class AdapterDeadlineTests(unittest.TestCase):
    def test_operation_deadline_reaches_child_and_timeout_preserves_cleanup(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        for seconds, timed_out, resolved in [(60, False, True), (900, True, True), (900, True, False)]:
            with self.subTest(seconds=seconds, timed_out=timed_out, resolved=resolved), tempfile.TemporaryDirectory(dir=scratch) as directory:
                events = []
                primary = subprocess.TimeoutExpired(['docker', 'run'], seconds)
                class Child:
                    returncode = 0
                    stdout = None
                    stderr = None
                    def communicate(self, *, timeout):
                        events.append(('communicate', timeout))
                        if timed_out:
                            raise primary
                        return b'output', b''
                child = Child()
                def cleanup(actual, remove_container):
                    self.assertIs(actual, child)
                    events.append(('cleanup',))
                    return remove_container()
                previous = object()
                with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), patch.object(check.owned, 'install_signal_handlers', return_value=previous), patch.object(check.owned, 'restore_signal_handlers') as restore, patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), patch.object(check.owned, 'cleanup_container_process', side_effect=cleanup), patch.object(check.owned, 'cleanup_cidfile_container', return_value={'resolved': resolved}), patch.object(check.subprocess, 'Popen', return_value=child):
                    if timed_out:
                        with self.assertRaises(subprocess.TimeoutExpired) as caught:
                            check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=seconds)
                        self.assertIs(caught.exception, primary)
                        if not resolved:
                            self.assertIn('cleanup unresolved', primary.__notes__[0])
                            failure = primary.cleanup_failure
                            self.assertIsInstance(failure, check.DockerCleanupError)
                            self.assertEqual(failure.child_status, 0)
                            self.assertIsNone(failure.stdout)
                            self.assertIsNone(failure.stderr)
                            self.assertEqual(failure.cleanup['resolved'], False)
                    else:
                        self.assertEqual(check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=seconds), (0, b'output', b''))
                    restore.assert_called_once_with(previous)
                self.assertEqual(events, [('communicate', seconds), ('cleanup',)])
                self.assertEqual(bool(list(Path(directory).iterdir())), not resolved)

    def test_verified_cleanup_resolves_under_native_lease_without_nested_window(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            events = []
            lease_held = False
            cleanup_depth = 0

            @contextmanager
            def native_lease():
                nonlocal lease_held
                lease_held = True
                try:
                    yield
                finally:
                    lease_held = False

            @contextmanager
            def cleanup_window():
                nonlocal cleanup_depth
                self.assertEqual(cleanup_depth, 0)
                cleanup_depth += 1
                events.append('cleanup-window-enter')
                try:
                    yield
                finally:
                    events.append('cleanup-window-exit')
                    cleanup_depth -= 1

            class Child:
                returncode = 0

                stdout = None

                stderr = None

                def communicate(self, *, timeout):
                    return b'output', b''

            child = Child()

            def cleanup(actual, remove_container):
                self.assertIs(actual, child)
                with check.owned.cleanup_window():
                    events.append('cleanup')
                    return {'resolved': True}

            def resolve_quarantine():
                self.assertTrue(lease_held)
                self.assertEqual(cleanup_depth, 0)
                events.append('resolve-quarantine')

            with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                    patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                    patch.object(check.owned, 'restore_signal_handlers'), \
                    patch.object(check.owned, 'cleanup_window', side_effect=cleanup_window), \
                    patch.object(check.owned, 'cleanup_container_process', side_effect=cleanup), \
                    patch.object(check.subprocess, 'Popen', return_value=child), \
                    native_lease():
                self.assertEqual(check.checked_docker(
                    ['docker', 'run'],
                    Path(directory),
                    timeout_seconds=60,
                    on_cleanup_verified=resolve_quarantine,
                ), (0, b'output', b''))
            self.assertEqual(events, ['cleanup-window-enter', 'cleanup', 'cleanup-window-exit', 'resolve-quarantine'])
            self.assertFalse(lease_held)

    def test_quarantine_resolution_error_is_reported_without_replacing_primary_failure(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            primary = subprocess.TimeoutExpired(['docker', 'run'], 60)
            resolution_failure = OSError('cleanup quarantine removal failed')
            resolution_failure.add_note('quarantine marker restoration failed')
            lease_held = False

            @contextmanager
            def native_lease():
                nonlocal lease_held
                lease_held = True
                try:
                    yield
                finally:
                    lease_held = False

            class Child:
                returncode = 0

                stdout = None

                stderr = None

                def communicate(self, *, timeout):
                    raise primary

            def cleanup(actual, remove_container):
                return {'resolved': True}

            resolutions = []

            def resolve_quarantine():
                self.assertTrue(lease_held)
                resolutions.append(True)
                raise resolution_failure

            with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                    patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                    patch.object(check.owned, 'restore_signal_handlers'), \
                    patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), \
                    patch.object(check.owned, 'cleanup_container_process', side_effect=cleanup), \
                    patch.object(check.subprocess, 'Popen', return_value=Child()):
                with native_lease(), self.assertRaises(subprocess.TimeoutExpired) as caught:
                    check.checked_docker(
                        ['docker', 'run'],
                        Path(directory),
                        timeout_seconds=60,
                        on_cleanup_verified=resolve_quarantine,
                    )
            self.assertIs(caught.exception, primary)
            self.assertFalse(lease_held)
            self.assertEqual(resolutions, [True])
            self.assertTrue(any('cleanup quarantine removal failed' in note for note in primary.__notes__))
            failure = primary.cleanup_failure
            self.assertIsInstance(failure, check.DockerCleanupError)
            self.assertEqual(failure.child_status, 0)
            self.assertEqual(failure.cleanup, {'resolved': True})
            self.assertIs(failure.resolution_error, resolution_failure)
            self.assertEqual(failure.resolution_notes, ('quarantine marker restoration failed',))
            self.assertIn('quarantine marker restoration failed', str(failure))

    def test_unresolved_cleanup_errors_carry_observed_child_output(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            cleanup_failure = OSError('container inspection failed')

            class Child:
                returncode = 7

                stdout = None

                stderr = None

                def communicate(self, *, timeout):
                    return b'child stdout', b'child stderr'

            def cleanup(actual, remove_container):
                raise cleanup_failure

            with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                    patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                    patch.object(check.owned, 'restore_signal_handlers'), \
                    patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), \
                    patch.object(check.owned, 'cleanup_container_process', side_effect=cleanup), \
                    patch.object(check.subprocess, 'Popen', return_value=Child()):
                with self.assertRaises(check.DockerCleanupError) as caught:
                    check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=60)
            failure = caught.exception
            self.assertEqual(failure.child_status, 7)
            self.assertEqual(failure.stdout, b'child stdout')
            self.assertEqual(failure.stderr, b'child stderr')
            self.assertIsNone(failure.cleanup)
            self.assertIs(failure.cleanup_error, cleanup_failure)
            self.assertIsNone(failure.resolution_error)

    def test_unresolved_cleanup_result_carries_observed_child_output(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            class Child:
                returncode = 3
                stdout = None
                stderr = None

                def communicate(self, *, timeout):
                    return b'partial output', b'partial diagnostic'

            def cleanup(actual, remove_container):
                return {'resolved': False, 'detail': 'container remains'}

            with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                    patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                    patch.object(check.owned, 'restore_signal_handlers'), \
                    patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), \
                    patch.object(check.owned, 'cleanup_container_process', side_effect=cleanup), \
                    patch.object(check.subprocess, 'Popen', return_value=Child()):
                with self.assertRaises(check.DockerCleanupError) as caught:
                    check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=60)
            failure = caught.exception
            self.assertEqual(failure.child_status, 3)
            self.assertEqual(failure.stdout, b'partial output')
            self.assertEqual(failure.stderr, b'partial diagnostic')
            self.assertEqual(failure.cleanup, {'resolved': False, 'detail': 'container remains'})
            self.assertIsNone(failure.cleanup_error)
            self.assertIsNone(failure.resolution_error)

    def test_verified_cleanup_resolves_quarantine_after_primary_timeout(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            primary = subprocess.TimeoutExpired(['docker', 'run'], 60)
            resolved = []

            class Child:
                returncode = 0

                stdout = None

                stderr = None

                def communicate(self, *, timeout):
                    raise primary

            def cleanup(actual, remove_container):
                return {'resolved': True}

            with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                    patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                    patch.object(check.owned, 'restore_signal_handlers'), \
                    patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), \
                    patch.object(check.owned, 'cleanup_container_process', side_effect=cleanup), \
                    patch.object(check.subprocess, 'Popen', return_value=Child()):
                with self.assertRaises(subprocess.TimeoutExpired) as caught:
                    check.checked_docker(
                        ['docker', 'run'],
                        Path(directory),
                        timeout_seconds=60,
                        on_cleanup_verified=lambda: resolved.append(True),
                    )
            self.assertIs(caught.exception, primary)
            self.assertEqual(resolved, [True])

    def test_interrupted_operation_with_unverified_cleanup_keeps_quarantine(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            interruption = check.owned.SignalReceived(signal.SIGTERM)
            resolved = []

            class Child:
                returncode = 0

                stdout = None

                stderr = None

                def communicate(self, *, timeout):
                    raise interruption

            def cleanup(actual, remove_container):
                return {'resolved': False, 'detail': 'container remains'}

            with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                    patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                    patch.object(check.owned, 'restore_signal_handlers'), \
                    patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), \
                    patch.object(check.owned, 'cleanup_container_process', side_effect=cleanup), \
                    patch.object(check.subprocess, 'Popen', return_value=Child()):
                with self.assertRaises(check.owned.SignalReceived) as caught:
                    check.checked_docker(
                        ['docker', 'run'],
                        Path(directory),
                        timeout_seconds=60,
                        on_cleanup_verified=lambda: resolved.append(True),
                    )
            self.assertIs(caught.exception, interruption)
            self.assertEqual(resolved, [])
            self.assertIn('cleanup unresolved', '\n'.join(interruption.__notes__))
            self.assertIsInstance(interruption.cleanup_failure, check.DockerCleanupError)
            self.assertFalse(interruption.cleanup_failure.cleanup['resolved'])

    def test_owned_pipe_descriptors_close_after_success_failure_timeout_and_cancellation(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        cleanup_failure = OSError('cleanup operation failed')
        for primary, cleanup_raises in (
                (None, False), (OSError('communication failed'), False),
                (subprocess.TimeoutExpired(['docker', 'run'], 1), False),
                (check.owned.SignalReceived(signal.SIGTERM), False),
                (check.owned.SignalReceived(signal.SIGTERM), True), (None, True)):
            with self.subTest(primary=primary, cleanup_raises=cleanup_raises), tempfile.TemporaryDirectory(dir=scratch) as directory:
                pipes = []
                for _ in range(2):
                    reader, writer = os.pipe()
                    os.close(writer)
                    pipes.append(os.fdopen(reader, 'rb'))
                descriptors = [pipe.fileno() for pipe in pipes]

                class Child:
                    stdout, stderr = pipes
                    returncode = 17

                    def communicate(self, *, timeout):
                        if primary is not None:
                            raise primary
                        return b'output', b'error'

                try:
                    with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                            patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                            patch.object(check.owned, 'restore_signal_handlers'), \
                            patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), \
                            patch.object(check.owned, 'cleanup_container_process',
                                         side_effect=cleanup_failure if cleanup_raises else None,
                                         return_value={'resolved': True}), \
                            patch.object(check.subprocess, 'Popen', return_value=Child()):
                        if primary is None and not cleanup_raises:
                            self.assertEqual(check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=1),
                                             (17, b'output', b'error'))
                        elif primary is None:
                            with self.assertRaises(check.DockerCleanupError):
                                check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=1)
                        else:
                            with self.assertRaises(type(primary)) as caught:
                                check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=1)
                            self.assertIs(caught.exception, primary)
                    self.assertTrue(all(pipe.closed for pipe in pipes))
                    for descriptor in descriptors:
                        with self.assertRaises(OSError):
                            os.fstat(descriptor)
                    self.assertEqual(bool(list(Path(directory).iterdir())), cleanup_raises)
                finally:
                    for pipe in pipes:
                        pipe.close()

    def test_pipe_close_failure_keeps_cleanup_quarantine_unresolved(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        for primary in (None, subprocess.TimeoutExpired(['docker', 'run'], 1)):
            with self.subTest(primary=primary), tempfile.TemporaryDirectory(dir=scratch) as directory:
                close_failure = OSError('pipe close failed')
                resolved = []

                class Pipe:
                    def close(self):
                        raise close_failure

                class Child:
                    stdout = Pipe()
                    stderr = None
                    returncode = 0

                    def communicate(self, *, timeout):
                        if primary is not None:
                            raise primary
                        return b'output', b''

                with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                        patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                        patch.object(check.owned, 'restore_signal_handlers'), \
                        patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), \
                        patch.object(check.owned, 'cleanup_container_process', return_value={'resolved': True}), \
                        patch.object(check.subprocess, 'Popen', return_value=Child()):
                    with self.assertRaises(type(primary) if primary is not None else check.DockerCleanupError) as caught:
                        check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=1,
                                             on_cleanup_verified=lambda: resolved.append(True))
                if primary is not None:
                    self.assertIs(caught.exception, primary)
                    self.assertIs(primary.cleanup_failure.cleanup_error, close_failure)
                else:
                    self.assertIs(caught.exception.cleanup_error, close_failure)
                self.assertEqual(resolved, [])
                self.assertTrue(list(Path(directory).iterdir()))

    def test_scratch_removal_failure_never_replaces_the_primary_failure(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        removal_failure = OSError('owned scratch not removable')
        for primary in (subprocess.TimeoutExpired(['docker', 'run'], 1), check.owned.SignalReceived(signal.SIGTERM)):
            with self.subTest(primary=primary), tempfile.TemporaryDirectory(dir=scratch) as directory:
                class Child:
                    stdout = None
                    stderr = None
                    returncode = 0

                    def communicate(self, *, timeout):
                        raise primary

                with patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                        patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                        patch.object(check.owned, 'restore_signal_handlers'), \
                        patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), \
                        patch.object(check.owned, 'cleanup_container_process', return_value={'resolved': True}), \
                        patch.object(check.subprocess, 'Popen', return_value=Child()), \
                        patch.object(check.shutil, 'rmtree', side_effect=removal_failure):
                    with self.assertRaises(type(primary)) as caught:
                        check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=1)
                self.assertIs(caught.exception, primary)
                self.assertTrue(any('owned scratch not removable' in note for note in primary.__notes__))
                self.assertFalse(hasattr(primary, 'cleanup_failure'))

    def test_scratch_removal_failure_is_raised_when_nothing_else_failed(self):
        scratch = check.ADAPTER_ROOT / '.work/tmp'
        removal_failure = OSError('owned scratch not removable')

        class Child:
            stdout = None
            stderr = None
            returncode = 0

            def communicate(self, *, timeout):
                return b'output', b''

        with tempfile.TemporaryDirectory(dir=scratch) as directory, \
                patch.object(check.owned, 'owned_docker_command', side_effect=lambda command, **_: command), \
                patch.object(check.owned, 'install_signal_handlers', return_value={}), \
                patch.object(check.owned, 'restore_signal_handlers'), \
                patch.object(check.owned, 'cleanup_window', side_effect=nullcontext), \
                patch.object(check.owned, 'cleanup_container_process', return_value={'resolved': True}), \
                patch.object(check.subprocess, 'Popen', return_value=Child()), \
                patch.object(check.shutil, 'rmtree', side_effect=removal_failure):
            with self.assertRaises(OSError) as caught:
                check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=1)
        self.assertIs(caught.exception, removal_failure)



if __name__ == '__main__':
    unittest.main()
