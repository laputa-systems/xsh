from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from dev.performance import check, tokei_corpus, tokei_scenario


class TokeiScenarioTests(unittest.TestCase):
    def test_cli_rejects_missing_config_as_incomplete(self):
        result = subprocess.run(
            [sys.executable, '-B', '-m', 'dev.performance.tokei_scenario', '--config',
             str(self.root / 'missing-config')],
            capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn('incomplete', result.stderr)

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
        subprocess.run(['git', '-C', str(self.source), 'config', 'user.name', 'Scenario Test'], check=True)
        (self.source / 'src').mkdir()
        (self.source / 'src/lib.rs').write_text('pub fn answer() -> u8 { 42 }\n')
        subprocess.run(['git', '-C', str(self.source), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(self.source), 'commit', '-qm', 'fixture'], check=True)
        self.revision = subprocess.check_output(
            ['git', '-C', str(self.source), 'rev-parse', 'HEAD'], text=True,
        ).strip()
        self.projection = self.root / 'projection'
        tokei_corpus.project_commit(self.source, self.projection, self.revision, min_files=1)
        self.xsh = self.root / 'xsh'
        self.tokei = self.root / 'tokei'
        self.script = self.source / 'showcase/tokei.xsh'
        self.script.parent.mkdir()
        self.script.write_text('fixture\n')
        self._write_products()

    def _write_products(self, version=tokei_scenario.TOKEI_VERSION):
        self.xsh.write_text(
            '#!/usr/bin/env python3\n'
            'import json,os,sys\n'
            'from pathlib import Path\n'
            'assert sys.argv[2] == "--root" and sys.argv[4] == "--json"\n'
            'root=Path(sys.argv[3])\n'
            'names=[str(path) for path in root.rglob("*.rs")]\n'
            'print(json.dumps({"Total":{"children":{"Rust":[{"name":name} for name in names]}}}))\n'
        )
        self.tokei.write_text(
            '#!/usr/bin/env python3\n'
            f'VERSION={version!r}\n'
            'import json,os,sys\n'
            'from pathlib import Path\n'
            'if sys.argv[1:] == ["--version"]:\n'
            ' print(VERSION); raise SystemExit(0)\n'
            'assert os.environ["RAYON_NUM_THREADS"] == "2"\n'
            'assert sys.argv[1:5] == ["--output", "json", "--types", "Rust"]\n'
            'assert sys.argv[5] == "--no-ignore"\n'
            'root=Path(sys.argv[-1])\n'
            'names=[str(path) for path in root.rglob("*.rs")]\n'
            'print(json.dumps({"Rust":{"reports":[{"name":name} for name in names]},'
            '"Total":{"reports":[]}}))\n'
        )
        self.xsh.chmod(0o755)
        self.tokei.chmod(0o755)

    def _run_fake_docker(self, command, scratch, *, timeout_seconds, on_cleanup_verified):
        image = next(value for value in command if value.startswith('sha256:'))
        index = command.index(image)
        driver = command[index + 4]
        self.assertEqual(driver, tokei_scenario._DRIVER)
        argv = [sys.executable, '-B', '-c', driver, *command[index + 5:]]
        result = subprocess.run(argv, cwd=self.source, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout_seconds)
        if result.returncode == 0:
            on_cleanup_verified()
        return result.returncode, result.stdout, result.stderr

    def _lease_owner(self):
        return mock.Mock()

    def _config_record(self, tokei_digest='c' * 64):
        tree = subprocess.check_output(
            ['git', '-C', str(self.source), 'rev-parse', 'HEAD^{tree}'], text=True,
        ).strip()
        return {
            'schema': 1,
            'image_id': 'sha256:' + 'a' * 64,
            'platform': 'linux/arm64',
            'source_root': str(self.source),
            'corpus_root': str(self.source),
            'commit': self.revision,
            'tree': tree,
            'xsh': str(self.xsh),
            'tokei': str(self.tokei),
            'scratch': str(self.root),
            'output': str(self.root / 'report.json'),
            'gate_binding': {
                'source_digest': 'a' * 64,
                'xsh_digest': 'b' * 64,
                'xsh_mode': 0o755,
                'tokei_digest': tokei_digest,
                'tokei_mode': 0o755,
            },
        }

    def test_config_records_the_observed_tokei_digest(self):
        config = tokei_scenario._parse_config(self._config_record())
        self.assertEqual(config.tokei_digest, 'c' * 64)
        with self.assertRaisesRegex(check.CheckError, 'full SHA-256 digest'):
            tokei_scenario._parse_config(self._config_record(tokei_digest='c' * 63))

    def test_config_rejects_symlinked_scratch_and_report_parent(self):
        linked = self.root / 'linked'
        linked.symlink_to(self.source, target_is_directory=True)
        config = self._config_record()
        config['scratch'] = str(linked)
        config['output'] = str(linked / 'report.json')
        with self.assertRaisesRegex(check.CheckError, 'scratch.*linked'):
            tokei_scenario._parse_config(config)
        config = self._config_record()
        config['output'] = str(linked / 'report.json')
        with self.assertRaisesRegex(check.CheckError, 'report parent.*linked'):
            tokei_scenario._parse_config(config)

    def test_line_counts_report_rust_differences_and_reject_invalid_counts(self):
        xsh = self.root / 'xsh-counts.json'
        tokei = self.root / 'tokei-counts.json'
        xsh.write_text(json.dumps({'Rust': {'code': 8, 'comments': 3, 'blanks': 2}}))
        tokei.write_text(json.dumps({'Rust': {'code': 10, 'comments': 2, 'blanks': 2}}))
        self.assertEqual(tokei_scenario._line_counts(xsh, tokei), {
            'xsh': {'code': 8, 'comments': 3, 'blanks': 2},
            'tokei': {'code': 10, 'comments': 2, 'blanks': 2},
            'xsh_minus_tokei': {'code': -2, 'comments': 1, 'blanks': 0},
        })
        tokei.write_text(json.dumps({'Rust': {'code': True, 'comments': 2, 'blanks': 2}}))
        with self.assertRaisesRegex(check.CheckError, 'Rust line counts'):
            tokei_scenario._line_counts(xsh, tokei)

    def test_fake_products_run_offline_and_reconcile_exact_rust_file_set(self):
        work = self.root / 'work'
        work.mkdir()
        xsh_report = self.root / 'xsh.json'
        tokei_report = self.root / 'tokei.json'
        config = mock.Mock(
            source_root=self.source,
            xsh=self.xsh,
            tokei=self.tokei,
            scratch=self.root,
            image_id='sha256:' + 'a' * 64,
            platform='linux/arm64',
        )
        owner = self._lease_owner()
        admission = {'command': ['perf-gate-ready'], 'exit_code': 0, 'stdout': '', 'stderr': ''}
        with mock.patch.object(tokei_scenario.focused, '_admission', return_value=admission) as ready, \
                mock.patch.object(tokei_scenario.focused, '_verify_image_id'), \
                mock.patch.object(tokei_scenario.check, 'checked_docker', side_effect=self._run_fake_docker) as docker:
            observed_admission = tokei_scenario._observe(config, self.projection, xsh_report, tokei_report, work, owner)

        argv = docker.call_args.args[0]
        self.assertEqual(argv[argv.index('--platform') + 1], 'linux/arm64')
        self.assertIn('--network', argv)
        self.assertIn('none', argv)
        self.assertIn('--read-only', argv)
        self.assertIn(tokei_scenario._mount(self.xsh, True), argv)
        self.assertIn(tokei_scenario._mount(self.tokei, True), argv)
        self.assertIn(tokei_scenario._mount(self.projection, True), argv)
        self.assertIn(tokei_scenario._mount(work, False), argv)
        result = tokei_corpus.reconcile(
            self.source, self.projection, xsh_report, tokei_report,
            revision=self.revision, min_files=1,
        )
        self.assertTrue(result['exact'])
        self.assertEqual(result['expected_files'], 1)
        owner.prepare_cleanup_unresolved.assert_called_once()
        owner.resolve_cleanup_unresolved.assert_called_once()
        self.assertEqual(observed_admission, admission)
        self.assertEqual(ready.call_count, 1)

    def test_unavailable_admission_rejects_before_container_launch(self):
        work = self.root / 'work'
        work.mkdir()
        config = mock.Mock(source_root=self.source, xsh=self.xsh, tokei=self.tokei,
                           scratch=self.root, image_id='sha256:' + 'a' * 64, platform='linux/amd64')
        owner = self._lease_owner()
        with mock.patch.object(tokei_scenario.focused, '_admission',
                               side_effect=check.CheckError('focused resource admission unavailable')), \
                mock.patch.object(tokei_scenario.focused, '_verify_image_id',
                                  side_effect=AssertionError('image verification ran before admission')), \
                mock.patch.object(tokei_scenario.check, 'checked_docker',
                                  side_effect=AssertionError('container launch ran before admission')):
            with self.assertRaisesRegex(check.CheckError, 'admission unavailable'):
                tokei_scenario._observe(
                    config, self.projection, self.root / 'xsh.json', self.root / 'tokei.json', work, owner,
                )
        owner.prepare_cleanup_unresolved.assert_not_called()

    def test_qualify_publishes_bound_products_source_and_exact_admission(self):
        tree = subprocess.check_output(
            ['git', '-C', str(self.source), 'rev-parse', 'HEAD^{tree}'], text=True,
        ).strip()
        xsh_digest = check.owned_digest(self.xsh)
        tokei_digest = check.owned_digest(self.tokei)
        config = tokei_scenario._parse_config(self._config_record(tokei_digest=tokei_digest))
        config = tokei_scenario._Config(
            **{**config.__dict__, 'xsh_digest': xsh_digest, 'output': self.root / 'report.json'},
        )
        admission = {'command': ['perf-gate-ready'], 'exit_code': 0, 'stdout': '', 'stderr': ''}
        projection_summary = {'revision': 'e7c9f3194280835c4487c2945c68d5f01ccacc8d', 'files': 671}
        reconciliation = {'exact': True, 'expected_files': 671}

        def project(_corpus_root, projection, _commit):
            projection.mkdir()
            return projection_summary

        owner = self._lease_owner()
        owner.__enter__ = mock.Mock()
        with mock.patch.object(tokei_scenario.focused, '_source_revision',
                               return_value={'commit': self.revision, 'tree': tree}), \
                mock.patch.object(tokei_scenario.focused, '_source_snapshot', return_value={'source': b'fixed'}), \
                mock.patch.object(tokei_scenario.focused, '_gate_source_digest', return_value='a' * 64), \
                mock.patch.object(tokei_scenario.tokei_corpus, 'project_commit', side_effect=project), \
                mock.patch.object(tokei_scenario.tokei_corpus, 'reconcile', return_value=reconciliation), \
                mock.patch.object(tokei_scenario, '_line_counts', return_value={'xsh_minus_tokei': {'code': -2}}), \
                mock.patch.object(tokei_scenario, '_observe', return_value=admission) as observe:
            report = tokei_scenario._qualify(config, owner)

        self.assertEqual(report['source_commit'], self.revision)
        self.assertEqual(report['source_tree'], tree)
        self.assertEqual(report['source_digest'], 'a' * 64)
        self.assertEqual(report['products']['xsh']['sha256'], xsh_digest)
        self.assertEqual(report['products']['tokei']['sha256'], tokei_digest)
        self.assertEqual(report['resource_admission'], admission)
        self.assertEqual(report['p0_qualification'], 'unqualified')
        self.assertEqual(report['rust_line_counts']['xsh_minus_tokei']['code'], -2)
        self.assertTrue(json.loads(config.output.read_text())['reconciliation']['exact'])
        observe.assert_called_once()

    def test_incomplete_reconciliation_does_not_publish_a_success_report(self):
        tree = subprocess.check_output(
            ['git', '-C', str(self.source), 'rev-parse', 'HEAD^{tree}'], text=True,
        ).strip()
        config = tokei_scenario._parse_config(self._config_record(tokei_digest=check.owned_digest(self.tokei)))
        config = tokei_scenario._Config(
            **{**config.__dict__, 'xsh_digest': check.owned_digest(self.xsh), 'output': self.root / 'report.json'},
        )

        def project(_corpus_root, projection, _commit):
            projection.mkdir()
            return {'revision': 'e7c9f3194280835c4487c2945c68d5f01ccacc8d', 'files': 671}

        with mock.patch.object(tokei_scenario.focused, '_source_revision',
                               return_value={'commit': self.revision, 'tree': tree}), \
                mock.patch.object(tokei_scenario.focused, '_source_snapshot', return_value={'source': b'fixed'}), \
                mock.patch.object(tokei_scenario.focused, '_gate_source_digest', return_value='a' * 64), \
                mock.patch.object(tokei_scenario.tokei_corpus, 'project_commit', side_effect=project), \
                mock.patch.object(tokei_scenario.tokei_corpus, 'reconcile', return_value={'exact': False}), \
                mock.patch.object(tokei_scenario, '_observe', return_value={'exit_code': 0}):
            with self.assertRaisesRegex(check.CheckError, 'selected file sets differ'):
                tokei_scenario._qualify(config, self._lease_owner())
        self.assertFalse(config.output.exists())

    def test_wrong_reference_version_fails_observation(self):
        self._write_products(version='tokei 15.0.0')
        work = self.root / 'work'
        work.mkdir()
        config = mock.Mock(source_root=self.source, xsh=self.xsh, tokei=self.tokei,
                           scratch=self.root, image_id='sha256:' + 'a' * 64, platform='linux/amd64')
        with mock.patch.object(tokei_scenario.focused, '_admission', return_value={'exit_code': 0}), \
                mock.patch.object(tokei_scenario.focused, '_verify_image_id'), \
                mock.patch.object(tokei_scenario.check, 'checked_docker', side_effect=self._run_fake_docker):
            with self.assertRaisesRegex(check.CheckError, 'observation failed'):
                tokei_scenario._observe(
                    config, self.projection, self.root / 'xsh.json', self.root / 'tokei.json',
                    work, self._lease_owner(),
                )

    def test_oversized_child_output_is_capped_and_child_is_stopped(self):
        marker = self.root / 'finished'
        self.tokei.write_text(
            '#!/usr/bin/env python3\n'
            'import os,sys\n'
            'if sys.argv[1:] == ["--version"]:\n'
            ' data=b"x"*65536\n'
            ' while True: os.write(1,data)\n'
            f' open({str(marker)!r},"w").write("finished")\n'
            ' raise SystemExit(0)\n'
            ' raise SystemExit(1)\n'
        )
        self.tokei.chmod(0o755)
        work = self.root / 'work'
        work.mkdir()
        config = mock.Mock(source_root=self.source, xsh=self.xsh, tokei=self.tokei,
                           scratch=self.root, image_id='sha256:' + 'a' * 64, platform='linux/amd64')
        with mock.patch.object(tokei_scenario.focused, '_admission', return_value={'exit_code': 0}), \
                mock.patch.object(tokei_scenario.focused, '_verify_image_id'), \
                mock.patch.object(tokei_scenario.check, 'checked_docker', side_effect=self._run_fake_docker):
            with self.assertRaisesRegex(check.CheckError, 'exceeded its 16777216-byte limit'):
                tokei_scenario._observe(
                    config, self.projection, self.root / 'xsh.json', self.root / 'tokei.json',
                    work, self._lease_owner(),
                )
        self.assertFalse(marker.exists())


if __name__ == '__main__':
    unittest.main()
