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
    baseline: dict | None = None


def parse_config(value):
    required = ('schema', 'source_root', 'commit', 'tree', 'laputa_root', 'product_receipt', 'xsh', 'xsht', 'scratch', 'functional')
    fields(value, (*required, 'baseline') if 'baseline' in value else required, 'config')
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
    baseline = value.get('baseline')
    if baseline is not None:
        fields(baseline, ('functional_close', 'profile', 'observer', 'forced', 'comparative', 'scheduler'), 'baseline')
        baseline = {name: canonical(path) for name, path in baseline.items()}
    return Config(**paths, commit=value['commit'], tree=value['tree'], functional=functional, baseline=baseline)


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True, timeout=15).strip()


def source_identity(config):
    for path in (config.source_root, config.laputa_root, config.product_receipt, config.xsh, config.xsht, config.scratch, *(config.functional or {}).values(), *(config.baseline or {}).values()):
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
        image = load_canonical(config).IMAGE
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


def load_canonical(config):
    return baseline_owner(config, 'dev/consolidation/final_canonical_replay.py', 'canonical')


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
    canonical = load_canonical(config)
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


def functional_inputs(config):
    require(config.functional is not None, 'functional inputs are not configured')
    paths = config.functional
    require(config.product_receipt == paths['canonical'], 'functional products require the actual canonical receipt')
    binaries = qualify_products(config)
    canonical = load_canonical(config)
    plan = baseline_owner(config, 'dev/consolidation/winddown/gate-tools/final-correctness-gates/plan_source.py', 'plan')
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
    return binaries, review


def functional(config):
    binaries, review = functional_inputs(config)
    paths = config.functional
    require(not paths['output'].exists(), 'functional close output already exists')
    canonical = load_canonical(config)
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


def baseline_owner(config, relative, name):
    path = config.source_root / relative
    require(path.is_file() and path == path.resolve(), f'baseline owner unavailable: {path}')
    key = f'performance_{name}_{hashlib.sha256(str(config.source_root).encode()).hexdigest()}'
    if key not in sys.modules:
        sys.path.insert(0, str(path.parent))
        spec = importlib.util.spec_from_file_location(key, path)
        module = importlib.util.module_from_spec(spec)
        sys.modules[key] = module
        spec.loader.exec_module(module)
    return sys.modules[key]


def baseline_util_linux(config):
    runtime = baseline_owner(config, 'dev/util-linux/check.py', 'util_linux')
    for name in ('acceptance', 'resources', 'release', 'leases'):
        module = __import__(name)
        require(Path(module.__file__).resolve() == config.source_root / f'dev/util-linux/{name}.py', f'util-linux {name} belongs to another source checkout')
    return runtime


def receipt_binding(path):
    return {'path': str(path), 'sha256': owned_digest(path)}


def validate_laputa_corpus(binding, config):
    fields(binding, ('path', 'source_identity', 'source_identity_after'), 'Laputa corpus binding')
    root = config.laputa_root
    require(binding['path'] == str(root) and root == root.resolve(), 'Laputa corpus path differs from explicit configuration')
    require(git(root, 'rev-parse', '--show-toplevel') == str(root), 'Laputa corpus must be a Git repository root')
    require(git(root, 'status', '--porcelain=v1', '--untracked-files=all') == '', 'Laputa corpus must be clean')
    identity = {'commit': git(root, 'rev-parse', 'HEAD'), 'git_tree': git(root, 'rev-parse', 'HEAD^{tree}'), 'status': 'clean'}
    require(binding['source_identity'] == identity and binding['source_identity_after'] == identity, 'Laputa corpus identity changed or differs from its owner binding')
    return identity


def completed_receipt(path, schema_key, source):
    value = read_json(path)
    require(type(value.get(schema_key)) is int and value[schema_key] == 1 and value.get('state') == 'complete', f'incomplete baseline receipt: {path}')
    require(all(value.get('source', {}).get(key) == expected for key, expected in source.items()), f'baseline source mismatch: {path}')
    return value


def accepted_timing(value, label, accepted='timing_accepted', discarded='timing_discarded', gate='performance_gate'):
    require(value.get(accepted) is True and value.get(discarded) is False, f'{label}: timing was not accepted')
    for boundary in ('before', 'after'):
        readiness = value if gate is None else value.get(gate, {})
        status = (readiness.get(boundary) or {}).get('exit_code')
        require(type(status) is int and status == 0, f'{label}: {boundary} readiness failed')


def validate_scheduler(config, path, profile, compare):
    value = read_json(path)
    require(type(value.get('schema_version')) is int and value['schema_version'] == 1 and value.get('state') == 'complete' and value.get('complete') is True, 'scheduler receipt incomplete')
    require(value.get('source_root') == str(config.source_root) and value.get('source_commit_expected') == config.commit, 'scheduler source mismatch')
    require(value.get('suite_round_count') == compare.SUITE_ROUNDS, 'scheduler requires three paired rounds')
    order = [{'round': index + 1, 'pairs': list(compare.suite_execution_order(index))} for index in range(compare.SUITE_ROUNDS)]
    require(value.get('suite_execution_order') == order, 'scheduler rounds are not interleaved in owner order')
    labels = [f"round-{row['round']}-{arm}-suite" for row in order for arm in row['pairs']]
    require([stage.get('label') for stage in value.get('stages', []) if stage.get('mode') == 'suite'] == labels, 'scheduler suite execution sequence mismatch')
    source_path = bound_file({'path': value['source_manifest'], 'sha256': value['source_manifest_sha256']}, path.parent)
    source = read_json(source_path)
    require(compare.capture_source(config.source_root, config.commit) == source, 'scheduler source manifest mismatch')
    binary_path = bound_file({'path': value['binary_manifest'], 'sha256': value['binary_manifest_sha256']}, path.parent)
    binaries = read_json(binary_path)
    reference_root = canonical(value['reference_source_root'])
    reference_path = bound_file({'path': value['reference_source_manifest'], 'sha256': value['reference_source_manifest_sha256']}, path.parent)
    reference_source = read_json(reference_path)
    require(compare.capture_reference_source(reference_root, config.commit) == reference_source, 'scheduler reference source changed')
    compare.validate_reference_overlay(source, reference_source, reference_root)
    compare.validate_reference_scheduler_ancestry(reference_root, config.commit)
    directories = {arm: Path(binaries[arm]['xsht']['path']).parent for arm in ('reference', 'candidate')}
    require(compare.capture_binaries(directories['reference'], directories['candidate']) == binaries, 'scheduler products changed')
    for arm in ('reference', 'candidate'):
        recorded = value[f'{arm}_build_receipt']
        build_path = bound_file(recorded, path.parent)
        manifest_path = source_path if arm == 'candidate' else reference_path
        manifest_source = read_json(manifest_path)
        root = config.source_root if arm == 'candidate' else Path(value['reference_source_root'])
        compare.validate_build_receipt(build_path, label=arm, source_root=root, source_commit=config.commit, source_manifest=manifest_source, binaries=binaries[arm])
        require(read_json(build_path) == value[f'{arm}_build_receipt_content'], f'scheduler {arm} build binding changed')
    candidate = value['candidate_build_receipt_content']
    for name, record in profile['binaries'].items():
        require(candidate.get('binaries', {}).get(name, {}).get('sha256') == record['sha256'], f'scheduler profile product mismatch: {name}')
    manifest = canonical(value['manifest_path'])
    ids = compare.read_manifest(manifest)
    require(compare.manifest_sha256(ids) == value['manifest']['ids_sha256'] and owned_digest(manifest) == value['manifest']['file_sha256'], 'scheduler manifest changed')
    stages = value.get('stages', [])
    runs = {}
    for stage in stages:
        label = stage['label']
        require(label not in runs, 'scheduler duplicate suite stage')
        runs[label] = stage
        accepted_timing(stage, label)
        check_cleanup(stage.get('container_cleanup', {}))
        require(type(stage.get('exit_code')) is int and stage['exit_code'] == 0 and type(stage.get('container_exit_code')) is int and stage['container_exit_code'] == 0, f'{label}: suite failed')
        bound_file({'path': stage['result_log'], 'sha256': stage['result_log_sha256']}, path.parent)
        for boundary in ('before', 'after'):
            cgroup_path = canonical(stage[f'cgroup_{boundary}'])
            require(cgroup_path == path.parent / f'{label}.cgroup-{boundary}.txt', f'{label}: unexpected cgroup path')
            bound_file({'path': str(cgroup_path), 'sha256': stage['artifact_hashes'][f'.cgroup-{boundary}.txt']}, path.parent)
        compare.validate_cgroup_snapshots(Path(stage['cgroup_before']).read_text(), Path(stage['cgroup_after']).read_text())
        require(stage.get('post_stage_validation') == {'source': compare.source_summary(source), 'binaries': binaries, 'unchanged': True}, f'{label}: source or products changed during suite')
    expected = {f'{arm}-{mode}' for arm in ('reference', 'candidate') for mode in ('list', 'help')}
    expected.update(f'round-{index + 1}-{arm}-suite' for index in range(compare.SUITE_ROUNDS) for arm in ('reference', 'candidate'))
    require(set(runs) == expected, 'scheduler requires both list/help stages and all six suite executions')
    require(value.get('expected_default_workers') == compare.WORKERS, 'scheduler worker policy mismatch')
    for arm in ('reference', 'candidate'):
        require(compare.read_manifest(Path(runs[f'{arm}-list']['result_log'])) == ids, f'scheduler {arm} list differs from manifest')
        text = value.get('default_policy_help', {}).get(f'{arm}_expected_text')
        require(isinstance(text, str) and text and text in Path(runs[f'{arm}-help']['result_log']).read_text(), f'scheduler {arm} help policy mismatch')
    for index in range(compare.SUITE_ROUNDS):
        pair = []
        for arm in ('reference', 'candidate'):
            matches = [stage for stage in runs.values() if stage['pair'] == arm and stage['label'] == f'round-{index + 1}-{arm}-suite']
            require(len(matches) == 1, f'scheduler missing round {index + 1} {arm}')
            pair.append(compare.validate_log(Path(matches[0]['result_log']), ids))
        require(compare.compare_pair(*pair, 0, 0)['accepted'] is True, f'scheduler round {index + 1} outcomes rejected')
    return value


def baseline_receipts(config):
    require(config.baseline is not None, 'baseline receipt paths are not configured')
    source = source_identity(config)
    paths = config.baseline
    require(config.functional is not None and paths['functional_close'] == config.functional['output'], 'baseline requires the configured canonical functional close')
    binaries, review = functional_inputs(config)
    close = read_json(paths['functional_close'])
    expected_evidence = {name: receipt_binding(path) for name, path in config.functional.items() if name != 'output'}
    require(close.get('functional_complete') is True and close.get('commit') == config.commit and close.get('tree') == config.tree, 'functional close source mismatch or incomplete')
    require(close.get('evidence') == expected_evidence and close.get('products_used') == binaries and close.get('reviewer') == review['reviewer'], 'functional close evidence or products changed')
    prefix = 'dev/consolidation/winddown/gate-tools/final-performance-gates/'
    forced = baseline_owner(config, prefix + 'forced-frames.py', 'forced')
    observer = baseline_owner(config, prefix + 'final-observers.py', 'observer')
    comparative = baseline_owner(config, prefix + 'final-comparative.py', 'comparative')
    compare = baseline_owner(config, 'dev/consolidation/winddown/gate-tools/native-scheduler-comparison/compare.py', 'scheduler')
    profile = forced.load_profile(paths['profile'], config.source_root, config.commit, source)
    observer.load_profile_binaries(paths['profile'], config.source_root, config.commit, 3)
    bridge = baseline_owner(config, 'dev/consolidation/final_scheduler_receipt_bridge.py', 'bridge')
    profile_receipt = read_json(paths['profile'])
    laputa = validate_laputa_corpus(profile_receipt.get('laputa_corpus'), config)
    require(type(profile_receipt.get('schema')) is int and profile_receipt['schema'] == 1 and profile_receipt.get('source_tree') == str(config.source_root), 'profile schema or explicit source root mismatch')
    require(profile_receipt.get('functional_close') == {**receipt_binding(paths['functional_close']), 'record': close}, 'profile functional close mismatch')
    for name in ('build', 'native_suite'):
        stage = profile_receipt['results'][0].get(name)
        bridge.require_successful_stage(stage, name)
        require(type(stage.get('status')) is int and stage['status'] == 0, f'profile {name} failed')
        require(stage.get('stage_validation') == {'source': source, 'binary_hashes': profile_receipt['results'][0]['binaries']}, f'profile {name} source or products changed')
        filename = 'build.log' if name == 'build' else 'native-suite.log'
        bound_file({'path': str(paths['profile'].parent / 'opt3' / filename), 'sha256': stage['log_sha256']}, paths['profile'].parent)
    close_binding = {**receipt_binding(paths['functional_close']), 'record': close}
    observed = completed_receipt(paths['observer'], 'schema_version', source)
    require(observed['source'].get('root') == str(config.source_root), 'observer source root mismatch')
    require(observed.get('execution_mode') == 'timed' and observed.get('functional_close') == close_binding and observed.get('profile_cost_receipt') == profile['receipt'], 'observer qualification mismatch')
    for name in ('selected_release_binaries', 'selected_release_binaries_after_lint', 'selected_release_binaries_after_gate_batch'):
        require(observed.get(name) == profile['binaries'], f'observer products changed: {name}')
    fields(observed.get('stages'), ('isolated_repository_lint', 'canonical_gate_batch'), 'observer stages')
    for name, stage in observed['stages'].items():
        accepted_timing(stage, name)
        require(stage.get('observation_accepted') is True and type(stage.get('status')) is int and stage['status'] == 0, f'observer stage failed: {name}')
        check_cleanup(stage.get('container_cleanup', {}))
        for stream in ('stdout', 'stderr'):
            bound_file(stage[stream], paths['observer'].parent)
        resource = stage.get('container_resources')
        require(isinstance(resource, dict), f'observer resource record absent: {name}')
        resource_path = bound_file(resource, paths['observer'].parent)
        require(observer.resource_receipt(resource_path) == resource, f'observer resource binding changed: {name}')
        observer.validate_gate_batch_resources(resource_path, observed['resource_policy']['gate_batch_cpu_set'])
        validation = stage.get('post_stage_validation', {})
        require(validation.get('source') == source and validation.get('selected_release_binaries') == profile['binaries'], f'observer source or products changed: {name}')
    require(observed.get('source_after') == source, 'observer final source mismatch')
    framed = completed_receipt(paths['forced'], 'schema', source)
    require(validate_laputa_corpus(framed.get('laputa_corpus'), config) == laputa, 'profile and forced-frame Laputa identities differ')
    require(framed.get('environment', {}).get('XSH_LAPUTA_CORPUS') == str(config.laputa_root), 'forced-frame Laputa corpus differs from explicit configuration')
    require(framed.get('functional_close') == close_binding and framed.get('profile_cost_receipt') == profile['receipt'], 'forced-frame qualification mismatch')
    require(framed.get('binaries_before') == profile['binaries'] and framed.get('inputs_unchanged') is True and framed.get('inputs_before') == framed.get('inputs_after'), 'forced-frame inputs changed')
    require(framed.get('inputs_before') == {'source': source, 'binaries': profile['binaries']}, 'forced-frame input qualification mismatch')
    stage = framed.get('stage', {})
    docker_command = stage.get('command')
    require(isinstance(docker_command, list) and all(isinstance(argument, str) for argument in docker_command), 'forced-frame executed Docker command missing')
    volumes = [docker_command[index + 1] for index, argument in enumerate(docker_command[:-1]) if argument == '-v']
    laputa_mounts = [volume for volume in volumes if len(volume.split(':')) >= 2 and volume.split(':')[1] == str(config.laputa_root)]
    require(laputa_mounts == [f'{config.laputa_root}:{config.laputa_root}:ro'], 'forced-frame executed Laputa mount differs from explicit configuration')
    environment = [docker_command[index + 1] for index, argument in enumerate(docker_command[:-1]) if argument == '-e']
    require([item for item in environment if item.startswith('XSH_LAPUTA_CORPUS=')] == [f'XSH_LAPUTA_CORPUS={config.laputa_root}'], 'forced-frame executed Laputa environment differs from explicit configuration')
    require(type(stage.get('status')) is int and stage['status'] == 0 and stage.get('cleanup_resolved') is True and stage.get('error') is None and stage.get('interrupted_signal') is None, 'forced-frame suite failed or cleanup unresolved')
    check_cleanup(stage.get('container_cleanup', {}))
    require(framed.get('command') == [str(Path(profile['directory']) / 'xsht'), 'test', '--force-frames'], 'forced-frame command mismatch')
    final = completed_receipt(paths['comparative'], 'schema', source)
    require(final.get('functional_close') == close_binding, 'comparative functional close mismatch')
    binding = comparative.profile_cost_receipt_binding(paths['profile'], config.source_root, config.commit, Path(profile['binaries']['xsh']['path']))
    require(final.get('profile_cost_receipt') == binding, 'comparative profile mismatch')
    timing = final.get('timing', {})
    accepted_timing(timing, 'comparative', accepted='accepted', discarded='discarded', gate=None)
    check_cleanup(timing.get('cleanup', {}))
    report_path = bound_file({'path': 'comparative.json', 'sha256': final['comparative_report_sha256']}, paths['comparative'].parent)
    bench = config.source_root / 'bench/consolidation'
    lock_path = bench / 'versions.lock.json'
    require(final.get('run_harness_sha256') == owned_digest(bench / 'run.py'), 'comparative harness changed')
    command = final['command']
    require(command[2] == str(bench / 'run.py') and command.count('--fixtures') == 1, 'comparative owner command mismatch')
    fixture_path = canonical(command[command.index('--fixtures') + 1])
    bench_owner = baseline_owner(config, 'bench/consolidation/run.py', 'comparative_bench')
    require(bench_owner.validate_fixtures(fixture_path) == final['fixtures_sha256'], 'comparative fixtures changed')
    comparative.verify_report(read_json(report_path), config.commit, final['comparative_image'], read_json(lock_path), owned_digest(lock_path), final['fixtures_sha256'], profile['binaries']['xsh']['sha256'], close)
    scheduler = validate_scheduler(config, paths['scheduler'], profile, compare)
    ids = compare.read_manifest(Path(scheduler['manifest_path']))
    require(compare.validate_log(Path(stage['log']), ids).accepted, 'forced-frame outcomes rejected')
    require(compare.validate_log(paths['profile'].parent / 'opt3/native-suite.log', ids).accepted, 'profile native outcomes rejected')
    require(source_identity(config) == source, 'baseline source changed during receipt validation')
    require(validate_laputa_corpus(profile_receipt['laputa_corpus'], config) == laputa and validate_laputa_corpus(framed['laputa_corpus'], config) == laputa, 'Laputa corpus changed during baseline receipt validation')
    return {**{name: receipt_binding(path) for name, path in paths.items()}, 'forced_stdout': receipt_binding(Path(stage['log']))}


def baseline(config, runtime=None):
    runtime = baseline_util_linux(config) if runtime is None else runtime
    require(runtime is baseline_util_linux(config) and runtime.ROOT == config.source_root, 'util-linux runtime must belong to the explicit frozen source')
    require(runtime._review_scope_active.get() and runtime._review_result.get() is not None, 'baseline requires an active one-use util-linux independent_review context before any gate execution')
    import acceptance
    review = runtime._review_result.get()
    acceptance._validate_gate_result(review, runtime.source_identity())
    before = baseline_receipts(config)
    with runtime.cancellation_scope(), runtime.campaign_leases(config.source_root, native=True, cargo=True):
        acceptance.close(runtime)
    require(baseline_receipts(config) == before, 'baseline receipts changed during util-linux close')


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
        elif args.mode == 'baseline':
            require(args.check, 'baseline requires --check')
            baseline(config)
        else:
            raise CheckError(f'{args.mode}: unavailable until existing owner is integrated')
        return 0
    except owned.SignalReceived as error:
        print(f'performance check cancelled: {error}', file=sys.stderr)
        for note in getattr(error, '__notes__', []):
            print(note, file=sys.stderr)
        return 128 + error.signum
    except (ValueError, RuntimeError, OSError, KeyError, TypeError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        print(f'performance check: {error}', file=sys.stderr)
        for note in getattr(error, '__notes__', []):
            print(note, file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
