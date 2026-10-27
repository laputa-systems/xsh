from dataclasses import replace
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest

from dev.performance import core_cases, focused


class CoreCasesTests(unittest.TestCase):
    def setUp(self):
        self.scratch = core_cases.SOURCE_ROOT / '.work/tmp'
        self.scratch.mkdir(parents=True, exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(dir=self.scratch)
        self.addCleanup(self.directory.cleanup)
        self.fixtures = Path(self.directory.name)

    def prepare(self, name):
        return core_cases.prepare_case(name, fixtures=self.fixtures)

    def test_focused_wc_binds_and_rechecks_the_generated_input(self):
        expected = self.prepare('core-wc-small')
        config = SimpleNamespace(source_root=core_cases.SOURCE_ROOT, fixtures=self.fixtures)
        case, root, snapshot, owner = focused._case(config, expected.name)
        self.assertEqual(case, expected)
        self.assertEqual(owner.snapshot(root), snapshot)
        (root / 'input').write_bytes(b'changed\n')
        with self.assertRaisesRegex(ValueError, 'file changed'):
            owner.snapshot(root)

    def test_each_family_scales_useful_work_and_keeps_exact_oracles(self):
        for family in ('wc', 'sort', 'awk', 'traversal', 'pipeline'):
            previous = None
            for shape in ('small', 'medium', 'large'):
                with self.subTest(family=family, shape=shape):
                    case = self.prepare(f'core-{family}-{shape}')
                    core_cases.validate_case(case, fixtures=self.fixtures)
                    counts = core_cases.work_counts(case)
                    self.assertTrue(case.expected_stdout.endswith(b'\n'))
                    self.assertEqual(case.useful_stdout_prefix, case.expected_stdout.splitlines(keepends=True)[0])
                    if previous:
                        self.assertTrue(all(counts[key] > previous[key] for key in counts))
                    previous = counts

    def test_wc_reads_binary_lines_instead_of_metadata_only_size(self):
        case = self.prepare('core-wc-small')
        data = (self.fixtures / case.name / 'input').read_bytes()
        self.assertIn(b'\x00\xff', data)
        self.assertEqual(len(data), 65536)
        self.assertEqual(case.arguments, ('--lines', str(self.fixtures / case.name / 'input')))
        self.assertEqual(case.expected_stdout, f'{data.count(bytes([10]))} {case.arguments[1]}\n'.encode())

    def test_sort_oracle_preserves_all_records_and_orders_numeric_keys(self):
        case = self.prepare('core-sort-small')
        original = (self.fixtures / case.name / 'input').read_bytes().splitlines()
        output = case.expected_stdout.splitlines()
        self.assertCountEqual(output, original)
        self.assertNotEqual(output, original)
        self.assertEqual([int(row.split(b'\t')[1]) for row in output], list(range(256)))

    def test_awk_oracle_observes_all_records_and_array_groups(self):
        case = self.prepare('core-awk-small')
        self.assertEqual(case.expected_stdout, b'256 32640 20 19\n')
        self.assertIn('groups[$3]++', case.arguments[0])
        self.assertIn('NR', case.arguments[0])

    def test_traversal_oracle_requires_hidden_entries_metadata_and_unfollowed_links(self):
        case = self.prepare('core-traversal-small')
        self.assertIn(b'.hidden-0001/file 000016.txt\tfile\t11\n', case.expected_stdout)
        self.assertIn(b'link-parent\tlink\t-\n', case.expected_stdout)
        self.assertIn(b'link-dangling\tlink\t-\n', case.expected_stdout)
        self.assertIn(b'.gitignore\tfile\t6\n', case.expected_stdout)
        self.assertEqual(len(case.expected_stdout.splitlines()), core_cases.work_counts(case)['completed_entries'])
        self.assertEqual(core_cases.work_counts(case)['regular_files'], 33)

    def test_pipeline_counts_total_starts_and_completed_output(self):
        case = self.prepare('core-pipeline-small')
        self.assertEqual(case.expected_stdout, b'probe 0\nprobe 1\n')
        self.assertEqual(case.expected_subprocesses, 4)
        self.assertEqual(core_cases.work_counts(case), {'completed_pipelines': 2, 'subprocess_starts': 4, 'completed_records': 2})

    def test_corrupt_inputs_or_oracles_and_extra_cleanup_artifacts_reject(self):
        case = self.prepare('core-awk-small')
        root = self.fixtures / case.name
        with self.assertRaisesRegex(ValueError, 'descriptor'):
            core_cases.validate_case(replace(case, expected_stdout=b'wrong\n'), fixtures=self.fixtures)
        (root / 'input').write_bytes(b'corrupt\n')
        with self.assertRaisesRegex(ValueError, 'file changed'):
            core_cases.validate_case(case, fixtures=self.fixtures)
        empty = self.prepare('core-pipeline-small')
        (self.fixtures / empty.name / 'leftover').write_bytes(b'unexpected')
        with self.assertRaisesRegex(ValueError, 'file changed'):
            core_cases.validate_case(empty, fixtures=self.fixtures)

    def test_changed_links_and_linked_fixture_parents_reject(self):
        case = self.prepare('core-traversal-small')
        link = self.fixtures / case.name / 'tree/link-parent'
        link.unlink()
        link.symlink_to(self.fixtures)
        with self.assertRaisesRegex(ValueError, 'link changed'):
            core_cases.validate_case(case, fixtures=self.fixtures)
        parent = self.fixtures / 'alias'
        parent.symlink_to(self.fixtures, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'nonsymlink'):
            core_cases.prepare_case('core-wc-small', fixtures=parent)

    def test_unknown_shapes_and_overwrites_fail_before_changing_fixtures(self):
        for name in ('../escape', 'core-wc-huge', 'core-cat-small', 'core-wc-small/escape'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.prepare(name)
        self.assertEqual(list(self.fixtures.iterdir()), [])
        case = self.prepare('core-wc-small')
        with self.assertRaises(FileExistsError):
            self.prepare(case.name)
        core_cases.validate_case(case, fixtures=self.fixtures)


if __name__ == '__main__':
    unittest.main()
