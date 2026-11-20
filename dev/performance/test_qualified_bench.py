import unittest

from dev.performance import qualified_bench as q


class OracleTests(unittest.TestCase):
    def test_tree_output(self):
        self.assertTrue(q.tree_complete(0, q.TREE_OUTPUT, b''))
        self.assertFalse(q.tree_complete(0, b'wrong\n', b''))
        self.assertFalse(q.tree_complete(1, q.TREE_OUTPUT, b''))

    def test_tooling_output(self):
        report = b'xsht check: 1 file in 0.1s (thread time by stage: check 0.1s)\n'
        self.assertTrue(q.tooling_complete('check', 2, [(0, b'', report), (0, b'', report)]))
        self.assertFalse(q.tooling_complete('check', 2, [(0, b'', report)]))
        self.assertFalse(q.tooling_complete('check', 1, [(0, b'warning\n', report)]))
        self.assertFalse(q.tooling_complete('check', 1, [(2, b'', report)]))
        self.assertFalse(q.tooling_complete('check', 1, [(0, b'', b'warning\n' + report)]))

    def test_products(self):
        self.assertTrue(q.products_complete({'xsh': 10, 'xsht': 20, 'xshi': 30}))
        self.assertFalse(q.products_complete({'xsh': 10, 'xsht': 20}))
        self.assertFalse(q.products_complete({'xsh': 10, 'xsht': 20, 'xshi': 0}))

    def test_verdict(self):
        anchor = {'value': 100, 'tolerance': 0.05, 'commit': 'abc', 'host': 'host', 'date': '2026-01-01'}
        self.assertEqual(q.verdict(105, anchor, True), ('within budget', None))
        self.assertEqual(q.verdict(106, anchor, True), ('regression', None))
        self.assertEqual(q.verdict(100, anchor, False), ('inconclusive', 'host readiness check failed'))
        self.assertEqual(q.verdict(100, anchor, True, 'other'), ('inconclusive', 'anchor host differs'))
        self.assertEqual(q.verdict(100, None, True), ('inconclusive', 'anchor missing'))
        self.assertEqual(q.verdict(100, None, False), ('inconclusive', 'host readiness check failed; anchor missing'))

    def test_anchor_tolerance_follows_observed_spread(self):
        self.assertEqual(q.anchor_tolerance([100, 100, 100, 100, 100]), q.MINIMUM_TOLERANCE)
        self.assertEqual(q.anchor_tolerance([90, 95, 100, 105, 110]), 0.15)
        self.assertEqual(q.anchor_tolerance(None), q.MINIMUM_TOLERANCE)

    def test_tree_rounds_take_the_median_peak(self):
        rounds = [{'value': peak, 'unit': 'bytes', 'completed_lines': 16} for peak in (30, 10, 12, 11, 13)]
        combined = q.combine_tree_rounds(rounds)
        self.assertEqual(combined['value'], 12)
        self.assertEqual(combined['samples_bytes'], [30, 10, 12, 11, 13])
        with self.assertRaises(ValueError):
            q.combine_tree_rounds(rounds[:4])
