import re
import unittest

from inference_workloads import FAMILIES, SCALING_SIZES, fixed_inference_workloads


EXPECTED_UNITS = (64, 128, 256, 512)
EXPECTED_OUTPUTS = {
    ('alias-chain', 'chain'): {64: b'42\n', 128: b'42\n', 256: b'42\n', 512: b'42\n'},
    ('dependency-chain', 'chain'): {64: b'64\n', 128: b'128\n', 256: b'256\n', 512: b'512\n'},
    ('records', 'wide'): {64: b'7\n', 128: b'7\n', 256: b'7\n', 512: b'7\n'},
    ('records', 'deep'): {64: b'7\n', 128: b'7\n', 256: b'7\n', 512: b'7\n'},
    ('higher-order-composition', 'chain'): {64: b'64\n', 128: b'128\n', 256: b'256\n', 512: b'512\n'},
    ('generic-stream', 'items'): {64: b'64\n64\n', 128: b'128\n128\n', 256: b'256\n256\n', 512: b'512\n512\n'},
    ('recursive-effect-component', 'component'): {64: b'42\n', 128: b'42\n', 256: b'42\n', 512: b'42\n'},
    ('module-diamond', 'diamonds'): {64: b'128\n', 128: b'256\n', 256: b'512\n', 512: b'1024\n'},
    ('finite-overload-chain', 'chain'): {64: b'1\n', 128: b'1\n', 256: b'1\n', 512: b'1\n'},
}
EXPECTED_OUTPUT_BYTES = {
    key: {size: len(output) for size, output in outputs.items()}
    for key, outputs in EXPECTED_OUTPUTS.items()
}
EXPECTED_SHAPES = {
    ('alias-chain', 'chain'): ('Int', 'size callable aliases and one Int specialization'),
    ('dependency-chain', 'chain'): ('Int', 'size direct function dependencies'),
    ('records', 'wide'): ('Int field projection', 'one projection from a size-field record'),
    ('records', 'deep'): ('Int leaf projection', 'one projection through a size-level record shape'),
    ('higher-order-composition', 'chain'): ('Int', 'size applications of one Int callback contract'),
    ('generic-stream', 'items'): ('List[Int] and List[Str]', 'two item-type instantiations and size yielded items each'),
    ('recursive-effect-component', 'component'): ('Int', 'one size-function recursive component with a time edge'),
    ('module-diamond', 'diamonds'): ('Int sum', 'size two-branch diamonds sharing one identity export'),
    ('finite-overload-chain', 'chain'): ('List[Int]', 'size selected List[Int] map overloads'),
}


class InferenceWorkloadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.workloads = fixed_inference_workloads()

    def test_eight_fixed_families_retain_every_requested_size(self):
        self.assertEqual(set(FAMILIES), {family for family, _ in EXPECTED_OUTPUTS})
        self.assertEqual(tuple(SCALING_SIZES), EXPECTED_UNITS)
        for family in FAMILIES:
            shapes = {shape for candidate_family, shape in EXPECTED_OUTPUTS if candidate_family == family}
            cases = [case for case in self.workloads if case.family == family]
            self.assertEqual({case.shape for case in cases}, shapes)
            self.assertEqual(len(cases), len(shapes) * len(EXPECTED_UNITS))
            for shape in shapes:
                selected = [case for case in cases if case.shape == shape]
                self.assertEqual(tuple(case.size for case in selected), EXPECTED_UNITS)

    def test_units_outputs_and_runtime_bytes_are_fixed_independently_of_sources(self):
        for case in self.workloads:
            key = (case.family, case.shape)
            with self.subTest(name=case.name):
                self.assertEqual(case.expected_units, case.size)
                self.assertEqual(case.expected_stdout, EXPECTED_OUTPUTS[key][case.size])
                self.assertEqual(case.expected_runtime_bytes, EXPECTED_OUTPUT_BYTES[key][case.size])
                self.assertEqual(case.expected_runtime_bytes, len(case.expected_stdout))
                self.assertEqual((case.expected_output_shape, case.expected_evidence_shape), EXPECTED_SHAPES[key])

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
                for sources in (case.annotated_sources, case.reduced_sources):
                    self.assertIn('print ', sources[case.entry])
                self.assertEqual(case.frontend_expectation, 'unprobed' if case.shape != 'deep' else {
                    64: 'boundary-pending', 128: 'boundary-pending', 256: 'boundary-pending',
                    512: 'expected-type-depth-rejection',
                }[case.size])

    def test_alias_and_dependency_unit_counts_are_source_independent(self):
        for case in self.workloads:
            if case.family == 'alias-chain':
                for sources in (case.annotated_sources, case.reduced_sources):
                    self.assertEqual(len(re.findall(r'^let alias_\d+ =', sources['main.xsh'], re.MULTILINE)), case.expected_units)
            if case.family == 'dependency-chain':
                for sources in (case.annotated_sources, case.reduced_sources):
                    self.assertEqual(len(re.findall(r'^pure step_\d+\(', sources['main.xsh'], re.MULTILINE)), case.expected_units)

    def test_record_shapes_retain_wide_and_depth_inputs(self):
        for case in self.workloads:
            annotated = case.annotated_sources['main.xsh']
            reduced = case.reduced_sources['main.xsh']
            if case.family != 'records':
                continue
            with self.subTest(name=case.name):
                if case.shape == 'wide':
                    self.assertEqual(len(re.findall(r'field_\d+: Int', annotated)), case.expected_units)
                    self.assertEqual(len(re.findall(r'field_\d+: \d+', reduced)), case.expected_units)
                    self.assertIn('value.field_0', annotated)
                    self.assertIn('value.field_0', reduced)
                else:
                    self.assertEqual(len(re.findall(r'^type Deep_\d+ =', annotated, re.MULTILINE)), case.expected_units + 1)
                    self.assertEqual(annotated.count('{next: '), 2 * case.expected_units)
                    self.assertEqual(reduced.count('{next: '), 2 * case.expected_units)
                    self.assertIn('.next' * case.expected_units + '.leaf', annotated)
                    self.assertIn('.next' * case.expected_units + '.leaf', reduced)

    def test_higher_order_stream_effect_and_overload_families_keep_their_operations(self):
        for case in self.workloads:
            annotated = case.annotated_sources['main.xsh']
            reduced = case.reduced_sources['main.xsh']
            with self.subTest(name=case.name):
                if case.family == 'higher-order-composition':
                    self.assertEqual(annotated.count('pure compose_'), case.expected_units)
                    self.assertEqual(reduced.count('pure compose_'), case.expected_units)
                    self.assertEqual(annotated.count('callback('), case.expected_units)
                    self.assertEqual(reduced.count('callback('), case.expected_units)
                elif case.family == 'generic-stream':
                    for source in (annotated, reduced):
                        self.assertIn('yield value', source)
                        self.assertIn(f'repeated(7, {case.size})', source)
                        self.assertIn(f'repeated("x", {case.size})', source)
                    self.assertIn('count: Int', annotated)
                    self.assertNotIn('count: Int', reduced)
                elif case.family == 'recursive-effect-component':
                    self.assertEqual(annotated.count('proc effect_step_'), case.expected_units)
                    self.assertEqual(reduced.count('proc effect_step_'), case.expected_units)
                    self.assertEqual(annotated.count('time.now()'), 1)
                    self.assertEqual(reduced.count('time.now()'), 1)
                    self.assertIn(f'effect_step_0(value - 1)', annotated)
                    self.assertIn(f'effect_step_0(value - 1)', reduced)
                elif case.family == 'finite-overload-chain':
                    self.assertEqual(annotated.count('|> map(identity)'), case.expected_units)
                    self.assertEqual(reduced.count('|> map(identity)'), case.expected_units)

    def test_module_diamonds_are_sourcefile_maps_with_shared_imports(self):
        for case in self.workloads:
            if case.family != 'module-diamond':
                continue
            expected_source_count = 2 * case.expected_units + 2
            with self.subTest(name=case.name):
                for sources in (case.annotated_sources, case.reduced_sources):
                    self.assertEqual(len(sources), expected_source_count)
                    self.assertIn('identity.xsh', sources)
                    self.assertIn('main.xsh', sources)
                    branches = [name for name in sources if name.startswith('diamond_')]
                    self.assertEqual(len(branches), 2 * case.expected_units)
                    self.assertTrue(all('use identity as shared' in sources[name] for name in branches))
                self.assertEqual(set(case.annotated_sources), set(case.reduced_sources))


if __name__ == '__main__':
    unittest.main()
