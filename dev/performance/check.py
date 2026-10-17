#!/usr/bin/env python3
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
ADAPTER_ROOT = Path(__file__).resolve().parents[2]
TOOLS = ADAPTER_ROOT / 'dev/consolidation'
PERFORMANCE = TOOLS / 'winddown/gate-tools/final-performance-gates'
sys.path.insert(0, str(PERFORMANCE))
import owned_containers as owned


class CheckError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise CheckError(message)


def fields(value, expected, label):
    require(isinstance(value, dict) and set(value) == set(expected), f'{label}: expected fields {sorted(expected)}')


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f'duplicate JSON field: {key}')
        result[key] = value
    return result


def read_json(path):
    return json.loads(path.read_text(), object_pairs_hook=unique_object)


def canonical(value):
    require(isinstance(value, str) and value != '', 'path must be explicit')
    path = Path(value)
    require(path.is_absolute() and str(path) == os.path.normpath(value) and '..' not in path.parts, f'path must be absolute and contain no symlinks: {path}')
    return path


@dataclass(frozen=True)
class Config:
    source_root: Path
    commit: str
    tree: str
    laputa_root: Path
    product_receipt: Path
    xsh: Path
    xsht: Path
    scratch: Path
    functional: dict | None


def parse_config(value):
    fields(value, ('schema', 'source_root', 'commit', 'tree', 'laputa_root', 'product_receipt', 'xsh', 'xsht', 'scratch', 'functional'), 'config')
    require(type(value['schema']) is int and value['schema'] == 1, 'unsupported config schema')
    for name in ('commit', 'tree'):
        require(isinstance(value[name], str) and re.fullmatch('[0-9a-f]{40}', value[name]), f'{name} must be a full lowercase Git ID')
    paths = {name: canonical(value[name]) for name in ('source_root', 'laputa_root', 'product_receipt', 'xsh', 'xsht', 'scratch')}
    require(paths['scratch'].is_relative_to(ADAPTER_ROOT / '.work/tmp'), 'scratch must be under the adapter checkout .work/tmp')
    functional = value['functional']
    if functional is not None:
        fields(functional, ('canonical', 'supplemental', 'skip_review', 'structural', 'review_evidence', 'output'), 'functional')
        functional = {name: canonical(path) for name, path in functional.items()}
        require(functional['output'].is_relative_to(paths['source_root'] / '.work'), 'functional output must be under source_root/.work')
    return Config(**paths, commit=value['commit'], tree=value['tree'], functional=functional)


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True, timeout=15).strip()


def source_identity(config):
    for path in (config.source_root, config.laputa_root, config.product_receipt, config.xsh, config.xsht, config.scratch, *(config.functional or {}).values()):
        require(path == path.resolve(), f'path contains a symlink: {path}')
    require(git(config.source_root, 'rev-parse', '--show-toplevel') == str(config.source_root), 'source_root must be a Git repository root')
    require(git(config.source_root, 'rev-parse', 'HEAD') == config.commit, 'source commit mismatch')
    require(git(config.source_root, 'rev-parse', 'HEAD^{tree}') == config.tree, 'source tree mismatch')
    require(git(config.source_root, 'status', '--porcelain=v1', '--untracked-files=all') == '', 'source must be clean')
    require(config.laputa_root.is_dir() and (config.laputa_root / 'pm').is_dir(), 'explicit Laputa checkout is unavailable')
    return {'commit': config.commit, 'git_tree': config.tree, 'status': 'clean'}


def check_product_policy(receipt, source, binaries):
    require(receipt.get('schema') == 1 and receipt.get('state') in ('complete', 'artifact-qualified'), 'expected completed profile-cost product qualification')
    require(receipt.get('source_commit') == source['commit'], 'product/source commit mismatch')
    results = [row for row in receipt.get('results', []) if row.get('opt_level') == 3]
    require(len(results) == 1, 'product receipt must have one opt3 result')
    build = results[0].get('build', {})
    require(type(build.get('status')) is int and build['status'] == 0, 'product build did not succeed')
    require(build.get('container_cleanup', {}).get('resolved') is True, 'product build cleanup unresolved')
    validation = build.get('stage_validation', {})
    require(validation.get('source') == source, 'product/source identity mismatch')
    for name, record in binaries.items():
        require(validation.get('binary_hashes', {}).get(name) == record, f'product qualification mismatch: {name}')


def qualify_products(config):
    identity = source_identity(config)
    binaries = {}
    for name in ('xsh', 'xsht'):
        path = getattr(config, name)
        require(path.is_file() and os.access(path, os.X_OK), f'missing executable: {path}')
        binaries[name] = {'path': str(path), 'sha256': owned_digest(path)}
    receipt = read_json(config.product_receipt)
    if receipt.get('kind') == 'canonical_gate_batch_replay':
        validate_canonical(config, receipt, config.product_receipt)
        check_canonical_products(receipt, config, binaries)
    else:
        check_product_policy(receipt, identity, binaries)
    return binaries


def owned_digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def check_result(status, stdout, stderr, expected):
    require(status == 0, f'command failed with status {status}: {stderr!r}')
    require(stdout == expected and stderr == b'', f'wrong output: stdout={stdout!r} stderr={stderr!r}')


def readiness_window(ready, operation):
    ready(False)
    try:
        result = operation()
    except BaseException as primary:
        try:
            ready(True)
        except BaseException as after:
            primary.add_note(f"post-measurement readiness failed: {after}")
        raise
    ready(True)
    return result


def check_cleanup(cleanup):
    require(cleanup.get('resolved') is True, f'owned container cleanup unresolved: {cleanup}')


def checked_docker(command, scratch):
    directory = Path(tempfile.mkdtemp(prefix='adapter-', dir=scratch))
    cidfile = directory / 'container.cid'
    owner = directory.name
    argv = owned.owned_docker_command(command, cidfile=cidfile, owner=owner, stage='adapter')
    process = None
    primary = None
    result = None
    previous = owned.install_signal_handlers()
    try:
        process = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        stdout, stderr = process.communicate(timeout=60)
        result = process.returncode, stdout, stderr
    except BaseException as error:
        primary = error
    try:
        with owned.cleanup_window():
            cleanup = owned.cleanup_cidfile_container(cidfile, owner=owner, stage='adapter') if process is None else owned.cleanup_container_process(process, lambda: owned.cleanup_cidfile_container(cidfile, owner=owner, stage='adapter'))
    finally:
        owned.restore_signal_handlers(previous)
    if cleanup.get('resolved') is True:
        shutil.rmtree(directory)
    if primary is not None:
        if cleanup.get('resolved') is not True:
            primary.add_note(f'cleanup unresolved; owned cidfile retained at {cidfile}: {cleanup}')
        raise primary
    if cleanup.get('resolved') is not True and result[0] != 0:
        raise CheckError(f'command exited {result[0]}: stdout={result[1]!r} stderr={result[2]!r}; cleanup unresolved; cidfile retained at {cidfile}: {cleanup}')
    check_cleanup(cleanup)
    return result


def self_test(config, adapter):
    suite = unittest.defaultTestLoader.discover(str(Path(__file__).parent), pattern='test_check.py')
    require(unittest.TextTestRunner(verbosity=1).run(suite).wasSuccessful(), 'adapter policy self-test failed')
    if adapter:
        qualify_products(config)
        require(config.scratch.is_dir(), 'scratch directory must already exist')
        image = load_canonical().IMAGE
        command = ['docker', 'run', '--rm', '--init', '--platform', 'linux/amd64', '--network', 'none', '--cpus=1', '--memory=512m', '--memory-swap=512m', '-v', f'{config.source_root}:/source:ro', '-v', f'{config.xsh}:/product/xsh:ro', '-w', '/source', image, '/product/xsh', 'bench/stdlib-port/cold_trivial.xsh']
        result = checked_docker(command, config.scratch)
        check_result(*result, b'hello\n')
        try:
            check_result(*result, b'intentionally wrong output\n')
        except CheckError:
            pass
        else:
            raise CheckError('wrong-output control was accepted')
        with tempfile.TemporaryDirectory(prefix='cleanup-control-', dir=config.scratch) as directory:
            cidfile = Path(directory) / 'container.cid'
            cidfile.write_text('invalid-container-id\n')
            cleanup = owned.cleanup_cidfile_container(cidfile, owner=Path(directory).name, stage='negative-control')
            try:
                check_cleanup(cleanup)
            except CheckError:
                pass
            else:
                raise CheckError('real invalid cidfile cleanup control was accepted')
        qualify_products(config)


def load_canonical():
    spec = importlib.util.spec_from_file_location('performance_canonical', TOOLS / 'final_canonical_replay.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check_source_records(records, config, label):
    require(isinstance(records, list) and len(records) >= 2, f'{label}: missing before/after source checks')
    for row in records:
        require(row.get('head') == config.commit and row.get('tree') == config.tree and row.get('clean') is True and row.get('status') == '', f'{label}: source changed or dirty')


def check_canonical_policy(receipt, config):
    require(receipt.get('schema_version') == 1 and receipt.get('kind') == 'canonical_gate_batch_replay', 'missing actual canonical receipt')
    require(receipt.get('source_expected') == {'commit': config.commit, 'tree': config.tree}, 'canonical source mismatch')
    require(receipt.get('status') == 'passed' and type(receipt.get('exit_code')) is int and receipt['exit_code'] == 0, 'canonical failed or incomplete')
    require(receipt.get('timed_out') is False and receipt.get('received_signal') is None and receipt.get('failure') is None, 'canonical interrupted or failed')
    check_cleanup(receipt.get('cleanup', {}))
    check_source_records(receipt.get('source_checks'), config, 'canonical')


def bound_file(record, base):
    require(isinstance(record, dict), 'missing evidence file binding')
    path = canonical(str(base / record['path'])) if not Path(record['path']).is_absolute() else canonical(record['path'])
    require(path == path.resolve(), f'evidence path contains a symlink: {path}')
    require(path.is_file() and owned_digest(path) == record['sha256'], f'evidence file hash mismatch: {path}')
    return path


def check_supplemental_policy(receipt, plan, config):
    require(receipt.get('schema') == 1 and receipt.get('status') == 'commands-passed-needs-privilege-skip-and-close-review', 'supplemental commands incomplete')
    require(receipt.get('source_expected') == {'commit': config.commit, 'tree': config.tree}, 'supplemental source mismatch')
    require(receipt.get('environment') == {key: plan['environment'][key] for key in ('target', 'image')}, 'supplemental target or image mismatch')
    check_source_records(receipt.get('source_checks'), config, 'supplemental')
    steps = receipt.get('steps')
    require(isinstance(steps, list) and steps, 'supplemental steps missing')
    ids = [step['id'] for step in steps]
    require(len(ids) == len(set(ids)), 'supplemental duplicate step')
    expected = {
        'verify-pinned-image-id': ['docker', 'image', 'inspect', '--format', '{{.Id}}', plan['environment']['image']],
        'tracked-diff-check': ['git', 'diff', '--check'],
    }
    for index, argv in enumerate(plan['products'], 1):
        packages = [argv[position + 1] for position, item in enumerate(argv[:-1]) if item == '-p']
        name = '-'.join(packages) if packages else 'bin'
        expected[f'build-product-{index:02d}-{name}'] = ['build', *argv]
    for harness in plan['rust_harness_builds']:
        argv = ['cargo', 'test', '--locked', '--target', plan['environment']['target']]
        if harness['profile'] == 'verification':
            argv += ['--profile', 'verification']
        expected[f"build-harness-{harness['id']}"] = ['build', *argv, *harness['args'], '--no-run', '--message-format=json']
    for index, argv in enumerate(plan['warning_deny'], 1):
        expected[f'warning-deny-{index:02d}'] = ['lint', *argv]
    context_requirements = {}
    for index, context in enumerate(plan['linux_context_runs'], 1):
        name = context.get('harness') or context.get('id', f"binary-{context['binary']}")
        if context.get('harness'):
            targets = receipt.get('harnesses', {}).get(context['harness'], {}).get('targets', {})
            require(len(targets) == 1, f'missing supplemental harness: {name}')
            executable = next(iter(targets.values()))['path']
            argv = [executable]
            if context.get('filter'):
                argv.append(context['filter'])
            argv += ['--nocapture', '--test-threads=1']
            if context.get('skip_filter'):
                argv += ['--skip', context['skip_filter']]
        else:
            argv = [str(Path(receipt['cargo_target_dirs']['products']) / plan['environment']['target'] / 'verification' / context['binary']), *context['argv']]
        step_id = f'linux-special-{index:02d}-{name}'
        expected[step_id] = argv
        context_requirements[step_id] = context
    require(all(name in ids for name in expected), 'missing required supplemental step or context')
    for name, argv in expected.items():
        step = steps[ids.index(name)]
        observed = step.get('argv', [])
        require(observed[-len(argv):] == argv, f'supplemental command mismatch: {name}')
        if name in context_requirements:
            context = context_requirements[name]
            require(observed[:2] == ['docker', 'run'] and plan['environment']['image'] in observed, f'supplemental context must use pinned Docker: {name}')
            for flag in (f"--network={context.get('network', 'none')}", f"--memory={context['memory']}", f"--memory-swap={context['memory']}", f"--cpus={context.get('cpus', '12')}"):
                require(flag in observed, f'supplemental context limit mismatch: {name}: {flag}')
            require(('--privileged' in observed) == context['privileged'], f'supplemental privilege context mismatch: {name}')
            user = observed.index('--user') if '--user' in observed else -1
            require(user >= 0 and observed[user + 1] == context.get('user', '0:0'), f'supplemental user context mismatch: {name}')
        if name.startswith(('build-', 'warning-deny-')):
            require(len(observed) == len(argv) + 2 and observed[1] == str(config.source_root) and observed[0].endswith('/run-native-final.sh'), f'supplemental runner mismatch: {name}')
    for step in steps:
        require(type(step.get('exit_code')) is int and step['exit_code'] == 0 and step.get('timed_out') is False, f"supplemental step failed: {step.get('id')}")
        if 'cleanup' in step:
            check_cleanup(step['cleanup'])
    hashes = receipt.get('hashes', {})
    products = hashes.get('products_after_initial_build')
    require(isinstance(products, dict) and set(products) == {'xsh-verification', 'xsht-verification', 'xshi-verification', 'xsh-fuzz-verification', 'xsh-test-helper-verification'}, 'supplemental product qualification incomplete')
    for name in ('binaries_after_build', 'binaries_before_tests', 'binaries_after_tests'):
        require(hashes.get(name) == products, f'supplemental products changed at {name}')
    for before, after in (('harnesses_before_tests', 'harnesses_after_tests'), ('harness_variant_binaries_before_tests', 'harness_variant_binaries_after_tests')):
        require(hashes.get(before) and hashes.get(before) == hashes.get(after), 'supplemental harness products missing or changed')
    return steps


def check_canonical_products(receipt, config, binaries):
    check_canonical_policy(receipt, config)
    for name, actual in binaries.items():
        record = receipt.get('products_after', {}).get(f'{name}-verification', {})
        require(record.get('path') and str(config.source_root / record['path']) == actual['path'] and record.get('sha256') == actual['sha256'], f'canonical product qualification mismatch: {name}')


def check_skip_review_policy(review, config, canonical_hash, supplemental_hash, contexts):
    fields(review, ('source_commit', 'source_tree', 'canonical_sha256', 'supplemental_sha256', 'reviewer', 'contexts'), 'skip review')
    require(review['source_commit'] == config.commit and review['source_tree'] == config.tree, 'skip review source mismatch')
    require(review['canonical_sha256'] == canonical_hash and review['supplemental_sha256'] == supplemental_hash, 'skip review receipt mismatch')
    require(isinstance(review['reviewer'], str) and review['reviewer'].strip(), 'skip review reviewer missing')
    require(isinstance(review['contexts'], dict) and set(review['contexts']) == set(contexts), 'missing skip review context')
    for context in review['contexts'].values():
        fields(context, ('result', 'reason'), 'skip review context')
        require(context['result'] == 'review_verified' and isinstance(context['reason'], str) and context['reason'].strip(), 'unreviewed mandatory skip or context')


def validate_canonical(config, receipt, receipt_path):
    canonical = load_canonical()
    check_canonical_policy(receipt, config)
    require(receipt.get('image') == {'id': canonical.IMAGE, 'platform': 'linux/amd64'}, 'canonical image mismatch')
    require(receipt.get('command') == canonical.batch_command(), 'canonical command mismatch')
    for record in receipt['logs'].values():
        bound_file(record, receipt_path.parent)
    resources = receipt['container_resources']
    resource_path = bound_file(resources, receipt_path.parent)
    require(read_json(resource_path) == resources['record'], 'canonical resource record differs from file')
    canonical.validate_resource_record(resources['record'], 0, canonical.batch_command())
    require(set(receipt['products_after']) == {'xsh-verification', 'xsht-verification', 'xshi-verification'}, 'canonical product set incomplete')
    for record in receipt['products_after'].values():
        bound_file(record, config.source_root)
    bootstrap = receipt['bootstrap']['build_receipt']
    bootstrap_path = bound_file(bootstrap, config.source_root)
    checked_bootstrap = canonical.validate_bootstrap_receipt(bootstrap_path, config.source_root, config.source_root / '.work/tmp', {'head': config.commit, 'tree': config.tree}, Path(bootstrap['artifact']['path']))
    require(checked_bootstrap == bootstrap, 'canonical bootstrap binding differs from validation')


def functional(config):
    require(config.functional is not None, 'functional inputs are not configured')
    paths = config.functional
    require(config.product_receipt == paths['canonical'], 'functional products require the actual canonical receipt')
    binaries = qualify_products(config)
    require(not paths['output'].exists(), 'functional close output already exists')
    canonical = load_canonical()
    spec = importlib.util.spec_from_file_location('performance_plan', TOOLS / 'winddown/gate-tools/final-correctness-gates/plan_source.py')
    plan = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plan)
    supplemental = read_json(paths['supplemental'])
    steps = check_supplemental_policy(supplemental, plan.plan_data(), config)
    for name, digest in supplemental['hashes']['binaries_after_tests'].items():
        binary = name.removesuffix('-verification')
        bound_file({'path': str(Path(supplemental['cargo_target_dirs']['products']) / canonical.TARGET / 'verification' / binary), 'sha256': digest}, config.source_root)
    for harness in supplemental['harnesses'].values():
        for record in harness['targets'].values():
            bound_file(record, config.source_root)
    for step in steps:
        for stream in ('log', 'stdout', 'stderr'):
            key = 'log' if stream == 'log' else f'{stream}_log'
            bound_file({'path': step[key], 'sha256': step[f'{stream}_sha256']}, paths['supplemental'].parent)
    review = read_json(paths['skip_review'])
    expected_contexts = ['canonical', *[step['id'] for step in steps if step['id'].startswith('linux-special-')]]
    check_skip_review_policy(review, config, owned_digest(paths['canonical']), owned_digest(paths['supplemental']), expected_contexts)
    with tempfile.TemporaryDirectory(prefix='functional-', dir=config.scratch) as directory:
        command = [str(config.xsh), str(config.source_root / 'dev/consolidation/metrics.xsh'), 'check-current', '--root', str(config.source_root), '--source-root', 'src', '--xsht-source-root', 'crates/xsht/src', '--baseline', str(paths['structural']), '--review-evidence', str(paths['review_evidence']), '--output', str(Path(directory) / 'current.json'), '--audit', str(Path(directory) / 'audit.json')]
        docker = ['docker', 'run', '--rm', '--init', '--platform', 'linux/amd64', '--network', 'none', '--cpus=2', '--memory=4g', '--memory-swap=4g', '-v', f'{config.source_root}:{config.source_root}:ro', '-v', f'{config.scratch}:{config.scratch}:rw', '-v', f'{config.xsh}:{config.xsh}:ro', '-w', str(config.source_root)]
        for path in (paths['structural'], paths['review_evidence']):
            docker += ['-v', f'{path}:{path}:ro']
        common = git(config.source_root, 'rev-parse', '--path-format=absolute', '--git-common-dir')
        docker += ['-v', f'{common}:{common}:ro', canonical.IMAGE, *command]
        status, stdout, stderr = checked_docker(docker, config.scratch)
        require(status == 0, f'metrics owner rejected current source: {stdout!r} {stderr!r}')
        require(read_json(Path(directory) / 'current.json') == read_json(paths['structural']), 'structural report does not match current source')
    qualify_products(config)
    evidence = {name: {'path': str(path), 'sha256': owned_digest(path)} for name, path in paths.items() if name != 'output'}
    close = {'functional_complete': True, 'commit': config.commit, 'tree': config.tree, 'reviewer': review['reviewer'], 'scope': {'qualification': 'fresh canonical, supplemental contexts, current structural and semantic review'}, 'evidence': evidence, 'products_used': binaries}
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', prefix='.functional-', dir=paths['output'].parent, delete=False) as output:
            temporary = Path(output.name)
            output.write(json.dumps(close, indent=2, sort_keys=True) + '\n')
            output.flush()
            os.fsync(output.fileno())
        os.link(temporary, paths['output'])
    finally:
        if temporary is not None:
            temporary.unlink()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', required=True, type=Path)
    commands = parser.add_subparsers(dest='mode', required=True)
    test = commands.add_parser('self-test')
    test.add_argument('--adapter', action='store_true')
    commands.add_parser('functional').add_argument('--check', required=True, action='store_true')
    for name in ('baseline', 'inference', 'measure', 'optimize', 'deployment', 'close'):
        commands.add_parser(name).add_argument('--check', action='store_true')
    args = parser.parse_args()
    try:
        config = parse_config(read_json(args.config))
        if args.mode == 'self-test':
            self_test(config, args.adapter)
        elif args.mode == 'functional':
            functional(config)
        else:
            raise CheckError(f'{args.mode}: unavailable until existing owner is integrated')
        return 0
    except owned.SignalReceived as error:
        print(f'performance check cancelled: {error}', file=sys.stderr)
        for note in getattr(error, '__notes__', []):
            print(note, file=sys.stderr)
        return 128 + error.signum
    except (CheckError, OSError, KeyError, TypeError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        print(f'performance check: {error}', file=sys.stderr)
        for note in getattr(error, '__notes__', []):
            print(note, file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
