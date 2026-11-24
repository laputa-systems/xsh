import unittest

from dev.performance.m1 import Run, evaluate


SUMMARY = 'sources summary 932 cached 0 fetched 0 bytes 0 failed cache /cache\n'
PROGRESS = 'sources cached musl a\nsources cached zlib b\n'


def run(stdout=SUMMARY, stderr=PROGRESS, status=0):
    return Run(status, stdout, stderr, 1.0)


def report(complete=True, dropped=0, workers=4, overlapping=True):
    regions = [{'kind': 'parallel-wait', 'wall_ns': 5, 'count': 1, 'overlapping': False}]
    if workers:
        regions.append({'kind': 'parallel-workers', 'wall_ns': 9, 'count': workers, 'overlapping': overlapping})
    return {'format': 'xsht-profile/1', 'complete': complete,
            'completeness': {'dropped': dropped, 'opaque_regions': regions}, 'sites': [], 'stages': [{}]}


class EvaluateTest(unittest.TestCase):
    def test_faithful_profile_passes(self):
        self.assertEqual(evaluate([run()], [run()], report()), [])

    def test_progress_order_may_differ_between_runs(self):
        reordered = 'sources cached zlib b\nsources cached musl a\n'
        self.assertEqual(evaluate([run()], [run(stderr=reordered)], report()), [])

    def test_changed_summary_fails(self):
        changed = SUMMARY.replace('932 cached', '931 cached')
        self.assertIn('profiled summary differs from the plain run',
                      evaluate([run()], [run(stdout=changed)], report()))

    def test_changed_progress_fails(self):
        self.assertIn('profiled progress lines differ from the plain run',
                      evaluate([run()], [run(stderr='sources cached musl a\n')], report()))

    def test_failed_run_fails(self):
        self.assertIn('profiled run exited 2', evaluate([run()], [run(status=2)], report()))

    def test_completeness_claimed_after_dropping_fails(self):
        failures = evaluate([run()], [run()], report(complete=True, dropped=3))
        self.assertIn('profile claims completeness after dropping 3 records', failures)

    def test_incomplete_profile_fails(self):
        self.assertIn('profile is incomplete: dropped 1',
                      evaluate([run()], [run()], report(complete=False, dropped=1)))

    def test_missing_stages_fails(self):
        data = report()
        for stages in (None, []):
            if stages is None:
                data.pop('stages', None)
            else:
                data['stages'] = stages
            self.assertIn('no parallel stages in profile', evaluate([run()], [run()], data))

    def test_serialized_stage_fails(self):
        expected = 'no parallel-worker region: the fetch stage did not run in parallel'
        self.assertIn(expected, evaluate([run()], [run()], report(workers=0)))
        self.assertIn(expected, evaluate([run()], [run()], report(workers=1)))
        self.assertIn(expected, evaluate([run()], [run()], report(overlapping=False)))


if __name__ == '__main__':
    unittest.main()
