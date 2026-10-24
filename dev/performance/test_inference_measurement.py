import base64
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

from dev.performance import check as check_owner
from dev.performance import inference_measurement as measurement
from dev.performance.inference_measurement import (
    ALLOCATION_STAGES,
    METRIC_FIELDS,
    SCALING_METRICS,
    allocation_traffic_complete,
    alternate_pairs,
    bracketed_measurement,
    canonical_allocation_traffic,
    canonical_inference_metrics,
    canonical_retained_bytes,
    check_budget,
    check_scaling,
    collect_frontend_metrics,
    collect_reference_frontend_metrics,
    frontend_metrics,
    qualify_measurements,
    run_depth_rejection_control,
    run_distribution,
    run_monomorphic_runtime_control,
    source_map_snapshot,
    source_maps,
    validate_observation,
)
from dev.performance.inference_workloads import fixed_inference_workloads


ROOT = Path(__file__).resolve().parents[2]


def observation(stdout=b'7\n', *, status=0, stderr=b'', effects=None, cpu=5, wall=8,
                rss=1024, timed_out=False, useful_time=2):
    return {
        'stdout_base64': base64.b64encode(stdout).decode(),
        'stderr_base64': base64.b64encode(stderr).decode(),
        'status': status,
        'timed_out': timed_out,
        'effects': {} if effects is None else effects,
        'cpu_ns': cpu,
        'wall_ns': wall,
        'max_rss_bytes': rss,
        'useful_stdout_observed': True,
        'first_useful_stdout_ns': useful_time,
    }


def allocation_stages(*, active=True):
    return [
        {'name': name, 'alloc_count': 3, 'alloc_bytes': 12, 'retained_bytes': 4,
         'peak_bytes': 8, 'item_count': 2, 'tracking_active': active}
        for name in ALLOCATION_STAGES
    ]


def frontend_result(entry='/src/main.xsh', *, counters=True, retained=7, active=True):
    record = {'path': entry, 'semantic_retained_bytes': retained,
              'function_body_check_entries': 4, 'stages': allocation_stages(active=active)}
    if counters:
        record['inference_work'] = {name: 2 for name in METRIC_FIELDS}
    return {'files': [record]}


class InferenceMeasurementTests(unittest.TestCase):
    def test_old_artifact_missing_schema_work_is_unavailable(self):
        value = {'files': [{'semantic_retained_bytes': 1, 'function_body_check_entries': 1,
                            'inference_work': {name: 0 for name in METRIC_FIELDS
                                               if name != 'generalization_binding_clones'}}]}
        with self.assertRaisesRegex(ValueError, 'unavailable.*generalization_binding_clones'):
            canonical_inference_metrics(value)

    def test_candidate_counter_reader_requires_canonical_entry_and_full_solver_shape(self):
        with self.assertRaisesRegex(ValueError, 'one entry-root'):
            canonical_inference_metrics({'totals': {'semantic_type_count': 4}})
        incomplete = {'files': [{'path': '/src/main.xsh', 'semantic_retained_bytes': 7,
                                'function_body_check_entries': 1,
                                'inference_work': {name: 0 for name in METRIC_FIELDS if name != 'evidence_nodes'}}]}
        with self.assertRaisesRegex(ValueError, 'evidence_nodes'):
            canonical_inference_metrics(incomplete)
        complete = frontend_result()
        parsed = frontend_metrics(json.dumps(complete).encode(), Path('/src/main.xsh'))
        self.assertEqual(parsed['retained_typing_bytes'], 7)
        self.assertEqual(parsed['evidence_nodes'], 2)
        self.assertEqual(parsed['function_body_check_entries'], 4)
        with self.assertRaisesRegex(ValueError, 'exact entry root'):
            frontend_metrics(json.dumps(complete).encode(), Path('/src/other.xsh'))

    def test_reference_reader_accepts_old_stats_without_new_solver_payload(self):
        value = frontend_result(counters=False, retained=31)
        self.assertEqual(canonical_retained_bytes(value), 31)
        self.assertEqual(len(canonical_allocation_traffic(value)), len(ALLOCATION_STAGES))
        with self.assertRaisesRegex(ValueError, 'retained-typing metric'):
            canonical_retained_bytes({'files': [{'path': '/src/main.xsh'}]})

    def test_runtime_phase_diagnostic_reads_uninstrumented_process_output_and_phase_clocks(self):
        (ROOT / '.work/tmp').mkdir(parents=True, exist_ok=True)
        phases = {
            'preparation_inclusive': 31,
            'project_parse_load_exclusive': 3,
            'check_solve_fact_build_exclusive': 7,
            'declaration_publication_exclusive': 5,
            'lower_verify_install_exclusive': 4,
            'preparation_other_exclusive': 12,
            'execution_finish_exclusive': 19,
            'diagnostic_finish_exclusive': None,
        }
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as temporary:
            report_path = Path(temporary) / 'phases.json'

            class Sampler:
                def invoke(self, argv, cwd, environment, timeout):
                    self.call = (argv, cwd, timeout)
                    report_path.write_text(json.dumps({
                        'status': 0, 'stdout_bytes': 2, 'stderr_bytes': 0,
                        'tracking_active': True,
                        'wall_time_ns': {'clock': 'monotonic_wall'} | phases,
                        'cpu_time_ns': {'clock': 'process_cpu'} | phases,
                    }))
                    return observation(stdout=b'7\n')

            sampler = Sampler()
            result = measurement.collect_runtime_phase_diagnostic(
                '/tool/xsht', Path('/input/main.xsh'), report_path, sampler, {}, b'7\n',
            )
        self.assertEqual(sampler.call[0][:3], ['/tool/xsht', 'runtime-stats', '--json'])
        self.assertEqual(result['wall_time_ns']['preparation_inclusive'], 31)
        self.assertEqual(result['cpu_time_ns']['execution_finish_exclusive'], 19)
        self.assertEqual(result['status'], 0)

    def test_exact_entry_frontend_stats_returns_candidate_and_reference_contracts(self):
        candidate_stats = frontend_result('/input/main.xsh')
        reference_stats = frontend_result('/input/main.xsh', counters=False, retained=11)

        class Sampler:
            def __init__(self, value):
                self.value = value

            def invoke(self, argv, cwd, environment, timeout):
                self.call = (argv, cwd, timeout)
                return {'status': 0, 'timed_out': False, 'stderr_base64': '',
                        'stdout_base64': base64.b64encode(json.dumps(self.value).encode()).decode()}

        candidate = Sampler(candidate_stats)
        parsed = collect_frontend_metrics('/product/xsht', Path('/input'), candidate, {}, timeout=31)
        self.assertEqual(candidate.call, (['/product/xsht', 'frontend-stats', '--json', '/input/main.xsh'],
                                          Path('/input'), 31))
        self.assertEqual(parsed['inference_work']['output_type_nodes'], 2)
        self.assertTrue(all(item['tracking_active'] for item in parsed['allocation_traffic'].values()))
        reference = Sampler(reference_stats)
        parsed_reference = collect_reference_frontend_metrics('/ref/xsht', Path('/input'), reference, {})
        self.assertEqual(parsed_reference['retained_typing_bytes'], 11)
        self.assertNotIn('inference_work', parsed_reference)
        self.assertEqual(reference.call[0][-1], '/input/main.xsh')

    def test_disabled_allocation_tracking_is_reported_incomplete(self):
        record = {'candidate': {
            'annotated': {'allocation_traffic': canonical_allocation_traffic(frontend_result(active=True))},
            'reduced': {'allocation_traffic': canonical_allocation_traffic(frontend_result(active=False))},
        }}
        result = allocation_traffic_complete({'work': {'frontend_expectation': 'unprobed',
                                                       'check': {'inference_work': record}}})
        self.assertFalse(result['candidate_complete'])
        self.assertEqual(len(result['records']), 2)

    def test_observation_requires_status_exact_output_effects_and_sample_accounting(self):
        accepted = validate_observation(observation(), b'7\n')
        self.assertEqual((accepted['status'], accepted['stdout_bytes'], accepted['stderr_bytes']), (0, 2, 0))
        self.assertEqual(accepted['cpu_ns'], 5)
        for row in (observation(status=1), observation(stdout=b'wrong'), observation(stderr=b'noise'),
                    observation(effects={'created': True}), observation(timed_out=True),
                    observation(cpu=None), observation(wall=None), observation(rss=None)):
            with self.subTest(row=row), self.assertRaises(ValueError):
                validate_observation(row, b'7\n')
        with self.assertRaisesRegex(ValueError, 'first useful output'):
            validate_observation(observation(useful_time=None), b'7\n', require_useful=True)

    def test_depth_rejection_control_rejects_parser_failure_and_requires_both_compilers(self):
        workload = next(case for case in fixed_inference_workloads()
                        if case.family == 'records' and case.shape == 'deep' and case.size == 512)
        calls = []

        def invoke(argv, cwd, environment, timeout):
            calls.append((argv, cwd, timeout))
            return observation(stdout=b'', status=2,
                               stderr=b'parameter inference exceeds the type depth limit')

        directories = {'annotated': Path('/input/annotated'), 'reduced': Path('/input/reduced')}
        products = {'candidate': {'xsht': '/candidate/xsht'}, 'reference': {'xsht': '/reference/xsht'}}
        result = run_depth_rejection_control(workload, directories, products,
                                             SimpleNamespace(invoke=invoke), {})
        self.assertEqual(set(result['expected_rejection_control']), {
            'reference_annotated', 'reference_reduced', 'candidate_annotated', 'candidate_reduced'})
        self.assertEqual(len(calls), 4)
        self.assertEqual(calls[0][0][:2], ['/reference/xsht', 'check'])
        self.assertEqual(calls[-1][0][:2], ['/candidate/xsht', 'check'])

        def wrong_exit(*_args, **_kwargs):
            return observation(stdout=b'', status=1,
                               stderr=b'parameter inference exceeds the type depth limit')

        wrong_status = run_depth_rejection_control(workload, directories, products,
                                                   SimpleNamespace(invoke=wrong_exit), {})
        self.assertFalse(any(row['valid'] for row in wrong_status['expected_rejection_control'].values()))

        def parser_error(*_args, **_kwargs):
            return observation(stdout=b'', status=1, stderr=b'err[parse.nesting-depth]')

        invalid = run_depth_rejection_control(workload, directories, products,
                                              SimpleNamespace(invoke=parser_error), {})
        self.assertFalse(any(row['valid'] for row in invalid['expected_rejection_control'].values()))
        self.assertTrue(all('parse.nesting-depth' in row['stderr_text']
                            for row in invalid['expected_rejection_control'].values()))

    def test_alternating_pairs_keep_raw_observations_and_candidate_reference_order(self):
        calls = []

        def run(arm):
            calls.append(arm)
            offset = len(calls)
            return {'row': observation(cpu=offset, wall=offset + 2, rss=100 + offset),
                    'expected_stdout': b'7\n'}

        result = alternate_pairs(100, run, ('reference', 'candidate'))
        self.assertEqual(len(calls), 200)
        self.assertEqual(calls[:6], ['reference', 'candidate', 'candidate', 'reference', 'reference', 'candidate'])
        self.assertEqual(result['pairs'], 100)
        reference = result['arms']['reference']
        self.assertEqual(reference['summaries']['cpu_ns'], {
            'samples': 100, 'median': 100.5, 'p95': 189, 'range': [1, 200], 'spread': 199})
        self.assertEqual(len(reference['samples']), 100)
        self.assertEqual(len(result['paired_differences']['wall_ns']['samples']), 100)

    def test_difference_inside_observed_spread_is_inconclusive(self):
        self.assertEqual(check_budget(100, 109, fraction=.05, absolute=1, spread=12)['status'], 'inconclusive')
        self.assertEqual(check_budget(100, 109, fraction=.10, absolute=1, spread=8)['status'], 'inconclusive')
        self.assertEqual(check_budget(100, 109, fraction=.10, absolute=1, spread=1)['status'], 'pass')
        self.assertEqual(check_budget(100, 130, fraction=.10, absolute=1, spread=8)['status'], 'fail')
        self.assertEqual(check_budget(100, 105, fraction=.10, absolute=1, spread=None)['status'], 'pass')

    def test_post_readiness_failure_marks_completed_window_discarded(self):
        events = []
        report = {'timing_discarded': False, 'samples_complete': True, 'timing_complete': True}

        def ready(after):
            events.append(('ready', after))
            if after:
                raise ValueError('host became busy')

        with self.assertRaisesRegex(ValueError, 'host became busy'):
            bracketed_measurement(ready, lambda: events.append(('identity', None)),
                                  lambda: events.append(('operation', None)) or {'samples': [1]},
                                  discard=lambda: (events.append(('discard', None)),
                                                   report.update(timing_discarded=True,
                                                                 samples_complete=False,
                                                                 timing_complete=False)))
        self.assertIn(('operation', None), events)
        self.assertEqual(events[-1], ('discard', None))
        self.assertEqual(report, {'timing_discarded': True, 'samples_complete': False,
                                  'timing_complete': False})

    def test_scaling_checks_each_counter_per_independent_output_node_and_requires_every_doubling(self):
        rows = {}
        for size in (64, 128, 256):
            metrics = {name: 10 for name in METRIC_FIELDS}
            metrics['output_type_nodes'] = size
            metrics['evidence_nodes'] = size
            metrics['function_body_check_entries'] = size
            rows[str(size)] = {
                'family': 'alias-chain', 'shape': 'chain', 'size': size,
                'frontend_expectation': 'unprobed',
                'inference_work': {'candidate': {
                    'annotated': {'inference_work': metrics},
                    'reduced': {'inference_work': metrics.copy()},
                }},
            }
        passed = check_scaling(rows)
        self.assertTrue(passed['passed'])
        self.assertEqual(len(passed['comparisons']), 2 * 2 * len(SCALING_METRICS))
        rows['128']['inference_work']['candidate']['reduced']['inference_work']['host_candidate_trials'] = 100_000
        failed = check_scaling(rows)
        self.assertFalse(failed['passed'])
        self.assertFalse(next(row for row in failed['comparisons']
                              if row['variant'] == 'reduced' and row['metric'] == 'host_candidate_trials')['passed'])
        rows['128']['inference_work'] = {}
        missing = check_scaling(rows)
        self.assertFalse(missing['passed'])
        self.assertTrue(missing['missing'])

    def test_materialized_module_graph_snapshot_detects_added_or_changed_source(self):
        workload = next(case for case in fixed_inference_workloads()
                        if case.family == 'module-diamond' and case.size == 64)
        (ROOT / '.work/tmp').mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as temporary:
            root = Path(temporary) / 'sources'
            paths = source_maps((workload,), root)
            for variant, expected in (('annotated', workload.annotated_sources),
                                      ('reduced', workload.reduced_sources)):
                directory = paths[workload.name][variant]
                snapshot = source_map_snapshot(directory)
                self.assertEqual(set(snapshot), set(expected))
                self.assertEqual({name: (directory / name).read_text() for name in expected}, dict(expected))
                (directory / 'extra.xsh').write_text('let extra = 1\n')
                self.assertNotEqual(source_map_snapshot(directory), snapshot)

    def test_distribution_uses_candidate_and_reference_products_and_checks_every_output(self):
        workload = next(case for case in fixed_inference_workloads()
                        if case.family == 'dependency-chain' and case.size == 64)
        calls = []

        def invoke(argv, cwd, environment, timeout, *, measured=False, useful_prefix=None):
            calls.append((argv, cwd, measured, useful_prefix))
            output = workload.expected_stdout if argv[0].endswith('/xsh') else b''
            return observation(stdout=output)

        directories = {'annotated': Path('/sources/annotated'), 'reduced': Path('/sources/reduced')}
        products = {
            'candidate': {'xsht': '/candidate/xsht', 'xsh': '/candidate/xsh'},
            'reference': {'xsht': '/reference/xsht', 'xsh': '/reference/xsh'},
        }
        result = run_distribution(workload, directories, products, SimpleNamespace(invoke=invoke), {},
                                  stage='runtime', count=2,
                                  arms={'reference': ('reference', 'annotated'),
                                        'candidate': ('candidate', 'annotated')})
        self.assertEqual(result['pairs'], 2)
        self.assertEqual(len(calls), 4)
        self.assertEqual(calls[0][0][:2], ['/reference/xsh', '/sources/annotated/main.xsh'])
        self.assertEqual(calls[1][0][:2], ['/candidate/xsh', '/sources/annotated/main.xsh'])
        self.assertTrue(all(measured for _, _, measured, _ in calls))
        self.assertTrue(all(prefix == workload.expected_stdout.splitlines(keepends=True)[0]
                            for _, _, _, prefix in calls))

    def test_check_distribution_measures_silent_success_for_candidate_annotation_arms(self):
        workload = next(case for case in fixed_inference_workloads()
                        if case.family == 'alias-chain' and case.size == 64)
        calls = []

        def invoke(argv, cwd, environment, timeout, *, measured=False, useful_prefix=None):
            calls.append((argv, cwd, measured, useful_prefix))
            return observation(stdout=b'')

        directories = {'annotated': Path('/sources/annotated'), 'reduced': Path('/sources/reduced')}
        products = {'candidate': {'xsht': '/candidate/xsht', 'xsh': '/candidate/xsh'}}
        result = run_distribution(workload, directories, products, SimpleNamespace(invoke=invoke), {},
                                  stage='check', count=1,
                                  arms={'annotated': ('candidate', 'annotated'),
                                        'reduced': ('candidate', 'reduced')})
        self.assertEqual(result['stage'], 'check')
        self.assertEqual(result['pairs'], 1)
        self.assertEqual(calls[0][0], ['/candidate/xsht', 'check', '/sources/annotated/main.xsh'])
        self.assertEqual(calls[1][0], ['/candidate/xsht', 'check', '/sources/reduced/main.xsh'])
        self.assertTrue(all(measured and prefix is None for _, _, measured, prefix in calls))

    def test_monomorphic_runtime_control_uses_source_identical_existing_benchmark(self):
        (ROOT / '.work/tmp').mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as temporary:
            root = Path(temporary)
            relative = Path('bench/stdlib-port/cold_trivial.xsh')
            for name in ('candidate', 'reference'):
                source = root / name / relative
                source.parent.mkdir(parents=True)
                source.write_bytes(b'print "hello"\n')
            calls = []

            def invoke(argv, cwd, environment, timeout, *, measured=False, useful_prefix=None):
                calls.append((argv, measured, useful_prefix))
                return observation(stdout=b'hello\n')

            products = {'candidate': {'xsh': '/candidate/xsh'},
                        'reference': {'xsh': '/reference/xsh'}}
            result = run_monomorphic_runtime_control(root / 'candidate', root / 'reference', products,
                                                     SimpleNamespace(invoke=invoke), {}, count=5)
            self.assertEqual(result['pairs'], 5)
            self.assertEqual(len(calls), 10)
            self.assertEqual(calls[0][0][0], '/reference/xsh')
            self.assertEqual(calls[1][0][0], '/candidate/xsh')
            self.assertTrue(all(measured and prefix == b'hello\n' for _, measured, prefix in calls))

    def test_measurement_completion_does_not_imply_qualification_when_reference_lacks_support(self):
        workloads = {}
        for size in (64, 128, 256):
            metrics = {name: 2 for name in METRIC_FIELDS}
            metrics['output_type_nodes'] = size
            metrics['evidence_nodes'] = size
            metrics['function_body_check_entries'] = size
            allocation = canonical_allocation_traffic(frontend_result())
            frontend = {
                'candidate': {
                    'annotated': {'inference_work': metrics | {'retained_typing_bytes': 64},
                                  'allocation_traffic': allocation},
                    'reduced': {'inference_work': metrics.copy() | {'retained_typing_bytes': 64},
                                'allocation_traffic': allocation},
                },
                'reference': {},
            }
            stage_check = {
                'stage': 'check', 'reference_support': {'observed': True, 'supported': False, 'status': 2},
                'inference_work': frontend,
                'phase_diagnostics': {'candidate_annotated': phase_diagnostic_fixture(),
                                      'candidate_reduced': phase_diagnostic_fixture()},
                'distributions': {'candidate_annotation': distribution(100, 1000, 1001, 0)},
            }
            stage_runtime = {
                'stage': 'runtime', 'reference_support': {'observed': True, 'supported': False, 'status': 2},
                'distributions': {'candidate_annotation': distribution(5, 1000, 1001, 0)},
            }
            workloads[f'case-{size}'] = {
                'family': 'alias-chain', 'shape': 'chain', 'size': size,
                'frontend_expectation': 'unprobed', 'check': stage_check, 'runtime': stage_runtime,
            }
        mono = distribution(5, 1000, 1001, 0) | {'name': 'cold_trivial'}
        qualified = qualify_measurements(
            workloads, monomorphic_runtime=mono,
            binary_sizes={'reference': {'xsh': 100, 'xsht': 100},
                          'candidate': {'xsh': 100, 'xsht': 100}},
        )
        self.assertTrue(qualified['samples_complete'])
        self.assertTrue(qualified['runtime_complete'])
        self.assertTrue(qualified['counters_complete'])
        self.assertFalse(qualified['qualification_complete'])
        self.assertTrue(qualified['phase_diagnostics']['complete'])
        self.assertEqual(len(qualified['baseline_unsupported']), 6)

    def test_spread_resolving_supported_reference_can_qualify_when_all_budgets_pass(self):
        workloads = {}
        for size in (64, 128, 256):
            metrics = {name: 2 for name in METRIC_FIELDS}
            metrics['output_type_nodes'] = size
            metrics['evidence_nodes'] = size
            metrics['function_body_check_entries'] = size
            allocation = canonical_allocation_traffic(frontend_result())
            candidate_annotated = {'inference_work': metrics | {'retained_typing_bytes': 64},
                                   'allocation_traffic': allocation}
            candidate_reduced = {'inference_work': metrics.copy() | {'retained_typing_bytes': 64},
                                 'allocation_traffic': allocation}
            reference_annotated = {'retained_typing_bytes': 64, 'allocation_traffic': allocation}
            check_stage = {
                'stage': 'check', 'reference_support': {'observed': True, 'supported': True},
                'phase_diagnostics': {'candidate_annotated': phase_diagnostic_fixture(),
                                      'candidate_reduced': phase_diagnostic_fixture(),
                                      'reference_annotated': phase_diagnostic_fixture()},
                'inference_work': {'candidate': {'annotated': candidate_annotated,
                                                 'reduced': candidate_reduced},
                                   'reference': {'annotated': reference_annotated}},
                'distributions': {
                    'candidate_annotation': distribution(100, 1000, 1100, 0),
                    'reference_candidate_annotated': distribution(100, 1000, 1010, 0),
                },
            }
            runtime_stage = {
                'stage': 'runtime', 'reference_support': {'observed': True, 'supported': True},
                'distributions': {'candidate_annotation': distribution(5, 1000, 1010, 0),
                                  'reference_candidate_annotated': distribution(5, 1000, 1010, 0)},
            }
            workloads[f'case-{size}'] = {
                'family': 'alias-chain', 'shape': 'chain', 'size': size,
                'frontend_expectation': 'unprobed', 'check': check_stage, 'runtime': runtime_stage,
            }
        mono = distribution(5, 1000, 1010, 0) | {'name': 'cold_trivial'}
        qualified = qualify_measurements(
            workloads, monomorphic_runtime=mono,
            binary_sizes={'reference': {'xsh': 100, 'xsht': 100},
                          'candidate': {'xsh': 90, 'xsht': 90}},
        )
        self.assertTrue(qualified['samples_complete'])
        self.assertTrue(qualified['runtime_complete'])
        self.assertTrue(qualified['counters_complete'])
        self.assertTrue(qualified['phase_diagnostics']['complete'])
        self.assertTrue(qualified['qualification_complete'])
        workloads['case-64']['check'].pop('phase_diagnostics')
        without_phase_diagnostics = qualify_measurements(
            workloads, monomorphic_runtime=mono,
            binary_sizes={'reference': {'xsh': 100, 'xsht': 100},
                          'candidate': {'xsh': 90, 'xsht': 90}},
        )
        self.assertFalse(without_phase_diagnostics['phase_diagnostics']['complete'])
        self.assertFalse(without_phase_diagnostics['qualification_complete'])

    def test_host_cli_dispatches_only_to_the_performance_scoped_owner(self):
        config = object()
        with patch.object(check_owner, 'read_json', return_value={'inference_performance': {}}) as read, \
                patch.object(check_owner, 'parse_config', return_value=config) as parse, \
                patch.object(check_owner, 'inference_performance', return_value={'performance_complete': True}) as run:
            self.assertEqual(measurement.main(['--run-performance', '--config', '/work/config.json']), 0)
        read.assert_called_once_with(Path('/work/config.json'))
        parse.assert_called_once_with({'inference_performance': {}})
        run.assert_called_once_with(config)

    def test_module_cli_returns_nonzero_when_performance_owner_rejects_input(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as temporary:
            scratch = Path(temporary)
            config_path = scratch / 'config.json'
            value = {
                'schema': 1,
                'source_root': str(ROOT),
                'commit': 'a' * 40,
                'tree': 'b' * 40,
                'laputa_root': str(scratch / 'laputa'),
                'product_receipt': str(scratch / 'products.json'),
                'xsh': str(scratch / 'xsh'),
                'xsht': str(scratch / 'xsht'),
                'scratch': str(scratch),
                'functional': None,
                'inference_performance': {
                    'output': str(scratch / 'missing' / 'report.json'),
                    'reference': {
                        'source_root': str(scratch / 'reference'),
                        'commit': 'c' * 40,
                        'tree': 'd' * 40,
                        'profile_receipt': str(scratch / 'reference-profile.json'),
                    },
                },
            }
            config_path.write_text(json.dumps(value))
            result = subprocess.run(
                [sys.executable, '-B', '-m', 'dev.performance.inference_measurement',
                 '--run-performance', '--config', str(config_path)],
                cwd=ROOT, capture_output=True, text=True, check=False,
            )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('requires an existing scratch directory and fresh output', result.stderr)


def distribution(pairs, reference, candidate, spread):
    arms = {
        name: {'summaries': {
            'cpu_ns': {'median': value, 'p95': value, 'range': [value, value], 'spread': 0, 'samples': pairs},
            'wall_ns': {'median': value, 'p95': value, 'range': [value, value], 'spread': 0, 'samples': pairs},
            'max_rss_bytes': {'median': value, 'p95': value, 'range': [value, value], 'spread': 0, 'samples': pairs},
        }, 'samples': []}
        for name, value in (('reference', reference), ('candidate', candidate),
                            ('annotated', reference), ('reduced', candidate))
    }
    return {'pairs': pairs, 'arms': arms,
            'paired_differences': {
                measure: {'summary': {'spread': spread}, 'samples': [candidate - reference] * pairs}
                for measure in ('cpu_ns', 'wall_ns', 'max_rss_bytes')
            }}


def phase_diagnostic_fixture():
    phases = {'preparation_inclusive': 10, 'preparation_other_exclusive': 2}
    return {'status': 0, 'wall_time_ns': phases, 'cpu_time_ns': phases}


if __name__ == '__main__':
    unittest.main()
