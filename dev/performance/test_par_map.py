import unittest

from dev.performance.par_map import Sample, evaluate


def sample(output, workers, complete=True):
    stage = {'workers': workers, 'items': 8, 'buffered_results': 8,
             'buffered_bytes': 64, 'input_bytes': 0,
             'worker': [{'setup_ns': 2, 'start_delay_ns': 3, 'busy_ns': 4}]}
    return Sample(output, 2, 1, {'complete': complete, 'stages': [stage],
                                 'sites': [{'alloc_bytes': 10}]})


class EvaluationTest(unittest.TestCase):
    def test_setup_counts_repeated_workers(self):
        row = evaluate('setup-5', [sample('8\n', 4)], [sample('8\n', 20)])
        self.assertIn('confirmed', row['verdict'])
        self.assertEqual(row['b']['workers'], 20)

    def test_rejects_mismatched_output_and_incomplete_profile(self):
        with self.assertRaises(ValueError):
            evaluate('setup', [sample('a', 4)], [sample('b', 20)])
        with self.assertRaises(ValueError):
            evaluate('setup', [sample('a', 4, False)], [sample('a', 20)])

    def test_nested_count_alone_does_not_prove_oversubscription(self):
        row = evaluate('nested-4', [sample('a', 16)], [sample('a', 20)])
        self.assertIn('inconclusive', row['verdict'])

    def test_retention_and_ordered_buffering_use_retained_bytes(self):
        small = sample('a', 4)
        large = sample('a', 4)
        large.report['stages'][0]['buffered_bytes'] = 128
        self.assertIn('confirmed', evaluate('retention', [small], [large])['verdict'])
        self.assertIn('confirmed', evaluate('ordered-buffering', [large], [large])['verdict'])
        self.assertIn('refuted', evaluate('ordered-buffering', [small], [large])['verdict'])

    def test_capture_attribution_is_inconclusive(self):
        row = evaluate('capture', [sample('a', 4)], [sample('a', 4)])
        self.assertIn('inconclusive', row['verdict'])


if __name__ == '__main__':
    unittest.main()
