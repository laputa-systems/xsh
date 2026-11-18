import base64
import json
from pathlib import Path
import tempfile
import unittest

from dev.performance import inference_measurement as measurement
from dev.performance.inference_measurement import (
    ALLOCATION_STAGES,
    METRIC_FIELDS,
    canonical_allocation_traffic,
    canonical_inference_metrics,
    frontend_json,
    source_map_snapshot,
    source_maps,
    validate_output,
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


CHECK_TIMING = b'xsht check: 1 file in 0.01s (thread time by stage: discover 0.00s, load 0.00s, check 0.00s, lower 0.00s)\n'




class InferenceMeasurementTests(unittest.TestCase):
    def test_check_stage_accepts_canonical_timing_footer_and_retains_raw_stderr(self):
        summaries = (CHECK_TIMING, CHECK_TIMING.replace(b'1 file', b'3 files').replace(b'0.01s', b'0.02s'))
        for summary in summaries:
            with self.subTest(summary=summary):
                result = validate_output(observation(stdout=b'', stderr=summary), b'', stage='check')
                self.assertEqual(result['stderr_bytes'], len(summary))
                self.assertEqual(result['stderr_sha256'], measurement.hashlib.sha256(summary).hexdigest())

    def test_check_footer_cannot_hide_diagnostics_failures_or_runtime_stderr(self):
        invalid = (
            observation(stdout=b'', status=2, stderr=CHECK_TIMING),
            observation(stdout=b'unexpected', stderr=CHECK_TIMING),
            observation(stdout=b'', stderr=b'err[check.type-mismatch]: bad input\n' + CHECK_TIMING),
            observation(stdout=b'', stderr=CHECK_TIMING + b'extra\n'),
            observation(stdout=b'', stderr=CHECK_TIMING + CHECK_TIMING),
            observation(stdout=b'', stderr=CHECK_TIMING.rstrip(b'\n')),
            observation(stdout=b'', stderr=CHECK_TIMING.replace(b'1 file', b'0 files')),
            observation(stdout=b'', stderr=CHECK_TIMING.replace(b'1 file', b'1 files')),
            observation(stdout=b'', stderr=CHECK_TIMING.replace(b', lower 0.00s', b'')),
            observation(stdout=b'', stderr=CHECK_TIMING.replace(b'xsht check:', b'xsht lint:')),
        )
        for row in invalid:
            with self.subTest(row=row), self.assertRaises(ValueError):
                validate_output(row, b'', stage='check')
        with self.assertRaises(ValueError):
            validate_output(observation(stdout=b'7\n', stderr=CHECK_TIMING), b'7\n', stage='runtime')

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
        parsed = canonical_inference_metrics(frontend_json(json.dumps(complete).encode(), Path('/src/main.xsh')))
        self.assertEqual(parsed['evidence_nodes'], 2)
        with self.assertRaisesRegex(ValueError, 'exact entry root'):
            frontend_json(json.dumps(complete).encode(), Path('/src/other.xsh'))

    def test_reference_reader_accepts_old_stats_without_new_solver_payload(self):
        value = frontend_result(counters=False, retained=31)
        self.assertEqual(len(canonical_allocation_traffic(value)), len(ALLOCATION_STAGES))

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


if __name__ == '__main__':
    unittest.main()
