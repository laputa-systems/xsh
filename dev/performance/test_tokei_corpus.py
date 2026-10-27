from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zlib


sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parent))
import tokei_corpus as corpus
from unittest import mock


class TokeiCorpusTests(unittest.TestCase):
    def setUp(self):
        scratch = Path(__file__).resolve().parents[2] / '.work/tmp'
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / 'source'
        self.source.mkdir()
        subprocess.run(['git', '-C', str(self.source), 'init', '-q'], check=True)
        subprocess.run(['git', '-C', str(self.source), 'config', 'user.email', 'test@example.invalid'], check=True)
        subprocess.run(['git', '-C', str(self.source), 'config', 'user.name', 'Corpus Test'], check=True)
        (self.source / 'src').mkdir()
        (self.source / 'src/main.rs').write_bytes(b'fn main() {}\n')
        (self.source / 'notes.txt').write_bytes(b'plain text\n')
        (self.source / 'tool').write_bytes(b'#!/usr/bin/env python3\nprint(1)\n')
        (self.source / 'Makefile').write_bytes(b'all:\n\ttrue\n')
        (self.source / 'unknown').write_bytes(b'no shebang\n')
        (self.source / 'ignored.ftl').write_bytes(b'unsupported\n')
        (self.source / '.hidden.py').write_bytes(b'print(0)\n')
        os.symlink('src/main.rs', self.source / 'alias.rs')
        (self.source / 'tool').chmod(0o755)
        subprocess.run(['git', '-C', str(self.source), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(self.source), 'commit', '-qm', 'fixture'], check=True)
        self.revision = subprocess.check_output(
            ['git', '-C', str(self.source), 'rev-parse', 'HEAD'], text=True,
        ).strip()
        self.projection = self.root / 'projection'

    def test_projection_reads_only_pinned_nonhidden_rust_blobs(self):
        (self.source / 'src/main.rs').write_bytes(b'fn changed() {}\n')
        (self.source / 'new.py').write_bytes(b'print(2)\n')
        result = corpus.project_commit(self.source, self.projection, self.revision, min_files=1)

        self.assertEqual(result['files'], 1)
        self.assertEqual(result['bytes'], len(b'fn main() {}\n'))
        self.assertEqual((self.projection / 'src/main.rs').read_bytes(), b'fn main() {}\n')
        self.assertFalse((self.projection / 'new.py').exists())
        self.assertFalse((self.projection / 'alias.rs').exists())
        self.assertFalse((self.projection / '.hidden.py').exists())
        self.assertFalse((self.projection / 'unknown').exists())
        self.assertFalse((self.projection / 'ignored.ftl').exists())

    def test_projection_limits_leave_no_partial_destination(self):
        with self.assertRaisesRegex(corpus.CorpusError, 'requires at least 500'):
            corpus.project_commit(self.source, self.projection, self.revision)
        with self.assertRaises(corpus.CorpusError):
            corpus.project_commit(self.source, self.projection, self.revision, max_files=0, min_files=1)
        self.assertFalse(self.projection.exists())
        with self.assertRaises(corpus.CorpusError):
            corpus.project_commit(self.source, self.projection, self.revision, min_files=2)
        self.assertFalse(self.projection.exists())

    def test_candidate_limit_rejects_before_git_blob_reads(self):
        with mock.patch.object(corpus, '_git_blob_files') as blob_read:
            with self.assertRaisesRegex(corpus.CorpusError, 'candidates exceed 0 files'):
                corpus._git_source_files(
                    self.source, self.revision, max_files=0, min_files=0,
                )
        blob_read.assert_not_called()

    def test_report_read_is_bounded_when_file_grows_after_open(self):
        report = self.root / 'report.json'
        report.write_bytes(b'{}')
        real_fdopen = os.fdopen
        read_sizes = []

        def grow_before_read(descriptor, mode):
            stream = real_fdopen(descriptor, mode)

            class GrowingReader:
                def __enter__(self):
                    return self

                def __exit__(self, exc_type, exc_value, traceback):
                    stream.close()

                def fileno(self):
                    return stream.fileno()

                def read(self, size):
                    read_sizes.append(size)
                    with report.open('ab') as output:
                        output.write(b'x' * 100000)
                    return stream.read(size)

            return GrowingReader()

        with mock.patch.object(corpus.os, 'fdopen', side_effect=grow_before_read), \
                mock.patch.object(corpus, 'MAX_REPORT_BYTES', 2):
            with self.assertRaisesRegex(corpus.CorpusError, 'report exceeds its 2-byte limit'):
                corpus._read_report(report)
        self.assertEqual(read_sizes, [3])

    def test_projection_read_is_bounded_when_file_grows_after_open(self):
        corpus.project_commit(self.source, self.projection, self.revision, min_files=1)
        source_file = self.projection / 'src/main.rs'
        original = source_file.read_bytes()
        real_fdopen = os.fdopen
        read_sizes = []

        def grow_before_read(descriptor, mode):
            stream = real_fdopen(descriptor, mode)

            class GrowingReader:
                def __enter__(self):
                    return self

                def __exit__(self, exc_type, exc_value, traceback):
                    stream.close()

                def fileno(self):
                    return stream.fileno()

                def read(self, size):
                    read_sizes.append(size)
                    with source_file.open('ab') as output:
                        output.write(b'x' * 100000)
                    return stream.read(size)

            return GrowingReader()

        with mock.patch.object(corpus.os, 'fdopen', side_effect=grow_before_read):
            with self.assertRaisesRegex(corpus.CorpusError, 'projection file src/main.rs exceeds'):
                corpus.reconcile(
                    self.source, self.projection, self.root / 'xsh.json', self.root / 'tokei.json',
                    revision=self.revision, min_files=1, max_file_bytes=len(original),
                )
        self.assertEqual(read_sizes, [len(original) + 1])

    def test_reconciliation_limits_directories_even_when_they_are_empty(self):
        corpus.project_commit(self.source, self.projection, self.revision, min_files=1)
        for name in ('empty-a', 'empty-b', 'empty-c'):
            (self.projection / name).mkdir()
        with mock.patch.object(corpus, 'MAX_PROJECTION_NODES', 3):
            with self.assertRaisesRegex(corpus.CorpusError, 'files and directories'):
                corpus.reconcile(
                    self.source, self.projection, self.root / 'xsh.json', self.root / 'tokei.json',
                    revision=self.revision, min_files=1,
                )

    def test_staging_cleanup_failure_is_attached_to_primary_error(self):
        original_rmtree = shutil.rmtree
        original_mkdtemp = tempfile.mkdtemp
        staging_paths = []
        write_error = OSError('projection write failed')
        cleanup_error = OSError('staging cleanup failed')

        def capture_staging(*args, **kwargs):
            path = original_mkdtemp(*args, **kwargs)
            staging_paths.append(path)
            return path

        try:
            with mock.patch.object(corpus.tempfile, 'mkdtemp', side_effect=capture_staging), \
                    mock.patch.object(Path, 'write_bytes', side_effect=write_error), \
                    mock.patch.object(corpus.shutil, 'rmtree', side_effect=cleanup_error):
                with self.assertRaisesRegex(OSError, 'projection write failed') as raised:
                    corpus.project_commit(self.source, self.projection, self.revision, min_files=1)
            self.assertTrue(any('staging cleanup failed' in note for note in raised.exception.__notes__))
        finally:
            for staging in staging_paths:
                if Path(staging).exists():
                    original_rmtree(staging)

    def test_bounded_process_rejects_excess_output_and_reaps_child(self):
        real_popen = subprocess.Popen
        children = []

        def start(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            return child

        command = [sys.executable, '-c', 'import os; os.write(1, b"x" * 1000000)']
        with mock.patch.object(corpus.subprocess, 'Popen', side_effect=start):
            with self.assertRaisesRegex(corpus.CorpusError, 'stdout exceeded its 32-byte limit'):
                corpus._run_bounded_process(
                    command, deadline=corpus._monotonic() + 5, stdout_limit=32,
                )
        self.assertEqual(len(children), 1)
        self.assertIsNotNone(children[0].poll())

    def test_git_listing_applies_output_limit_during_capture(self):
        with self.assertRaisesRegex(corpus.CorpusError, 'stdout exceeded its 8-byte limit'):
            corpus._git(
                self.source, 'ls-tree', '-r', '--name-only', self.revision,
                stdout_limit=8,
            )

    def test_bounded_process_deadline_and_cancellation_reap_children(self):
        real_popen = subprocess.Popen
        children = []

        def start(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            return child

        command = [sys.executable, '-c', 'import os, signal; os.write(1, b"ready"); signal.pause()']
        with mock.patch.object(corpus.subprocess, 'Popen', side_effect=start), \
                mock.patch.object(corpus, '_monotonic', side_effect=(0.0, 2.0)):
            with self.assertRaisesRegex(corpus.CorpusError, 'exceeded its deadline'):
                corpus._run_bounded_process(command, deadline=1.0, stdout_limit=32)
        self.assertIsNotNone(children[0].poll())

        children.clear()
        with mock.patch.object(corpus.subprocess, 'Popen', side_effect=start), \
                mock.patch.object(corpus.selectors.DefaultSelector, 'select', side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                corpus._run_bounded_process(
                    command, deadline=corpus._monotonic() + 5, stdout_limit=32,
                )
        self.assertEqual(len(children), 1)
        self.assertIsNotNone(children[0].poll())

    def test_projection_ignores_git_replace_refs_for_pinned_commit(self):
        original = (self.source / 'src/main.rs').read_bytes()
        (self.source / 'src/main.rs').write_bytes(b'fn replacement() {}\n')
        (self.source / 'replacement.rs').write_bytes(b'fn replacement_file() {}\n')
        subprocess.run(['git', '-C', str(self.source), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(self.source), 'commit', '-qm', 'replacement'], check=True)
        replacement = subprocess.check_output(
            ['git', '-C', str(self.source), 'rev-parse', 'HEAD'], text=True,
        ).strip()
        subprocess.run(['git', '-C', str(self.source), 'replace', self.revision, replacement], check=True)
        replaced_tree = subprocess.check_output(
            ['git', '-C', str(self.source), 'ls-tree', '-r', '--name-only', self.revision],
        )
        self.assertIn(b'replacement.rs', replaced_tree)

        corpus.project_commit(self.source, self.projection, self.revision, min_files=1)

        self.assertEqual((self.projection / 'src/main.rs').read_bytes(), original)
        self.assertFalse((self.projection / 'replacement.rs').exists())

    def test_projection_ignores_inherited_object_directory_overrides(self):
        original = (self.source / 'src/main.rs').read_bytes()
        replacement = b'fn forged() {}\n'
        blob = subprocess.check_output(
            ['git', '-C', str(self.source), 'rev-parse', f'{self.revision}:src/main.rs'], text=True,
        ).strip()
        alternate = self.root / 'alternate-objects'
        loose_object = alternate / blob[:2] / blob[2:]
        loose_object.parent.mkdir(parents=True)
        loose_object.write_bytes(zlib.compress(b'blob ' + str(len(replacement)).encode() + b'\0' + replacement))
        with mock.patch.dict(os.environ, {
            'GIT_OBJECT_DIRECTORY': str(alternate),
            'GIT_ALTERNATE_OBJECT_DIRECTORIES': str(self.source / '.git/objects'),
        }):
            substituted = subprocess.check_output(
                ['git', '-C', str(self.source), 'cat-file', 'blob', blob],
            )
            self.assertEqual(substituted, replacement)
            corpus.project_commit(self.source, self.projection, self.revision, min_files=1)

        self.assertEqual((self.projection / 'src/main.rs').read_bytes(), original)

    def test_reconciliation_checks_expected_xsh_and_tokei_file_sets(self):
        corpus.project_commit(self.source, self.projection, self.revision, min_files=1)
        names = ('src/main.rs',)
        xsh = {
            'Total': {'children': {
                'Rust': [{'name': str(self.projection / names[0])}],
            }},
        }
        tokei = {
            'Total': {'blanks': 0, 'code': 1, 'comments': 0, 'reports': []},
            'Rust': {'reports': [{'name': names[0]}]},
        }
        xsh_path = self.root / 'xsh.json'
        tokei_path = self.root / 'tokei.json'
        xsh_path.write_text(json.dumps(xsh))
        tokei_path.write_text(json.dumps(tokei))

        result = corpus.reconcile(self.source, self.projection, xsh_path, tokei_path,
                                  revision=self.revision, min_files=1)
        self.assertTrue(result['exact'])
        self.assertEqual(result['expected_files'], 1)
        tokei['Rust']['reports'] = []
        tokei_path.write_text(json.dumps(tokei))
        result = corpus.reconcile(self.source, self.projection, xsh_path, tokei_path,
                                  revision=self.revision, min_files=1)
        self.assertFalse(result['exact'])
        self.assertEqual(result['tokei']['missing'], ['src/main.rs'])

    def test_reconciliation_rejects_projection_content_drift_and_path_escape(self):
        corpus.project_commit(self.source, self.projection, self.revision, min_files=1)
        xsh_path = self.root / 'xsh.json'
        tokei_path = self.root / 'tokei.json'
        xsh_path.write_text(json.dumps({'Total': {'children': {}}}))
        tokei_path.write_text(json.dumps({}))
        (self.projection / 'src/main.rs').write_bytes(b'changed size\n')
        with self.assertRaises(corpus.CorpusError):
            corpus.reconcile(self.source, self.projection, xsh_path, tokei_path,
                             revision=self.revision, min_files=1)

        (self.projection / 'src/main.rs').write_bytes(b'fn main() {}\n')
        xsh_path.write_text(json.dumps({'Total': {'children': {
            'Rust': [{'name': str(self.root / 'outside.rs')}],
        }}}))
        with self.assertRaises(corpus.CorpusError):
            corpus.reconcile(self.source, self.projection, xsh_path, tokei_path,
                             revision=self.revision, min_files=1)


if __name__ == '__main__':
    unittest.main()
