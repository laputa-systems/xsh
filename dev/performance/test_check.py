from copy import deepcopy
import importlib.util
from pathlib import Path
import sys
import unittest

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



if __name__ == '__main__':
    unittest.main()
