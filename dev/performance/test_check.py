from copy import deepcopy
from contextlib import ExitStack, nullcontext, redirect_stderr
from dataclasses import replace
import importlib.util
from io import StringIO
import json
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('performance_check', Path(__file__).with_name('check.py'))
check = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = check
spec.loader.exec_module(check)


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
        steps = [
            {'id': 'verify-pinned-image-id', 'argv': ['docker', 'image', 'inspect', '--format', '{{.Id}}', 'pinned-image']},
            {'id': 'build-product-01-xsh', 'argv': ['/tools/run-native-final.sh', str(config.source_root), 'build', 'cargo', 'build', '-p', 'xsh']},
            {'id': 'warning-deny-01', 'argv': ['/tools/run-native-final.sh', str(config.source_root), 'lint', 'cargo', 'clippy']},
            {'id': 'linux-special-01-boundary', 'argv': ['docker', 'run', '--network=none', '--memory=1g', '--memory-swap=1g', '--cpus=12', '--user', '0:0', 'pinned-image', '/products/target/verification/xsht', 'test', 'boundary.xsh']},
            {'id': 'tracked-diff-check', 'argv': ['git', 'diff', '--check']},
        ]
        for step in steps:
            step.update(exit_code=0, timed_out=False)
        receipt = {'schema': 1, 'status': 'commands-passed-needs-privilege-skip-and-close-review', 'source_expected': canonical['source_expected'], 'source_checks': canonical['source_checks'], 'steps': steps, 'cargo_target_dirs': {'products': '/products'}, 'environment': plan['environment']}
        products = {name + '-verification': 'c' * 64 for name in ('xsh', 'xsht', 'xshi', 'xsh-fuzz', 'xsh-test-helper')}
        receipt['hashes'] = {name: products for name in ('products_after_initial_build', 'binaries_after_build', 'binaries_before_tests', 'binaries_after_tests')}
        receipt['hashes'].update({name: {'harness': 'd' * 64} for name in ('harnesses_before_tests', 'harnesses_after_tests', 'harness_variant_binaries_before_tests', 'harness_variant_binaries_after_tests')})
        check.check_supplemental_policy(receipt, plan, config)
        for name in ('linux-special-01-boundary', 'build-product-01-xsh', 'warning-deny-01'):
            wrong = deepcopy(receipt)
            wrong['steps'] = [step for step in wrong['steps'] if step['id'] != name]
            with self.assertRaisesRegex(check.CheckError, 'required supplemental step'):
                check.check_supplemental_policy(wrong, plan, config)
        wrong = deepcopy(receipt)
        wrong['steps'][2]['argv'][-1] = 'check'
        with self.assertRaisesRegex(check.CheckError, 'command mismatch'):
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



class BaselineTests(unittest.TestCase):
    def setUp(self):
        self.config = check.parse_config(PolicyTests().config())

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
        runtime = check.baseline_util_linux(self.config)
        acceptance = sys.modules['acceptance']
        source = acceptance.SourceIdentity(self.config.commit, self.config.tree, True)
        review = acceptance.GateResult('adversarial-review', source, 0, 1, 1, 0, (), (), ('independent reviewed item',), ())
        events = []
        def close(actual):
            self.assertIs(actual, runtime)
            self.assertEqual(actual.adversarial_review(source), review)
            events.append('close')
        with patch.object(runtime, 'source_identity', return_value=source), patch.object(check, 'baseline_receipts', side_effect=lambda _: events.append('receipts') or {}), patch.object(runtime, 'campaign_leases', return_value=nullcontext()), patch.object(acceptance, 'close', side_effect=close):
            with runtime.independent_review(review):
                check.baseline(self.config, runtime)
                with self.assertRaisesRegex(check.CheckError, 'independent_review'):
                    check.baseline(self.config, runtime)
        self.assertEqual(events, ['receipts', 'close', 'receipts'])

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
