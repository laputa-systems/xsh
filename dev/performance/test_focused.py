import base64
from contextlib import contextmanager, redirect_stderr, redirect_stdout
from dataclasses import asdict, dataclass, replace
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

from bench.consolidation import fixtures
from dev.performance import check, focused, resources, workloads


@dataclass(frozen=True)
class _DiagnosticWindow:
    scope: resources.GroupScope
    telemetry_complete: bool
    declared_workers: tuple[tuple[str, int], ...]
    cpu_usage_ns: int | resources.Unavailable
    pressure_stall_us: dict[str, dict[str, int | resources.Unavailable]]


def _resource_observation(*, complete=True, event_deltas=None, violations=None):
    missing = resources.Unavailable(resources.Availability.UNSUPPORTED, 'counter unavailable')
    scope = resources.GroupScope.FRESH_CASE_CONTAINER.value
    events = {'oom': 0, 'oom_kill': 0, 'oom_group_kill': 0} if event_deltas is None else event_deltas
    expected_violations = [
        f'memory.events {name} increased by {events[name]}'
        for name in ('oom', 'oom_kill', 'oom_group_kill')
        if type(events.get(name)) is int and events[name] > 0
    ]
    return {
        'scope': scope,
        'identity': {'device': 1, 'inode': 2, 'namespace_inode': 3, 'membership': '/case'},
        'memory_events_hierarchical': True if complete else False,
        'pressure_accounting_enabled': True if complete else missing.__dict__,
        'memory_peak_sample_bytes': 65536,
        'cpu_usage_ns': 12000 if complete else missing.__dict__,
        'cpu_throttled_ns': 0,
        'cpu_throttled_periods': 0,
        'cpu_max': '200000 100000',
        'memory_max': '4194304',
        'cpuset_cpus_effective': '0-3',
        'memory_events_delta': events,
        'pressure_stall_us': {
            'cpu': {'some': 0, 'full': 0},
            'memory': missing.__dict__ if not complete else {'some': 0, 'full': 0},
            'io': {'some': 0, 'full': 0},
        },
        'declared_workers': [['container_init', 1], ['python_driver', 1], ['xsh_workload', 1]],
        'observed_cgroup_tasks_peak': 3,
        'observed_cgroup_tasks_current': 2,
        'observer_peak_rss_bytes': 8192,
        'memory_stat_boundary': {'anon': 4096, 'file': 2048},
        'violations': expected_violations if violations is None else list(violations),
        'telemetry_complete': complete,
    }


def _resource_diagnostic(*, complete=True, event_deltas=None, violations=None):
    return {
        'schema': 1,
        'authority': 'diagnostic_only',
        'scope': resources.GroupScope.FRESH_CASE_CONTAINER.value,
        'status': 'complete' if complete else 'incomplete',
        'observation': _resource_observation(
            complete=complete,
            event_deltas=event_deltas,
            violations=violations,
        ),
    }


class FocusedTests(unittest.TestCase):
    TEST_IMAGE_ID = 'sha256:' + 'a' * 64

    def setUp(self):
        (check.ADAPTER_ROOT / '.work/tmp').mkdir(parents=True, exist_ok=True)
        self.lease_calls = []
        self.lease_held = False
        self.cleanup_quarantined = False
        self.prepare_error = None
        self.resolve_error = None
        self.lease_busy = None

        class LeaseBusy(Exception):
            pass

        @contextmanager
        def campaign_leases(root, *, native=False):
            self.lease_calls.append(('campaign', root, native))
            if self.lease_busy is not None:
                raise self.lease_busy
            self.lease_held = True
            try:
                yield
            finally:
                self.lease_held = False

        def require_native_lease():
            self.assertTrue(self.lease_held)
            self.lease_calls.append(('required',))

        def prepare_cleanup_unresolved(detail):
            self.assertTrue(self.lease_held)
            self.lease_calls.append(('prepare', detail))
            self.cleanup_quarantined = True
            if self.prepare_error is not None:
                raise self.prepare_error

        def resolve_cleanup_unresolved():
            self.assertTrue(self.lease_held)
            self.assertTrue(self.cleanup_quarantined)
            self.lease_calls.append(('resolve',))
            if self.resolve_error is not None:
                raise self.resolve_error
            self.cleanup_quarantined = False

        self.lease_owner = SimpleNamespace(
            LeaseBusy=LeaseBusy,
            require_native_lease=require_native_lease,
            prepare_cleanup_unresolved=prepare_cleanup_unresolved,
            resolve_cleanup_unresolved=resolve_cleanup_unresolved,
            campaign_leases=campaign_leases,
        )
        self.lease_patch = patch.object(focused, '_leases', return_value=self.lease_owner)
        self.lease_patch.start()
        self.addCleanup(self.lease_patch.stop)

    def run_driver(self, validation):
        events = []
        snapshots = iter((object(), object()))
        workload = {
            'status': 0,
            'timed_out': False,
            'stderr_base64': '',
            'effects': {},
            'stdout_base64': base64.b64encode(b'case output\n').decode(),
        }
        run = ModuleType('run')

        def invoke(argv, work, environment, timeout):
            events.append(('run', argv, work, timeout))
            return workload

        def capture():
            events.append(('capture',))
            return next(snapshots)

        def validate(before, after, *, declared_workers, scope):
            events.append(('validate', before, after, declared_workers, scope))
            if isinstance(validation, BaseException):
                raise validation
            return validation

        run.invoke = invoke
        old_path = sys.path[:]
        output = io.StringIO()
        try:
            with patch.object(resources, 'capture_current_cgroup', side_effect=capture), \
                    patch.object(resources, 'validate_case_window', side_effect=validate), \
                    patch.object(sys, 'argv', ['focused-driver', 'work', 'xsh', 'arg']), \
                    patch.dict(sys.modules, {'run': run}), redirect_stdout(output):
                exec(compile(focused._DRIVER_SOURCE, '<focused-driver>', 'exec'), {'__name__': '__main__'})
        finally:
            sys.path[:] = old_path
        return events, workload, json.loads(output.getvalue())

    @contextmanager
    def config(self):
        with tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
            root = Path(directory)
            source = root / 'source'
            source.mkdir()
            subprocess.run(['git', 'init', '-q', str(source)], check=True)
            script = source / 'bench/consolidation/xsh/json.xsh'
            script.parent.mkdir(parents=True)
            script.write_text('print "committed workload"\n')
            (source / 'bench/consolidation/fixtures.py').write_bytes(Path(fixtures.__file__).read_bytes())
            subprocess.run(['git', '-C', str(source), 'add', '.'], check=True)
            subprocess.run([
                'git', '-C', str(source), '-c', 'user.name=Focused Test', '-c',
                'user.email=focused@example.invalid', 'commit', '-qm', 'initial',
            ], check=True)
            commit = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
            tree = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD^{tree}'], text=True).strip()
            script.write_text('print "working tree workload"\n')
            scratch = root / 'scratch'
            scratch.mkdir()
            fixture_root = root / 'fixtures'
            fixtures.generate(fixture_root / '2048')
            product = root / 'xsh'
            product.write_bytes(b'prebuilt product')
            product.chmod(0o755)
            config = focused._FocusedConfig(
                source_root=source,
                commit=commit,
                tree=tree,
                xsh=product,
                scratch=scratch,
                fixtures=fixture_root,
                output=scratch / 'report.json',
                image_id=self.TEST_IMAGE_ID,
                source_digest='c' * 64,
                product_digest=check.owned_digest(product),
                product_mode=0o755,
            )
            yield config

    @staticmethod
    def config_value(config):
        return {
            'schema': 1,
            'image_id': config.image_id,
            'source_root': str(config.source_root),
            'commit': config.commit,
            'tree': config.tree,
            'xsh': str(config.xsh),
            'scratch': str(config.scratch),
            'measurement': {
                'fixtures': str(config.fixtures),
                'output': str(config.output),
            },
            'gate_binding': {
                'source_digest': config.source_digest,
                'product_digest': config.product_digest,
                'product_mode': config.product_mode,
            },
        }

    def invoke(self, config, *, observation=None, resource_diagnostics=None, cleanup_failure=False,
               interruption=None, mutate=None, gate_digests=None):
        calls = []

        def launch(command, scratch, *, timeout_seconds, on_cleanup_verified=None):
            calls.append(command)
            self.assertIn('--network', command)
            self.assertIn('none', command)
            self.assertEqual(timeout_seconds, 120)
            self.assertEqual(scratch, config.scratch)
            self.assertTrue(self.lease_held)
            self.assertTrue(self.cleanup_quarantined)
            if cleanup_failure:
                self.assertTrue(callable(on_cleanup_verified))
                raise check.CheckError('owned container cleanup unresolved')
            if interruption is not None:
                raise interruption
            row = {
                'status': 0,
                'timed_out': False,
                'stderr_base64': '',
                'effects': {},
                'stdout_base64': base64.b64encode(
                    (config.fixtures / '2048/expected/json').read_bytes(),
                ).decode(),
            }
            row.update(observation or {})
            diagnostics = resource_diagnostics or _resource_diagnostic()
            mounts = [command[index + 1] for index, value in enumerate(command[:-1]) if value == '--mount']
            writable = [mount for mount in mounts if not mount.endswith(',readonly')]
            self.assertEqual(len(writable), 1)
            work_mount = writable[0]
            self.assertTrue(work_mount.startswith('type=bind,src='))
            work_path = work_mount.removeprefix('type=bind,src=').split(',dst=', 1)[0]
            self.assertEqual(Path(work_path).name, 'work')
            self.assertNotIn(f'type=bind,src={config.scratch},dst={config.scratch}', mounts)
            self.assertIn(f'type=bind,src={config.xsh},dst={config.xsh},readonly', mounts)
            self.assertNotIn('-v', command)
            self.assertIn(config.image_id, command)
            self.assertIn('run.invoke', command[command.index('-c') + 1])
            if mutate:
                mutate()
            self.assertTrue(callable(on_cleanup_verified))
            on_cleanup_verified()
            return 0, json.dumps({'workload': row, 'resource_diagnostics': diagnostics}).encode(), b''

        with patch.object(check, 'baseline_owner', return_value=fixtures), \
                patch.object(focused, '_admission', return_value={'command': ['perf-gate-ready'], 'exit_code': 0,
                                                                   'stdout': '', 'stderr': ''}), \
                patch.object(focused, '_gate_source_digest',
                             side_effect=gate_digests or [config.source_digest] * 2), \
                patch.object(focused, '_verify_image_id', side_effect=lambda image_id: self.lease_calls.append(('image', image_id))), \
                patch.object(check, 'load_canonical', side_effect=AssertionError('canonical owner must not be loaded')), \
                patch.object(check, 'checked_docker', side_effect=launch), \
                patch.object(check, 'qualify_products', side_effect=AssertionError('full product gate called')), \
                patch.object(check, 'baseline_util_linux', side_effect=AssertionError('live CloseReport gate called')):
            with self.lease_owner.campaign_leases(config.source_root, native=True):
                report = focused._qualify(config, 'json-n', self.lease_owner)
        return report, calls

    def test_workload_selection_preserves_the_default_cohort(self):
        expected_names = (
            'startup-trivial', 'startup-template',
            'stream-count-n', 'stream-map-where-n',
            'stream-count-2n', 'stream-map-where-2n',
            'stream-count-4n', 'stream-map-where-4n',
            'json-n', 'directory-n', 'json-2n', 'directory-2n',
            'json-4n', 'directory-4n', 'spawn-128',
            'json-package-range-16384', 'directory-package-range-16384',
            'json-package-range-32768', 'directory-package-range-32768',
            'json-package-range-65536', 'directory-package-range-65536',
        )
        with tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
            fixtures_root = Path(directory)
            for count in fixtures.PACKAGE_ROW_SHAPES + fixtures.DEMANDING_PACKAGE_ROW_SHAPES:
                expected = fixtures_root / str(count) / 'expected'
                expected.mkdir(parents=True)
                (expected / 'json').write_bytes(b'[]\n')
                (expected / 'directory').write_bytes(b'')
            all_cases = workloads.frozen_workloads(fixtures_root)
            self.assertEqual(tuple(case.name for case in all_cases), expected_names)
            selected = workloads.frozen_workloads(fixtures_root, selected=('json-n',), source_root=Path('/source'))
            self.assertEqual(tuple(case.name for case in selected), ('json-n',))
            self.assertEqual(selected[0].script, Path('/source/bench/consolidation/xsh/json.xsh'))
            self.assertEqual(selected[0].arguments, (str(fixtures_root / '2048'),))
            with self.assertRaisesRegex(ValueError, 'unknown selected workload'):
                workloads.frozen_workloads(fixtures_root, selected=('unknown',))
            with self.assertRaisesRegex(ValueError, 'nonempty'):
                workloads.frozen_workloads(fixtures_root, selected=())

    def test_config_requires_explicit_full_immutable_image_digest(self):
        with self.config() as config:
            value = self.config_value(config)
            self.assertEqual(focused._parse_config(value).image_id, self.TEST_IMAGE_ID)
            missing = dict(value)
            del missing['image_id']
            with self.assertRaisesRegex(check.CheckError, 'focused config: expected fields'):
                focused._parse_config(missing)
            for image_id in ('pyramid:latest', 'sha256:abc', 'sha256:' + 'A' * 64):
                with self.subTest(image_id=image_id):
                    tagged = dict(value)
                    tagged['image_id'] = image_id
                    with self.assertRaisesRegex(check.CheckError, 'explicit full sha256 digest'):
                        focused._parse_config(tagged)
            special_mode = self.config_value(config)
            special_mode['gate_binding']['product_mode'] = 0o4755
            self.assertEqual(focused._parse_config(special_mode).product_mode, 0o4755)
            for name, invalid in (
                ('source_digest', 'tag'),
                ('product_digest', '0' * 63),
                ('product_mode', True),
                ('product_mode', 0o10000),
            ):
                with self.subTest(binding=name, value=invalid):
                    malformed = self.config_value(config)
                    malformed['gate_binding'][name] = invalid
                    with self.assertRaises(check.CheckError):
                        focused._parse_config(malformed)

    def test_gate_source_and_product_binding_must_match_before_container_launch(self):
        for field in ('source_digest', 'product_digest', 'product_mode'):
            with self.subTest(field=field), self.config() as config:
                source_digest = 'd' * 64 if field == 'source_digest' else config.source_digest
                message = 'source differs from gate selection' if field == 'source_digest' \
                    else 'product differs from gate preparation'
                with patch.object(focused, '_gate_source_digest', return_value=source_digest), \
                        patch.object(focused, '_product_snapshot',
                                     return_value={'record': {'sha256': 'd' * 64, 'mode': 0o700},
                                                   'identity': (1, 2, 3)}), \
                        patch.object(check, 'baseline_owner', return_value=fixtures), \
                        patch.object(focused, '_admission', return_value={'command': ['perf-gate-ready'],
                                                                           'exit_code': 0, 'stdout': '',
                                                                           'stderr': ''}), \
                        patch.object(focused, '_verify_image_id'), \
                        patch.object(check, 'checked_docker') as launch, \
                        self.lease_owner.campaign_leases(config.source_root, native=True), \
                        self.assertRaisesRegex(check.CheckError, message):
                    focused._qualify(config, 'json-n', self.lease_owner)
                launch.assert_not_called()

    def test_docker_resolved_local_image_id_must_match_before_container_launch(self):
        identity = SimpleNamespace(returncode=0, stdout=self.TEST_IMAGE_ID + '\n', stderr='')
        for declared_volumes in ('null\n', '{}\n'):
            with self.subTest(declared_volumes=declared_volumes), patch.object(
                    focused.subprocess, 'run', side_effect=[identity,
                        SimpleNamespace(returncode=0, stdout=declared_volumes, stderr='')]) as inspect:
                focused._verify_image_id(self.TEST_IMAGE_ID)
            self.assertEqual(inspect.call_args_list[0].args[0], [
                'docker', 'image', 'inspect', '--format={{.Id}}', self.TEST_IMAGE_ID,
            ])
            self.assertEqual(inspect.call_args_list[1].args[0], [
                'docker', 'image', 'inspect', '--format={{json .Config.Volumes}}', self.TEST_IMAGE_ID,
            ])
        with patch.object(focused.subprocess, 'run', return_value=SimpleNamespace(
                returncode=0, stdout='sha256:' + 'b' * 64 + '\n', stderr='')):
            with self.assertRaisesRegex(check.CheckError, 'different focused image ID'):
                focused._verify_image_id(self.TEST_IMAGE_ID)

    def test_declared_image_volumes_reject_before_container_launch(self):
        with self.config() as config:
            work = config.scratch / 'work'
            work.mkdir()
            case = SimpleNamespace(name='json-n', expected_stdout=b'', argv=lambda product: [str(product)])
            image_identity = SimpleNamespace(returncode=0, stdout=self.TEST_IMAGE_ID + '\n', stderr='')
            volume_declaration = SimpleNamespace(returncode=0, stdout='{"persisted":{}}\n', stderr='')
            with self.lease_owner.campaign_leases(config.source_root, native=True), \
                    patch.object(focused.subprocess, 'run', side_effect=[image_identity, volume_declaration]), \
                    patch.object(check, 'checked_docker') as launch, \
                    self.assertRaisesRegex(check.CheckError, 'declares writable volumes'):
                focused._observe(config, case, work, self.lease_owner)
            launch.assert_not_called()
            self.assertFalse(self.cleanup_quarantined)

    def test_bind_mount_paths_reject_unencoded_delimiters_before_effects(self):
        with self.config() as config:
            for field, prefix in (
                ('source_root', None),
                ('xsh', None),
                ('scratch', None),
                ('measurement', 'fixtures'),
            ):
                for delimiter in (':', ','):
                    with self.subTest(field=field, delimiter=delimiter):
                        value = self.config_value(config)
                        if field == 'measurement':
                            value[field][prefix] += delimiter + 'bad'
                        else:
                            value[field] += delimiter + 'bad'
                        with self.assertRaisesRegex(check.CheckError, 'unsupported delimiter'):
                            focused._parse_config(value)
            config_path = config.scratch / 'config.json'
            value = self.config_value(config)
            value['source_root'] += ':bad'
            config_path.write_text(json.dumps(value))
            with patch.object(focused, '_leases', side_effect=AssertionError('lease effect ran')), \
                    patch.object(focused, '_admission', side_effect=AssertionError('admission effect ran')), \
                    patch.object(focused, '_verify_image_id', side_effect=AssertionError('Docker effect ran')):
                stderr = io.StringIO()
                with redirect_stderr(stderr):
                    status = focused.main(['--config', str(config_path), '--case', 'json-n'])
            self.assertEqual(status, 2)
            self.assertIn('unsupported delimiter', stderr.getvalue())
            with self.assertRaisesRegex(check.CheckError, 'unsupported delimiter'):
                focused._mount_spec(Path('/path/with,comma'), True)

    def test_dirty_workload_qualifies_only_its_behavior_and_labels_limits(self):
        with self.config() as config:
            report, calls = self.invoke(config)
            self.assertEqual(len(calls), 1)
            self.assertEqual(self.lease_calls, [
                ('campaign', config.source_root, True),
                ('required',),
                ('image', config.image_id),
                ('prepare', 'focused json-n: container launch in progress'),
                ('resolve',),
            ])
            self.assertTrue(report['correctness_complete'])
            self.assertTrue(report['cleanup_complete'])
            self.assertFalse(report['full_cohort_run'])
            self.assertFalse(report['performance_complete'])
            self.assertFalse(report['timing_complete'])
            self.assertEqual(report['product_source_binding'], 'gate-prepared-product-bound-to-source-snapshot')
            self.assertEqual(report['gate_binding'], {
                'source_digest': config.source_digest,
                'product_digest': config.product_digest,
                'product_mode': config.product_mode,
            })
            self.assertEqual(report['performance_status'], 'not-measured')
            self.assertIn('bench/consolidation/xsh/json.xsh', report['tested_source'])
            self.assertEqual(report['product']['sha256'], check.owned_digest(config.xsh))
            self.assertEqual(report['source_commit'], config.commit)
            self.assertEqual(report['source_tree'], config.tree)
            self.assertEqual(report['image_id'], config.image_id)
            self.assertEqual(check.read_json(config.output), report)
            self.assertEqual(list(config.scratch.iterdir()), [config.output])

    def test_container_driver_brackets_workload_and_keeps_unavailable_values_explicit(self):
        window = _DiagnosticWindow(
            scope=resources.GroupScope.FRESH_CASE_CONTAINER,
            telemetry_complete=False,
            declared_workers=(('container_init', 1), ('python_driver', 1), ('xsh_workload', 1)),
            cpu_usage_ns=resources.Unavailable(resources.Availability.UNSUPPORTED, 'cpu.stat unavailable'),
            pressure_stall_us={
                'memory': {'some': resources.Unavailable(resources.Availability.INCOMPLETE, 'partial PSI rows')},
            },
        )
        events, workload, envelope = self.run_driver(window)
        self.assertEqual([event[0] for event in events], ['capture', 'run', 'capture', 'validate'])
        self.assertEqual(events[1][1:], (['xsh', 'arg'], Path('work'), 60))
        self.assertEqual(events[3][3], {
            'container_init': 1,
            'python_driver': 1,
            'xsh_workload': 1,
        })
        self.assertIs(events[3][4], resources.GroupScope.FRESH_CASE_CONTAINER)
        self.assertEqual(envelope['workload'], workload)
        diagnostic = envelope['resource_diagnostics']
        self.assertEqual(diagnostic['authority'], 'diagnostic_only')
        self.assertEqual(diagnostic['scope'], focused._RESOURCE_SCOPE)
        self.assertEqual(diagnostic['status'], 'incomplete')
        self.assertEqual(diagnostic['observation']['cpu_usage_ns'], {
            'availability': 'unsupported',
            'reason': 'cpu.stat unavailable',
        })
        self.assertEqual(diagnostic['observation']['pressure_stall_us']['memory']['some'], {
            'availability': 'incomplete',
            'reason': 'partial PSI rows',
        })

    def test_identity_failure_is_diagnostic_and_does_not_discard_workload_result(self):
        events, workload, envelope = self.run_driver(
            resources.ResourceWindowError('cgroup membership changed during case'),
        )
        self.assertEqual([event[0] for event in events], ['capture', 'run', 'capture', 'validate'])
        self.assertEqual(envelope['workload'], workload)
        diagnostic = envelope['resource_diagnostics']
        self.assertEqual(diagnostic['authority'], 'diagnostic_only')
        self.assertEqual(diagnostic['status'], 'incomplete')
        self.assertEqual(diagnostic['reason'], 'cgroup membership changed during case')
        self.assertNotIn('observation', diagnostic)

    def test_incomplete_resource_diagnostics_do_not_gate_correctness_or_claim_performance(self):
        diagnostics = _resource_diagnostic(complete=False)
        with self.config() as config:
            report, _ = self.invoke(config, resource_diagnostics=diagnostics)
            self.assertTrue(report['correctness_complete'])
            self.assertFalse(report['performance_complete'])
            self.assertFalse(report['timing_complete'])
            self.assertEqual(report['resource_diagnostics'], diagnostics)
            self.assertEqual(check.read_json(config.output), report)

    def test_malformed_resource_diagnostics_are_reported_incomplete_without_losing_correctness(self):
        malformed = {
            'schema': 1,
            'authority': 'diagnostic_only',
            'scope': focused._RESOURCE_SCOPE,
            'status': 'complete',
            'observation': {'scope': focused._RESOURCE_SCOPE, 'telemetry_complete': False},
        }
        with self.config() as config:
            report, _ = self.invoke(config, resource_diagnostics=malformed)
            self.assertTrue(report['correctness_complete'])
            self.assertFalse(report['performance_complete'])
            self.assertFalse(report['timing_complete'])
            self.assertEqual(report['resource_diagnostics']['status'], 'incomplete')
            self.assertIn('malformed', report['resource_diagnostics']['reason'])

    def test_empty_and_boolean_version_complete_diagnostics_are_rejected(self):
        malformed = (
            {
                'schema': 1,
                'authority': 'diagnostic_only',
                'scope': focused._RESOURCE_SCOPE,
                'status': 'complete',
                'observation': {'scope': focused._RESOURCE_SCOPE, 'telemetry_complete': True},
            },
            {
                **_resource_diagnostic(),
                'schema': True,
            },
        )
        for value in malformed:
            with self.subTest(value=value):
                result = focused._resource_diagnostics(value)
                self.assertEqual(result['status'], 'incomplete')
                self.assertIn('malformed', result['reason'])

    def test_complete_resource_diagnostics_keep_violations_visible(self):
        violation = 'memory.events oom_kill increased by 1'
        diagnostics = _resource_diagnostic(event_deltas={
            'oom': 0,
            'oom_kill': 1,
            'oom_group_kill': 0,
        })
        self.assertEqual(focused._resource_diagnostics(diagnostics), diagnostics)
        with self.config() as config:
            report, _ = self.invoke(config, resource_diagnostics=diagnostics)
            self.assertTrue(report['correctness_complete'])
            self.assertTrue(report['resource_diagnostics']['observation']['telemetry_complete'])
            self.assertEqual(report['resource_diagnostics']['observation']['violations'], [violation])
            self.assertFalse(report['performance_complete'])
            self.assertFalse(report['timing_complete'])

    def test_observer_record_round_trips_through_host_validator_with_oom_violation(self):
        identity = resources.CgroupIdentity(1, 2, 3, '/case')
        before = resources.CgroupSnapshot(
            identity=identity,
            memory_events_hierarchical=True,
            pressure_accounting_enabled=True,
            memory_peak_bytes=65536,
            memory_current_bytes=32768,
            memory_max='4194304',
            memory_stat={'anon': 16384},
            memory_events={'oom': 0, 'oom_kill': 0, 'oom_group_kill': 0},
            cpu_stat={'usage_usec': 100, 'user_usec': 60, 'system_usec': 40,
                      'nr_periods': 3, 'nr_throttled': 1, 'throttled_usec': 7},
            cpu_max='200000 100000',
            cpuset_cpus_effective='0-3',
            pids_current=2,
            pids_peak=4,
            pressure={name: {'some': 10, 'full': 5} for name in ('cpu', 'memory', 'io')},
            observer_pid=99,
            observer_rss_bytes=4096,
            observer_peak_rss_bytes=8192,
        )
        after = replace(
            before,
            memory_peak_bytes=131072,
            memory_current_bytes=65536,
            memory_stat={'anon': 32768},
            memory_events={'oom': 0, 'oom_kill': 1, 'oom_group_kill': 0},
            cpu_stat={'usage_usec': 120, 'user_usec': 70, 'system_usec': 50,
                      'nr_periods': 4, 'nr_throttled': 2, 'throttled_usec': 9},
            pids_current=3,
            pids_peak=5,
            pressure={name: {'some': 13, 'full': 7} for name in ('cpu', 'memory', 'io')},
        )
        observed = resources.validate_case_window(
            before,
            after,
            declared_workers=dict(focused._DECLARED_WORKERS),
            scope=resources.GroupScope.FRESH_CASE_CONTAINER,
        )
        diagnostic = {
            'schema': 1,
            'authority': 'diagnostic_only',
            'scope': resources.GroupScope.FRESH_CASE_CONTAINER.value,
            'status': 'complete' if observed.telemetry_complete else 'incomplete',
            'observation': asdict(observed),
        }
        round_trip = json.loads(json.dumps(diagnostic))
        self.assertTrue(observed.telemetry_complete)
        self.assertEqual(round_trip['observation']['memory_peak_sample_bytes'], 131072)
        self.assertNotIn('case_memory_peak_bytes', round_trip['observation'])
        self.assertNotIn('container_peak_bytes_at_capture_end', round_trip['observation'])
        self.assertEqual(round_trip['observation']['violations'], ['memory.events oom_kill increased by 1'])
        self.assertEqual(focused._resource_diagnostics(round_trip), round_trip)

    def test_resource_diagnostic_rejects_semantic_mismatches(self):
        cases = (
            ('cpu_max', 'garbage'),
            ('memory_max', 'unknown'),
            ('cpuset_cpus_effective', '??'),
            ('memory_events_delta', {}),
            ('memory_stat_boundary', {}),
            ('memory_stat_boundary', {'': 1}),
            ('memory_stat_boundary', {'é': 1}),
            ('memory_stat_boundary', {'bad key': 1}),
            ('memory_events_delta', {'oom': 0, 'oom_kill': 0, 'oom_group_kill': 0, 'é': 0}),
            ('declared_workers', [['invented', 1]]),
            ('violations', ['memory.events oom_kill increased by 1']),
        )
        for field, value in cases:
            with self.subTest(field=field):
                diagnostic = _resource_diagnostic()
                diagnostic['observation'][field] = value
                result = focused._resource_diagnostics(diagnostic)
                self.assertEqual(result['status'], 'incomplete')
                self.assertIn('malformed', result['reason'])
        diagnostic = _resource_diagnostic()
        diagnostic['observation']['identity']['membership'] = '/tenant/../outside'
        result = focused._resource_diagnostics(diagnostic)
        self.assertEqual(result['status'], 'incomplete')
        self.assertIn('malformed', result['reason'])

    def test_malformed_fixture_and_unavailable_admission_reject_before_container(self):
        with self.config() as config:
            (config.fixtures / '2048/expected/json').write_bytes(b'forged oracle\n')
            with patch.object(check, 'baseline_owner', return_value=fixtures), \
                    patch.object(focused, '_admission', return_value={'command': ['perf-gate-ready'], 'exit_code': 0,
                                                                       'stdout': '', 'stderr': ''}), \
                    patch.object(focused, '_gate_source_digest', return_value=config.source_digest), \
                    patch.object(check, 'checked_docker') as launch, \
                    self.assertRaisesRegex(check.CheckError, 'fixture/golden mismatch'), \
                    self.lease_owner.campaign_leases(config.source_root, native=True):
                focused._qualify(config, 'json-n', self.lease_owner)
            launch.assert_not_called()
            self.assertFalse(config.output.exists())
        with self.config() as config:
            with patch.object(check, 'baseline_owner', return_value=SimpleNamespace(
                    perf_gate_check=lambda: {'command': ['perf-gate-ready'], 'exit_code': 1,
                                             'stdout': '', 'stderr': 'busy'})), \
                    self.assertRaisesRegex(check.CheckError, 'admission unavailable'):
                focused._admission(config)

    def test_admission_follows_fixture_qualification_before_launch(self):
        with self.config() as config:
            with patch.object(check, 'baseline_owner', return_value=fixtures), \
                    patch.object(focused, '_gate_source_digest', return_value=config.source_digest), \
                    patch.object(focused, '_case', wraps=focused._case) as case, \
                    patch.object(focused, '_admission', side_effect=check.CheckError('busy')), \
                    patch.object(check, 'checked_docker') as launch, \
                    self.assertRaisesRegex(check.CheckError, 'busy'), \
                    self.lease_owner.campaign_leases(config.source_root, native=True):
                focused._qualify(config, 'json-n', self.lease_owner)
            case.assert_called_once()
            launch.assert_not_called()
            self.assertFalse(config.output.exists())

    def test_report_publication_is_create_only_and_output_parents_cannot_be_links(self):
        with tempfile.TemporaryDirectory(dir=check.ADAPTER_ROOT / '.work/tmp') as directory:
            scratch = Path(directory) / 'scratch'
            scratch.mkdir()
            output = scratch / 'report.json'
            output.write_bytes(b'previous report')
            config = SimpleNamespace(scratch=scratch, output=output)
            with self.assertRaisesRegex(check.CheckError, 'fresh and under owned scratch'):
                focused._output_path(config)
            with self.assertRaises(FileExistsError):
                focused._publish_report(output, {'schema': 1})
            self.assertEqual(output.read_bytes(), b'previous report')
            self.assertEqual(list(scratch.iterdir()), [output])
            alias = scratch / 'alias'
            alias.symlink_to(scratch, target_is_directory=True)
            config.output = alias / 'report.json'
            with self.assertRaisesRegex(check.CheckError, 'fresh and under owned scratch'):
                focused._output_path(config)

    def test_wrong_output_state_timeout_and_cleanup_cannot_publish_pass(self):
        for row in (
            {'stdout_base64': 'd3Jvbmc='},
            {'effects': {'created': ['surprise']}},
            {'timed_out': True},
            {'status': 1},
            {'stderr_base64': 'ZXJyb3I='},
        ):
            with self.subTest(row=row), self.config() as config:
                with self.assertRaisesRegex(check.CheckError, 'oracle failed'):
                    self.invoke(config, observation=row)
                self.assertFalse(config.output.exists())
        with self.config() as config:
            with self.assertRaisesRegex(check.CheckError, 'cleanup unresolved'):
                self.invoke(config, cleanup_failure=True)
            self.assertTrue(self.cleanup_quarantined)
            self.assertFalse(self.lease_held)
            self.assertFalse(config.output.exists())

    def test_prepare_failure_and_interruption_never_start_or_release_quarantine(self):
        with self.config() as config:
            self.prepare_error = OSError('marker fsync failed')
            with patch.object(check, 'baseline_owner', return_value=fixtures), \
                    patch.object(focused, '_admission', return_value={'command': ['perf-gate-ready'], 'exit_code': 0,
                                                                       'stdout': '', 'stderr': ''}), \
                    patch.object(focused, '_gate_source_digest', return_value=config.source_digest), \
                    patch.object(focused, '_verify_image_id'), \
                    patch.object(check, 'checked_docker') as launch, \
                    self.lease_owner.campaign_leases(config.source_root, native=True), \
                    self.assertRaisesRegex(OSError, 'marker fsync failed'):
                focused._qualify(config, 'json-n', self.lease_owner)
            launch.assert_not_called()
            self.assertTrue(self.cleanup_quarantined)
            self.assertFalse(config.output.exists())
        self.prepare_error = None
        with self.config() as config:
            interruption = check.owned.SignalReceived(signal.SIGTERM)
            with self.assertRaises(check.owned.SignalReceived):
                self.invoke(config, interruption=interruption)
            self.assertTrue(self.cleanup_quarantined)
            self.assertFalse(config.output.exists())

    def test_source_product_and_fixture_edits_invalidate_result(self):
        for kind in ('source', 'product', 'fixture', 'revision', 'selection'):
            with self.subTest(kind=kind), self.config() as config:
                if kind == 'selection':
                    with self.assertRaisesRegex(check.CheckError, 'gate source selection changed'):
                        self.invoke(config, gate_digests=[config.source_digest, 'd' * 64])
                    self.assertFalse(config.output.exists())
                    continue
                if kind == 'revision':
                    mutate = lambda: subprocess.run([
                        'git', '-C', str(config.source_root), '-c', 'user.name=Focused Test', '-c',
                        'user.email=focused@example.invalid', 'commit', '--allow-empty', '-qm', 'changed head',
                    ], check=True)
                else:
                    path = {
                        'source': config.source_root / 'bench/consolidation/xsh/json.xsh',
                        'product': config.xsh,
                        'fixture': config.fixtures / '2048/index.json',
                    }[kind]
                    mutate = lambda path=path: path.write_bytes(b'changed')
                message = 'differs from focused configuration' if kind == 'revision' else 'changed during focused check'
                with self.assertRaisesRegex(check.CheckError, message):
                    self.invoke(config, mutate=mutate)
                self.assertFalse(config.output.exists())

    def test_source_and_revision_ignore_inherited_git_overrides(self):
        with self.config() as config:
            before = focused._source_snapshot(config.source_root)
            with patch.dict(os.environ, {'GIT_DIR': str(config.source_root / 'missing'),
                                             'GIT_INDEX_FILE': str(config.source_root / 'empty-index')}):
                self.assertEqual(focused._source_snapshot(config.source_root), before)
                focused._source_revision(config)

    def test_source_snapshot_tracks_staged_untracked_deleted_and_non_utf8_paths(self):
        with self.config() as config:
            strange = os.fsdecode(b'non-utf8-\xff\n.xsh')
            (config.source_root / strange).write_bytes(b'one')
            subprocess.run(['git', '-C', str(config.source_root), 'add', '.'], check=True)
            before = focused._source_snapshot(config.source_root)
            (config.source_root / strange).write_bytes(b'two')
            after = focused._source_snapshot(config.source_root)
            self.assertNotEqual(before[strange], after[strange])
            (config.source_root / strange).unlink()
            self.assertEqual(focused._source_snapshot(config.source_root)[strange], {'kind': 'deleted'})
            (config.source_root / 'dangling').symlink_to('missing')
            self.assertEqual(focused._source_snapshot(config.source_root)['dangling'],
                             {'kind': 'link', 'target': 'missing'})
            (config.source_root / 'escape').symlink_to(config.xsh)
            with self.assertRaisesRegex(check.CheckError, 'escapes checkout'):
                focused._source_snapshot(config.source_root)

    def test_cli_uses_the_native_lease_and_reports_scope(self):
        with self.config() as config:
            config_path = config.scratch / 'config.json'
            config_value = self.config_value(config)
            self.assertEqual(
                set(config_value),
                {'schema', 'image_id', 'source_root', 'commit', 'tree', 'xsh', 'scratch', 'measurement',
                 'gate_binding'},
            )
            config_path.write_text(json.dumps(config_value))
            with patch.object(focused, '_admission', return_value={'command': ['perf-gate-ready'], 'exit_code': 0,
                                                                    'stdout': '', 'stderr': ''}), \
                    patch.object(check, 'baseline_owner', return_value=fixtures), \
                    patch.object(focused, '_gate_source_digest', return_value=config.source_digest), \
                    patch.object(focused, '_verify_image_id', side_effect=lambda image_id: self.lease_calls.append(('image', image_id))), \
                    patch.object(check, 'load_canonical', side_effect=AssertionError('canonical owner must not be loaded')), \
                    patch.object(check, 'checked_docker') as launch:
                def docker(command, scratch, *, timeout_seconds, on_cleanup_verified):
                    row = {
                        'status': 0,
                        'timed_out': False,
                        'stderr_base64': '',
                        'effects': {},
                        'stdout_base64': base64.b64encode(
                            (config.fixtures / '2048/expected/json').read_bytes(),
                        ).decode(),
                    }
                    on_cleanup_verified()
                    diagnostic = {
                        'schema': 1,
                        'authority': 'diagnostic_only',
                        'scope': focused._RESOURCE_SCOPE,
                        'status': 'incomplete',
                        'reason': 'container resource telemetry unavailable in test adapter',
                    }
                    return 0, json.dumps({
                        'workload': row,
                        'resource_diagnostics': diagnostic,
                    }).encode(), b''
                launch.side_effect = docker
                output = io.StringIO()
                with redirect_stdout(output):
                    status = focused.main(['--config', str(config_path), '--case', 'json-n'])
            self.assertEqual(status, 0)
            self.assertEqual(self.lease_calls, [
                ('campaign', config.source_root, True),
                ('required',),
                ('image', config.image_id),
                ('prepare', 'focused json-n: container launch in progress'),
                ('resolve',),
            ])
            self.assertIn('Product source binding: gate-prepared product bound to selected source', output.getvalue())
            self.assertIn('Performance: not measured', output.getvalue())
            self.assertIn('Full cohort: not run', output.getvalue())

    def test_lease_busy_returns_incomplete_status_two(self):
        with self.config() as config:
            config_path = config.scratch / 'config.json'
            config_path.write_text(json.dumps(self.config_value(config)))
            self.lease_busy = self.lease_owner.LeaseBusy('native lease busy')
            with patch.object(focused, '_qualify', side_effect=AssertionError('busy lease must block work')):
                stderr = io.StringIO()
                with redirect_stderr(stderr):
                    status = focused.main(['--config', str(config_path), '--case', 'json-n'])
            self.assertEqual(status, 2)
            self.assertIn('native lease busy', stderr.getvalue())
            self.assertIn('Performance: not measured', stderr.getvalue())

    def test_cli_reports_cleanup_and_marker_errors_with_the_primary_timeout(self):
        with self.config() as config:
            config_path = config.scratch / 'config.json'
            config_path.write_text(json.dumps(self.config_value(config)))
            primary = subprocess.TimeoutExpired(['docker', 'run'], 120)
            primary.add_note('owned container cleanup failed: cleanup command failed')
            primary.add_note('failed to mark unresolved cleanup: marker write failed')
            with patch.object(focused, '_qualify', side_effect=primary):
                stderr = io.StringIO()
                with redirect_stderr(stderr):
                    status = focused.main(['--config', str(config_path), '--case', 'json-n'])
            self.assertEqual(status, 2)
            self.assertIn('cleanup command failed', stderr.getvalue())
            self.assertIn('marker write failed', stderr.getvalue())

    def test_cli_exit_statuses_keep_failure_incomplete_and_cancellation_distinct(self):
        for failure, expected in (
            (focused._WorkloadFailure('oracle failed'), 1),
            (check.CheckError('admission unavailable'), 2),
            (check.owned.SignalReceived(signal.SIGTERM), 128 + signal.SIGTERM),
        ):
            with self.subTest(expected=expected), self.config() as config:
                config_path = config.scratch / 'config.json'
                config_path.write_text(json.dumps(self.config_value(config)))
                with patch.object(focused, '_qualify', side_effect=failure):
                    stderr = io.StringIO()
                    with redirect_stderr(stderr):
                        status = focused.main(['--config', str(config_path), '--case', 'json-n'])
                self.assertEqual(status, expected)
                self.assertIn('Performance: not measured', stderr.getvalue())
                self.assertIn('Full cohort: not run', stderr.getvalue())


if __name__ == '__main__':
    unittest.main()
