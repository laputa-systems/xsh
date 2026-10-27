from pathlib import Path
from unittest import TestCase, mock
import os
import subprocess
import tempfile

from dev.performance import laputa_plan as plan


class LaputaPlanTests(TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=os.environ['TMPDIR'])
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_git_root_requires_absolute_top_level(self):
        subprocess.run(['git', 'init', '--quiet', str(self.root)], check=True)
        child = self.root / 'child'
        child.mkdir()
        self.assertEqual(plan.repository(str(self.root))[0], self.root)
        with self.assertRaises(ValueError):
            plan.repository(str(child))
        with self.assertRaises(ValueError):
            plan.repository('relative')

    def test_repository_ignores_inherited_git_directory(self):
        subprocess.run(['git', 'init', '--quiet', str(self.root)], check=True)
        expected = plan.repository(str(self.root))
        with mock.patch.dict(os.environ, {'GIT_DIR': str(self.root / 'missing')}):
            self.assertEqual(plan.repository(str(self.root)), expected)

    def test_product_binds_bytes_and_mode(self):
        path = self.root / 'xsh'
        path.write_bytes(b'first')
        path.chmod(0o755)
        first = plan.product(path)
        path.write_bytes(b'second')
        self.assertNotEqual(first, plan.product(path))
        path.chmod(0o700)
        self.assertNotEqual(first[1], plan.product(path)[1])

    def test_command_confines_mounts_and_selects_only_two_ids(self):
        root = Path('/fixture/laputa')
        metadata = Path('/fixture/git')
        xsh = Path('/fixture/bin/xsh')
        xsht = Path('/fixture/bin/xsht')
        scratch = Path('/fixture/work')
        argv = plan.command('sha256:' + 'a' * 64, root, metadata, xsh, xsht, scratch)
        self.assertIn('--network', argv)
        self.assertIn('none', argv)
        self.assertIn('--read-only', argv)
        self.assertIn('--cpus=2', argv)
        self.assertIn('--memory=4g', argv)
        mounts = [argv[i + 1] for i, arg in enumerate(argv) if arg == '--mount']
        self.assertEqual(sum(not value.endswith(',readonly') for value in mounts), 1)
        self.assertEqual([argv[i + 1] for i, arg in enumerate(argv) if arg == '--exact-id'], list(plan.IDS))
        with self.assertRaises(ValueError):
            plan.command('sha256:' + 'a' * 64, root, metadata, xsh, xsht, root / 'scratch')
        with self.assertRaises(ValueError):
            plan.mount(Path('/fixture/a,b'), True)

    def test_kbuild_discovery_selects_only_the_synthetic_exact_case(self):
        root = Path('/fixture/laputa')
        metadata = Path('/fixture/git')
        xsh = Path('/fixture/bin/xsh')
        xsht = Path('/fixture/bin/xsht')
        scratch = Path('/fixture/work')
        argv = plan.command('sha256:' + 'a' * 64, root, metadata, xsh, xsht, scratch,
                            case=plan.KBUILD_DISCOVERY_CASE)
        self.assertEqual(plan.selected_ids(plan.KBUILD_DISCOVERY_CASE), plan.KBUILD_DISCOVERY_IDS)
        self.assertEqual(
            [argv[i + 1] for i, arg in enumerate(argv) if arg == '--exact-id'],
            list(plan.KBUILD_DISCOVERY_IDS),
        )
        with self.assertRaisesRegex(ValueError, 'unknown Laputa case'):
            plan.command('sha256:' + 'a' * 64, root, metadata, xsh, xsht, scratch, case='laputa/full')

    def test_pm_root_composition_selects_isolated_inventory_and_collision_tests(self):
        argv = plan.command('sha256:' + 'a' * 64, Path('/fixture/laputa'), Path('/fixture/git'),
                            Path('/fixture/bin/xsh'), Path('/fixture/bin/xsht'), Path('/fixture/work'),
                            case='laputa/pm.root/compose')
        self.assertEqual(
            [argv[i + 1] for i, arg in enumerate(argv) if arg == '--exact-id'],
            [
                'tests/pm/pm_root.xsh::test_root_preserves_file_mode_symlink_and_empty_directory',
                'tests/pm/pm_root.xsh::test_root_plan_and_receipt_are_deterministic_for_multiple_artifacts',
                'tests/pm/pm_root.xsh::test_root_coalesces_identical_nested_directories_with_a_canonical_owner',
                'tests/pm/pm_root.xsh::test_root_rejects_collisions_and_same_owner_duplicate_entries_before_mutation',
                'tests/pm/pm_root.xsh::test_root_identifies_the_missing_payload_inventory_entry',
            ],
        )

    def test_pm_plan_inventory_selects_graph_reuse_and_executor_contracts(self):
        argv = plan.command('sha256:' + 'a' * 64, Path('/fixture/laputa'), Path('/fixture/git'),
                            Path('/fixture/bin/xsh'), Path('/fixture/bin/xsht'), Path('/fixture/work'),
                            case='laputa/pm.plan/inventory')
        self.assertEqual(
            [argv[i + 1] for i, arg in enumerate(argv) if arg == '--exact-id'],
            [
                'tests/pm/pm_plan.xsh::test_build_plan_is_deterministic_and_has_dependency_first_order',
                'tests/pm/pm_plan.xsh::test_build_plan_reuses_exact_remote_and_carries_retrieval',
                'tests/pm/pm_plan.xsh::test_executor_payload_contract_changes_require_explicit_epoch_invalidation',
                'tests/pm/pm_plan.xsh::test_build_plan_keeps_same_package_dependency_edges_by_kind',
            ],
        )

    def test_kbuild_graph_cache_selects_only_synthetic_graph_tests(self):
        argv = plan.command('sha256:' + 'a' * 64, Path('/fixture/laputa'), Path('/fixture/git'),
                            Path('/fixture/bin/xsh'), Path('/fixture/bin/xsht'), Path('/fixture/work'),
                            case='laputa/kbuild/graph-cache')
        self.assertEqual(
            [argv[i + 1] for i, arg in enumerate(argv) if arg == '--exact-id'],
            [
                'packages/linux/tests/main.xsh::test_kbuild_plans_x86_efi_stub_objects',
                'packages/linux/tests/main.xsh::test_kbuild_local_record_graph_matches_default',
                'packages/linux/tests/main.xsh::test_kbuild_worker_records_bind_included_inputs',
                'packages/linux/tests/main.xsh::test_kbuild_local_record_cache_reuses_and_invalidates',
                'packages/linux/tests/main.xsh::test_kbuild_local_record_cache_binds_parsed_configuration',
                'packages/linux/tests/main.xsh::test_kbuild_rebuilds_legacy_local_records_after_make_semantics_change',
                'packages/linux/tests/main.xsh::test_kbuild_rebuilds_legacy_discovered_plan_cache',
                'packages/linux/tests/main.xsh::test_kbuild_rebuilds_legacy_compile_flags_cache',
                'packages/linux/tests/main.xsh::test_kbuild_compile_flags_cache_binds_parsed_include',
                'packages/linux/tests/main.xsh::test_kbuild_archive_plan_cache_binds_parsed_compile_flags',
                'packages/linux/tests/main.xsh::test_kbuild_archive_plan_cache_invalidates_when_source_appears',
                'packages/linux/tests/main.xsh::test_kbuild_parallel_archive_analysis_matches_serial',
                'packages/linux/tests/main.xsh::test_kbuild_plan_cache_invalidates_nested_and_included_makefiles',
                'packages/linux/tests/main.xsh::test_kbuild_plan_cache_distinguishes_source_architectures',
                'packages/linux/tests/main.xsh::test_kbuild_runs_archive_plan_output_from_json',
                'tests/pm/pm_make.xsh::make_tasks_rebuild_for_subsecond_input_changes',
                'tests/pm/linux_recipe.xsh::test_linux_discovery_pool_executes_worker_from_staged_recipe',
                'tests/pm/linux_recipe.xsh::test_linux_plan_cache_rejects_inputs_changed_after_worker_scan',
                'packages/linux/tests/main.xsh::test_kbuild_writes_text_plan',
            ],
        )

    def test_world_plan_selects_isolated_host_plan_contract(self):
        argv = plan.command('sha256:' + 'a' * 64, Path('/fixture/laputa'), Path('/fixture/git'),
                            Path('/fixture/bin/xsh'), Path('/fixture/bin/xsht'), Path('/fixture/work'),
                            case='laputa/world/host-plan')
        self.assertEqual(
            [argv[i + 1] for i, arg in enumerate(argv) if arg == '--exact-id'],
            ['tests/system/seed.xsh::test_world_host_plan_scopes_the_mirror_and_keeps_typed_selection'],
        )

    def test_kbuild_make_semantics_selects_regressions_and_flag_consumers(self):
        argv = plan.command('sha256:' + 'a' * 64, Path('/fixture/laputa'), Path('/fixture/git'),
                            Path('/fixture/bin/xsh'), Path('/fixture/bin/xsht'), Path('/fixture/work'),
                            case='laputa/kbuild/make-semantics')
        self.assertEqual(
            [argv[i + 1] for i, arg in enumerate(argv) if arg == '--exact-id'],
            [
                'packages/linux/tests/main.xsh::test_kbuild_expands_nested_make_functions_and_replaces_object_assignments',
                'packages/linux/tests/main.xsh::test_kbuild_simple_assignments_replace_prior_object_lists',
                'packages/linux/tests/main.xsh::test_kbuild_defined_empty_flags_reject_conditional_defaults',
                'packages/linux/tests/main.xsh::test_kbuild_applies_object_and_subdir_cflags',
                'packages/linux/tests/main.xsh::test_kbuild_applies_composite_makefile_cflags_to_subdir_members',
            ],
        )

    def test_pm_reuse_selects_only_the_exact_reuse_count_case(self):
        root = Path('/fixture/laputa')
        metadata = Path('/fixture/git')
        xsh = Path('/fixture/bin/xsh')
        xsht = Path('/fixture/bin/xsht')
        scratch = Path('/fixture/work')
        argv = plan.command('sha256:' + 'a' * 64, root, metadata, xsh, xsht, scratch,
                            case=plan.PM_REUSE_CASE, xshi=Path('/fixture/bin/xshi'))
        self.assertEqual(plan.selected_ids(plan.PM_REUSE_CASE), plan.PM_REUSE_IDS)
        self.assertEqual(
            [argv[i + 1] for i, arg in enumerate(argv) if arg == '--exact-id'],
            list(plan.PM_REUSE_IDS),
        )
        mounts = [argv[i + 1] for i, arg in enumerate(argv) if arg == '--mount']
        self.assertIn('type=bind,src=/fixture/bin/xshi,dst=/fixture/bin/xshi,readonly', mounts)
        with self.assertRaisesRegex(ValueError, 'requires the XSHI product'):
            plan.command('sha256:' + 'a' * 64, root, metadata, xsh, xsht, scratch,
                         case=plan.PM_REUSE_CASE)
        with self.assertRaisesRegex(ValueError, 'sibling xsh, xsht and xshi'):
            plan.command('sha256:' + 'a' * 64, root, metadata, xsh, Path('/fixture/other/xsht'), scratch,
                         case=plan.PM_REUSE_CASE, xshi=Path('/fixture/bin/xshi'))
        with self.assertRaisesRegex(ValueError, 'sibling xsh, xsht and xshi'):
            plan.command('sha256:' + 'a' * 64, root, metadata, xsh, xsht, scratch,
                         case=plan.PM_REUSE_CASE, xshi=Path('/fixture/bin/not-xshi'))

    def test_pm_reuse_binds_xshi_before_and_after_container_execution(self):
        from contextlib import contextmanager
        from types import SimpleNamespace
        from unittest import mock

        owner = self.root / 'owner'
        scratch = owner / '.work/tmp'
        scratch.mkdir(parents=True)
        laputa = self.root / 'laputa'
        laputa.mkdir()
        metadata = laputa / '.git'
        metadata.mkdir()
        xsh, xsht, xshi = (Path('/fixture/bin') / name for name in ('xsh', 'xsht', 'xshi'))
        seen_products = []

        @contextmanager
        def lease(root, *, native):
            self.assertEqual(root, owner)
            self.assertTrue(native)
            yield

        lease_owner = SimpleNamespace(
            campaign_leases=lease,
            prepare_cleanup_unresolved=lambda detail: None,
            resolve_cleanup_unresolved=lambda: None,
        )

        def docker(argv, work, *, timeout_seconds, on_cleanup_verified):
            mounts = [argv[i + 1] for i, arg in enumerate(argv) if arg == '--mount']
            self.assertIn('type=bind,src=/fixture/bin/xshi,dst=/fixture/bin/xshi,readonly', mounts)
            (work / 'events.jsonl').write_text('fixture')
            (work / 'xsht-status').write_text('0\n')
            on_cleanup_verified()
            return 0, b'', b''

        def observe_product(path):
            seen_products.append(path)
            return ('product', 0o755)

        with mock.patch.object(plan, 'repository', side_effect=[(owner, owner / '.git'), (laputa, metadata)]), \
                mock.patch.object(plan, 'snapshot', side_effect=['xsh-source', 'laputa-source', 'xsh-source', 'laputa-source']), \
                mock.patch.object(plan, 'product', side_effect=observe_product), \
                mock.patch.object(plan, 'verify_image'), \
                mock.patch.object(plan.check, 'checked_docker', side_effect=docker), \
                mock.patch.object(plan.results, 'reconcile_xsht', return_value={'status': 0}):
            result = plan.run(owner, laputa, xsh, xsht, scratch, 'sha256:' + 'a' * 64, lease_owner,
                              expected=('xsh-source', 'laputa-source'), case=plan.PM_REUSE_CASE, xshi=xshi)
        self.assertEqual(result[:2], (0, 0))
        self.assertEqual(seen_products, [xsh, xsht, xshi, xsh, xsht, xshi])

    def test_cli_passes_the_selected_laputa_case_to_the_shared_runner(self):
        from io import BytesIO
        from types import SimpleNamespace
        from unittest import mock

        owner = self.root / 'owner'
        laputa = self.root / 'laputa'
        args = [
            '--case', plan.PM_REUSE_CASE,
            '--owner', str(owner), '--observe', str(laputa),
            '--xsh', str(self.root / 'xsh'), '--xsht', str(self.root / 'xsht'), '--xshi', str(self.root / 'xshi'),
            '--scratch', str(self.root / 'scratch'), '--image', 'sha256:' + 'a' * 64,
            '--xsh-source', 'xsh-source', '--laputa-source', 'laputa-source',
        ]
        output_text, diagnostic_text = [], []
        output = SimpleNamespace(buffer=BytesIO(), write=output_text.append)
        diagnostics = SimpleNamespace(buffer=BytesIO(), write=diagnostic_text.append)
        with mock.patch.object(plan, 'repository', side_effect=[(owner, owner / '.git'), (laputa, laputa / '.git')]), \
                mock.patch.object(plan, 'load_leases', return_value=object()), \
                mock.patch.object(plan, 'run', return_value=(0, 0, b'output', b'diagnostics')) as run, \
                mock.patch.object(plan.sys, 'stdout', output), mock.patch.object(plan.sys, 'stderr', diagnostics):
            self.assertEqual(plan.main(args), 0)
        self.assertEqual(run.call_args.kwargs['case'], plan.PM_REUSE_CASE)
        self.assertEqual(output.buffer.getvalue(), b'output')
        self.assertEqual(diagnostics.buffer.getvalue(), b'diagnostics')
        self.assertIn(plan.PM_REUSE_CASE, ''.join(output_text))

    def test_fake_container_preserves_failure_and_rejects_changed_source(self):
        from contextlib import contextmanager
        from types import SimpleNamespace
        from unittest import mock

        owner = self.root / 'xsh'
        scratch = owner / '.work/tmp'
        scratch.mkdir(parents=True)
        laputa = self.root / 'laputa'
        laputa.mkdir()
        metadata = laputa / '.git'
        metadata.mkdir()
        xsh = self.root / 'xsh-bin'
        xsht = self.root / 'xsht-bin'
        for path in (xsh, xsht):
            path.write_bytes(b'product')
            path.chmod(0o755)
        transitions = []

        @contextmanager
        def lease(root, *, native):
            self.assertEqual(root, owner)
            self.assertTrue(native)
            transitions.append('acquire')
            yield
            transitions.append('release')

        lease_owner = SimpleNamespace(
            campaign_leases=lease,
            prepare_cleanup_unresolved=lambda detail: transitions.append('quarantine'),
            resolve_cleanup_unresolved=lambda: transitions.append('resolved'),
        )

        def docker(argv, work, *, timeout_seconds, on_cleanup_verified):
            self.assertEqual(timeout_seconds, 300)
            (work / 'events.jsonl').write_text('fixture')
            (work / 'xsht-status').write_text(f'{status}\n')
            on_cleanup_verified()
            return status, b'out', b'err'

        for status, expected in ((0, 2), (2, 2), (1, 1), (130, 130)):
            with self.subTest(status=status), \
                    mock.patch.object(plan, 'repository', side_effect=[(owner, owner / '.git'), (laputa, metadata)]), \
                    mock.patch.object(plan, 'snapshot', side_effect=['before-x', 'before-l', 'after-x', 'after-l']), \
                    mock.patch.object(plan, 'verify_image'), \
                    mock.patch.object(plan.check, 'checked_docker', side_effect=docker), \
                    mock.patch.object(plan.results, 'reconcile_xsht', return_value={'status': 0}):
                result = plan.run(owner, laputa, xsh, xsht, scratch, 'sha256:' + 'a' * 64, lease_owner)
                self.assertEqual(result[:2], (expected, status))
                self.assertFalse(list(scratch.iterdir()))
        self.assertEqual(transitions, ['acquire', 'quarantine', 'resolved', 'release'] * 4)

    def test_json_events_reconcile_exact_scope(self):
        import json
        common = {'schema': 1, 'run_id': 'fixture'}
        rows = [{**common, 'event': 'run_started', 'selected': list(plan.IDS)}]
        for name in plan.IDS:
            rows.append({**common, 'event': 'case_started', 'case_id': name})
            rows.append({**common, 'event': 'case_finished', 'case_id': name,
                         'outcome': 'passed', 'reason': None, 'duration_ns': 1})
        counts = {name: int(name == 'passed') * 2 for name in plan.results.OUTCOMES}
        rows.append({**common, 'event': 'run_finished', 'status': 0, 'reason': None,
                     'selected': 2, 'completed': 2, 'counts': counts, 'duration_ns': 2})
        events = ''.join(json.dumps(row) + '\n' for row in rows)
        self.assertEqual(plan.results.reconcile_xsht(events, plan.IDS, child_status=0)['status'], 0)
        with self.assertRaises(ValueError):
            plan.results.reconcile_xsht(events.replace(plan.IDS[0], 'other'), plan.IDS, child_status=0)

    def test_cleanup_failure_preserves_observed_child_failure(self):
        from contextlib import redirect_stderr
        from io import StringIO
        from unittest import mock
        args = ['--owner', '/owner', '--observe', '/laputa', '--xsh', '/xsh', '--xsht', '/xsht',
                '--scratch', '/scratch', '--image', 'sha256:' + 'a' * 64,
                '--xsh-source', 'x', '--laputa-source', 'l']
        for status, expected in ((1, 1), (125, 2), (2, 2), (0, 2), (130, 2), (None, 2)):
            error = plan.check.DockerCleanupError('cleanup unresolved', child_status=status,
                stdout=b'child out', stderr=b'child err', cleanup={'resolved': False},
                cleanup_error=None, resolution_error=None)
            error.workload_status = 1 if status == 1 else None
            with self.subTest(status=status), \
                    mock.patch.object(plan, 'repository', side_effect=[(Path('/owner'), Path('/git')), (Path('/laputa'), Path('/git'))]), \
                    mock.patch.object(plan, 'load_leases'), mock.patch.object(plan, 'run', side_effect=error), \
                    redirect_stderr(StringIO()) as diagnostics:
                self.assertEqual(plan.main(args), expected)
                self.assertIn(f'raw_status={status}', diagnostics.getvalue())
                self.assertIn('child err', diagnostics.getvalue())

    def test_selection_source_change_prevents_container_launch(self):
        from contextlib import contextmanager
        from types import SimpleNamespace
        from unittest import mock
        owner = self.root / 'owner'
        scratch = owner / '.work/tmp'
        scratch.mkdir(parents=True)
        laputa = self.root / 'laputa'
        laputa.mkdir()
        xsh = self.root / 'xsh'
        xsht = self.root / 'xsht'
        for path in (xsh, xsht):
            path.write_bytes(b'product')
            path.chmod(0o755)
        @contextmanager
        def lease(root, *, native):
            yield
        leases = SimpleNamespace(campaign_leases=lease)
        with mock.patch.object(plan, 'repository', side_effect=[(owner, owner / '.git'), (laputa, laputa / '.git')]), \
                mock.patch.object(plan, 'snapshot', side_effect=['changed', 'laputa']), \
                mock.patch.object(plan, 'verify_image') as image, \
                mock.patch.object(plan.check, 'checked_docker') as docker, \
                self.assertRaisesRegex(ValueError, 'source changed since selection'):
            plan.run(owner, laputa, xsh, xsht, scratch, 'sha256:' + 'a' * 64, leases, expected=('original', 'laputa'))
        image.assert_not_called()
        docker.assert_not_called()

    def test_linked_worktree_mounts_git_common_directory_read_only(self):
        checkout = self.root / 'checkout'
        subprocess.run(['git', 'init', '--quiet', str(checkout)], check=True)
        subprocess.run(['git', '-C', str(checkout), '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                        'commit', '--quiet', '--allow-empty', '-m', 'fixture'], check=True)
        worktree = self.root / 'worktree'
        subprocess.run(['git', '-C', str(checkout), 'worktree', 'add', '--quiet', '--detach', str(worktree)], check=True)
        root, metadata = plan.repository(str(worktree))
        self.assertEqual(metadata, checkout / '.git')
        argv = plan.command('sha256:' + 'a' * 64, root, metadata, self.root / 'xsh', self.root / 'xsht', self.root / 'scratch')
        self.assertIn(plan.mount(metadata, True), argv)

    def test_docker_launch_status_is_not_an_xsht_failure(self):
        status = self.root / 'xsht-status'
        for raw in (1, 125, 126, 127):
            self.assertIsNone(plan.workload_status(self.root, raw))
        status.write_text('1\n')
        self.assertEqual(plan.workload_status(self.root, 1), 1)
        self.assertIsNone(plan.workload_status(self.root, 125))
        status.write_text('127\n')
        self.assertIsNone(plan.workload_status(self.root, 127))

    def test_product_mutation_after_workload_makes_success_incomplete(self):
        from contextlib import contextmanager
        from types import SimpleNamespace
        from unittest import mock
        owner = self.root / 'owner'
        scratch = owner / '.work/tmp'
        scratch.mkdir(parents=True)
        laputa = self.root / 'laputa'
        laputa.mkdir()
        xsh = self.root / 'xsh'
        xsht = self.root / 'xsht'
        for path in (xsh, xsht):
            path.write_bytes(b'product')
            path.chmod(0o755)
        @contextmanager
        def lease(root, *, native):
            yield
        leases = SimpleNamespace(campaign_leases=lease, prepare_cleanup_unresolved=lambda detail: None,
                                 resolve_cleanup_unresolved=lambda: None)
        def docker(argv, work, *, timeout_seconds, on_cleanup_verified):
            (work / 'xsht-status').write_text('0\n')
            (work / 'events.jsonl').write_text('fixture')
            xsh.write_bytes(b'changed')
            on_cleanup_verified()
            return 0, b'', b''
        with mock.patch.object(plan, 'repository', side_effect=[(owner, owner / '.git'), (laputa, laputa / '.git')]), \
                mock.patch.object(plan, 'snapshot', return_value='unchanged'), \
                mock.patch.object(plan, 'verify_image'), mock.patch.object(plan.check, 'checked_docker', side_effect=docker), \
                mock.patch.object(plan.results, 'reconcile_xsht', return_value={'status': 0}):
            result = plan.run(owner, laputa, xsh, xsht, scratch, 'sha256:' + 'a' * 64, leases)
        self.assertEqual(result[:2], (2, 0))

    def test_interrupted_runner_preserves_verified_failure_evidence(self):
        import json
        common = {'schema': 1, 'run_id': 'fixture'}
        rows = [{**common, 'event': 'run_started', 'selected': list(plan.IDS)},
                {**common, 'event': 'case_started', 'case_id': plan.IDS[0]},
                {**common, 'event': 'case_finished', 'case_id': plan.IDS[0], 'outcome': 'failed',
                 'reason': {'kind': 'assertion', 'detail': 'failed'}, 'duration_ns': 1}]
        events = self.root / 'events.jsonl'
        events.write_text(''.join(json.dumps(row) + '\n' for row in rows) + '{"truncated"')
        self.assertEqual(plan.interrupted_evidence(self.root), 'failed')
        rows[-1]['outcome'] = 'cancelled'
        events.write_text(''.join(json.dumps(row) + '\n' for row in rows))
        self.assertEqual(plan.interrupted_evidence(self.root), 'cancelled')
        rows[0]['selected'] = ['unrelated']
        events.write_text(''.join(json.dumps(row) + '\n' for row in rows))
        self.assertIsNone(plan.interrupted_evidence(self.root))

    def test_kbuild_interrupted_evidence_is_bound_to_its_exact_id(self):
        import json
        common = {'schema': 1, 'run_id': 'fixture'}
        selected = plan.KBUILD_DISCOVERY_IDS
        rows = [{**common, 'event': 'run_started', 'selected': list(selected)},
                {**common, 'event': 'case_started', 'case_id': selected[0]},
                {**common, 'event': 'case_finished', 'case_id': selected[0], 'outcome': 'failed',
                 'reason': {'kind': 'assertion', 'detail': 'failure'}, 'duration_ns': 1}]
        (self.root / 'events.jsonl').write_text(
            ''.join(json.dumps(row) + '\n' for row in rows) + '{"truncated"'
        )
        self.assertEqual(plan.interrupted_evidence(self.root, selected), 'failed')
        rows[0]['selected'] = list(plan.IDS)
        (self.root / 'events.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in rows))
        self.assertIsNone(plan.interrupted_evidence(self.root, selected))

    def test_killed_container_keeps_failure_and_bare_130_is_incomplete(self):
        from contextlib import contextmanager, nullcontext
        from types import SimpleNamespace
        from unittest import mock
        import json
        owner = self.root / 'owner'
        scratch = owner / '.work/tmp'
        scratch.mkdir(parents=True)
        laputa = self.root / 'laputa'
        laputa.mkdir()
        xsh, xsht = self.root / 'xsh', self.root / 'xsht'
        for path in (xsh, xsht):
            path.write_bytes(b'product')
            path.chmod(0o755)
        @contextmanager
        def lease(root, *, native):
            yield
        leases = SimpleNamespace(campaign_leases=lease, prepare_cleanup_unresolved=lambda detail: None,
                                 resolve_cleanup_unresolved=lambda: None)
        common = {'schema': 1, 'run_id': 'fixture'}
        rows = [{**common, 'event': 'run_started', 'selected': list(plan.IDS)},
                {**common, 'event': 'case_started', 'case_id': plan.IDS[0]},
                {**common, 'event': 'case_finished', 'case_id': plan.IDS[0], 'outcome': 'failed',
                 'reason': {'kind': 'assertion', 'detail': 'failure'}, 'duration_ns': 1}]
        def docker(argv, work, *, timeout_seconds, on_cleanup_verified):
            if raw == 137:
                rows[-1]['outcome'] = outcome
                (work / 'events.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in rows))
            on_cleanup_verified()
            return raw, b'', b''
        for raw, outcome, expected in ((137, 'failed', 1), (137, 'cancelled', 130), (130, 'failed', 2), (125, 'failed', 2)):
            with self.subTest(raw=raw, outcome=outcome), \
                    mock.patch.object(plan, 'repository', side_effect=[(owner, owner / '.git'), (laputa, laputa / '.git')]), \
                    mock.patch.object(plan, 'snapshot', return_value='unchanged'), \
                    mock.patch.object(plan, 'verify_image'), mock.patch.object(plan.check, 'checked_docker', side_effect=docker), \
                    (mock.patch.object(plan.shutil, 'rmtree', side_effect=OSError('scratch cleanup failed')) if raw == 137 else nullcontext()):
                self.assertEqual(plan.run(owner, laputa, xsh, xsht, scratch, 'sha256:' + 'a' * 64, leases)[0], expected)

    def test_event_reader_rejects_fifo_and_oversize_without_blocking(self):
        path = self.root / 'events.jsonl'
        os.mkfifo(path)
        with self.assertRaises(ValueError):
            plan.event_bytes(self.root)
        path.unlink()
        with path.open('wb') as stream:
            stream.truncate(16 * 1024 * 1024 + 1)
        with self.assertRaises(ValueError):
            plan.event_bytes(self.root)

    def test_container_entrypoint_records_actual_child_completion(self):
        status = self.root / 'status'
        argv = plan.command('sha256:' + 'a' * 64, Path('/laputa'), Path('/git'), Path('/xsh'), Path('/xsht'), Path('/scratch'))
        script = argv[argv.index('-c') + 1]
        for code in (0, 1, 2):
            result = subprocess.run(['/bin/sh', '-c', script, 'fixture', str(status), '/bin/sh', '-c', f'exit {code}'])
            self.assertEqual(result.returncode, code)
            self.assertEqual(status.read_text(), f'{code}\n')
