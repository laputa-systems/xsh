from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from dev.performance import census


class CensusTests(unittest.TestCase):
    def test_discovery_classification_duplicates_and_summary(self):
        scratch = Path('.work/tmp').resolve()
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '-q', str(root)], check=True, timeout=30)
            sources = {
                'bin/start': '#!/usr/bin/env -S xsh --\nuse lib.helper\nlet plugin = module.load(path)\n',
                'lib/helper.xsh': 'pure value() -> Int { 1 }\n',
                'tests/xsh/case.xsh': 'test case { assert true }\n',
                'tests/fixtures/check/invalid.xsh': 'invalid',
                'examples/a.xsh': 'print "same"\n',
                'examples/b.xsh': 'print "same"\n',
                'bin/other': '#!/bin/sh\necho hi\n',
                'xsht-config.ini': 'module_path = .\nexclude = tests/fixtures/**/*.xsh\n',
            }
            for path, data in sources.items():
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(data)
            subprocess.run(['git', '-C', str(root), 'add', '.'], check=True, timeout=30)
            subprocess.run(['git', '-C', str(root), '-c', 'user.name=Test', '-c',
                            'user.email=test@example.com', 'commit', '-qm', 'fixture'], check=True, timeout=30)
            with patch.object(census, 'parser_path', return_value='/stub/xsht'):
                report = census.census([root], parse_file=lambda xsht, repo, path: 'parsed')
            files = {item['path']: item for item in report['files']}
            self.assertEqual(set(files), set(sources) - {'bin/other', 'xsht-config.ini'})
            self.assertEqual(files['bin/start']['role'], 'program')
            self.assertEqual(files['lib/helper.xsh']['role'], 'library')
            self.assertEqual(files['tests/xsh/case.xsh']['role'], 'test')
            self.assertEqual(files['tests/fixtures/check/invalid.xsh']['role'], 'rejection fixture')
            self.assertEqual(files['tests/fixtures/check/invalid.xsh']['status'], 'excluded')
            self.assertEqual(files['bin/start']['imports'][0]['resolved'], 'lib/helper.xsh')
            self.assertEqual(files['bin/start']['dynamic_imports'], ['module.load(path)'])
            self.assertEqual({f['path'] for f in report['duplicates'][0]['files']}, {'examples/a.xsh', 'examples/b.xsh'})
            summary = report['summary']
            self.assertEqual(summary['candidates'], 6)
            self.assertEqual(summary['classified'], 6)
            self.assertEqual(summary['unclassified'], 0)
            self.assertEqual(summary['unresolved_imports'], 0)
            self.assertEqual(sum(c['count'] for c in summary['counts']), 6)

    def test_exclusion_globs_respect_path_segments(self):
        self.assertTrue(census.matches('tests/fixtures/a.xsh', 'tests/fixtures/**/*.xsh'))
        self.assertTrue(census.matches('tests/fixtures/deep/a.xsh', 'tests/fixtures/**/*.xsh'))
        self.assertFalse(census.matches('docs/snippets/a/deep/a.xsh', 'docs/snippets/*/a.xsh'))

    def test_shebang_interpreters(self):
        self.assertTrue(census.shebang(b'#!/bin/xsh\n'))
        self.assertTrue(census.shebang(b'#!/usr/bin/env xsh\n'))
        self.assertFalse(census.shebang(b'#!/usr/bin/env bash\n# xsh\n'))


if __name__ == '__main__':
    unittest.main()
