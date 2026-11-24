import unittest

from dev.performance.stats_shape import Sample, VARIANTS, evaluate


class StatsShapeTests(unittest.TestCase):
    def samples(self, cycles=True):
        return {name: [Sample('same\n', cpu, int(cpu * 100) if cycles else None)
                       for cpu in (1.0, 1.1, 1.2)] for name in VARIANTS}

    def test_cliff_requires_ratio_and_disjoint_ranges(self):
        samples = self.samples()
        samples['renamed'] = [Sample('same\n', x, int(x * 100)) for x in (1.4, 1.5, 1.6)]
        self.assertEqual(evaluate(samples)['verdict'], 'cliff shown')
        samples['renamed'][0] = Sample('same\n', 1.2, 120)
        self.assertEqual(evaluate(samples)['verdict'], 'no cliff')
        samples['renamed'] = [Sample('same\n', x, int(x * 100)) for x in (1.21, 1.25, 1.3)]
        self.assertEqual(evaluate(samples)['verdict'], 'no cliff')

    def test_cpu_fallback(self):
        samples = self.samples(False)
        samples['extra'] = [Sample('same\n', x, None) for x in (1.4, 1.5, 1.6)]
        self.assertEqual(evaluate(samples)['metric'], 'cpu')
        self.assertEqual(evaluate(samples)['verdict'], 'cliff shown')

    def test_output_mismatch(self):
        samples = self.samples()
        samples['fewer'][1] = Sample('different\n', 1.0, 100)
        with self.assertRaisesRegex(ValueError, 'output mismatch'):
            evaluate(samples)

    def test_non_counterfactual_does_not_trigger(self):
        samples = self.samples()
        samples['exact-blob'] = [Sample('same\n', 5, 500)] * 3
        self.assertEqual(evaluate(samples)['verdict'], 'no cliff')


if __name__ == '__main__':
    unittest.main()
