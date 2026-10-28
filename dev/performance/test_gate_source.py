import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from dev import gate_source


ROOT = Path(__file__).resolve().parents[2]
OBSERVER = ROOT / 'dev/gate_source.py'


class SourceSnapshotTests(unittest.TestCase):
    def test_tracked_link_observes_changes_to_ignored_intermediate_link(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / '.gitignore').write_text('alias\n')
            (repo / 'first').write_text('first\n')
            (repo / 'second').write_text('second\n')
            (repo / 'alias').symlink_to('first')
            (repo / 'link').symlink_to('alias')
            subprocess.run(['git', '-C', str(repo), 'add', '.gitignore', 'first', 'second', 'link'], check=True)
            command = ['python3', '-B', str(OBSERVER)]
            before = subprocess.check_output(command, cwd=repo)
            (repo / 'alias').unlink()
            (repo / 'alias').symlink_to('second')
            self.assertNotEqual(subprocess.check_output(command, cwd=repo), before)

    def test_symlink_to_special_file_cannot_be_observed_as_source(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / '.gitignore').write_text('pipe\n')
            os.mkfifo(repo / 'pipe')
            (repo / 'link').symlink_to('pipe')
            subprocess.run(['git', '-C', str(repo), 'add', '.gitignore', 'link'], check=True)
            result = subprocess.run(['python3', '-B', str(OBSERVER)], cwd=repo, capture_output=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn(b'unsupported symlink target', result.stderr)

    def test_directory_symlink_rejects_unobserved_contents(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / '.gitignore').write_text('tree/hidden\n')
            (repo / 'tree').mkdir()
            (repo / 'tree/visible').write_text('observed\n')
            (repo / 'link').symlink_to('tree', target_is_directory=True)
            subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
            command = ['python3', '-B', str(OBSERVER)]
            before = subprocess.check_output(command, cwd=repo)
            (repo / 'tree/empty').mkdir()
            self.assertNotEqual(before, subprocess.check_output(command, cwd=repo))
            (repo / 'tree/hidden').write_text('unobserved\n')
            result = subprocess.run(command, cwd=repo, capture_output=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn(b'unobserved symlink target', result.stderr)

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

    def test_index_flag_snapshot_ignores_in_memory_bits_and_keeps_persistent_bits(self):
        def snapshot(flags):
            data = (b'file\0  ctime: 0:0\n  mtime: 0:0\n  dev: 0\tino: 0\n'
                    b'  uid: 0\tgid: 0\n  size: 0\tflags: ' + flags + b'\n')
            with patch.object(gate_source, 'git', return_value=data):
                return gate_source.index_flags(b'/repo')
        baseline = snapshot(b'0')
        self.assertEqual(snapshot(b'200000'), baseline)
        for flags in (b'8000', b'20000000', b'40004000'):
            with self.subTest(flags=flags):
                self.assertNotEqual(snapshot(flags), baseline)
        with self.assertRaisesRegex(ValueError, 'unsupported persistent'):
            snapshot(b'80000000')

    def test_index_stat_refresh_does_not_change_source_but_semantic_index_changes_do(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            path = repo / 'tracked'
            path.write_text('original\n')
            subprocess.run(['git', '-C', str(repo), 'add', 'tracked'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=Test', '-c',
                            'user.email=test@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            info = path.stat()
            os.utime(path, ns=(info.st_atime_ns, info.st_mtime_ns + 2_000_000_000))
            command = ['python3', '-B', str(OBSERVER)]
            before = subprocess.check_output(command, cwd=repo)
            subprocess.run(['git', '-C', str(repo), 'status', '--short'], check=True, stdout=subprocess.DEVNULL)
            self.assertEqual(subprocess.check_output(command, cwd=repo), before)
            for flag in ('--assume-unchanged', '--skip-worktree'):
                subprocess.run(['git', '-C', str(repo), 'update-index', flag, 'tracked'], check=True)
                self.assertNotEqual(subprocess.check_output(command, cwd=repo), before)
                subprocess.run(['git', '-C', str(repo), 'update-index', flag.replace('--', '--no-'), 'tracked'], check=True)
            new = repo / 'new\nfile'
            new.write_text('new\n')
            untracked = subprocess.check_output(command, cwd=repo)
            subprocess.run(['git', '-C', str(repo), 'add', '-N', new.name], check=True)
            self.assertNotEqual(subprocess.check_output(command, cwd=repo), untracked)
            subprocess.run(['git', '-C', str(repo), 'reset', '-q', 'HEAD', '--', new.name], check=True)
            new.unlink()
            path.write_text('staged change\n')
            subprocess.run(['git', '-C', str(repo), 'add', 'tracked'], check=True)
            path.write_text('original\n')
            self.assertNotEqual(subprocess.check_output(command, cwd=repo), before)

    def test_index_stat_refresh_during_source_inspection_is_not_a_source_change(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            path = repo / 'tracked'
            path.write_text('original\n')
            subprocess.run(['git', '-C', str(repo), 'add', 'tracked'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=Test', '-c',
                            'user.email=test@example.invalid', 'commit', '-qm', 'fixture'], check=True)
            info = path.stat()
            os.utime(path, ns=(info.st_atime_ns, info.st_mtime_ns + 2_000_000_000))
            original = gate_source.index_flags
            calls = 0
            def flags(root):
                nonlocal calls
                calls += 1
                if calls == 2:
                    subprocess.run(['git', '-C', str(repo), 'status', '--short'], check=True, stdout=subprocess.DEVNULL)
                return original(root)
            with patch.object(gate_source, 'index_flags', side_effect=flags):
                observed = gate_source.capture(repo)
            self.assertEqual(calls, 2)
            self.assertEqual(observed, gate_source.capture(repo))

    def test_snapshot_preserves_unmerged_index_stages(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo / 'tracked').write_text('worktree\n')
            def blob(value):
                return subprocess.check_output(['git', '-C', str(repo), 'hash-object', '-w', '--stdin'], input=value).strip()
            first, second, third, replacement = (blob(value) for value in (b'base', b'ours', b'theirs', b'other'))
            def stages(ours):
                records = b'0 ' + b'0' * 40 + b'\ttracked\n' + b''.join(
                    b'100644 ' + oid + b' ' + str(stage).encode() + b'\ttracked\n'
                    for stage, oid in ((1, first), (2, ours), (3, third)))
                subprocess.run(['git', '-C', str(repo), 'update-index', '--index-info'], input=records, check=True)
            stages(second)
            command = ['python3', '-B', str(OBSERVER)]
            before = subprocess.check_output(command, cwd=repo)
            stages(replacement)
            self.assertNotEqual(subprocess.check_output(command, cwd=repo), before)

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

    def test_a_branch_advanced_in_another_worktree_does_not_change_the_snapshot(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            repo = Path(directory) / 'repo'
            linked = Path(directory) / 'linked'
            identity = ['-c', 'user.name=Test', '-c', 'user.email=test@example.invalid']
            subprocess.run(['git', 'init', '-q', '-b', 'main', str(repo)], check=True)
            (repo / 'tracked').write_text('original\n')
            subprocess.run(['git', '-C', str(repo), 'add', 'tracked'], check=True)
            subprocess.run(['git', '-C', str(repo), *identity, 'commit', '-qm', 'fixture'], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'branch.autoSetupMerge=always', 'worktree', 'add', '-q',
                            '-b', 'lane', str(linked), 'main'], check=True)
            before = subprocess.check_output(['python3', '-B', str(OBSERVER)], cwd=linked)
            (repo / 'other').write_text('other\n')
            subprocess.run(['git', '-C', str(repo), 'add', 'other'], check=True)
            subprocess.run(['git', '-C', str(repo), *identity, 'commit', '-qm', 'advance'], check=True)
            self.assertEqual(subprocess.check_output(['python3', '-B', str(OBSERVER)], cwd=linked), before)
            (linked / 'tracked').write_text('edited\n')
            self.assertNotEqual(subprocess.check_output(['python3', '-B', str(OBSERVER)], cwd=linked), before)

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
