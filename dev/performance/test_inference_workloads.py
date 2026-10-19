import re
import unittest

from inference_workloads import FAMILIES, SCALING_SIZES, fixed_inference_workloads


EXPECTED_UNITS = (64, 128, 256, 512)


def semantic_stdout(case):
    if case.family == 'alias-chain' or case.family == 'recursive-effect-component':
        return b'42\n'
    if case.family == 'records':
        return b'7\n'
    if case.family in ('dependency-chain', 'higher-order-composition'):
        return f'{case.size}\n'.encode()
    if case.family == 'generic-stream':
        return f'{case.size}\n{case.size}\n'.encode()
    if case.family == 'module-diamond':
        return f'{2 * case.size}\n'.encode()
    if case.family == 'finite-overload-chain':
        return b'1\n'
    raise AssertionError(f'unexpected inference family: {case.family}')


class InferenceWorkloadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.workloads = fixed_inference_workloads()

    def test_eight_fixed_families_retain_every_requested_size(self):
        self.assertEqual(len(set(FAMILIES)), 8)
        self.assertEqual(tuple(SCALING_SIZES), EXPECTED_UNITS)
        self.assertEqual(len(self.workloads), 36)
        for size in EXPECTED_UNITS:
            cases = [case for case in self.workloads if case.size == size]
            self.assertEqual({case.family for case in cases}, set(FAMILIES))
            records = [case for case in cases if case.family == 'records']
            self.assertEqual({case.shape for case in records}, {'wide', 'deep'})
            self.assertEqual(len(cases), 9)
            for family in FAMILIES:
                family_cases = [case for case in cases if case.family == family]
                self.assertEqual(len(family_cases), 2 if family == 'records' else 1)
                for case in family_cases:
                    self.assertEqual(case.expected_units, size)

    def test_logical_outputs_and_runtime_byte_counts_are_semantic(self):
        for case in self.workloads:
            with self.subTest(name=case.name):
                output = semantic_stdout(case)
                self.assertEqual(case.expected_stdout, output)
                self.assertEqual(case.expected_runtime_bytes, len(output))
                self.assertIsInstance(case.logical_output_shape, str)
                self.assertTrue(case.logical_output_shape)
                self.assertIsInstance(case.logical_evidence_shape, str)
                self.assertTrue(case.logical_evidence_shape)
        generic = next(case for case in self.workloads if case.family == 'generic-stream')
        self.assertIn('List[Int]', generic.logical_output_shape)
        self.assertIn('List[Str]', generic.logical_output_shape)

    def test_each_annotation_pair_retains_the_same_entry_and_output_operations(self):
        for case in self.workloads:
            with self.subTest(name=case.name):
                self.assertIn(case.entry, case.annotated_sources)
                self.assertIn(case.entry, case.reduced_sources)
                self.assertEqual(set(case.annotated_sources), set(case.reduced_sources))
                self.assertNotEqual(case.annotated_sources, case.reduced_sources)
                annotated_output = [line for line in case.annotated_sources[case.entry].splitlines()
                                    if line.startswith('print ')]
                reduced_output = [line for line in case.reduced_sources[case.entry].splitlines()
                                  if line.startswith('print ')]
                self.assertEqual(annotated_output, reduced_output)
                expected_frontend = 'unprobed'
                if case.family == 'records' and case.shape == 'deep':
                    expected_frontend = 'expected-type-depth-rejection' if case.size == 512 else 'boundary-pending'
                self.assertEqual(case.frontend_expectation, expected_frontend)

    def test_alias_and_dependency_unit_counts_match_the_fixed_inputs(self):
        for case in self.workloads:
            if case.family == 'alias-chain':
                for sources in (case.annotated_sources, case.reduced_sources):
                    self.assertEqual(len(re.findall(r'^let alias_\d+ =', sources['main.xsh'], re.MULTILINE)), case.size)
            if case.family == 'dependency-chain':
                for sources in (case.annotated_sources, case.reduced_sources):
                    self.assertEqual(len(re.findall(r'^pure step_\d+\(', sources['main.xsh'], re.MULTILINE)), case.size)

    def test_record_shapes_retain_wide_and_depth_inputs(self):
        for case in self.workloads:
            if case.family != 'records':
                continue
            annotated = case.annotated_sources['main.xsh']
            reduced = case.reduced_sources['main.xsh']
            with self.subTest(name=case.name):
                if case.shape == 'wide':
                    self.assertEqual(len(re.findall(r'field_\d+: Int', annotated)), case.size)
                    self.assertEqual(len(re.findall(r'field_\d+: \d+', reduced)), case.size)
                    self.assertIn('value.field_0', annotated)
                    self.assertIn('value.field_0', reduced)
                    self.assertIn('field projection', case.logical_output_shape)
                else:
                    self.assertEqual(len(re.findall(r'^type Deep_\d+ =', annotated, re.MULTILINE)), case.size + 1)
                    self.assertEqual(annotated.count('{next: '), 2 * case.size)
                    self.assertEqual(reduced.count('{next: '), 2 * case.size)
                    self.assertIn('.next' * case.size + '.leaf', annotated)
                    self.assertIn('.next' * case.size + '.leaf', reduced)
                    self.assertIn('leaf projection', case.logical_output_shape)

    def test_higher_order_stream_effect_and_overload_sources_retain_operations(self):
        for case in self.workloads:
            annotated = case.annotated_sources['main.xsh']
            reduced = case.reduced_sources['main.xsh']
            with self.subTest(name=case.name):
                if case.family == 'higher-order-composition':
                    self.assertEqual(annotated.count('pure compose_'), case.size)
                    self.assertEqual(reduced.count('pure compose_'), case.size)
                    self.assertEqual(annotated.count('callback('), case.size)
                    self.assertEqual(reduced.count('callback('), case.size)
                elif case.family == 'generic-stream':
                    for source in (annotated, reduced):
                        self.assertIn('yield value', source)
                        self.assertIn(f'repeated(7, {case.size})', source)
                        self.assertIn(f'repeated("x", {case.size})', source)
                    self.assertIn('count: Int', annotated)
                    self.assertNotIn('count: Int', reduced)
                elif case.family == 'recursive-effect-component':
                    self.assertEqual(annotated.count('proc effect_step_'), case.size)
                    self.assertEqual(reduced.count('proc effect_step_'), case.size)
                    self.assertEqual(annotated.count('time.now()'), 1)
                    self.assertEqual(reduced.count('time.now()'), 1)
                    self.assertIn('effect_step_0(value - 1)', annotated)
                    self.assertIn('effect_step_0(value - 1)', reduced)
                elif case.family == 'finite-overload-chain':
                    self.assertEqual(annotated.count('|> map(identity)'), case.size)
                    self.assertEqual(reduced.count('|> map(identity)'), case.size)

    def test_module_diamonds_are_sourcefile_maps_with_shared_imports(self):
        for case in self.workloads:
            if case.family != 'module-diamond':
                continue
            expected_source_count = 2 * case.size + 2
            with self.subTest(name=case.name):
                for sources in (case.annotated_sources, case.reduced_sources):
                    self.assertEqual(len(sources), expected_source_count)
                    self.assertIn('identity.xsh', sources)
                    self.assertIn('main.xsh', sources)
                    branches = [name for name in sources if name.startswith('diamond_')]
                    self.assertEqual(len(branches), 2 * case.size)
                    self.assertTrue(all('use identity as shared' in sources[name] for name in branches))
                self.assertEqual(set(case.annotated_sources), set(case.reduced_sources))


if __name__ == '__main__':
    unittest.main()
