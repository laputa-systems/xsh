import argparse
import subprocess
from pathlib import Path
import unittest
from unittest.mock import patch

from dev.performance import bench


def options(**changes):
    return argparse.Namespace(tier=None, workload=None, skip=None, area=None, full=False, list=False, **changes)


class SelectionTests(unittest.TestCase):
    def test_ceiling_and_exclusion(self):
        args = options()
        args.tier = 'change'
        selected, excluded = bench.select(args)
        self.assertEqual([row[0] for row in selected], ['harness-tests', 'consolidation-startup', 'stdlib-cold'])
        self.assertIn(('legacy-baseline', 'above tier ceiling'), excluded)

    def test_unknown_empty_and_full_guard(self):
        for changes in ({'workload': ['no-such-workload']}, {'tier': 'edit', 'skip': ['harness-tests']}, {'tier': 'full'}):
            args = options()
            for key, value in changes.items(): setattr(args, key, value)
            with self.assertRaises(ValueError): bench.select(args)

    def test_hot_process_warning_makes_timing_inconclusive(self):
        self.assertTrue(bench.ready(subprocess.CompletedProcess([], 0, stdout='')))
        self.assertFalse(bench.ready(subprocess.CompletedProcess([], 0, stdout='perf-gate-ready: hot process tree')))
        self.assertFalse(bench.ready(subprocess.CompletedProcess([], 1, stdout='')))

    def test_preflight_reports_missing_product_for_each_consumer(self):
        rows = [row for row in bench.REGISTRY if row[0] in ('consolidation-startup', 'stdlib-cold')]
        with patch.object(bench, 'BINARY', Path('/missing/xsh')):
            failures = bench.preflight(rows)
        self.assertTrue(any('consolidation-startup: missing executable' in failure for failure in failures))
        self.assertTrue(any('stdlib-cold: missing executable' in failure for failure in failures))


if __name__ == '__main__': unittest.main()
