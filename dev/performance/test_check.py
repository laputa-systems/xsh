from copy import deepcopy
import base64
from contextlib import ExitStack, nullcontext, redirect_stderr
from dataclasses import replace
import importlib.util
from io import StringIO
import json
import os
from pathlib import Path
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


class CohortTests(unittest.TestCase):
    def setUp(self):
        (check.ADAPTER_ROOT / '.work/tmp').mkdir(parents=True, exist_ok=True)

    def test_syscall_summary_and_complete_helper_lifecycles_are_required(self):
        rows = [{'type': 'syscall.summary', 'syscall_count': 17},
                {'payload': {'type': 'run.start'}}, {'payload': {'type': 'run.end', 'pid': 19, 'error': None}}]
        self.assertEqual(workloads.trace_counts(iter(rows)), {'syscalls': 17, 'subprocesses': 1})
        for changed in (rows[1:], rows[:2], rows + rows[:1], rows[:2] + [{'payload': {'type': 'run.end', 'pid': None, 'error': 'failed'}}]):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, 'attribution unavailable'):
                workloads.trace_counts(iter(changed))

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
                events = [{'payload': {'type': 'run.start', 'argv': []}},
                          {'payload': {'type': 'run.end', 'pid': 19, 'status': {'code': 0}, 'error': None}}] if helper else []
                summary = {'type': 'syscall.summary', 'syscall_count': syscall_count, 'syscall_seconds': 0,
                           'top_syscalls_by_count': [], 'per_program_top_syscalls': [], 'per_process_top_syscalls': []}
                Path(argv[argv.index('--trace-file') + 1]).write_text(''.join(json.dumps(row) + '\n' for row in [*events, summary]))
            row = {'wall_ns': None, 'first_useful_stdout_ns': None, 'useful_stdout_observed': True,
                   'status': 0, 'timed_out': False, 'stdout_base64': base64.b64encode(b'ready\n').decode(),
                   'stderr_base64': '', 'effects': {}}
            return row | (failure or {})
        config = check.parse_config(PolicyTests().config())
        owner = check.baseline_owner(config, 'bench/consolidation/run.py', 'comparative_bench')
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


class PolicyTests(unittest.TestCase):
    def config(self):
        root = Path(__file__).resolve().parents[2]
        return {'schema': 1, 'source_root': str(root), 'commit': 'a' * 40, 'tree': 'b' * 40, 'laputa_root': str(root / '.work/laputa'), 'product_receipt': str(root / '.work/products.json'), 'xsh': str(root / '.work/xsh'), 'xsht': str(root / '.work/xsht'), 'scratch': str(root / '.work/tmp'), 'functional': None}

    def test_config_has_no_implicit_source_or_product(self):
        for key in self.config():
            with self.subTest(key=key):
                value = self.config()
                del value[key]
                with self.assertRaises(check.CheckError):
                    check.parse_config(value)
        value = self.config()
        value['normal_checkout'] = '/checkout'
        with self.assertRaises(check.CheckError):
            check.parse_config(value)

    def test_rejects_relative_paths_and_external_scratch(self):
        for key, replacement in [('source_root', '.'), ('scratch', '/tmp'), ('xsh', 'xsh')]:
            value = self.config()
            value[key] = replacement
            with self.assertRaises(check.CheckError):
                check.parse_config(value)

    def test_rejects_duplicate_json_and_boolean_schema(self):
        with self.assertRaises(check.CheckError):
            check.unique_object([('schema', 1), ('schema', 1)])
        value = self.config()
        value['schema'] = True
        with self.assertRaises(check.CheckError):
            check.parse_config(value)

    def test_optional_measurement_paths_are_explicit_and_do_not_change_old_configs(self):
        value = self.config()
        self.assertIsNone(check.parse_config(value).measurement)
        value['measurement'] = {'fixtures': '/frozen/fixtures', 'output': '/frozen/output.json'}
        self.assertEqual(check.parse_config(value).measurement['fixtures'], Path('/frozen/fixtures'))
        for changed in ({'fixtures': 'relative', 'output': '/frozen/output.json'}, {'fixtures': '/frozen/fixtures'}, {'fixtures': '/frozen/fixtures', 'output': '/frozen/output.json', 'accepted': True}):
            with self.subTest(changed=changed), self.assertRaises(check.CheckError):
                check.parse_config(value | {'measurement': changed})

    def test_measure_without_live_close_report_fails_before_effects(self):
        config = check.parse_config(self.config())
        with patch.object(check, 'qualify_products', side_effect=AssertionError('unexpected qualification')), patch.object(check, 'checked_docker', side_effect=AssertionError('unexpected Docker')):
            with self.assertRaisesRegex(check.CheckError, 'live.*CloseReport'):
                check.measure(config)

    def product(self):
        source = {'commit': 'a' * 40, 'git_tree': 'b' * 40, 'status': 'clean'}
        binaries = {'xsh': {'path': '/product/xsh', 'sha256': 'c' * 64}, 'xsht': {'path': '/product/xsht', 'sha256': 'd' * 64}}
        receipt = {'schema': 1, 'state': 'complete', 'source_commit': source['commit'], 'results': [{'opt_level': 3, 'build': {'status': 0, 'container_cleanup': {'resolved': True}, 'stage_validation': {'source': source, 'binary_hashes': binaries}}}]}
        return receipt, deepcopy(source), deepcopy(binaries)

    def test_product_source_and_exact_binary_bindings(self):
        receipt, source, binaries = self.product()
        check.check_product_policy(receipt, source, binaries)
        for target in ('commit', 'git_tree', 'status'):
            wrong = dict(source, **{target: 'mismatch'})
            with self.assertRaises(check.CheckError):
                check.check_product_policy(receipt, wrong, binaries)
        for key in ('sha256', 'path'):
            wrong = deepcopy(binaries)
            wrong['xsh'][key] = 'mismatch'
            with self.assertRaises(check.CheckError):
                check.check_product_policy(receipt, source, wrong)

    def test_failed_build_and_cleanup_cannot_qualify_products(self):
        for field, value in [('status', 1), ('status', False), ('container_cleanup', {'resolved': False})]:
            receipt, source, binaries = self.product()
            receipt['results'][0]['build'][field] = value
            with self.assertRaises(check.CheckError):
                check.check_product_policy(receipt, source, binaries)

    def test_wrong_output_stderr_and_status_fail(self):
        check.check_result(0, b'hello\n', b'', b'hello\n')
        for status, stdout, stderr in [(0, b'wrong\n', b''), (0, b'hello\n', b'error'), (1, b'hello\n', b'')]:
            with self.assertRaises(check.CheckError):
                check.check_result(status, stdout, stderr, b'hello\n')

    def test_readiness_before_prevents_execution(self):
        events = []
        def ready(after):
            events.append(after)
            raise check.CheckError('busy')
        with self.assertRaisesRegex(check.CheckError, 'busy'):
            check.readiness_window(ready, lambda: events.append('operation'))
        self.assertEqual(events, [False])

    def test_readiness_after_rejects_success_and_runs_after_failure(self):
        for operation_fails in (False, True):
            events = []
            def ready(after):
                events.append(after)
                if after:
                    raise check.CheckError('busy after')
            def operation():
                events.append('operation')
                if operation_fails:
                    raise RuntimeError('primary')
            expected = RuntimeError if operation_fails else check.CheckError
            message = 'primary' if operation_fails else 'busy after'
            with self.assertRaisesRegex(expected, message) as caught:
                check.readiness_window(ready, operation)
            if operation_fails:
                self.assertIn('busy after', caught.exception.__notes__[0])
            self.assertEqual(events, [False, 'operation', True])

    def canonical(self):
        config = check.parse_config(self.config())
        source = {'head': config.commit, 'tree': config.tree, 'clean': True, 'status': ''}
        receipt = {'schema_version': 1, 'kind': 'canonical_gate_batch_replay', 'source_expected': {'commit': config.commit, 'tree': config.tree}, 'status': 'passed', 'exit_code': 0, 'timed_out': False, 'received_signal': None, 'failure': None, 'cleanup': {'resolved': True}, 'source_checks': [source, deepcopy(source)]}
        return receipt, config

    def laputa_binding(self, root):
        root.mkdir()
        (root / 'pm').mkdir()
        (root / 'pm/generation.xsh').write_text('print "generation"\n')
        subprocess.run(['git', 'init', '--quiet', str(root)], check=True)
        subprocess.run(['git', '-C', str(root), 'add', 'pm/generation.xsh'], check=True)
        subprocess.run(['git', '-C', str(root), '-c', 'user.name=G1 fixture', '-c', 'user.email=g1-fixture@example.invalid', 'commit', '--quiet', '-m', 'test(performance): freeze Laputa corpus fixture'], check=True)
        identity = {
            'commit': check.git(root, 'rev-parse', 'HEAD'),
            'git_tree': check.git(root, 'rev-parse', 'HEAD^{tree}'),
            'status': 'clean',
        }
        return {'path': str(root), 'source_identity': identity, 'source_identity_after': deepcopy(identity)}

    def supplemental_docker_argv(self, root, command):
        return [
            'docker', 'run', '--network=none', '--memory=1g', '--memory-swap=1g',
            '--cpus=12', '--user', '0:0', '-v', f'{root}:{root}:ro',
            '-e', f'XSH_LAPUTA_CORPUS={root}', 'pinned-image', *command,
        ]

    def supplemental_step(self, logs, step_id, argv, *, root=None, docker_argv=None):
        step = {'id': step_id, 'argv': argv, 'env': {}, 'exit_code': 0, 'timed_out': False}
        log = logs / f'{step_id}.log'
        log.write_text(f'{step_id}\n')
        step['log'] = str(log)
        step['log_sha256'] = check.owned_digest(log)
        if docker_argv is not None:
            step['docker_argv'] = docker_argv
            if root is not None:
                step['env']['XSH_GATE_LAPUTA_CORPUS'] = str(root)
                capture = logs / f'{step_id}.docker-argv'
                capture.write_bytes(b'\0'.join(item.encode() for item in docker_argv) + b'\0')
                step['env']['XSH_GATE_DOCKER_ARGV_CAPTURE'] = str(capture)
                step['docker_argv_capture'] = {'path': str(capture), 'sha256': check.owned_digest(capture)}
        return step

    def supplemental_stage_fixture(self, directory, config, plan, logs):
        target = plan['environment']['target']
        products_root = Path(directory) / 'stage-products'
        harness_root = Path(directory) / 'stage-harness'
        source = products_root / target / 'verification' / 'xsht'
        harness_xsh = harness_root / target / 'verification' / 'xsh'
        destination = harness_xsh.with_name('xsht')
        source.parent.mkdir(parents=True)
        harness_xsh.parent.mkdir(parents=True)
        source.write_bytes(b'qualified product xsht')
        source.chmod(0o755)
        harness_xsh.write_bytes(b'qualified harness xsh')
        harness_xsh.chmod(0o755)
        destination.write_bytes(source.read_bytes())
        destination.chmod(0o755)
        source_hash = check.owned_digest(source)
        harness_xsh_hash = check.owned_digest(harness_xsh)
        features = ['default', 'native-tests', 'net', 'tools']
        harness = {
            'target_variant': 'harness',
            'target_dir': str(harness_root),
            'xsh_variant': {
                'path': str(harness_xsh),
                'sha256': harness_xsh_hash,
                'qualification': {
                    'target_variant': 'harness',
                    'profile_name': 'verification',
                    'features': features,
                    'source': {'clean': True, 'commit': config.commit, 'tree': config.tree},
                },
            },
        }
        staging = {
            'action': 'staged',
            'source': {
                'path': str(source),
                'sha256': source_hash,
                'qualified_product_variant': {
                    'profile': 'verification',
                    'target_variant': 'products',
                    'commit': config.commit,
                    'tree': config.tree,
                },
            },
            'destination': {
                'path': str(destination),
                'sha256': source_hash,
                'harness': 'core-compat-extra',
                'harness_xsh_path': str(harness_xsh),
                'harness_xsh_sha256': harness_xsh_hash,
                'target_variant': 'harness',
                'profile': 'verification',
                'features': features,
            },
        }
        cidfile = Path(directory) / 'container-ids/007-stage-core-compat-extra-xsht.cid'
        cidfile.parent.mkdir()
        log = logs / '007-stage-core-compat-extra-xsht.log'
        log.write_text('stage-core-compat-extra-xsht\n')
        script = '\n'.join([
            'set -eu',
            'source=$1',
            'destination=$2',
            'expected=$3',
            'hash_file() { result=$(sha256sum -- "$1") || return; printf \'%s\\n\' "${result%% *}"; }',
            '[ ! -L "$source" ] && [ -f "$source" ] && [ -x "$source" ]',
            '[ "$(hash_file "$source")" = "$expected" ]',
            'if [ -L "$destination" ]; then exit 1; fi',
            'if [ -e "$destination" ]; then',
            '    [ -f "$destination" ] && [ -x "$destination" ]',
            '    [ "$(hash_file "$destination")" = "$expected" ]',
            '    exit 0',
            'fi',
            'temporary=$(mktemp "${destination}.stage.XXXXXX")',
            "trap 'rm -f -- \"$temporary\"' EXIT HUP INT TERM",
            'cp -p -- "$source" "$temporary"',
            '[ -f "$temporary" ] && [ -x "$temporary" ]',
            '[ "$(hash_file "$temporary")" = "$expected" ]',
            'ln -- "$temporary" "$destination"',
            'rm -- "$temporary"',
            'trap - EXIT HUP INT TERM',
        ])
        scratch = config.source_root / '.work/tmp'
        argv = [
            'docker', 'run', '--rm', '--init', '--read-only', '--cidfile', str(cidfile),
            '--platform', 'linux/amd64', '--cpus=2', '--memory=1g', '--memory-swap=1g',
            '--network=none', '-v', f'{products_root}:{products_root}:ro',
            '-v', f'{harness_root}:{harness_root}:rw', '-v', f'{scratch}:{scratch}:rw',
            '-e', f'TMPDIR={scratch}', '--user', '0:0', plan['environment']['image'],
            'sh', '-euc', script, 'xsh-gate-stage-xsht', str(source), str(destination), source_hash,
        ]
        step = {
            'id': 'stage-core-compat-extra-xsht',
            'argv': argv,
            'docker_argv': argv,
            'env': {'XSH_GATE_CIDFILE': str(cidfile)},
            'cwd': str(config.source_root),
            'log': str(log),
            'exit_code': 0,
            'timed_out': False,
        }
        return {
            'step': step,
            'staging': staging,
            'harness': harness,
            'products_root': str(products_root),
            'harness_root': str(harness_root),
            'source_hash': source_hash,
        }

    def rewrite_supplemental_stage_roots(self, receipt, plan, products_root, harness_root):
        target = plan['environment']['target']
        staging = receipt['harness_artifact_staging']['core-compat-extra-xsht']
        old_products_root = Path(receipt['cargo_target_dirs']['products'])
        old_harness_root = Path(receipt['cargo_target_dirs']['harness'])
        source = Path(staging['source']['path'])
        destination = Path(staging['destination']['path'])
        old_harness_xsh = Path(staging['destination']['harness_xsh_path'])
        new_source = products_root / target / 'verification' / 'xsht'
        new_harness_xsh = harness_root / target / 'verification' / 'xsh'
        new_destination = new_harness_xsh.with_name('xsht')
        receipt['cargo_target_dirs']['products'] = str(products_root)
        receipt['cargo_target_dirs']['harness'] = str(harness_root)
        staging['source']['path'] = str(new_source)
        staging['destination']['path'] = str(new_destination)
        staging['destination']['harness_xsh_path'] = str(new_harness_xsh)
        harness = receipt['harnesses']['core-compat-extra']
        harness['target_dir'] = str(harness_root)
        harness['xsh_variant']['path'] = str(new_harness_xsh)
        replacements = {
            str(source): str(new_source),
            str(destination): str(new_destination),
            f'{old_products_root}:{old_products_root}:ro': f'{products_root}:{products_root}:ro',
            f'{old_harness_root}:{old_harness_root}:rw': f'{harness_root}:{harness_root}:rw',
        }
        for step in receipt['steps']:
            if step.get('id') != 'stage-core-compat-extra-xsht':
                continue
            for key in ('argv', 'docker_argv'):
                step[key] = [replacements.get(argument, argument) for argument in step[key]]

    def test_bare_boolean_failed_canonical_and_cleanup_are_rejected(self):
        receipt, config = self.canonical()
        check.check_canonical_policy(receipt, config)
        for field, value in [('kind', 'functional_complete'), ('status', 'failed-or-incomplete'), ('exit_code', False), ('cleanup', {'resolved': False}), ('source_checks', []), ('failure', 'primary'), ('timed_out', True)]:
            with self.subTest(field=field):
                wrong = dict(receipt, **{field: value})
                with self.assertRaises(check.CheckError):
                    check.check_canonical_policy(wrong, config)
        with self.assertRaises(check.CheckError):
            check.check_canonical_policy({'functional_complete': True}, config)

    def test_required_supplemental_context_and_build_steps_are_not_optional(self):
        canonical, config = self.canonical()
        plan = {'environment': {'image': 'pinned-image', 'target': 'target'}, 'products': [['cargo', 'build', '-p', 'xsh']], 'rust_harness_builds': [], 'warning_deny': [['cargo', 'clippy']], 'linux_context_runs': [{'id': 'boundary', 'binary': 'xsht', 'argv': ['test', 'boundary.xsh'], 'memory': '1g', 'network': 'none', 'privileged': False}]}
        with tempfile.TemporaryDirectory(dir=config.scratch) as directory:
            root = Path(directory) / 'laputa'
            binding = self.laputa_binding(root)
            source_root = Path(directory) / 'source'
            (source_root / '.work/tmp').mkdir(parents=True)
            config = replace(config, source_root=source_root, laputa_root=root)
            logs = Path(directory) / 'logs'
            logs.mkdir()
            stage = self.supplemental_stage_fixture(directory, config, plan, logs)
            helper_argv = self.supplemental_docker_argv(root, ['/bin/sh', '-lc', 'cargo build'])
            lint_argv = self.supplemental_docker_argv(root, ['/bin/sh', '-lc', 'cargo clippy'])
            context_argv = self.supplemental_docker_argv(root, [str(Path(stage['products_root']) / 'target/verification/xsht'), 'test', 'boundary.xsh'])
            steps = [
                {'id': 'verify-pinned-image-id', 'argv': ['docker', 'image', 'inspect', '--format', '{{.Id}}', 'pinned-image'], 'exit_code': 0, 'timed_out': False},
                self.supplemental_step(logs, 'build-product-01-xsh', ['/tools/run-native-final.sh', str(config.source_root), 'build', 'cargo', 'build', '-p', 'xsh'], root=root, docker_argv=helper_argv),
                self.supplemental_step(logs, 'warning-deny-01', ['/tools/run-native-final.sh', str(config.source_root), 'lint', 'cargo', 'clippy'], root=root, docker_argv=lint_argv),
                stage['step'],
                self.supplemental_step(logs, 'linux-special-01-boundary', context_argv, docker_argv=context_argv),
                {'id': 'tracked-diff-check', 'argv': ['git', 'diff', '--check'], 'exit_code': 0, 'timed_out': False},
            ]
            environment = {
                'target': plan['environment']['target'],
                'image': plan['environment']['image'],
                'host_uid': 1000,
                'host_gid': 1000,
                'container_cpus': 12,
                'container_memory': '24g',
                'container_swap': 'disabled',
                'cargo_lock_timeout_seconds': 900,
                'native_runner_base_sha256': 'a' * 64,
                'native_lock_path': '/work/native-suite.lock',
                'native_cleanup_marker_path': '/work/native-suite.cleanup-unresolved',
                'native_lock_scope': 'acquired before product, harness, Clippy and supplemental context runs',
                'no_docker_self_test_sha256': 'b' * 64,
                'final_runner_sha256': 'c' * 64,
                'launch_sha256': 'd' * 64,
                'manifest_sha256': 'e' * 64,
                'manifest_body_sha256': 'f' * 64,
                'build_tools': {'/usr/local/bin/mold': '1' * 64},
                'runtime_cargo_forwarder_sha256': '2' * 64,
                'runtime_rustc_wrapper_sha256': '3' * 64,
            }
            receipt = {
                'schema': 1,
                'status': 'commands-passed-needs-privilege-skip-and-close-review',
                'source_expected': canonical['source_expected'],
                'source_checks': canonical['source_checks'],
                'steps': steps,
                'cargo_target_dirs': {'products': stage['products_root'], 'harness': stage['harness_root']},
                'environment': environment,
                'laputa_corpus': binding,
                'harnesses': {'core-compat-extra': stage['harness']},
                'harness_artifact_staging': {'core-compat-extra-xsht': stage['staging']},
            }
            products = {name + '-verification': 'c' * 64 for name in ('xsh', 'xsht', 'xshi', 'xsh-fuzz', 'xsh-test-helper')}
            products['xsht-verification'] = stage['source_hash']
            receipt['hashes'] = {name: products for name in ('products_after_initial_build', 'binaries_after_build', 'binaries_before_tests', 'binaries_after_tests')}
            receipt['hashes'].update({name: {'harness': 'd' * 64} for name in ('harnesses_before_tests', 'harnesses_after_tests', 'harness_variant_binaries_before_tests', 'harness_variant_binaries_after_tests')})
            check.check_supplemental_policy(receipt, plan, config)
            stage_index = next(index for index, step in enumerate(receipt['steps']) if step['id'] == 'stage-core-compat-extra-xsht')
            context_index = next(index for index, step in enumerate(receipt['steps']) if step['id'] == 'linux-special-01-boundary')
            for field, value in (('target', 'wrong-target'), ('image', 'wrong-image')):
                wrong = deepcopy(receipt)
                wrong['environment'][field] = value
                with self.subTest(field=field, value=value), self.assertRaisesRegex(check.CheckError, 'supplemental target or image mismatch'):
                    check.check_supplemental_policy(wrong, plan, config)
            for field in ('target', 'image'):
                wrong = deepcopy(receipt)
                del wrong['environment'][field]
                with self.subTest(missing=field), self.assertRaisesRegex(check.CheckError, 'supplemental target or image mismatch'):
                    check.check_supplemental_policy(wrong, plan, config)
            wrong = deepcopy(receipt)
            wrong['environment'] = None
            with self.assertRaisesRegex(check.CheckError, 'supplemental target or image mismatch'):
                check.check_supplemental_policy(wrong, plan, config)
            qualified_without_stage = deepcopy(receipt)
            qualified_without_stage['steps'] = [step for step in qualified_without_stage['steps'] if step['id'] != 'stage-core-compat-extra-xsht']
            qualified_without_stage['harness_artifact_staging']['core-compat-extra-xsht']['action'] = 'already-qualified'
            check.check_supplemental_policy(qualified_without_stage, plan, config)
            for root_field in ('products', 'harness'):
                for overlap in ('equal', 'ancestor', 'descendant'):
                    wrong = deepcopy(receipt)
                    laputa = Path(config.laputa_root)
                    conflicting_root = {
                        'equal': laputa,
                        'ancestor': laputa.parent,
                        'descendant': laputa / f'{root_field}-target',
                    }[overlap]
                    products_root = Path(wrong['cargo_target_dirs']['products'])
                    harness_root = Path(wrong['cargo_target_dirs']['harness'])
                    if root_field == 'products':
                        products_root = conflicting_root
                    else:
                        harness_root = conflicting_root
                    self.rewrite_supplemental_stage_roots(wrong, plan, products_root, harness_root)
                    with (
                        self.subTest(root=root_field, overlap=overlap),
                        patch.object(check, 'validate_laputa_corpus', return_value={}),
                        patch.object(check, 'bound_file', side_effect=AssertionError('stage root boundary checked after file validation')),
                        self.assertRaisesRegex(check.CheckError, 'supplemental stage roots overlap selected Laputa corpus'),
                    ):
                        check.check_supplemental_policy(wrong, plan, config)
            stage_scratch = config.source_root / '.work/tmp'
            for overlap, laputa in (
                ('equal', stage_scratch),
                ('ancestor', config.source_root),
                ('descendant', stage_scratch / 'laputa'),
            ):
                wrong = deepcopy(receipt)
                wrong['laputa_corpus']['path'] = str(laputa)
                scratch_config = replace(config, laputa_root=laputa)
                with (
                    self.subTest(root='source-scratch', overlap=overlap),
                    patch.object(check, 'validate_laputa_corpus', return_value={}),
                    patch.object(check, 'bound_file', side_effect=AssertionError('stage root boundary checked after file validation')),
                    self.assertRaisesRegex(check.CheckError, 'supplemental stage roots overlap selected Laputa corpus'),
                ):
                    check.check_supplemental_policy(wrong, plan, scratch_config)
            for mutation in ('missing_step', 'missing_staging_record', 'wrong_source', 'stale_source', 'wrong_destination', 'wrong_image', 'wrong_mount_mode', 'wrong_shell_argv', 'wrong_docker_argv', 'wrong_action'):
                wrong = deepcopy(receipt)
                stage_step = wrong['steps'][stage_index]
                staging = wrong['harness_artifact_staging']['core-compat-extra-xsht']
                if mutation == 'missing_step':
                    wrong['steps'].pop(stage_index)
                elif mutation == 'missing_staging_record':
                    del wrong['harness_artifact_staging']
                elif mutation == 'wrong_source':
                    staging['source']['sha256'] = 'f' * 64
                elif mutation == 'stale_source':
                    staging['source']['qualified_product_variant']['tree'] = 'f' * 40
                elif mutation == 'wrong_destination':
                    staging['destination']['path'] = str(Path(staging['destination']['path']).with_name('wrong-xsht'))
                elif mutation == 'wrong_image':
                    image_index = stage_step['argv'].index(plan['environment']['image'])
                    stage_step['argv'][image_index] = 'wrong-image'
                    stage_step['docker_argv'][image_index] = 'wrong-image'
                elif mutation == 'wrong_mount_mode':
                    mount_index = next(index for index, value in enumerate(stage_step['argv']) if value.endswith(':ro'))
                    stage_step['argv'][mount_index] = stage_step['argv'][mount_index].removesuffix(':ro') + ':rw'
                    stage_step['docker_argv'][mount_index] = stage_step['argv'][mount_index]
                elif mutation == 'wrong_shell_argv':
                    stage_step['argv'][-1] = 'wrong-hash'
                    stage_step['docker_argv'][-1] = 'wrong-hash'
                elif mutation == 'wrong_docker_argv':
                    stage_step['docker_argv'][-1] = 'wrong-hash'
                else:
                    staging['action'] = 'unknown'
                with self.subTest(stage_mutation=mutation), self.assertRaises(check.CheckError):
                    check.check_supplemental_policy(wrong, plan, config)
            for name in ('linux-special-01-boundary', 'build-product-01-xsh', 'warning-deny-01'):
                wrong = deepcopy(receipt)
                wrong['steps'] = [step for step in wrong['steps'] if step['id'] != name]
                with self.assertRaisesRegex(check.CheckError, 'required supplemental step'):
                    check.check_supplemental_policy(wrong, plan, config)
            wrong = deepcopy(receipt)
            wrong['steps'][2]['argv'][-1] = 'check'
            with self.assertRaisesRegex(check.CheckError, 'command mismatch'):
                check.check_supplemental_policy(wrong, plan, config)
            wrong = deepcopy(receipt)
            del wrong['laputa_corpus']
            with self.assertRaisesRegex(check.CheckError, 'Laputa corpus binding'):
                check.check_supplemental_policy(wrong, plan, config)
            for mutation in ('wrong_path', 'stale_identity'):
                wrong = deepcopy(receipt)
                if mutation == 'wrong_path':
                    wrong['laputa_corpus']['path'] = str(root.parent)
                else:
                    wrong['laputa_corpus']['source_identity_after']['git_tree'] = 'e' * 40
                with self.subTest(mutation=mutation), self.assertRaises(check.CheckError):
                    check.check_supplemental_policy(wrong, plan, config)

            helper = receipt['steps'][1]
            capture = Path(helper['docker_argv_capture']['path'])
            original_capture = capture.read_bytes()
            capture.write_bytes(original_capture + b'changed\0')
            try:
                with self.assertRaisesRegex(check.CheckError, 'evidence file hash mismatch'):
                    check.check_supplemental_policy(receipt, plan, config)
            finally:
                capture.write_bytes(original_capture)
            capture.write_bytes(original_capture + b'changed\0')
            try:
                wrong = deepcopy(receipt)
                wrong['steps'][1]['docker_argv_capture']['sha256'] = check.owned_digest(capture)
                with self.assertRaisesRegex(check.CheckError, 'captured command'):
                    check.check_supplemental_policy(wrong, plan, config)
            finally:
                capture.write_bytes(original_capture)

            for mutation in ('duplicate_mount', 'duplicate_env', 'ancestor_source_alias', 'ancestor_mount_source_alias'):
                wrong = deepcopy(receipt)
                helper = wrong['steps'][1]
                actual = list(helper['docker_argv'])
                image_index = actual.index('pinned-image')
                if mutation == 'duplicate_mount':
                    actual[image_index:image_index] = ['-v', f'{root}:{root}:rw']
                elif mutation == 'duplicate_env':
                    actual[image_index:image_index] = ['-e', 'XSH_LAPUTA_CORPUS=/another/laputa']
                elif mutation == 'ancestor_source_alias':
                    actual[image_index:image_index] = ['-v', f'{root.parent}:/laputa-alias:rw']
                else:
                    actual[image_index:image_index] = ['--mount', f'type=bind,src={root.parent},dst=/laputa-alias,rw']
                capture = Path(helper['docker_argv_capture']['path'])
                original_capture = capture.read_bytes()
                capture.write_bytes(b'\0'.join(item.encode() for item in actual) + b'\0')
                helper['docker_argv'] = actual
                helper['docker_argv_capture']['sha256'] = check.owned_digest(capture)
                try:
                    with self.subTest(mutation=mutation), self.assertRaises(check.CheckError):
                        check.check_supplemental_policy(wrong, plan, config)
                finally:
                    capture.write_bytes(original_capture)

            wrong = deepcopy(receipt)
            del wrong['steps'][1]['docker_argv_capture']
            with self.assertRaises(check.CheckError):
                check.check_supplemental_policy(wrong, plan, config)
            wrong = deepcopy(receipt)
            wrong['steps'][context_index]['docker_argv_capture'] = {'path': '/unused/capture', 'sha256': 'f' * 64}
            with self.assertRaisesRegex(check.CheckError, 'direct Docker argv must not use a helper capture'):
                check.check_supplemental_policy(wrong, plan, config)
            wrong = deepcopy(receipt)
            direct = wrong['steps'][context_index]
            env_index = direct['argv'].index('-e')
            del direct['argv'][env_index:env_index + 2]
            direct['docker_argv'] = direct['argv']
            with self.assertRaisesRegex(check.CheckError, 'configured Laputa corpus'):
                check.check_supplemental_policy(wrong, plan, config)

    def test_canonical_products_bootstrap_without_profile_or_prior_close(self):
        receipt, config = self.canonical()
        binaries = {name: {'path': str(config.source_root / f'target/verification/{name}'), 'sha256': 'c' * 64} for name in ('xsh', 'xsht')}
        receipt['products_after'] = {name + '-verification': {'path': f'target/verification/{name}', 'sha256': record['sha256']} for name, record in binaries.items()}
        check.check_canonical_products(receipt, config, binaries)
        wrong = deepcopy(binaries)
        wrong['xsh']['sha256'] = 'd' * 64
        with self.assertRaisesRegex(check.CheckError, 'canonical product qualification mismatch'):
            check.check_canonical_products(receipt, config, wrong)

    def test_skip_review_is_bound_and_each_context_requires_disposition(self):
        config = check.parse_config(self.config())
        review = {'source_commit': config.commit, 'source_tree': config.tree, 'canonical_sha256': 'c' * 64, 'supplemental_sha256': 'd' * 64, 'reviewer': 'reviewer', 'contexts': {'canonical': {'result': 'review_verified', 'reason': 'no mandatory skips'}, 'linux-special-01-boundary': {'result': 'review_verified', 'reason': 'all privilege cases executed'}}}
        contexts = list(review['contexts'])
        def validate(value):
            check.check_skip_review_policy(value, config, 'c' * 64, 'd' * 64, contexts)
        validate(review)
        for field in ('source_commit', 'source_tree', 'canonical_sha256', 'supplemental_sha256', 'reviewer'):
            wrong = deepcopy(review)
            wrong[field] = ''
            with self.assertRaises(check.CheckError):
                validate(wrong)
        for name in contexts:
            missing = deepcopy(review)
            del missing['contexts'][name]
            with self.assertRaises(check.CheckError):
                validate(missing)
            unresolved = deepcopy(review)
            unresolved['contexts'][name]['result'] = 'open'
            with self.assertRaises(check.CheckError):
                validate(unresolved)



class FunctionalTests(unittest.TestCase):
    def test_structural_scan_runs_as_the_caller_uid_and_gid(self):
        root = Path(__file__).resolve().parents[2]
        (root / '.work/tmp').mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=root / '.work/tmp') as directory:
            paths = {name: Path(directory) / f'{name}.json' for name in ('canonical', 'supplemental', 'skip_review', 'structural', 'review_evidence', 'output')}
            value = PolicyTests().config()
            value['product_receipt'] = str(paths['canonical'])
            value['scratch'] = directory
            value['functional'] = {name: str(path) for name, path in paths.items()}
            config = check.parse_config(value)

            def docker(command, scratch, *, timeout_seconds):
                user_index = command.index('--user')
                self.assertEqual(command[user_index + 1], f'{os.getuid()}:{os.getgid()}')
                self.assertLess(user_index, command.index('pinned-image'))
                self.assertEqual(scratch, config.scratch)
                self.assertEqual(timeout_seconds, 900)
                return 1, b'', b'dubious ownership'

            with (
                patch.object(check, 'functional_inputs', return_value=({}, {'reviewer': 'fixture'})),
                patch.object(check, 'load_canonical', return_value=SimpleNamespace(IMAGE='pinned-image')),
                patch.object(check, 'git', return_value=str(root / '.git')),
                patch.object(check, 'checked_docker', side_effect=docker),
                self.assertRaisesRegex(check.CheckError, 'metrics owner rejected current source'),
            ):
                check.functional(config)
            self.assertFalse(paths['output'].exists())


class AdapterDeadlineTests(unittest.TestCase):
    def test_operation_deadline_reaches_child_and_timeout_preserves_cleanup(self):
        scratch = check.parse_config(PolicyTests().config()).scratch
        for seconds, timed_out, resolved in [(60, False, True), (900, True, True), (900, True, False)]:
            with self.subTest(seconds=seconds, timed_out=timed_out, resolved=resolved), tempfile.TemporaryDirectory(dir=scratch) as directory:
                events = []
                primary = subprocess.TimeoutExpired(['docker', 'run'], seconds)
                class Child:
                    returncode = 0
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
                    else:
                        self.assertEqual(check.checked_docker(['docker', 'run'], Path(directory), timeout_seconds=seconds), (0, b'output', b''))
                    restore.assert_called_once_with(previous)
                self.assertEqual(events, [('communicate', seconds), ('cleanup',)])
                self.assertEqual(bool(list(Path(directory).iterdir())), not resolved)


class BaselineTests(unittest.TestCase):
    def setUp(self):
        self.config = check.parse_config(PolicyTests().config())

    def test_forced_receipt_requires_bound_mold_and_offline_local_context(self):
        mold = {'path': '/usr/local/bin/mold', 'sha256': 'e' * 64, 'size_bytes': 17, 'mode': '0o0755'}
        source = {'commit': 'a' * 40}
        binaries = {'xsh': {'sha256': 'b' * 64}}
        profile = {'toolchain': {'mold': {key: mold[key] for key in ('path', 'sha256')}}}
        command = ['docker', 'run', '--network', 'none', '--hostname', 'xsh-test', '--add-host', 'xsh-test:127.0.0.1', '-v', '/usr/local/bin/mold:/usr/local/bin/mold:ro', 'image']
        framed = {'inputs_before': {'source': source, 'binaries': binaries, 'mold': mold}, 'stage': {'command': command}}
        owner = SimpleNamespace(executable_snapshot=lambda _: mold, IMAGE='image')
        check.check_forced_context(framed, source, binaries, profile, owner)
        for mutation in ('missing_mold', 'mismatched_mold', 'changed_mold', 'missing_profile_mold', 'mismatched_profile_mold', 'network', 'hostname', 'hosts', 'duplicate_hosts', 'writable_mold', 'missing_mount'):
            with self.subTest(mutation=mutation):
                changed = deepcopy(framed)
                selected_profile = deepcopy(profile)
                selected_owner = owner
                if mutation == 'missing_mold':
                    del changed['inputs_before']['mold']
                elif mutation == 'mismatched_mold':
                    changed['inputs_before']['mold']['sha256'] = '0' * 64
                elif mutation == 'changed_mold':
                    selected_owner = SimpleNamespace(executable_snapshot=lambda _: {**mold, 'sha256': '0' * 64}, IMAGE='image')
                elif mutation == 'missing_profile_mold':
                    del selected_profile['toolchain']['mold']
                elif mutation == 'mismatched_profile_mold':
                    selected_profile['toolchain']['mold']['sha256'] = '0' * 64
                elif mutation in ('network', 'hostname', 'hosts'):
                    flag = {'network': '--network', 'hostname': '--hostname', 'hosts': '--add-host'}[mutation]
                    changed['stage']['command'][command.index(flag) + 1] = 'wrong'
                elif mutation == 'duplicate_hosts':
                    changed['stage']['command'][2:2] = ['--add-host', 'xsh-test:127.0.0.1']
                elif mutation == 'writable_mold':
                    changed['stage']['command'][command.index('-v') + 1] = '/usr/local/bin/mold:/usr/local/bin/mold:rw'
                elif mutation == 'missing_mount':
                    del changed['stage']['command'][command.index('-v'):command.index('-v') + 2]
                with self.assertRaises(check.CheckError):
                    check.check_forced_context(changed, source, binaries, selected_profile, selected_owner)
        for override in (
            ['--hostname=wrong'], ['--network=host'], ['--add-host=xsh-test:192.0.2.1'],
            ['--hostname'], ['--network'], ['--add-host'], ['-v'],
            ['--net', 'host'], ['--net=host'], ['-h', 'wrong'], ['-hwrong'],
            ['--volume', '/wrong:/usr/local/bin/mold:rw'], ['--volume=/wrong:/usr/local/bin/mold:rw'],
            ['-v/wrong:/usr/local/bin/mold:rw'], ['-v=/wrong:/usr/local/bin/mold:rw'],
            ['--mount', 'type=bind,src=/wrong,dst=/usr/local/bin/mold'],
            ['--mount=type=bind,src=/wrong,dst=/usr/local/bin/mold'],
        ):
            with self.subTest(override=override):
                changed = deepcopy(framed)
                changed['stage']['command'][-1:-1] = override
                with self.assertRaises(check.CheckError):
                    check.check_forced_context(changed, source, binaries, profile, owner)
                child_arguments = deepcopy(framed)
                child_arguments['stage']['command'].extend(override)
                check.check_forced_context(child_arguments, source, binaries, profile, owner)

    def test_measure_discards_the_whole_batch_after_readiness_failure_and_preserves_primary(self):
        self.config.scratch.mkdir(parents=True, exist_ok=True)
        runtime = check.baseline_util_linux(self.config)
        import acceptance
        sampler = check.baseline_owner(self.config, 'bench/consolidation/run.py', 'comparative_bench')
        for operation_fails in (False, True):
            with self.subTest(operation_fails=operation_fails), tempfile.TemporaryDirectory(dir=self.config.scratch) as directory:
                root = Path(directory)
                fixtures = root / 'fixtures'
                fixtures.mkdir()
                output = root / 'samples.json'
                profile_path = root / 'profile.json'
                profile_path.write_text(json.dumps({'native_image': {'image_id': 'pinned-image'}}))
                binaries = {}
                for name in ('xsh', 'xsht'):
                    binary = root / name
                    binary.write_text(name)
                    binaries[name] = {'path': str(binary)}
                config = replace(self.config, baseline={'profile': profile_path}, measurement={'fixtures': fixtures, 'output': output})
                events = []
                def ready(after=False):
                    events.append('after' if after else 'before')
                    return {'exit_code': 1 if after else 0}
                def docker(command, scratch, *, timeout_seconds):
                    arguments = command[command.index('-c') + 2:]
                    self.assertEqual(arguments[:4], [str(config.source_root), str(fixtures), binaries['xsh']['path'], binaries['xsht']['path']])
                    self.assertEqual(arguments[5], str(output))
                    self.assertEqual(timeout_seconds, 900)
                    mode = arguments[6]
                    events.append(mode)
                    if mode == 'diagnostic':
                        output.write_text(json.dumps({'complete': False, 'attribution_complete': True, 'samples': []}))
                    else:
                        report = check.read_json(output)
                        report.update(timing_complete=True, samples=[{'workload': 'first', 'wall_ns': 17}, {'workload': 'last', 'wall_ns': 23}])
                        output.write_text(json.dumps(report))
                        if operation_fails:
                            raise RuntimeError('primary Docker failure')
                    return 0, b'', b''
                fixture_owner = SimpleNamespace(PACKAGE_ROW_SHAPES=(2048,), generate=lambda path, **_: path.mkdir(), snapshot=lambda _: {})
                def owner(config, relative, name):
                    if relative.endswith('/fixtures.py'):
                        return fixture_owner
                    if relative.endswith('/run.py'):
                        return sampler
                    if relative.endswith('/final-comparative.py'):
                        return SimpleNamespace(perf_gate_check=ready)
                    self.fail(f'unexpected owner: {relative}')
                with (patch.object(check, 'baseline_util_linux', return_value=runtime),
                      patch.object(acceptance, 'validate_close_report'),
                      patch.object(check, 'baseline_receipts', return_value={}),
                      patch.object(check, 'measurement_profile', return_value={'binaries': binaries}),
                      patch.object(check, 'baseline_owner', side_effect=owner),
                      patch.object(check, 'load_canonical', return_value=SimpleNamespace(IMAGE='pinned-image')),
                      patch.object(sampler, 'validate_fixtures'),
                      patch.object(sampler, 'host_allowed_cpus', return_value=[0]),
                      patch.object(check, 'checked_docker', side_effect=docker)):
                    error = RuntimeError if operation_fails else check.CheckError
                    with self.assertRaisesRegex(error, 'primary Docker failure' if operation_fails else 'host busy') as caught:
                        check.measure(config, object())
                if operation_fails:
                    self.assertIn('post-measurement readiness failed', caught.exception.__notes__[0])
                self.assertEqual(events, ['diagnostic', 'before', 'timed', 'after'])
                report = check.read_json(output)
                self.assertFalse(report['complete'])
                self.assertTrue(report['timing_discarded'])
                self.assertEqual([row['wall_ns'] for row in report['samples']], [17, 23])
                self.assertEqual(report['performance_gate']['after']['exit_code'], 1)

    def laputa_root(self, root):
        root.mkdir()
        (root / 'pm').mkdir()
        (root / 'pm/generation.xsh').write_text('print "generation"\n')
        subprocess.run(['git', 'init', '--quiet', str(root)], check=True)
        subprocess.run(['git', '-C', str(root), 'add', 'pm/generation.xsh'], check=True)
        subprocess.run(['git', '-C', str(root), '-c', 'user.name=G1 fixture', '-c', 'user.email=g1-fixture@example.invalid', 'commit', '--quiet', '-m', 'test(performance): freeze Laputa corpus fixture'], check=True)
        return root

    def test_optional_baseline_requires_every_explicit_receipt(self):
        value = PolicyTests().config()
        self.assertIsNone(check.parse_config(value).baseline)
        paths = {name: str(self.config.source_root / '.work' / f'{name}.json') for name in ('functional_close', 'profile', 'observer', 'forced', 'comparative', 'scheduler')}
        value['baseline'] = paths
        self.assertEqual(check.parse_config(value).baseline, {name: Path(path) for name, path in paths.items()})
        for name in paths:
            invalid = deepcopy(value)
            del invalid['baseline'][name]
            with self.subTest(missing=name), self.assertRaises(check.CheckError):
                check.parse_config(invalid)
        value['baseline']['profile'] = 'relative.json'
        with self.assertRaises(check.CheckError):
            check.parse_config(value)

    def test_review_is_required_before_receipts_or_expensive_effects(self):
        runtime = check.baseline_util_linux(self.config)
        with patch.object(check, 'baseline_receipts', side_effect=AssertionError('receipt access')), patch.object(runtime, 'campaign_leases', side_effect=AssertionError('effects')):
            with self.assertRaisesRegex(check.CheckError, 'independent_review'):
                check.baseline(self.config, runtime)

    def test_standalone_cli_fails_without_live_review(self):
        error = StringIO()
        with patch.object(sys, 'argv', ['check.py', '--config', '/explicit/config.json', 'baseline', '--check']), patch.object(check, 'read_json', return_value=PolicyTests().config()), patch.object(check, 'baseline_receipts', side_effect=AssertionError('receipt access')), redirect_stderr(error):
            self.assertEqual(check.main(), 1)
        self.assertIn('active one-use util-linux independent_review context', error.getvalue())

    def test_actual_one_use_review_context_routes_only_owner_close(self):
        with tempfile.TemporaryDirectory(dir=self.config.scratch) as directory:
            laputa_root = self.laputa_root(Path(directory) / 'laputa')
            config = replace(self.config, laputa_root=laputa_root)
            runtime = check.baseline_util_linux(config)
            acceptance = sys.modules['acceptance']
            source = acceptance.SourceIdentity(config.commit, config.tree, True)
            review = acceptance.GateResult('adversarial-review', source, 0, 1, 1, 0, (), (), ('independent reviewed item',), ())
            events = []
            def close(actual):
                self.assertIs(actual, runtime)
                self.assertEqual(actual.adversarial_review(source), review)
                events.append('close')
            with patch.object(runtime, 'source_identity', return_value=source), patch.object(check, 'baseline_receipts', side_effect=lambda _: events.append('receipts') or {}), patch.object(runtime, 'campaign_leases', return_value=nullcontext()), patch.object(acceptance, 'close', side_effect=close):
                with runtime.independent_review(review):
                    check.baseline(config, runtime)
                    with self.assertRaisesRegex(check.CheckError, 'independent_review'):
                        check.baseline(config, runtime)
            self.assertEqual(events, ['receipts', 'close', 'receipts'])

    def test_close_dispatch_uses_the_configured_laputa_checkout(self):
        runtime = check.baseline_util_linux(self.config)
        acceptance = sys.modules['acceptance']
        resources = sys.modules['resources']
        release = sys.modules['release']
        source = acceptance.SourceIdentity(self.config.commit, self.config.tree, True)
        review = acceptance.GateResult('adversarial-review', source, 0, 1, 1, 0, (), (), ('independent reviewed item',), ())
        with tempfile.TemporaryDirectory(dir=self.config.scratch) as directory:
            laputa_root = self.laputa_root(Path(directory) / 'laputa')
            config = replace(self.config, laputa_root=laputa_root)
            def close(actual):
                self.assertIs(actual, runtime)
                self.assertEqual(resources._selected_laputa_corpus(), config.laputa_root)
                mounts = resources._cargo_mounts(runtime, config.scratch / 'target')
                environment = resources._cargo_environment()
                build_script = release._build_script(source)
                build_driver = resources._build_driver_source({}, acceptance.PairOrder.BASELINE_FIRST, 0)
                self.assertIn(
                    f'type=bind,source={laputa_root},target={laputa_root},readonly',
                    mounts,
                )
                self.assertIn(f'XSH_LAPUTA_CORPUS={laputa_root}', environment)
                self.assertIn(f'XSH_LAPUTA_CORPUS={laputa_root}', build_script)
                self.assertIn(f'"XSH_LAPUTA_CORPUS": {str(laputa_root)!r}', build_driver)
            with (
                patch.object(runtime, 'source_identity', return_value=source),
                patch.object(check, 'baseline_receipts', return_value={}),
                patch.object(runtime, 'campaign_leases', return_value=nullcontext()),
                patch.object(acceptance, 'close', side_effect=close),
                patch.object(resources.shutil, 'which', return_value='/usr/local/bin/kache'),
                patch.object(resources, 'MOLD_PATH', laputa_root / 'mold'),
                patch.object(resources.os, 'access', return_value=True),
                patch.object(Path, 'is_file', return_value=True),
                patch.object(Path, 'is_dir', return_value=True),
            ):
                with runtime.independent_review(review):
                    check.baseline(config, runtime)

    def test_missing_receipt_and_stale_source_fail(self):
        with tempfile.TemporaryDirectory(dir=self.config.scratch) as directory:
            path = Path(directory) / 'receipt.json'
            with self.assertRaises(FileNotFoundError):
                check.completed_receipt(path, 'schema', {})
            path.write_text(json.dumps({'schema': 1, 'state': 'complete', 'source': {'commit': 'wrong'}}))
            with self.assertRaisesRegex(check.CheckError, 'source mismatch'):
                check.completed_receipt(path, 'schema', {'commit': self.config.commit})
            path.write_text(json.dumps({'schema': 1, 'state': 'artifact-qualified', 'source': {}}))
            with self.assertRaisesRegex(check.CheckError, 'incomplete'):
                check.completed_receipt(path, 'schema', {})

    def test_laputa_owner_binding_matches_actual_clean_git_before_and_after(self):
        with tempfile.TemporaryDirectory(dir=self.config.scratch) as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '--quiet', str(root)], check=True)
            fixture = root / 'generation.xsh'
            fixture.write_text('print "original"\n')
            subprocess.run(['git', '-C', str(root), 'add', fixture.name], check=True)
            commit = ['git', '-C', str(root), '-c', 'user.name=G2 fixture', '-c', 'user.email=g2-fixture@example.invalid', 'commit', '--quiet', '-m', 'test(performance): freeze Laputa binding control']
            subprocess.run(commit, check=True)
            config = replace(self.config, laputa_root=root)
            identity = {'commit': check.git(root, 'rev-parse', 'HEAD'), 'git_tree': check.git(root, 'rev-parse', 'HEAD^{tree}'), 'status': 'clean'}
            binding = {'path': str(root), 'source_identity': identity, 'source_identity_after': deepcopy(identity)}
            self.assertEqual(check.validate_laputa_corpus(binding, config), identity)
            for field in binding:
                invalid = deepcopy(binding)
                del invalid[field]
                with self.subTest(missing=field), self.assertRaises(check.CheckError):
                    check.validate_laputa_corpus(invalid, config)
            for mutation in ('path', 'before_commit', 'after_tree', 'before_dirty', 'after_dirty'):
                invalid = deepcopy(binding)
                if mutation == 'path':
                    invalid['path'] = str(root / 'another')
                elif mutation == 'before_commit':
                    invalid['source_identity']['commit'] = 'f' * 40
                elif mutation == 'after_tree':
                    invalid['source_identity_after']['git_tree'] = 'e' * 40
                elif mutation == 'before_dirty':
                    invalid['source_identity']['status'] = 'dirty'
                else:
                    invalid['source_identity_after']['status'] = 'dirty'
                with self.subTest(mutation=mutation), self.assertRaises(check.CheckError):
                    check.validate_laputa_corpus(invalid, config)
            fixture.write_text('print "changed"\n')
            with self.assertRaisesRegex(check.CheckError, 'must be clean'):
                check.validate_laputa_corpus(binding, config)
            subprocess.run(['git', '-C', str(root), 'add', fixture.name], check=True)
            subprocess.run(commit, check=True)
            with self.assertRaisesRegex(check.CheckError, 'identity changed'):
                check.validate_laputa_corpus(binding, config)

    def test_timing_requires_both_readiness_boundaries(self):
        value = {'timing_accepted': True, 'timing_discarded': False, 'performance_gate': {'before': {'exit_code': 0}, 'after': {'exit_code': 0}}}
        check.accepted_timing(value, 'owner')
        for boundary in ('before', 'after'):
            for status in (None, False, 1):
                invalid = deepcopy(value)
                invalid['performance_gate'][boundary] = {'exit_code': status}
                with self.subTest(boundary=boundary, status=status), self.assertRaises(check.CheckError):
                    check.accepted_timing(invalid, 'owner')

    def scheduler_fixture(self, root, compare):
        def write(name, value):
            path = root / name
            path.write_text(json.dumps(value))
            return path
        source = {'commit': self.config.commit, 'source_diff_sha256': 'd' * 64, 'worktree_status': 'clean', 'tracked_file_count': 1, 'tracked_tree_sha256': 'e' * 64}
        binaries = {}
        for arm in ('reference', 'candidate'):
            directory = root / arm
            directory.mkdir()
            binaries[arm] = {}
            for name in compare.BINARIES:
                binary = directory / name
                binary.write_text(arm if name == 'xsht' else name)
                binary.chmod(0o755)
        binaries = compare.capture_binaries(root / 'reference', root / 'candidate')
        source_path = write('source.json', source)
        binary_path = write('binaries.json', binaries)
        manifest = root / 'manifest.txt'
        manifest.write_text('tests/example.xsh::test_output\n')
        record = {'schema_version': 1, 'state': 'complete', 'complete': True, 'source_root': str(self.config.source_root), 'source_commit_expected': self.config.commit, 'suite_round_count': 3, 'source_manifest': str(source_path), 'source_manifest_sha256': check.owned_digest(source_path), 'reference_source_manifest': str(source_path), 'reference_source_manifest_sha256': check.owned_digest(source_path), 'reference_source_root': str(self.config.source_root), 'binary_manifest': str(binary_path), 'binary_manifest_sha256': check.owned_digest(binary_path), 'manifest_path': str(manifest), 'manifest': {'ids_sha256': compare.manifest_sha256(compare.read_manifest(manifest)), 'file_sha256': check.owned_digest(manifest)}, 'stages': []}
        for arm in ('reference', 'candidate'):
            build = {'schema_version': 1, 'source_root': str(self.config.source_root), 'base_head': self.config.commit, 'source_diff_sha256': source['source_diff_sha256'], 'working_directory': str(self.config.source_root), 'target': compare.TARGET, 'profile': 'release', 'build_image': compare.PINNED_IMAGE, 'binaries': {name: {key: row[key] for key in ('path', 'sha256')} for name, row in binaries[arm].items()}, 'build_command': ['cargo', 'build', '--release']}
            build_path = write(f'{arm}-build.json', build)
            record[f'{arm}_build_receipt'] = check.receipt_binding(build_path)
            record[f'{arm}_build_receipt_content'] = build
        cgroup = root / 'cgroup.txt'
        cgroup.write_text('[cpu.max]\n3200000 100000\n[cpuset.cpus.effective]\n0-31\n[memory.max]\n42949672960\n[memory.current]\n1048576\n[memory.peak]\n2097152\n[memory.swap.current]\n0\n[memory.swap.max]\n0\n[memory.events]\noom 0\noom_kill 0\n[pids.current]\n4\n[pids.peak]\n7\n[pids.max]\nmax\n')
        record['suite_execution_order'] = [{'round': index + 1, 'pairs': list(compare.suite_execution_order(index))} for index in range(3)]
        for index in range(3):
            for arm in compare.suite_execution_order(index):
                label = f'round-{index + 1}-{arm}-suite'
                log = root / f'{label}.log'
                log.write_text('running 1 tests\ntests/example.xsh::test_output ... ok 1ms\ntest result: ok. 1 passed; 0 failed; 0 skipped\n')
                record['stages'].append({'mode': 'suite', 'pair': arm, 'label': label, 'timing_accepted': True, 'timing_discarded': False, 'performance_gate': {'before': {'exit_code': 0}, 'after': {'exit_code': 0}}, 'container_cleanup': {'resolved': True}, 'exit_code': 0, 'container_exit_code': 0, 'result_log': str(log), 'result_log_sha256': check.owned_digest(log), 'cgroup_before': str(cgroup), 'cgroup_after': str(cgroup), 'post_stage_validation': {'source': compare.source_summary(source), 'binaries': binaries, 'unchanged': True}})
        record['expected_default_workers'] = compare.WORKERS
        record['default_policy_help'] = {}
        for arm in ('reference', 'candidate'):
            text = f'{arm} worker policy'
            record['default_policy_help'][f'{arm}_expected_text'] = text
            for mode in ('list', 'help'):
                log = root / f'{arm}-{mode}.log'
                log.write_text(manifest.read_text() if mode == 'list' else text)
                stage = deepcopy(record['stages'][0])
                stage.update(mode=mode, pair=arm, label=f'{arm}-{mode}', result_log=str(log), result_log_sha256=check.owned_digest(log))
                record['stages'].append(stage)
        for stage in record['stages']:
            stage['artifact_hashes'] = {}
            for boundary in ('before', 'after'):
                suffix = f'.cgroup-{boundary}.txt'
                snapshot = root / f"{stage['label']}{suffix}"
                snapshot.write_text(cgroup.read_text())
                stage[f'cgroup_{boundary}'] = str(snapshot)
                stage['artifact_hashes'][suffix] = check.owned_digest(snapshot)
        return record, source, binaries

    def test_scheduler_replays_real_owner_log_and_build_validators(self):
        compare = check.baseline_owner(self.config, 'dev/consolidation/winddown/gate-tools/native-scheduler-comparison/compare.py', 'scheduler')
        with tempfile.TemporaryDirectory(dir=self.config.scratch) as directory, ExitStack() as stack:
            root = Path(directory)
            record, source, binaries = self.scheduler_fixture(root, compare)
            stack.enter_context(patch.object(compare, 'capture_source', return_value=source))
            stack.enter_context(patch.object(compare, 'capture_reference_source', return_value=source))
            stack.enter_context(patch.object(compare, 'validate_reference_overlay'))
            stack.enter_context(patch.object(compare, 'validate_reference_scheduler_ancestry'))
            path = root / 'receipt.json'
            profile = {'binaries': binaries['candidate']}
            def validate(value):
                path.write_text(json.dumps(value))
                return check.validate_scheduler(self.config, path, profile, compare)
            self.assertEqual(validate(record), record)
            for mutation in ('missing_round', 'duplicate_round', 'dirty_source', 'product', 'cleanup', 'readiness', 'failed', 'wrong_output'):
                invalid = deepcopy(record)
                if mutation == 'missing_round':
                    invalid['stages'].pop()
                elif mutation == 'duplicate_round':
                    invalid['stages'][-1] = invalid['stages'][0]
                elif mutation == 'dirty_source':
                    invalid['source_commit_expected'] = 'f' * 40
                elif mutation == 'product':
                    invalid['candidate_build_receipt_content']['binaries']['xsh']['sha256'] = 'f' * 64
                elif mutation == 'cleanup':
                    invalid['stages'][0]['container_cleanup']['resolved'] = False
                elif mutation == 'readiness':
                    invalid['stages'][0]['performance_gate']['after']['exit_code'] = 1
                elif mutation == 'failed':
                    invalid['stages'][0]['container_exit_code'] = 2
                else:
                    log = root / 'wrong.log'
                    log.write_text('running 1 tests\ntests/example.xsh::test_other ... ok 1ms\ntest result: ok. 1 passed; 0 failed; 0 skipped\n')
                    invalid['stages'][0].update(result_log=str(log), result_log_sha256=check.owned_digest(log))
                with self.subTest(mutation=mutation), self.assertRaises((check.CheckError, ValueError)):
                    validate(invalid)


if __name__ == '__main__':
    unittest.main()
