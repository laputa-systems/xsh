import base64
import importlib.util
import os
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'bench/consolidation'))
from bench.consolidation.run import equivalent
from dev.performance import core_cases, inference_measurement, matrix


def observation(stdout):
    return {'status': 0, 'timed_out': False, 'stdout_base64': base64.b64encode(stdout).decode(),
            'stderr_base64': '', 'effects': {}}


class OracleMutants(unittest.TestCase):
    def test_core_text_and_json_outputs(self):
        fixtures = matrix.ROOT / 'dev/performance/oracle_mutants'
        fixtures.mkdir(exist_ok=True)
        for family in ('wc', 'sort', 'awk', 'traversal', 'json'):
            with self.subTest(family=family):
                case = core_cases.describe_case(f'core-{family}-small', fixtures=fixtures, source_root=matrix.ROOT)
                good = observation(case.expected_stdout)
                self.assertTrue(equivalent(good, case.expected_stdout))
                lines = case.expected_stdout.splitlines(keepends=True)
                wrong = b''.join(reversed(lines)) if len(lines) > 1 else case.expected_stdout[:-1]
                self.assertFalse(equivalent(observation(wrong), case.expected_stdout))
                self.assertFalse(equivalent(good | {'effects': {'leftover': 'file'}}, case.expected_stdout))

    def test_consolidation_goldens_reject_dropped_record(self):
        expected = b'first\nsecond\n'
        self.assertTrue(equivalent(observation(expected), expected))
        self.assertFalse(equivalent(observation(b'first\n'), expected))

    def test_inference_output_rejects_wrong_result(self):
        good = observation(b'64\n')
        inference_measurement.validate_output(good, b'64\n')
        with self.assertRaises(ValueError):
            inference_measurement.validate_output(observation(b'63\n'), b'64\n')

    def test_warm_rebuild_oracle_rejects_build(self):
        oracle = (matrix.ROOT / 'dev/performance/oracle_mutants/build_oracle.sh').read_text()
        adapters = '''
jaq() { python3 -c 'import json,sys; n=len(json.load(open(sys.argv[1]))["nodes"]); print(n) if n else sys.exit(1)' "$3"; }
rg() { python3 -c 'import re,sys; rows=open(sys.argv[3]).read().splitlines(); n=sum(bool(re.search(sys.argv[2], row)) for row in rows); print(n) if n and sys.argv[1] == "-c" else None; sys.exit(0 if n else 1)' "$@"; }
'''
        (matrix.ROOT / 'dev/performance/oracle_mutants').mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=matrix.ROOT / 'dev/performance/oracle_mutants') as directory:
            plan = Path(directory) / 'plan.json'
            log = Path(directory) / 'log'
            plan.write_text('{"nodes":[{},{}]}')
            def run(contents):
                log.write_text(contents + '\n')
                return subprocess.run(['bash', '-c', adapters + oracle + '\nbuild_oracle "$1" "$2"',
                                       'oracle', str(plan), str(log)], capture_output=True, timeout=10).returncode
            self.assertEqual(run('repo build abc123 2 artifacts'), 0)
            self.assertNotEqual(run('repo build start abc123\nrepo build abc123 2 artifacts'), 0)
            self.assertNotEqual(run('repo build abc123 1 artifacts'), 0)

    def test_parallel_archive_oracle_rejects_dropped_archive(self):
        assertions = (matrix.ROOT / 'dev/performance/oracle_mutants/parallel_assertions.xsh').read_text()
        triple = os.environ.get('TARGET', '')
        profile = os.environ.get('DIST_PROFILE', 'verification')
        candidates = [str(matrix.ROOT / 'target' / triple / profile / 'xsh'),
                      str(matrix.ROOT / 'target/verification/xsh'), str(matrix.ROOT / 'target/debug/xsh'), shutil.which('xsh')]
        binary = next((p for p in candidates if p and Path(p).is_file()), None)
        self.assertIsNotNone(binary, 'a built XSH executable is required to run the native oracle')
        mutants = matrix.ROOT / 'dev/performance/oracle_mutants'
        mutants.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=mutants) as directory:
            script = Path(directory) / 'parallel.xsh'
            def run(archives):
                fields = 'link_inputs: ["l"], generated_objects: ["g"], missing_sources: ["m"]'
                script.write_text('let serial = {archives: ["a", "b"], ' + fields + '}\n'
                                  + 'let parallel = {archives: ' + archives + ', ' + fields + '}\n'
                                  + assertions)
                return subprocess.run([binary, str(script)], capture_output=True, timeout=10).returncode
            self.assertEqual(run('["a", "b"]'), 0)
            self.assertNotEqual(run('["a"]'), 0)

    def test_service_oracle_rejects_stale_pid(self):
        spec = importlib.util.spec_from_file_location('laputa_bench_scenario', matrix.ROOT / 'dev/performance/oracle_mutants/service_oracle.py')
        scenario = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(scenario)
        before = {'pid': 0, 'desired': 'down'}
        after = {'pid': 42, 'ready': True, 'state': 'running'}
        self.assertTrue(scenario.service_oracle(before, after, 42))
        self.assertFalse(scenario.service_oracle(before, after, 41))

    def test_matrix(self):
        checked, unchecked = matrix.validate()
        self.assertGreater(checked, 0)
        self.assertGreater(unchecked, 0)
        self.assertEqual(set(matrix.FAMILIES), {row[0] for row in matrix.WORKLOADS} | {row[0] for row in matrix.GAPS})


if __name__ == '__main__':
    unittest.main()
