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
