import base64
from copy import deepcopy
from dataclasses import replace
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

from dev.performance import frontend_cases as cases
from dev.performance import frontend_scenario, owned_case
from dev.performance import inference_measurement as measurement


class FrontendCasesTests(unittest.TestCase):
    def setUp(self):
        scratch = Path(__file__).resolve().parents[2] / '.work/tmp'
        scratch.mkdir(parents=True, exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def prepare(self, tier='edit'):
        directories = cases.prepare_cases(tier, root=self.root / 'sources')
        workload = cases.representatives(tier)[0]
        return workload, directories[workload.name]

    def evidence(self, workload, directories):
        runtime = {'status': 0, 'timed_out': False, 'effects': {}, 'stderr_base64': '',
                   'stdout_base64': base64.b64encode(workload.expected_stdout).decode()}
        phases = {'status': 0, 'stdout_bytes': len(workload.expected_stdout), 'stderr_bytes': 0,
                  'stdout_sha256': measurement.validate_output(runtime, workload.expected_stdout)['stdout_sha256']}
        for clock in ('wall_time_ns', 'cpu_time_ns'):
            phases[clock] = {'preparation_inclusive': 10, 'preparation_other_exclusive': 1}
        frontend = {}
        for variant, directory in directories.items():
            record = {
                'path': str(directory / workload.entry), 'semantic_retained_bytes': 80,
                'function_body_check_entries': 64, 'diagnostics': 0, 'lowered_blocker_events': 0,
                'reconcile_delta': 0, 'retained_after_drop_bytes': 120,
                'lowered_constructed_functions': 64,
                'inference_work': {name: 2 for name in measurement.METRIC_FIELDS},
                'stages': [{'name': name, 'alloc_count': 3, 'alloc_bytes': 40, 'retained_bytes': 120,
                            'peak_bytes': 200, 'item_count': 64, 'tracking_active': True}
                           for name in measurement.ALLOCATION_STAGES],
            }
            frontend[variant] = {'status': 0, 'timed_out': False, 'effects': {}, 'stderr_base64': '',
                                 'stdout_base64': base64.b64encode(json.dumps({'files': [record]}).encode()).decode()}
        return {'directories': directories, 'frontend_rows': frontend,
                'runtime_rows': {variant: deepcopy(runtime) for variant in measurement.VARIANTS},
                'phase_diagnostics': {variant: deepcopy(phases) for variant in measurement.VARIANTS}}

    def mutate_frontend(self, evidence, operation):
        value = json.loads(base64.b64decode(evidence['frontend_rows']['reduced']['stdout_base64']))
        operation(value['files'][0])
        evidence['frontend_rows']['reduced']['stdout_base64'] = base64.b64encode(json.dumps(value).encode()).decode()

    def test_frontend_scenario_quarantines_process_and_preserves_failure_status(self):
        marks = []
        lease = SimpleNamespace(prepare_cleanup_unresolved=lambda _: marks.append('launch'),
                                resolve_cleanup_unresolved=lambda: marks.append('clean'))
        def invoke(argv, directory, environment, timeout, *, on_launch, on_cleanup_verified):
            on_launch()
            on_cleanup_verified()
            return {'status': status, 'timed_out': False, 'effects': {}, 'stderr_base64': base64.b64encode(b'diagnostic detail').decode() if status else ''}
        sampler = frontend_scenario.Sampler(lease)
        with patch.object(owned_case.run, 'invoke', side_effect=invoke):
            status = 0
            sampler.invoke(['tool'], self.root, {}, 60)
            self.assertEqual(marks, ['launch', 'clean'])
            status = 1
            with self.assertRaisesRegex(owned_case.WorkloadFailure, 'diagnostic detail'):
                sampler.invoke(['tool'], self.root, {}, 60)
            status = 2
            with self.assertRaises(owned_case.WorkloadFailure):
                sampler.invoke(['tool'], self.root, {}, 60)
            status = 130
            with self.assertRaises(owned_case.WorkloadFailure):
                sampler.invoke(['tool'], self.root, {}, 60)

    def test_frontend_sampler_treats_unfinished_timeout_as_incomplete(self):
        lease = SimpleNamespace(prepare_cleanup_unresolved=lambda _: None, resolve_cleanup_unresolved=lambda: None)
        row = {'status': 0, 'timed_out': True, 'effects': {}, 'stderr_base64': ''}
        with patch.object(owned_case.run, 'invoke', return_value=row):
            with self.assertRaises(ValueError) as error:
                frontend_scenario.Sampler(lease).invoke(['tool'], self.root, {}, 60)
        self.assertIs(type(error.exception), ValueError)

    def test_frontend_sampler_rejects_wrong_phase_output_as_failure(self):
        lease = SimpleNamespace(prepare_cleanup_unresolved=lambda _: None, resolve_cleanup_unresolved=lambda: None)
        row = {'status': 0, 'timed_out': False, 'effects': {}, 'stderr_base64': '',
               'stdout_base64': base64.b64encode(b'wrong').decode()}
        sampler = frontend_scenario.Sampler(lease)
        sampler.expected_stdout = b'expected'
        with patch.object(owned_case.run, 'invoke', return_value=row):
            with self.assertRaisesRegex(owned_case.WorkloadFailure, 'output differs'):
                sampler.invoke(['tool'], self.root, {}, 60)

    def test_frontend_scenario_rejects_wrong_runtime_output(self):
        xsh = self.root / 'xsh'
        xsht = self.root / 'xsht'
        xsh.touch()
        xsht.touch()
        class Sampler:
            def __init__(self, leases):
                pass
            def invoke(self, argv, directory, environment, timeout):
                return {'stdout_base64': base64.b64encode(b'wrong output').decode()}
        with patch.object(frontend_scenario, 'Sampler', Sampler):
            with self.assertRaisesRegex(owned_case.WorkloadFailure, 'runtime output differs'):
                frontend_scenario.check_frontend(xsh, xsht, self.root, None)

    def test_closed_tiers_select_existing_small_generators_without_scaling_claim(self):
        edit = cases.representatives(cases.FrontendTier.EDIT)
        change = cases.representatives('change')
        self.assertEqual(len(edit), 2)
        self.assertEqual(len(change), 8)
        self.assertTrue(set(item.name for item in edit) < set(item.name for item in change))
        self.assertEqual({item.family for item in change}, {
            'dependency-chain', 'generic-stream', 'alias-chain', 'records',
            'higher-order-composition', 'recursive-effect-component', 'module-diamond', 'finite-overload-chain'})
        self.assertEqual({item.size for item in change}, {64})
        for tier in ('full', 'scenario', '', None):
            with self.subTest(tier=tier), self.assertRaises(ValueError):
                cases.representatives(tier)

    def test_import_diamond_sources_and_work_units_remain_bound_to_generator(self):
        directories = cases.prepare_cases('change', root=self.root / 'sources')
        for workload in cases.representatives('change'):
            cases.validate_sources(workload, directories[workload.name])
        workload = next(item for item in cases.representatives('change') if item.family == 'module-diamond')
        inputs = cases.diagnostic_inputs(workload, directories[workload.name]['reduced'],
                                         xsh=Path('/products/xsh'), xsht=Path('/products/xsht'), phase_report=self.root / 'phases')
        self.assertEqual(inputs['source_files'], 130)
        self.assertEqual(inputs['input_units'], 64)
        self.assertEqual(inputs['expected_stdout'], b'128\n')
        self.assertEqual(inputs['required_profile'], 'verification')
        self.assertEqual(inputs['frontend_argv'][1:3], ('frontend-stats', '--json'))
        self.assertEqual(inputs['phase_argv'][1:3], ('runtime-stats', '--json'))
        self.assertTrue(all('frontend' in control and 'drop' in control for control in inputs['required_disposal_controls']))

    def test_nonzero_after_drop_retention_is_valid_but_does_not_prove_disposal_execution(self):
        workload, directories = self.prepare()
        result = cases.validate_case_evidence(workload, **self.evidence(workload, directories))
        self.assertEqual(result['input_units'], 64)
        self.assertEqual(result['variants']['reduced']['frontend']['retained_after_frontend_drop_bytes'], 120)
        self.assertFalse(result['frontend_disposal_execution_complete'])
        self.assertFalse(result['scaling_complete'])
        self.assertEqual(result['logical_evidence_shape'], workload.logical_evidence_shape)

    def test_missing_or_mismatched_annotation_variants_reject(self):
        workload, directories = self.prepare()
        for field in ('directories', 'frontend_rows', 'runtime_rows', 'phase_diagnostics'):
            evidence = self.evidence(workload, directories)
            evidence[field] = {'annotated': evidence[field]['annotated']}
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'both annotation variants'):
                cases.validate_case_evidence(workload, **evidence)

    def test_runtime_and_phase_outputs_must_match_workload_specific_result(self):
        workload, directories = self.prepare()
        for field, key, value in (('runtime_rows', 'stdout_base64', base64.b64encode(b'42\n').decode()),
                                  ('frontend_rows', 'status', 2),
                                  ('frontend_rows', 'timed_out', True),
                                  ('frontend_rows', 'stderr_base64', base64.b64encode(b'failed').decode()),
                                  ('runtime_rows', 'effects', {'unexpected': 'file'}),
                                  ('phase_diagnostics', 'stdout_sha256', 'unrelated'),
                                  ('phase_diagnostics', 'stdout_bytes', 0)):
            evidence = self.evidence(workload, directories)
            evidence[field]['reduced'][key] = value
            with self.subTest(field=field, key=key), self.assertRaises(ValueError):
                cases.validate_case_evidence(workload, **evidence)

    def test_check_lower_allocation_and_disposal_failures_cannot_qualify(self):
        workload, directories = self.prepare()
        operations = (
            lambda row: row.update(diagnostics=1),
            lambda row: row.update(lowered_blocker_events=1),
            lambda row: row.update(reconcile_delta=1),
            lambda row: row['inference_work'].update(evidence_nodes=0),
            lambda row: row['inference_work'].pop('output_type_nodes'),
            lambda row: row['stages'][0].update(tracking_active=False),
            lambda row: row['stages'].append(deepcopy(row['stages'][0])),
            lambda row: row.update(retained_after_drop_bytes=0),
            lambda row: row.update(lowered_constructed_functions=63),
            lambda row: row.update(path='/different/main.xsh'),
        )
        for index, operation in enumerate(operations):
            evidence = self.evidence(workload, directories)
            self.mutate_frontend(evidence, operation)
            with self.subTest(index=index), self.assertRaises(ValueError):
                cases.validate_case_evidence(workload, **evidence)

    def test_source_or_oracle_mutation_and_links_reject(self):
        workload, directories = self.prepare()
        with self.assertRaisesRegex(ValueError, 'generator'):
            cases.validate_sources(replace(workload, expected_stdout=b'changed\n'), directories)
        entry = directories['reduced'] / workload.entry
        entry.write_text('print "64"\n')
        with self.assertRaisesRegex(ValueError, 'source map changed'):
            cases.validate_sources(workload, directories)
        entry.unlink()
        entry.symlink_to(directories['annotated'] / workload.entry)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            cases.validate_sources(workload, directories)

    def test_linked_parent_and_existing_inputs_are_never_overwritten(self):
        alias = self.root / 'alias'
        alias.symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'nonsymlink'):
            cases.prepare_cases('edit', root=alias / 'sources')
        workload, directories = self.prepare()
        with self.assertRaises(FileExistsError):
            cases.prepare_cases('edit', root=self.root / 'sources')
        cases.validate_sources(workload, directories)


if __name__ == '__main__':
    unittest.main()
