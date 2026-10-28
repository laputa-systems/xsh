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
    if case.family == 'named-generic-constructors':
        return b'9\n' if case.shape == 'shared-default' else b'7\n'
    raise AssertionError(f'unexpected inference family: {case.family}')


class InferenceWorkloadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.workloads = fixed_inference_workloads()

    def test_existing_families_and_named_constructors_retain_every_requested_size(self):
        self.assertEqual(len(set(FAMILIES)), 9)
        self.assertEqual(tuple(SCALING_SIZES), EXPECTED_UNITS)
        self.assertEqual(len(self.workloads), 52)
        for size in EXPECTED_UNITS:
            cases = [case for case in self.workloads if case.size == size]
            self.assertEqual({case.family for case in cases}, set(FAMILIES))
            records = [case for case in cases if case.family == 'records']
            self.assertEqual({case.shape for case in records}, {'wide', 'deep'})
            self.assertEqual(len(cases), 13)
            for family in FAMILIES:
                family_cases = [case for case in cases if case.family == family]
                self.assertEqual(len(family_cases), 2 if family == 'records' else 4 if family == 'named-generic-constructors' else 1)
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
        self.assertIn('7 and 9', generic.logical_output_shape)
        self.assertNotIn('List[Str]', generic.logical_output_shape)

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
                    for source in (annotated, reduced):
                        self.assertEqual(len(re.findall(r'^pure make_\d+\(', source, re.MULTILINE)), case.size + 1)
                        self.assertEqual(len(re.findall(r'^pure project_\d+\(', source, re.MULTILINE)), case.size + 1)
                        depth = 0
                        maximum_depth = 0
                        for line in source.splitlines():
                            depth += line.count('{') - line.count('}')
                            maximum_depth = max(maximum_depth, depth)
                            self.assertGreaterEqual(depth, 0)
                        self.assertEqual(depth, 0)
                        self.assertLessEqual(maximum_depth, 2)
                        self.assertNotRegex(source, r'\.next{2,}')
                        self.assertIn(f'project_{case.size}(make_{case.size}(7))', source)
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
                        self.assertIn(f'repeated(9, {case.size})', source)
                        self.assertNotIn('repeated("x"', source)
                    self.assertIn('(value: Int, count: Int) -> Stream[Int]', annotated)
                    self.assertNotIn('value: Int', reduced)
                    normalized = annotated.replace('(value: Int, count: Int) -> Stream[Int]', '(value, count)')
                    normalized = normalized.replace(': List[Int]', '')
                    self.assertEqual(normalized, reduced)
                elif case.family == 'recursive-effect-component':
                    self.assertEqual(annotated.count('proc effect_step_'), case.size)
                    self.assertEqual(reduced.count('proc effect_step_'), case.size)
                    self.assertEqual(annotated.count('time.now()'), 1)
                    self.assertEqual(reduced.count('time.now()'), 1)
                    self.assertIn('effect_step_0(value - 1)', annotated)
                    self.assertIn('effect_step_0(value - 1)', reduced)
                    for source in (annotated, reduced):
                        self.assertIn(f'= effect_step_0({case.size})?\n', source)
                elif case.family == 'finite-overload-chain':
                    self.assertEqual(annotated.count('|> map(identity)'), case.size)
                    self.assertEqual(reduced.count('|> map(identity)'), case.size)

    def test_named_constructors_keep_independent_binders_and_observable_values(self):
        for case in self.workloads:
            if case.family != 'named-generic-constructors':
                continue
            for source in (case.annotated_sources['main.xsh'], case.reduced_sources['main.xsh']):
                with self.subTest(name=case.name):
                    constructor = {'independent': 'Box(value: value_', 'shared-default': 'Box(value:)',
                                   'ground-carrier': 'Branded(payload:', 'phantom-context': 'Phantom(value:'}[case.shape]
                    self.assertEqual(source.count(constructor), case.size)
                    if case.shape == 'independent':
                        signature = next(line for line in source.splitlines() if line.startswith('pure build('))
                        self.assertEqual(len(re.findall(r'value_\d+', signature)), case.size)
                        self.assertEqual(len(re.findall(r'^assert result.box_\d+\.value ==', source, re.MULTILINE)), case.size)
                        self.assertIn('== "kept"', source)
                    elif case.shape == 'shared-default':
                        self.assertEqual(source.count('.items.len() == 0'), case.size)
                        self.assertEqual(source.count('.extra == 9'), case.size)
                    elif case.shape == 'ground-carrier':
                        self.assertEqual(source.count('tag: closed_brand()'), case.size)
                        self.assertEqual(source.count('.tag.value == 7'), case.size)
                    else:
                        self.assertEqual(source.count(': Phantom[Int] ='), case.size)
                        self.assertEqual(len(re.findall(r'assert item_\d+\.value ==', source)), case.size)

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
                    self.assertTrue(sources['identity.xsh'].startswith('##! '))
                    self.assertIn('\n## ', sources['identity.xsh'])
                    branches = [name for name in sources if name.startswith('diamond_')]
                    self.assertEqual(len(branches), 2 * case.size)
                    for name in branches:
                        branch = sources[name]
                        self.assertIn('##! ', branch)
                        self.assertIn('use identity as shared', branch)
                        self.assertIn('\n## ', branch)
                    main = sources['main.xsh']
                    totals = [line for line in main.splitlines() if re.match(r'let total_\d+ =', line)]
                    self.assertEqual(len(totals), case.size)
                    self.assertEqual(main.count('.value'), 2 * case.size)
                    self.assertEqual(main.count(' + '), 2 * case.size - 1)
                    self.assertLessEqual(max(line.count(' + ') for line in totals), 2)
                annotated_main = case.annotated_sources['main.xsh']
                reduced_main = case.reduced_sources['main.xsh']
                self.assertEqual(annotated_main.replace('let answer: Int = ', 'let answer = '), reduced_main)
                self.assertEqual(set(case.annotated_sources), set(case.reduced_sources))


if __name__ == '__main__':
    unittest.main()
