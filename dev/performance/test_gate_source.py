import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
OBSERVER = ROOT / 'dev/gate_source.py'


class SourceSnapshotTests(unittest.TestCase):
    def test_symlink_to_ignored_file_cannot_be_observed_as_source(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / '.gitignore').write_text('ignored\n')
            (repo / 'ignored').write_text('unobserved\n')
            (repo / 'link').symlink_to('ignored')
            subprocess.run(['git', '-C', str(repo), 'add', '.gitignore', 'link'], check=True)
            result = subprocess.run(['python3', '-B', str(OBSERVER)], cwd=repo, capture_output=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn(b'unobserved symlink target', result.stderr)
            (repo / 'ignored').unlink()
            self.assertEqual(subprocess.run(['python3', '-B', str(OBSERVER)], cwd=repo,
                                            capture_output=True).returncode, 0)
            (repo / 'ignored').write_text('tracked\n')
            subprocess.run(['git', '-C', str(repo), 'add', '-f', 'ignored'], check=True)
            self.assertEqual(subprocess.run(['python3', '-B', str(OBSERVER)], cwd=repo,
                                            capture_output=True).returncode, 0)

    def test_inherited_index_cannot_hide_tracked_ignored_file(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / '.gitignore').write_text('tracked\n')
            (repo / 'tracked').write_text('original\n')
            subprocess.run(['git', '-C', str(repo), 'add', '-f', 'tracked'], check=True)
            index = Path(directory) / 'empty-index'
            subprocess.run(['git', '-C', str(repo), 'read-tree', '--empty'],
                           env={**os.environ, 'GIT_INDEX_FILE': str(index)}, check=True)
            env = {**os.environ, 'GIT_INDEX_FILE': str(index)}
            before = subprocess.check_output(['python3', '-B', str(OBSERVER)], cwd=repo, env=env)
            (repo / 'tracked').write_text('changed\n')
            after = subprocess.check_output(['python3', '-B', str(OBSERVER)], cwd=repo, env=env)
            self.assertNotEqual(before, after)

    def test_git_overrides_do_not_change_discovery_or_revision(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / '.gitignore').write_text('ignored\n')
            (repo / 'tracked').write_text('original\n')
            subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=Test', '-c',
                            'user.email=test@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            (repo / 'tracked').write_text('changed\n')
            (repo / 'untracked').write_text('new\n')
            (repo / 'ignored').write_text('ignored\n')
            clean = os.environ.copy()
            polluted = {**clean, 'GIT_DIR': str(Path(directory) / 'missing'),
                        'GIT_INDEX_FILE': str(Path(directory) / 'missing-index'),
                        'GIT_CONFIG_COUNT': '1', 'GIT_CONFIG_KEY_0': 'core.excludesFile',
                        'GIT_CONFIG_VALUE_0': str(repo / 'tracked')}
            for mode in ('--changed', '--commit', '--tree'):
                command = ['python3', '-B', str(OBSERVER), mode]
                expected = subprocess.check_output(command, cwd=repo, env=clean)
                self.assertEqual(subprocess.check_output(command, cwd=repo, env=polluted), expected)
                if mode == '--changed':
                    self.assertEqual(set(expected.split(b'\0')[:-1]), {b'tracked', b'untracked'})

    def test_changed_paths_rejects_hidden_index_entries(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / 'tracked').write_text('original\n')
            subprocess.run(['git', '-C', str(repo), 'add', 'tracked'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=Test', '-c',
                            'user.email=test@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            (repo / 'tracked').write_text('changed\n')
            command = ['python3', '-B', str(OBSERVER), '--changed']
            env = {**os.environ, 'GIT_INDEX_FILE': str(Path(directory) / 'foreign-index')}
            self.assertEqual(subprocess.check_output(command, cwd=repo, env=env), b'tracked\0')
            for flag in ('--assume-unchanged', '--skip-worktree'):
                subprocess.run(['git', '-C', str(repo), 'update-index', flag, 'tracked'], check=True)
                result = subprocess.run(command, cwd=repo, env=env, capture_output=True)
                self.assertEqual(result.returncode, 2, flag)
                self.assertIn(b'tracked', result.stderr)
                self.assertIn(flag[2:].encode(), result.stderr)
                self.assertEqual(result.stdout, b'')
                subprocess.run(['git', '-C', str(repo), 'update-index', flag.replace('--', '--no-'), 'tracked'], check=True)

    def test_replace_ref_cannot_hide_staged_changes_or_change_head_tree(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / 'tracked').write_text('original\n')
            subprocess.run(['git', '-C', str(repo), 'add', 'tracked'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=Test', '-c',
                            'user.email=test@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            head = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
            tree = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD^{tree}'], text=True)
            (repo / 'tracked').write_text('staged\n')
            subprocess.run(['git', '-C', str(repo), 'add', 'tracked'], check=True)
            staged_tree = subprocess.check_output(['git', '-C', str(repo), 'write-tree'], text=True).strip()
            replacement = subprocess.check_output(['git', '-C', str(repo), '-c', 'user.name=Test', '-c',
                                                   'user.email=test@example.invalid', 'commit-tree', staged_tree,
                                                   '-m', 'replacement'], text=True).strip()
            subprocess.run(['git', '-C', str(repo), 'replace', head, replacement], check=True)
            self.assertEqual(subprocess.check_output(['python3', '-B', str(OBSERVER), '--changed'], cwd=repo), b'tracked\0')
            self.assertEqual(subprocess.check_output(['python3', '-B', str(OBSERVER), '--tree'], cwd=repo, text=True), tree)

    def test_common_dir_uses_linked_worktree_under_git_overrides(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            linked = Path(directory) / 'linked'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / 'tracked').write_text('original\n')
            subprocess.run(['git', '-C', str(repo), 'add', 'tracked'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=Test', '-c',
                            'user.email=test@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            subprocess.run(['git', '-C', str(repo), 'worktree', 'add', '-q', '--detach', str(linked)], check=True)
            env = {**os.environ, 'GIT_DIR': str(Path(directory) / 'foreign'),
                   'GIT_COMMON_DIR': str(Path(directory) / 'other')}
            output = subprocess.check_output(['python3', '-B', str(OBSERVER), '--common-dir'], cwd=linked, env=env)
            self.assertEqual(os.fsdecode(output).removesuffix('\n'), str(repo / '.git'))

    def test_submodule_changes_affect_the_snapshot(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            root = Path(directory)
            child = root / 'child'
            parent = root / 'parent'
            for repo in (child, parent):
                subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (child / 'input').write_text('original\n')
            subprocess.run(['git', '-C', str(child), 'add', 'input'], check=True)
            subprocess.run(['git', '-C', str(child), '-c', 'user.name=Test', '-c',
                            'user.email=test@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            subprocess.run(['git', '-C', str(parent), '-c', 'protocol.file.allow=always',
                            'submodule', 'add', '-q', str(child), 'nested'], check=True)

            def snapshot():
                return subprocess.check_output(['python3', '-B', str(OBSERVER)], cwd=parent,
                                               env={**os.environ, 'GIT_OPTIONAL_LOCKS': '0'}, text=True).strip()

            before = snapshot()
            (parent / 'nested/input').write_text('changed\n')
            self.assertNotEqual(before, snapshot())
            (parent / 'nested/input').write_text('original\n')
            (parent / 'nested/untracked').write_text('new\n')
            self.assertNotEqual(before, snapshot())
            (parent / 'nested').rename(root / 'parked')
            missing = subprocess.run(['python3', '-B', str(OBSERVER)], cwd=parent, capture_output=True)
            self.assertEqual(missing.returncode, 2)
            (parent / 'replacement').mkdir()
            (parent / 'nested').symlink_to('replacement', target_is_directory=True)
            linked = subprocess.run(['python3', '-B', str(OBSERVER)], cwd=parent, capture_output=True)
            self.assertEqual(linked.returncode, 2)
