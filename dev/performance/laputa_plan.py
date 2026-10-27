import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile

from dev.performance import check, results


CASE = 'laputa/pm.make/plan'
IDS = (
    'tests/pm/pm_make.xsh::make_plan_preserves_graph_inventory_without_effects',
    'tests/pm/pm_make.xsh::make_plan_rejects_invalid_graphs',
)
KBUILD_DISCOVERY_CASE = 'laputa/kbuild/discovery'
KBUILD_DISCOVERY_IDS = (
    'packages/linux/tests/main.xsh::test_kbuild_discovers_configured_obj_y_dirs_and_objects',
)
KBUILD_GRAPH_CASE = 'laputa/kbuild/graph-cache'
KBUILD_GRAPH_IDS = (
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
)
KBUILD_MAKE_CASE = 'laputa/kbuild/make-semantics'
KBUILD_MAKE_IDS = (
    'packages/linux/tests/main.xsh::test_kbuild_expands_nested_make_functions_and_replaces_object_assignments',
    'packages/linux/tests/main.xsh::test_kbuild_simple_assignments_replace_prior_object_lists',
    'packages/linux/tests/main.xsh::test_kbuild_defined_empty_flags_reject_conditional_defaults',
    'packages/linux/tests/main.xsh::test_kbuild_applies_object_and_subdir_cflags',
    'packages/linux/tests/main.xsh::test_kbuild_applies_composite_makefile_cflags_to_subdir_members',
)
PM_REUSE_CASE = 'laputa/pm.execute/reuse-count'
PM_REUSE_IDS = (
    'tests/pm/pm_execute.xsh::test_execute_counts_build_hooks_and_reused_artifacts',
)
PM_ROOT_CASE = 'laputa/pm.root/compose'
PM_ROOT_IDS = (
    'tests/pm/pm_root.xsh::test_root_preserves_file_mode_symlink_and_empty_directory',
    'tests/pm/pm_root.xsh::test_root_plan_and_receipt_are_deterministic_for_multiple_artifacts',
    'tests/pm/pm_root.xsh::test_root_coalesces_identical_nested_directories_with_a_canonical_owner',
    'tests/pm/pm_root.xsh::test_root_rejects_collisions_and_same_owner_duplicate_entries_before_mutation',
    'tests/pm/pm_root.xsh::test_root_identifies_the_missing_payload_inventory_entry',
)
PM_INVENTORY_CASE = 'laputa/pm.plan/inventory'
PM_INVENTORY_IDS = (
    'tests/pm/pm_plan.xsh::test_build_plan_is_deterministic_and_has_dependency_first_order',
    'tests/pm/pm_plan.xsh::test_build_plan_reuses_exact_remote_and_carries_retrieval',
    'tests/pm/pm_plan.xsh::test_executor_payload_contract_changes_require_explicit_epoch_invalidation',
    'tests/pm/pm_plan.xsh::test_build_plan_keeps_same_package_dependency_edges_by_kind',
)
WORLD_PLAN_CASE = 'laputa/world/host-plan'
WORLD_PLAN_IDS = (
    'tests/system/seed.xsh::test_world_host_plan_scopes_the_mirror_and_keeps_typed_selection',
)
CASE_IDS = {
    CASE: IDS,
    KBUILD_DISCOVERY_CASE: KBUILD_DISCOVERY_IDS,
    KBUILD_GRAPH_CASE: KBUILD_GRAPH_IDS,
    KBUILD_MAKE_CASE: KBUILD_MAKE_IDS,
    PM_REUSE_CASE: PM_REUSE_IDS,
    PM_ROOT_CASE: PM_ROOT_IDS,
    PM_INVENTORY_CASE: PM_INVENTORY_IDS,
    WORLD_PLAN_CASE: WORLD_PLAN_IDS,
}


def repository(value):
    path = Path(value)
    if not path.is_absolute() or str(path) != os.path.normpath(value) or path != path.resolve(strict=True):
        raise ValueError('repository root must be an absolute canonical path')
    output = subprocess.check_output(
        ['git', '-C', str(path), '-c', f'safe.directory={path}', 'rev-parse', '--show-toplevel'],
        text=True, timeout=15, env=check.gate_source.git_environment(),
    ).removesuffix('\n')
    if output != str(path):
        raise ValueError('repository root must be the Git top level')
    common = subprocess.check_output(
        ['git', '-C', str(path), '-c', f'safe.directory={path}', 'rev-parse', '--path-format=absolute', '--git-common-dir'],
        text=True, timeout=15, env=check.gate_source.git_environment(),
    ).removesuffix('\n')
    metadata = Path(common)
    if not metadata.is_absolute() or metadata != metadata.resolve(strict=True) or not metadata.is_dir():
        raise ValueError('Git common directory must be canonical')
    return path, metadata


def product(path):
    path = Path(path)
    if not path.is_absolute() or path != path.resolve(strict=True):
        raise ValueError('product must be an absolute canonical file')
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW), 'rb') as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or not before.st_mode & 0o111:
            raise ValueError('product must be an executable regular file')
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        after = os.fstat(stream.fileno())
    def fingerprint(info):
        return (info.st_dev, info.st_ino, info.st_mode, info.st_size, info.st_mtime_ns, info.st_ctime_ns)
    if fingerprint(before) != fingerprint(after) or fingerprint(path.lstat()) != fingerprint(after):
        raise ValueError('product changed during inspection')
    return digest, stat.S_IMODE(after.st_mode)


def snapshot(owner, root):
    return subprocess.check_output(
        [sys.executable, '-B', str(owner / 'dev/gate_source.py')], cwd=root,
        text=True, timeout=30, env={**os.environ, 'GIT_OPTIONAL_LOCKS': '0'},
    ).strip()


def mount(path, readonly):
    value = str(path)
    if ',' in value or '\n' in value:
        raise ValueError('unsupported Docker mount path')
    return f'type=bind,src={value},dst={value}' + (',readonly' if readonly else '')


def selected_ids(case):
    try:
        return CASE_IDS[case]
    except KeyError as error:
        raise ValueError(f'unknown Laputa case: {case}') from error


def command(image, laputa, metadata, xsh, xsht, scratch, platform="linux/amd64", case=CASE, xshi=None):
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', image):
        raise ValueError('image must be an explicit immutable Docker image ID')
    if case == PM_REUSE_CASE and xshi is None:
        raise ValueError('PM reuse requires the XSHI product')
    if case != PM_REUSE_CASE and xshi is not None:
        raise ValueError('XSHI is only required for the PM reuse case')
    if case == PM_REUSE_CASE:
        products = (Path(xsh), Path(xsht), Path(xshi))
        if tuple(path.name for path in products) != ('xsh', 'xsht', 'xshi') or len({path.parent for path in products}) != 1:
            raise ValueError('PM reuse requires sibling xsh, xsht and xshi executables')
    paths = [laputa, metadata, xsh, xsht] + ([xshi] if xshi is not None else [])
    for path in paths:
        if scratch == path or scratch in path.parents or path in scratch.parents:
            raise ValueError('scratch overlaps a read-only input')
    argv = ['docker', 'run', '--rm', '--init', '--platform', platform, '--network', 'none', '--read-only',
            '--cpus=2', '--memory=4g', '--memory-swap=4g', '--user', f'{os.getuid()}:{os.getgid()}']
    for path in dict.fromkeys(paths):
        argv += ['--mount', mount(path, True)]
    argv += ['--mount', mount(scratch, False), '-w', str(laputa),
             '-e', f'HOME={scratch}', '-e', f'TMPDIR={scratch}', '-e', f'PATH={xsh.parent}:/usr/bin:/bin',
             '--entrypoint', '/bin/sh', image, '-c',
             'status_file=$1; shift; if "$@"; then status=0; else status=$?; fi; printf "%s\\n" "$status" > "$status_file"; exit "$status"',
             'laputa-plan', str(scratch / 'xsht-status'), str(xsht), 'test', '--jobs', '1',
             '--events-jsonl', str(scratch / 'events.jsonl')]
    for test_id in selected_ids(case):
        argv += ['--exact-id', test_id]
    return argv


def workload_status(work, docker_status):
    try:
        descriptor = os.open(work / 'xsht-status', os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(descriptor, 'rb') as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                return None
            data = stream.read(16)
        if data not in (b'0\n', b'1\n', b'2\n', b'130\n'):
            return None
        status = int(data)
        return status if status == docker_status else None
    except OSError:
        return None


def event_bytes(work):
    descriptor = os.open(work / 'events.jsonl', os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, 'rb') as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError('XSHT events must be a regular file')
        data = stream.read(16 * 1024 * 1024 + 1)
    if len(data) > 16 * 1024 * 1024:
        raise ValueError('XSHT event stream exceeds limit')
    return data


def interrupted_evidence(work, selected=IDS):
    try:
        data = event_bytes(work)
        if len(data) > 16 * 1024 * 1024 or b'\n' not in data:
            return None
        complete = data[:data.rfind(b'\n') + 1].decode('utf-8')
        rows = results._records(complete)
        if not rows:
            return None
        first = rows[0]
        results._fields(first, ('selected',))
        if (type(first.get('schema')) is not int or first['schema'] != 1 or first.get('event') != 'run_started'
                or not isinstance(first.get('run_id'), str) or not first['run_id']
                or results._ids(first.get('selected')) != results._ids(selected)):
            return None
        states = dict.fromkeys(selected, 'pending')
        observed = None
        for row in rows[1:]:
            if type(row.get('schema')) is not int or row['schema'] != 1 or row.get('run_id') != first['run_id']:
                return None
            event = row.get('event')
            if event == 'run_finished':
                if row is not rows[-1] or row.get('status') not in (0, 1, 2, 130):
                    return None
                reconciled = results.reconcile_xsht(complete, selected, child_status=row['status'])
                return 'cancelled' if reconciled['status'] == 130 else 'failed' if reconciled['status'] == 1 else None
            name = row.get('case_id')
            if name not in states:
                return None
            if event == 'case_started':
                results._fields(row, ('case_id',))
                if states[name] != 'pending':
                    return None
                states[name] = 'running'
            elif event == 'case_finished':
                results._fields(row, ('case_id', 'outcome', 'reason', 'duration_ns'))
                outcome = row.get('outcome')
                if (outcome not in results.OUTCOMES or not results._integer(row.get('duration_ns'))
                        or (outcome == 'passed' and row.get('reason') is not None)
                        or (outcome != 'passed' and not results._reason(row.get('reason')))
                        or not ((states[name] == 'running' and outcome != 'not_executed')
                                or (states[name] == 'pending' and outcome == 'not_executed'))):
                    return None
                states[name] = 'finished'
                if outcome == 'cancelled':
                    observed = 'cancelled'
                elif outcome == 'failed' and observed != 'cancelled':
                    observed = 'failed'
            else:
                return None
        return observed
    except (OSError, UnicodeError, ValueError, TypeError):
        return None


def verify_image(image):
    result = subprocess.run(['docker', 'image', 'inspect', '--format={{.Id}}', image],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=15, check=False)
    if result.returncode != 0 or result.stdout.strip() != image:
        raise ValueError('Docker image inspection did not resolve the requested image ID')
    volumes = subprocess.run(['docker', 'image', 'inspect', '--format={{json .Config.Volumes}}', image],
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=15, check=False)
    if volumes.returncode != 0 or json.loads(volumes.stdout) not in (None, {}):
        raise ValueError('Docker image declares writable volumes')


def run(owner, laputa, xsh, xsht, scratch, image, lease_owner, expected=None, platform="linux/amd64", case=CASE, xshi=None):
    owner, _ = repository(str(owner))
    laputa, metadata = repository(str(laputa))
    xsh, xsht = Path(xsh), Path(xsht)
    scratch = Path(scratch)
    if scratch != scratch.resolve(strict=True) or not scratch.is_dir() or not scratch.is_relative_to(owner / '.work/tmp'):
        raise ValueError('scratch must be a canonical directory under XSH .work/tmp')
    with lease_owner.campaign_leases(owner, native=True):
        work = Path(tempfile.mkdtemp(prefix='laputa-plan-', dir=scratch))
        cleanup_verified = False
        launch_started = False
        verdict = None
        try:
            selected = selected_ids(case)
            argv = command(image, laputa, metadata, xsh, xsht, work, platform, case, xshi)
            before = (snapshot(owner, owner), snapshot(owner, laputa), product(xsh), product(xsht))
            if xshi is not None:
                before += (product(xshi),)
            if expected is not None and before[:2] != expected:
                raise ValueError('source changed since selection')
            verify_image(image)
            lease_owner.prepare_cleanup_unresolved(f'{case}: container launch in progress')
            launch_started = True
            try:
                status, stdout, stderr = check.checked_docker(
                    argv, work, timeout_seconds=300, on_cleanup_verified=lease_owner.resolve_cleanup_unresolved,
                )
            except check.DockerCleanupError as error:
                error.workload_status = workload_status(work, error.child_status)
                error.event_evidence = interrupted_evidence(work, selected)
                raise
            cleanup_verified = True
            observed = workload_status(work, status)
            if observed is None:
                evidence = interrupted_evidence(work, selected)
                print(f'{case}: XSHT completion was not observed; Docker status={status}; evidence={evidence}', file=sys.stderr)
                verdict = 130 if evidence == 'cancelled' else 1 if evidence == 'failed' else 2
                return verdict, status, stdout, stderr
            try:
                after = (snapshot(owner, owner), snapshot(owner, laputa), product(xsh), product(xsht))
                if xshi is not None:
                    after += (product(xshi),)
            except (OSError, ValueError, subprocess.SubprocessError):
                if status == 130:
                    verdict = 130
                    return verdict, status, stdout, stderr
                if status != 0:
                    verdict = 2 if status == 2 else 1
                    return verdict, status, stdout, stderr
                raise
            try:
                event_text = event_bytes(work).decode('utf-8')
                result = results.reconcile_xsht(event_text, selected, child_status=status)
            except (OSError, UnicodeError, ValueError):
                result = {'status': 1 if status not in (0, 2, 130) else 2}
            if 'cases' in result:
                print(results.render_report(result))
            if status == 130:
                verdict = 130
            elif status != 0:
                verdict = 2 if status == 2 else 1
            elif before != after:
                verdict = 2
            else:
                verdict = result['status']
            return verdict, status, stdout, stderr
        finally:
            if cleanup_verified or not launch_started:
                try:
                    shutil.rmtree(work)
                except OSError as error:
                    if verdict not in (1, 130):
                        raise
                    print(f'{case}: scratch cleanup failed: {error}', file=sys.stderr)


def load_leases(owner):
    path = owner / 'dev/util-linux/leases.py'
    spec = importlib.util.spec_from_file_location('laputa_native_leases', path)
    if spec is None or spec.loader is None:
        raise ValueError('native lease owner is unavailable')
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument('--case', choices=tuple(CASE_IDS), default=CASE)
    parser.add_argument('--observe', type=Path, required=True)
    parser.add_argument('--owner', type=Path, required=True)
    parser.add_argument('--xsh', type=Path)
    parser.add_argument('--xsht', type=Path)
    parser.add_argument('--xshi', type=Path)
    parser.add_argument('--scratch', type=Path)
    parser.add_argument('--image')
    parser.add_argument('--platform', default='linux/amd64')
    parser.add_argument('--xsh-source')
    parser.add_argument('--laputa-source')
    args = parser.parse_args(argv)
    try:
        owner, _ = repository(str(args.owner))
        root, _ = repository(str(args.observe))
        if args.xsh is None:
            print(snapshot(owner, root))
            return 0
        if any(value is None for value in (args.xsht, args.scratch, args.image, args.xsh_source, args.laputa_source)):
            raise ValueError('execution requires products, image, scratch and selection observations')
        if args.case == PM_REUSE_CASE and args.xshi is None:
            raise ValueError('PM reuse execution requires XSHI')
        if args.case != PM_REUSE_CASE and args.xshi is not None:
            raise ValueError('XSHI is only accepted for the PM reuse case')
        verdict, status, stdout, stderr = run(owner, root, args.xsh, args.xsht, args.scratch, args.image,
            load_leases(owner), expected=(args.xsh_source, args.laputa_source), platform=args.platform,
            case=args.case, xshi=args.xshi)
        sys.stdout.buffer.write(stdout)
        sys.stderr.buffer.write(stderr)
        print(f'{args.case}: raw_status={status}, verdict={verdict}')
        return verdict
    except check.owned.SignalReceived as error:
        print(f'{args.case}: cancelled: {error}', file=sys.stderr)
        for note in getattr(error, '__notes__', ()):
            print(f'{args.case}: {note}', file=sys.stderr)
        return 130
    except check.DockerCleanupError as error:
        status = error.child_status
        print(f'{args.case}: cleanup unresolved; raw_status={status}: {error}', file=sys.stderr)
        if getattr(error, 'workload_status', None) == 130 or getattr(error, 'event_evidence', None) == 'cancelled':
            return 130
        if getattr(error, 'workload_status', None) == 1 or getattr(error, 'event_evidence', None) == 'failed':
            return 1
        return 2
    except Exception as error:
        print(f'{args.case}: incomplete: {error}', file=sys.stderr)
        for note in getattr(error, '__notes__', ()):
            print(f'{args.case}: {note}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
