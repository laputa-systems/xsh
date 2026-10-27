from dataclasses import replace
import base64
from contextlib import nullcontext, redirect_stdout
import io
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import signal

from dev.performance import core_cases, core_trace, focused, owned_case


class CoreCasesTests(unittest.TestCase):
    def setUp(self):
        self.scratch = core_cases.SOURCE_ROOT / '.work/tmp'
        self.scratch.mkdir(parents=True, exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(dir=self.scratch)
        self.addCleanup(self.directory.cleanup)
        self.fixtures = Path(self.directory.name)

    def prepare(self, name):
        return core_cases.prepare_case(name, fixtures=self.fixtures)

    def test_focused_cases_bind_and_recheck_generated_inputs(self):
        config = SimpleNamespace(source_root=core_cases.SOURCE_ROOT, fixtures=self.fixtures)
        for name in ('core-wc-small', 'core-wc-medium', 'core-wc-large', 'core-sort-small', 'core-sort-medium', 'core-sort-large', 'core-traversal-small', 'core-traversal-medium', 'core-traversal-large', 'core-awk-small', 'core-awk-medium', 'core-awk-large'):
            with self.subTest(name=name):
                expected = self.prepare(name)
                case, root, snapshot, owner = focused._case(config, name)
                self.assertEqual(case, expected)
                self.assertEqual(owner.snapshot(root), snapshot)
                (root / 'unexpected').write_bytes(b'changed\n')
                with self.assertRaisesRegex(ValueError, 'file changed'):
                    owner.snapshot(root)

    def test_pipeline_diagnostic_requires_output_and_complete_trace(self):
        xsht = self.fixtures / 'xsht'
        xsht.write_text('fixture')
        status = {'kind': 'exit', 'code': 0, 'success': True}
        rows = []
        for identity in (3, 7):
            rows.append({'kind': 'pipeline.enter', 'event_id': identity, 'payload': {'type': 'none'}})
            for index in (0, 1):
                rows.append({'kind': 'pipeline.segment.end', 'event_id': identity + index + 1, 'parent_event_id': identity,
                             'payload': {'type': 'pipeline.segment.end', 'index': index, 'pid': identity * 10 + index,
                                         'error': None, 'status': status}})
            rows.append({'kind': 'pipeline.exit', 'event_id': identity + 3, 'parent_event_id': identity,
                         'payload': {'type': 'pipeline.end', 'error': None, 'status': status}})
        rows.append({'type': 'syscall.summary', 'syscall_count': 17})

        def invoke(command, cwd, environment, timeout, on_launch, on_cleanup_verified):
            on_launch()
            Path(command[command.index('--trace-file') + 1]).write_text(''.join(json.dumps(row) + '\n' for row in rows))
            on_cleanup_verified()
            return {'status': 0, 'timed_out': False, 'effects': {},
                    'stdout_base64': base64.b64encode(b'probe 0\nprobe 1\n').decode(), 'stderr_base64': ''}

        lease = SimpleNamespace(prepare_cleanup_unresolved=lambda _: None, resolve_cleanup_unresolved=lambda: None)
        with patch.object(core_trace.run, 'invoke', side_effect=invoke):
            self.assertEqual(core_trace.check_case(xsht, self.fixtures, lease)['subprocesses'], 4)
            rows.pop(1)
            with self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
                core_trace.check_case(xsht, self.fixtures, lease)

    def test_pipeline_diagnostic_uses_selected_shape(self):
        xsht = self.fixtures / 'xsht'
        xsht.write_text('fixture')
        status = {'kind': 'exit', 'code': 0, 'success': True}
        rows = []
        for identity in range(1, 33, 4):
            rows.append({'kind': 'pipeline.enter', 'event_id': identity, 'payload': {'type': 'none'}})
            for index in (0, 1):
                rows.append({'kind': 'pipeline.segment.end', 'event_id': identity + index + 1, 'parent_event_id': identity,
                             'payload': {'type': 'pipeline.segment.end', 'index': index, 'pid': identity * 10 + index,
                                         'error': None, 'status': status}})
            rows.append({'kind': 'pipeline.exit', 'event_id': identity + 3, 'parent_event_id': identity,
                         'payload': {'type': 'pipeline.end', 'error': None, 'status': status}})
        rows.append({'type': 'syscall.summary', 'syscall_count': 17})
        def invoke(command, cwd, environment, timeout, on_launch, on_cleanup_verified):
            self.assertEqual(command[-1], '8')
            on_launch()
            Path(command[command.index('--trace-file') + 1]).write_text(''.join(json.dumps(row) + '\n' for row in rows))
            on_cleanup_verified()
            return {'status': 0, 'timed_out': False, 'effects': {},
                    'stdout_base64': base64.b64encode(b''.join(f'probe {i}\n'.encode() for i in range(8))).decode(),
                    'stderr_base64': ''}
        lease = SimpleNamespace(prepare_cleanup_unresolved=lambda _: None, resolve_cleanup_unresolved=lambda: None)
        with patch.object(core_trace.run, 'invoke', side_effect=invoke):
            self.assertEqual(core_trace.check_case(xsht, self.fixtures, lease, 'core-pipeline-medium')['subprocesses'], 16)
            rows.pop(1)
            with self.assertRaisesRegex(ValueError, 'helper attribution unavailable'):
                core_trace.check_case(xsht, self.fixtures, lease, 'core-pipeline-medium')

    def test_sort_trace_requires_exact_output_and_complete_summary(self):
        xsht = self.fixtures / 'xsht'
        xsht.write_text('fixture')
        rows = [{'type': 'syscall.summary', 'syscall_count': 17}]
        case = core_cases.describe_case('core-sort-small', fixtures=self.fixtures, source_root=core_cases.SOURCE_ROOT)
        def invoke(command, cwd, environment, timeout, on_launch, on_cleanup_verified):
            on_launch()
            Path(command[command.index('--trace-file') + 1]).write_text(''.join(json.dumps(row) + '\n' for row in rows))
            on_cleanup_verified()
            return {'status': 0, 'timed_out': False, 'effects': {},
                    'stdout_base64': base64.b64encode(case.expected_stdout).decode(), 'stderr_base64': ''}
        lease = SimpleNamespace(prepare_cleanup_unresolved=lambda _: None, resolve_cleanup_unresolved=lambda: None)
        with patch.object(core_trace.run, 'invoke', side_effect=invoke):
            self.assertEqual(core_trace.check_case(xsht, self.fixtures, lease, 'core-sort-small')['subprocesses'], 0)
            argument = {'display': 'x', 'hex': '78'}
            rows[:0] = [{'kind': 'run.start', 'event_id': 1,
                         'payload': {'type': 'run.start', 'target': argument, 'argv': [], 'cwd': argument, 'env': []}},
                        {'kind': 'run.end', 'event_id': 2, 'parent_event_id': 1,
                         'payload': {'type': 'run.end', 'pid': 3, 'status': {'kind': 'exit', 'code': 0, 'success': True}, 'error': None}}]
            with self.assertRaisesRegex(core_trace.WorkloadFailure, 'helper count'):
                core_trace.check_case(xsht, self.fixtures, lease, 'core-sort-small')
            rows.clear()
            with self.assertRaises(ValueError):
                core_trace.check_case(xsht, self.fixtures, lease, 'core-sort-small')

    def test_pipeline_diagnostic_rejects_unsupported_host_before_quarantine(self):
        xsht = self.fixtures / 'xsht'
        xsht.write_text('fixture')
        marks = []
        lease = SimpleNamespace(prepare_cleanup_unresolved=lambda message: marks.append(message))
        with patch.object(core_trace.sys, 'platform', 'darwin'):
            with self.assertRaisesRegex(ValueError, 'Linux pidfd'):
                core_trace.check_case(xsht, self.fixtures, lease)
        self.assertEqual(marks, [])

    def test_pipeline_diagnostic_preserves_trace_failure_and_cancellation(self):
        xsht = self.fixtures / 'xsht'
        xsht.write_text('fixture')
        lease = SimpleNamespace(prepare_cleanup_unresolved=lambda _: None, resolve_cleanup_unresolved=lambda: None)
        def invoke(command, cwd, environment, timeout, on_launch, on_cleanup_verified):
            on_launch()
            on_cleanup_verified()
            return {'status': raw, 'timed_out': False, 'effects': {}, 'stdout_base64': '',
                    'stderr_base64': base64.b64encode(b'trace unavailable').decode()}
        with patch.object(core_trace.run, 'invoke', side_effect=invoke):
            raw = 2
            with self.assertRaisesRegex(core_trace.WorkloadFailure, 'status=2.*trace unavailable'):
                core_trace.check_case(xsht, self.fixtures, lease)
            raw = 1
            with self.assertRaisesRegex(core_trace.WorkloadFailure, 'status=1.*trace unavailable'):
                core_trace.check_case(xsht, self.fixtures, lease)
            raw = 130
            with self.assertRaisesRegex(core_trace.WorkloadFailure, 'status=130'):
                core_trace.check_case(xsht, self.fixtures, lease)

    def test_pipeline_treats_unfinished_timeout_as_incomplete(self):
        xsht = self.fixtures / 'xsht'
        xsht.write_text('fixture')
        lease = SimpleNamespace(prepare_cleanup_unresolved=lambda _: None, resolve_cleanup_unresolved=lambda: None)
        row = {'status': 0, 'timed_out': True, 'effects': {}, 'stdout_base64': '', 'stderr_base64': ''}
        with patch.object(core_trace.run, 'invoke', return_value=row):
            with self.assertRaises(ValueError) as error:
                core_trace.check_case(xsht, self.fixtures, lease)
        self.assertIs(type(error.exception), ValueError)

    def test_pipeline_controller_reports_termination_as_cancellation(self):
        class LeaseBusy(Exception):
            pass
        lease = SimpleNamespace(campaign_leases=lambda *args, **kwargs: nullcontext(), LeaseBusy=LeaseBusy)
        argv = ['pipeline', '--xsht', '/xsht', '--scratch', '/scratch']
        for cancellation in (SystemExit(143), owned_case.check.owned.SignalReceived(signal.SIGTERM)):
            with self.subTest(cancellation=cancellation), patch.object(core_trace.sys, 'argv', argv), \
                    patch.object(owned_case.check, 'baseline_owner', return_value=lease), \
                    patch.object(core_trace, 'check_case', side_effect=cancellation), redirect_stdout(io.StringIO()):
                self.assertEqual(core_trace.main(), 130)

    def test_pipeline_controller_preserves_child_status_when_cleanup_is_unresolved(self):
        class LeaseBusy(Exception):
            pass
        lease = SimpleNamespace(campaign_leases=lambda *args, **kwargs: nullcontext(), LeaseBusy=LeaseBusy)
        argv = ['pipeline', '--xsht', '/xsht', '--scratch', '/scratch']
        for raw, expected in ((1, 1), (2, 1), (130, 1), (-9, 2), (-11, 1), (-15, 1), (None, 2)):
            error = core_trace.run.ProcessCleanupUnresolved('cleanup unresolved', raw)
            output = io.StringIO()
            with self.subTest(raw=raw), patch.object(core_trace.sys, 'argv', argv), \
                    patch.object(owned_case.check, 'baseline_owner', return_value=lease), \
                    patch.object(core_trace, 'check_case', side_effect=error), redirect_stdout(output):
                self.assertEqual(core_trace.main(), expected)
            self.assertIn('cleanup unresolved', output.getvalue())

    def test_pipeline_controller_defers_signal_during_handler_installation(self):
        install = owned_case.check.owned.install_signal_handlers
        def install_then_signal():
            previous = install()
            os.kill(os.getpid(), signal.SIGTERM)
            return previous
        argv = ['pipeline', '--xsht', '/xsht', '--scratch', '/scratch']
        with patch.object(core_trace.sys, 'argv', argv), \
                patch.object(owned_case.check.owned, 'install_signal_handlers', side_effect=install_then_signal), \
                patch.object(owned_case.check, 'baseline_owner', side_effect=AssertionError('lease should not be loaded')), \
                redirect_stdout(io.StringIO()):
            self.assertEqual(core_trace.main(), 130)

    def test_pipeline_controller_catches_signal_entering_handler_restoration(self):
        class LeaseBusy(Exception):
            pass
        lease = SimpleNamespace(campaign_leases=lambda *args, **kwargs: nullcontext(), LeaseBusy=LeaseBusy)
        previous = {number: signal.getsignal(number) for number in (signal.SIGINT, signal.SIGTERM)}
        real_mask = signal.pthread_sigmask
        blocks = []
        def mask(how, values):
            if how == signal.SIG_BLOCK:
                blocks.append(True)
                if len(blocks) == 2:
                    os.kill(os.getpid(), signal.SIGTERM)
            return real_mask(how, values)
        argv = ['pipeline', '--xsht', '/xsht', '--scratch', '/scratch']
        try:
            with patch.object(core_trace.sys, 'argv', argv), \
                    patch.object(owned_case.check, 'baseline_owner', return_value=lease), \
                    patch.object(core_trace, 'check_case', return_value={'subprocesses': 4, 'syscalls': 17}), \
                    patch.object(owned_case.signal, 'pthread_sigmask', side_effect=mask), redirect_stdout(io.StringIO()):
                self.assertEqual(core_trace.main(), 130)
                self.assertEqual({number: signal.getsignal(number) for number in previous}, previous)
        finally:
            for number, handler in previous.items():
                signal.signal(number, handler)

    def test_each_family_scales_useful_work_and_keeps_exact_oracles(self):
        for family in ('wc', 'sort', 'awk', 'traversal', 'pipeline'):
            previous = None
            for shape in ('small', 'medium', 'large'):
                with self.subTest(family=family, shape=shape):
                    case = self.prepare(f'core-{family}-{shape}')
                    core_cases.validate_case(case, fixtures=self.fixtures)
                    counts = core_cases.work_counts(case)
                    self.assertTrue(case.expected_stdout.endswith(b'\n'))
                    self.assertEqual(case.useful_stdout_prefix, case.expected_stdout.splitlines(keepends=True)[0])
                    if previous:
                        self.assertTrue(all(counts[key] > previous[key] for key in counts))
                    previous = counts

    def test_wc_reads_binary_lines_instead_of_metadata_only_size(self):
        case = self.prepare('core-wc-small')
        data = (self.fixtures / case.name / 'input').read_bytes()
        self.assertIn(b'\x00\xff', data)
        self.assertEqual(len(data), 65536)
        self.assertEqual(case.arguments, ('--lines', str(self.fixtures / case.name / 'input')))
        self.assertEqual(case.expected_stdout, f'{data.count(bytes([10]))} {case.arguments[1]}\n'.encode())

    def test_sort_oracle_preserves_all_records_and_orders_numeric_keys(self):
        case = self.prepare('core-sort-small')
        original = (self.fixtures / case.name / 'input').read_bytes().splitlines()
        output = case.expected_stdout.splitlines()
        self.assertCountEqual(output, original)
        self.assertNotEqual(output, original)
        self.assertEqual([int(row.split(b'\t')[1]) for row in output], list(range(256)))

    def test_awk_oracle_observes_all_records_and_array_groups(self):
        case = self.prepare('core-awk-small')
        self.assertEqual(case.expected_stdout, b'256 32640 20 19\n')
        self.assertIn('groups[$3]++', case.arguments[0])
        self.assertIn('NR', case.arguments[0])

    def test_traversal_oracle_requires_hidden_entries_metadata_and_unfollowed_links(self):
        case = self.prepare('core-traversal-small')
        self.assertIn(b'.hidden-0001/file 000016.txt\tfile\t11\n', case.expected_stdout)
        self.assertIn(b'link-parent\tlink\t-\n', case.expected_stdout)
        self.assertIn(b'link-dangling\tlink\t-\n', case.expected_stdout)
        self.assertIn(b'.gitignore\tfile\t6\n', case.expected_stdout)
        self.assertEqual(len(case.expected_stdout.splitlines()), core_cases.work_counts(case)['completed_entries'])
        self.assertEqual(core_cases.work_counts(case)['regular_files'], 33)

    def test_pipeline_counts_total_starts_and_completed_output(self):
        case = self.prepare('core-pipeline-small')
        self.assertEqual(case.expected_stdout, b'probe 0\nprobe 1\n')
        self.assertEqual(case.expected_subprocesses, 4)
        self.assertEqual(core_cases.work_counts(case), {'completed_pipelines': 2, 'subprocess_starts': 4, 'completed_records': 2})

    def test_corrupt_inputs_or_oracles_and_extra_cleanup_artifacts_reject(self):
        case = self.prepare('core-awk-small')
        root = self.fixtures / case.name
        with self.assertRaisesRegex(ValueError, 'descriptor'):
            core_cases.validate_case(replace(case, expected_stdout=b'wrong\n'), fixtures=self.fixtures)
        (root / 'input').write_bytes(b'corrupt\n')
        with self.assertRaisesRegex(ValueError, 'file changed'):
            core_cases.validate_case(case, fixtures=self.fixtures)
        empty = self.prepare('core-pipeline-small')
        (self.fixtures / empty.name / 'leftover').write_bytes(b'unexpected')
        with self.assertRaisesRegex(ValueError, 'file changed'):
            core_cases.validate_case(empty, fixtures=self.fixtures)

    def test_changed_links_and_linked_fixture_parents_reject(self):
        case = self.prepare('core-traversal-small')
        link = self.fixtures / case.name / 'tree/link-parent'
        link.unlink()
        link.symlink_to(self.fixtures)
        with self.assertRaisesRegex(ValueError, 'link changed'):
            core_cases.validate_case(case, fixtures=self.fixtures)
        parent = self.fixtures / 'alias'
        parent.symlink_to(self.fixtures, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'nonsymlink'):
            core_cases.prepare_case('core-wc-small', fixtures=parent)

    def test_unknown_shapes_and_overwrites_fail_before_changing_fixtures(self):
        for name in ('../escape', 'core-wc-huge', 'core-cat-small', 'core-wc-small/escape'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.prepare(name)
        self.assertEqual(list(self.fixtures.iterdir()), [])
        case = self.prepare('core-wc-small')
        with self.assertRaises(FileExistsError):
            self.prepare(case.name)
        core_cases.validate_case(case, fixtures=self.fixtures)


if __name__ == '__main__':
    unittest.main()
