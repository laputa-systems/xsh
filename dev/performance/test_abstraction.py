import unittest

from dev.performance.abstraction import Sample, evaluate, rank, ratio


def sample(output, execution, preparation=1):
    return Sample(output, 3, preparation, execution, 3)


class AbstractionTests(unittest.TestCase):
    def test_ratio_and_zero_denominator(self):
        self.assertEqual(ratio(6, 2), 3)
        with self.assertRaises(ValueError):
            ratio(1, 0)

    def test_rank_uses_execution_ratio(self):
        first = evaluate('first', 'a', 'b', [sample('ok', 2)], [sample('ok', 3)])
        second = evaluate('second', 'a', 'b', [sample('ok', 2)], [sample('ok', 4)])
        self.assertEqual([row['name'] for row in rank([first, second])], ['second', 'first'])
        self.assertEqual(first['execution_ratio'], 1.5)

    def test_mismatch_in_any_repeat_fails(self):
        with self.assertRaisesRegex(ValueError, 'output mismatch'):
            evaluate('pair', 'a', 'b', [sample('ok', 1), sample('wrong', 1)],
                     [sample('ok', 1), sample('ok', 1)])

    def test_explanation_requires_cited_cause(self):
        unexplained = evaluate('pair', 'a', 'b', [sample('ok', 1)], [sample('ok', 2)])
        explained = evaluate('pair', 'a', 'b', [sample('ok', 1)], [sample('ok', 2)],
                             cause={'description': 'helper frame', 'citation': 'profile.json'})
        self.assertEqual(unexplained['status'], 'unexplained')
        self.assertIsNone(unexplained['explanation'])
        self.assertEqual(explained['status'], 'explained')
        self.assertEqual(explained['explanation']['citation'], 'profile.json')
        with self.assertRaisesRegex(ValueError, 'requires a cause and citation'):
            evaluate('pair', 'a', 'b', [sample('ok', 1)], [sample('ok', 2)],
                     cause={'description': 'helper frame'})


if __name__ == '__main__':
    unittest.main()
