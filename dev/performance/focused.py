import argparse
import base64
import binascii
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

from dev.performance import check, resources, workloads


_CASE_NAMES = ('json-n', 'core-wc-small')
_RESOURCE_SCOPE = resources.GroupScope.FRESH_CASE_CONTAINER.value
_RESOURCE_FIELDS = frozenset(resources.CaseResources.__dataclass_fields__)
_DECLARED_WORKERS = (('container_init', 1), ('python_driver', 1), ('xsh_workload', 1))

_DRIVER_SOURCE = '''
import json,sys
from dataclasses import asdict
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/"bench/consolidation"))
import run
from dev.performance import resources
work=Path(sys.argv[1])
env={"PATH":"/usr/bin:/bin","LC_ALL":"C","LANG":"C","TZ":"UTC","HOME":str(work),"TMPDIR":str(work),"PYTHONDONTWRITEBYTECODE":"1"}
before=resources.capture_current_cgroup()
row=run.invoke(sys.argv[2:],work,env,60)
after=resources.capture_current_cgroup()
try:
    result=resources.validate_case_window(before,after,declared_workers=__DECLARED_WORKERS__,scope=resources.GroupScope.FRESH_CASE_CONTAINER)
    diagnostic={"schema":1,"authority":"diagnostic_only","scope":resources.GroupScope.FRESH_CASE_CONTAINER.value,"status":"complete" if result.telemetry_complete else "incomplete","observation":asdict(result)}
except resources.ResourceWindowError as error:
    diagnostic={"schema":1,"authority":"diagnostic_only","scope":resources.GroupScope.FRESH_CASE_CONTAINER.value,"status":"incomplete","reason":str(error)}
print(json.dumps({"workload":row,"resource_diagnostics":diagnostic}))
'''.replace('__DECLARED_WORKERS__', repr(dict(_DECLARED_WORKERS)))


@dataclass(frozen=True)
class _FocusedConfig:
    source_root: Path
    commit: str
    tree: str
    xsh: Path
    scratch: Path
    fixtures: Path
    output: Path
    image_id: str
    source_digest: str
    product_digest: str
    product_mode: int


class _WorkloadFailure(check.CheckError):
    pass


def _leases(config):
    return check.baseline_owner(config, 'dev/util-linux/leases.py', 'focused_leases')


def _validate_mount_paths(paths):
    for path in paths:
        value = os.fspath(path)
        check.require(':' not in value and ',' not in value,
                      f'Docker bind mount path contains an unsupported delimiter: {path}')


def _mount_spec(path, readonly):
    value = os.fspath(path)
    check.require(':' not in value and ',' not in value,
                  f'Docker bind mount path contains an unsupported delimiter: {path}')
    mode = ',readonly' if readonly else ''
    return f'type=bind,src={value},dst={value}{mode}'


def _parse_config(value):
    check.fields(value, ('schema', 'image_id', 'source_root', 'commit', 'tree', 'xsh', 'scratch', 'measurement',
                         'gate_binding'),
                 'focused config')
    check.require(type(value['schema']) is int and value['schema'] == 1, 'unsupported focused config schema')
    for name in ('commit', 'tree'):
        check.require(isinstance(value[name], str) and re.fullmatch('[0-9a-f]{40}', value[name]),
                      f'focused {name} must be a full lowercase Git ID')
    paths = {name: check.canonical(value[name]) for name in ('source_root', 'xsh', 'scratch')}
    check.require(paths['scratch'].is_relative_to(check.ADAPTER_ROOT / '.work/tmp'),
                  'focused scratch must be under the adapter checkout .work/tmp')
    check.fields(value['measurement'], ('fixtures', 'output'), 'focused measurement')
    measurement = {name: check.canonical(path) for name, path in value['measurement'].items()}
    _validate_mount_paths((paths['source_root'], paths['xsh'], paths['scratch'], measurement['fixtures']))
    check.require(measurement['output'].is_relative_to(paths['scratch']),
                  'focused output must be under owned scratch')
    check.require(
        isinstance(value['image_id'], str) and re.fullmatch('sha256:[0-9a-f]{64}', value['image_id']),
        'focused image_id must be an explicit full sha256 digest',
    )
    binding = value['gate_binding']
    check.fields(binding, ('source_digest', 'product_digest', 'product_mode'), 'focused gate binding')
    for name in ('source_digest', 'product_digest'):
        check.require(isinstance(binding[name], str) and re.fullmatch('[0-9a-f]{64}', binding[name]),
                      f'focused gate {name} must be a full lowercase SHA-256 digest')
    check.require(type(binding['product_mode']) is int and 0 <= binding['product_mode'] <= 0o7777,
                  'focused gate product_mode must be mode bits')
    return _FocusedConfig(
        source_root=paths['source_root'],
        commit=value['commit'],
        tree=value['tree'],
        xsh=paths['xsh'],
        scratch=paths['scratch'],
        fixtures=measurement['fixtures'],
        output=measurement['output'],
        image_id=value['image_id'],
        source_digest=binding['source_digest'],
        product_digest=binding['product_digest'],
        product_mode=binding['product_mode'],
    )


def _admission(config):
    owner = check.baseline_owner(
        config,
        'dev/consolidation/winddown/gate-tools/final-performance-gates/final-comparative.py',
        'focused_admission',
    )
    admission = owner.perf_gate_check()
    check.require(isinstance(admission, dict) and type(admission.get('exit_code')) is int,
                  'focused resource admission result is malformed')
    check.require(admission['exit_code'] == 0, 'focused resource admission unavailable')
    return admission


def _source_snapshot(root):
    check.require(root == root.resolve() and root.is_dir(), 'source root must be an existing nonsymlink directory')
    check.require(check.git(root, 'rev-parse', '--show-toplevel') == str(root),
                  'source root must be a Git repository root')
    names = check.gate_source.git(os.fsencode(root), b'ls-files', b'-z', b'--cached', b'--others',
                                  b'--exclude-standard', timeout=15)
    result = {}
    for name in sorted(set(names.split(b'\0')) - {b''}):
        relative = os.fsdecode(name)
        path = root / relative
        check.require(path.resolve().is_relative_to(root), f'source link escapes checkout: {relative}')
        if not path.exists() and not path.is_symlink():
            result[relative] = {'kind': 'deleted'}
            continue
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode):
            result[relative] = {'kind': 'link', 'target': os.readlink(path)}
            if path.resolve().is_file():
                result[relative]['sha256'] = check.owned_digest(path)
        else:
            check.require(stat.S_ISREG(info.st_mode), f'unsupported source input: {relative}')
            result[relative] = {
                'kind': 'file',
                'mode': stat.S_IMODE(info.st_mode),
                'sha256': check.owned_digest(path),
            }
    return result


def _source_revision(config):
    actual = {
        'commit': check.git(config.source_root, 'rev-parse', 'HEAD'),
        'tree': check.git(config.source_root, 'rev-parse', 'HEAD^{tree}'),
    }
    check.require(actual == {'commit': config.commit, 'tree': config.tree},
                  'source commit or tree differs from focused configuration')
    return actual


def _gate_source_digest(root):
    try:
        result = subprocess.run(
            [sys.executable, '-B', str(root / 'dev/gate_source.py')],
            cwd=root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise check.CheckError(f'gate source observation failed: {error}') from error
    check.require(result.returncode == 0, f'gate source observation failed: {result.stderr.strip()}')
    digest = result.stdout.strip()
    check.require(re.fullmatch('[0-9a-f]{64}', digest) is not None,
                  'gate source observation returned an invalid SHA-256 digest')
    return digest


def _case(config, case_name):
    check.require(case_name in _CASE_NAMES, 'unsupported focused workload')
    if case_name == 'core-wc-small':
        fixture_owner = check.baseline_owner(
            config, 'dev/performance/core_cases.py',
            'focused_core_' + check.owned_digest(config.source_root / 'dev/performance/core_cases.py'),
        )
        fixture_root = config.fixtures / case_name
        case = fixture_owner.describe_case(case_name, fixtures=config.fixtures, source_root=config.source_root)
        fixture_owner.validate_case(case, fixtures=config.fixtures, source_root=config.source_root)
        return case, fixture_root, fixture_owner.snapshot(fixture_root), fixture_owner
    fixture_owner = check.baseline_owner(
        config,
        'bench/consolidation/fixtures.py',
        'focused_fixtures_' + check.owned_digest(config.source_root / 'bench/consolidation/fixtures.py'),
    )
    fixture_root = config.fixtures / str(fixture_owner.PACKAGE_ROW_SHAPES[0])
    check.require(fixture_root.is_dir() and fixture_root == fixture_root.resolve(),
                  'focused fixture root unavailable or linked')
    with tempfile.TemporaryDirectory(prefix='focused-reference-', dir=config.scratch) as directory:
        reference = Path(directory) / str(fixture_owner.PACKAGE_ROW_SHAPES[0])
        fixture_owner.generate(reference, package_rows=fixture_owner.PACKAGE_ROW_SHAPES[0])
        fixture_snapshot = fixture_owner.snapshot(fixture_root)
        check.require(fixture_snapshot == fixture_owner.snapshot(reference), 'focused fixture/golden mismatch')
    cases = workloads.frozen_workloads(
        config.fixtures,
        selected=(case_name,),
        source_root=config.source_root,
    )
    check.require(len(cases) == 1 and cases[0].name == case_name, 'focused workload selection mismatch')
    case = cases[0]
    check.require(case.arguments == (str(fixture_root),), 'focused workload input binding mismatch')
    return case, fixture_root, fixture_snapshot, fixture_owner


def _output_path(config):
    check.require(config.scratch.is_dir() and config.scratch == config.scratch.resolve(),
                  'focused scratch unavailable or linked')
    output = config.output
    check.require(
        output.is_relative_to(config.scratch)
        and output.parent.is_dir()
        and output.parent == output.parent.resolve()
        and not output.exists()
        and not output.is_symlink(),
        'focused output must be fresh and under owned scratch',
    )
    return output


def _product_snapshot(path):
    check.require(path == path.resolve(), 'focused product must be an existing nonsymlink executable')
    info = path.lstat()
    check.require(stat.S_ISREG(info.st_mode) and os.access(path, os.X_OK),
                  'focused product must be an existing nonsymlink executable')
    return {
        'record': {'path': str(path), 'sha256': check.owned_digest(path), 'bytes': info.st_size,
                   'mode': stat.S_IMODE(info.st_mode)},
        'identity': (info.st_dev, info.st_ino, info.st_mode),
    }


def _assert_unchanged(config, source, source_digest, revision, product, fixture_root, fixtures, fixture_owner):
    check.require(_source_revision(config) == revision, 'source commit or tree changed during focused check')
    check.require(_source_snapshot(config.source_root) == source, 'tested source changed during focused check')
    check.require(_gate_source_digest(config.source_root) == source_digest,
                  'gate source selection changed during focused check')
    current_product = _product_snapshot(config.xsh)
    check.require(current_product == product, 'tested product changed during focused check')
    check.require(current_product['record']['sha256'] == config.product_digest
                  and current_product['record']['mode'] == config.product_mode,
                  'gate-prepared product changed during focused check')
    check.require(fixture_owner.snapshot(fixture_root) == fixtures, 'tested fixtures changed during focused check')


def _verify_image_id(image_id):
    try:
        result = subprocess.run(
            ['docker', 'image', 'inspect', '--format={{.Id}}', image_id],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=15,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise check.CheckError(f'focused Docker image inspection failed: {error}') from error
    check.require(result.returncode == 0, f'focused Docker image inspection failed: {result.stderr.strip()}')
    check.require(result.stdout.strip() == image_id, 'Docker resolved a different focused image ID')
    try:
        volumes = subprocess.run(
            ['docker', 'image', 'inspect', '--format={{json .Config.Volumes}}', image_id],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=15,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise check.CheckError(f'focused Docker volume inspection failed: {error}') from error
    check.require(volumes.returncode == 0,
                  f'focused Docker volume inspection failed: {volumes.stderr.strip()}')
    try:
        declared_volumes = json.loads(volumes.stdout)
    except json.JSONDecodeError as error:
        raise check.CheckError('focused Docker volume inspection returned malformed JSON') from error
    check.require(declared_volumes is None or declared_volumes == {},
                  'focused Docker image declares writable volumes')


def _resource_diagnostics(value):
    incomplete = {
        'schema': 1,
        'authority': 'diagnostic_only',
        'scope': _RESOURCE_SCOPE,
        'status': 'incomplete',
        'reason': 'resource diagnostic payload is malformed',
    }
    if not isinstance(value, dict) or type(value.get('schema')) is not int or value['schema'] != 1 \
            or value.get('authority') != 'diagnostic_only' \
            or value.get('scope') != _RESOURCE_SCOPE:
        return incomplete
    status = value.get('status')
    observation = value.get('observation')
    envelope = {'schema', 'authority', 'scope', 'status'}
    if status == 'complete' and set(value) == envelope | {'observation'} \
            and _valid_resource_observation(observation, complete=True):
        return value
    if status == 'incomplete':
        if set(value) == envelope | {'reason'} and isinstance(value['reason'], str) and value['reason']:
            return value
        if set(value) == envelope | {'observation'} and _valid_resource_observation(observation, complete=False):
            return value
    return incomplete


def _valid_resource_observation(value, *, complete):
    if not isinstance(value, dict) or set(value) != _RESOURCE_FIELDS:
        return False

    def unavailable(item):
        return (
            isinstance(item, dict)
            and set(item) == {'availability', 'reason'}
            and isinstance(item['availability'], str)
            and item['availability'] in {'unsupported', 'incomplete'}
            and isinstance(item['reason'], str)
            and bool(item['reason'])
        )

    def counter(item):
        return (type(item) is int and item >= 0) or (not complete and unavailable(item))

    def counters(item, required=(), *, nonempty=False):
        if unavailable(item):
            return not complete
        return (
            isinstance(item, dict)
            and (not nonempty or bool(item))
            and (not complete or set(required) <= item.keys())
            and all(
                isinstance(name, str) and bool(name) and name.isascii()
                and not any(character.isspace() for character in name)
                and type(count) is int and count >= 0
                for name, count in item.items()
            )
        )

    identity = value['identity']
    if not isinstance(identity, dict) or set(identity) != {'device', 'inode', 'namespace_inode', 'membership'} \
            or any(type(identity[name]) is not int or identity[name] < 0 for name in ('device', 'inode', 'namespace_inode')) \
            or not isinstance(identity['membership'], str):
        return False
    membership = resources._absolute_posix_path(identity['membership'])
    if isinstance(membership, resources.Unavailable) or str(membership) != identity['membership']:
        return False
    if value['scope'] != _RESOURCE_SCOPE or type(value['telemetry_complete']) is not bool \
            or value['telemetry_complete'] is not complete:
        return False
    if complete and value['memory_events_hierarchical'] is not True:
        return False
    if not complete and type(value['memory_events_hierarchical']) is not bool \
            and not unavailable(value['memory_events_hierarchical']):
        return False
    if complete and value['pressure_accounting_enabled'] is not True:
        return False
    if not complete and type(value['pressure_accounting_enabled']) is not bool \
            and not unavailable(value['pressure_accounting_enabled']):
        return False
    numeric = (
        'memory_peak_sample_bytes', 'cpu_usage_ns',
        'cpu_throttled_ns', 'cpu_throttled_periods', 'observed_cgroup_tasks_peak',
        'observed_cgroup_tasks_current', 'observer_peak_rss_bytes',
    )
    if any(not counter(value[name]) for name in numeric):
        return False
    limits = (
        ('cpu_max', resources._valid_cpu_limit),
        ('memory_max', resources._valid_memory_limit),
        ('cpuset_cpus_effective', resources._valid_cpuset),
    )
    if any(
            not (isinstance(value[name], str) and valid(value[name]))
            and not (not complete and unavailable(value[name]))
            for name, valid in limits):
        return False
    events = value['memory_events_delta']
    required_events = {'oom', 'oom_kill', 'oom_group_kill'}
    if not counters(events, required_events, nonempty=True):
        return False
    if not counters(value['memory_stat_boundary'], nonempty=True):
        return False
    pressure = value['pressure_stall_us']
    if not isinstance(pressure, dict) or set(pressure) != {'cpu', 'memory', 'io'}:
        return False
    for category in pressure.values():
        if unavailable(category):
            if complete:
                return False
        elif not isinstance(category, dict) or set(category) != {'some', 'full'} \
                or any(not counter(count) for count in category.values()):
            return False
    workers = value['declared_workers']
    if not isinstance(workers, list) or any(
            not isinstance(worker, list) or len(worker) != 2
            or not isinstance(worker[0], str) or not worker[0]
            or type(worker[1]) is not int or worker[1] < 0
            for worker in workers) or not any(worker[1] for worker in workers):
        return False
    if [worker for worker in workers] != [list(worker) for worker in _DECLARED_WORKERS]:
        return False
    violations = value['violations']
    if not isinstance(violations, list) or any(not isinstance(violation, str) or not violation for violation in violations):
        return False
    expected_violations = []
    if isinstance(events, dict):
        for name in ('oom', 'oom_kill', 'oom_group_kill'):
            count = events.get(name)
            if type(count) is int and count > 0:
                expected_violations.append(f'memory.events {name} increased by {count}')
    if violations != expected_violations:
        return False
    return True


def _observe(config, case, work, lease_owner):
    command = [
        'docker', 'run', '--rm', '--init', '--platform', 'linux/amd64', '--network', 'none', '--read-only',
        '--cpus=2', '--memory=4g', '--memory-swap=4g', '--user', f'{os.getuid()}:{os.getgid()}',
    ]
    for path in dict.fromkeys((config.source_root, config.xsh, config.fixtures, work)):
        command += ['--mount', _mount_spec(path, path != work)]
    command += [
        '-w', str(config.source_root), config.image_id, '/usr/bin/python3', '-B', '-c', _DRIVER_SOURCE,
        str(work), *case.argv(config.xsh),
    ]
    _verify_image_id(config.image_id)
    lease_owner.prepare_cleanup_unresolved(f'focused {case.name}: container launch in progress')
    status, stdout, stderr = check.checked_docker(
        command,
        config.scratch,
        timeout_seconds=120,
        on_cleanup_verified=lease_owner.resolve_cleanup_unresolved,
    )
    check.require(status == 0 and stderr == b'',
                  f'focused observation driver failed: status={status}, stderr={stderr!r}')
    try:
        observation = json.loads(stdout, object_pairs_hook=check.unique_object)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise check.CheckError('focused observation driver returned malformed JSON') from error
    check.require(
        isinstance(observation, dict) and isinstance(observation.get('workload'), dict)
        and isinstance(observation['workload'].get('stdout_base64'), str),
        'malformed focused observation',
    )
    workload = observation['workload']
    try:
        actual_stdout = base64.b64decode(workload['stdout_base64'], validate=True)
        actual_stderr = base64.b64decode(workload.get('stderr_base64'), validate=True)
    except (binascii.Error, TypeError, ValueError) as error:
        raise check.CheckError('malformed focused observation output') from error
    if not (
        type(workload.get('status')) is int
        and workload['status'] == 0
        and workload.get('timed_out') is False
        and actual_stderr == b''
        and workload.get('effects') == {}
        and actual_stdout == case.expected_stdout
    ):
        raise _WorkloadFailure('focused workload status/output/stderr/effects oracle failed')
    return actual_stdout, _resource_diagnostics(observation.get('resource_diagnostics'))


def _publish_report(output, report):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='wb', prefix='.focused-', dir=output.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write((json.dumps(report, indent=2, sort_keys=True) + '\n').encode())
            stream.flush()
            os.fsync(stream.fileno())
        os.link(temporary, output)
    finally:
        if temporary is not None:
            temporary.unlink()


def _qualify(config, case_name, lease_owner):
    lease_owner.require_native_lease()
    output = _output_path(config)
    revision = _source_revision(config)
    source = _source_snapshot(config.source_root)
    source_digest = _gate_source_digest(config.source_root)
    check.require(source_digest == config.source_digest, 'source differs from gate selection')
    product = _product_snapshot(config.xsh)
    check.require(product['record']['sha256'] == config.product_digest
                  and product['record']['mode'] == config.product_mode,
                  'product differs from gate preparation')
    case, fixture_root, fixtures, fixture_owner = _case(config, case_name)
    check.require(case.script.is_file() and case.script == case.script.resolve(),
                  'focused workload script unavailable or linked')
    with tempfile.TemporaryDirectory(prefix='focused-', dir=config.scratch) as directory:
        work = Path(directory) / 'work'
        work.mkdir()
        admission = _admission(config)
        actual_stdout, resource_diagnostics = _observe(
            config,
            case,
            work,
            lease_owner,
        )
    _assert_unchanged(config, source, source_digest, revision, product, fixture_root, fixtures, fixture_owner)
    report = {
        'schema': 1,
        'mode': 'focused-correctness',
        'case': case.name,
        'script': str(case.script),
        'arguments': list(case.arguments),
        'correctness_complete': True,
        'cleanup_complete': True,
        'full_cohort_run': False,
        'performance_complete': False,
        'timing_complete': False,
        'product': product['record'],
        'product_source_binding': 'gate-prepared-product-bound-to-source-snapshot',
        'gate_binding': {
            'source_digest': source_digest,
            'product_digest': config.product_digest,
            'product_mode': config.product_mode,
        },
        'performance_status': 'not-measured',
        'performance_unresolved': 'controlled performance measurements are not qualified',
        'source_commit': revision['commit'],
        'source_tree': revision['tree'],
        'image_id': config.image_id,
        'tested_source': source,
        'fixture_root': str(fixture_root),
        'tested_fixtures': fixtures,
        'resource_admission': admission,
        'resource_diagnostics': resource_diagnostics,
        'oracle': {
            'status': 0,
            'stdout_sha256': hashlib.sha256(actual_stdout).hexdigest(),
            'stderr_bytes': 0,
            'effects': {},
        },
    }
    _publish_report(output, report)
    return report


def _reason(error):
    return '\n'.join((str(error), *(f'Detail: {note}' for note in getattr(error, '__notes__', ()))))


def _binding_label(verified):
    return ('gate-prepared product bound to selected source' if verified
            else 'gate-prepared product binding not verified')


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', required=True, type=Path)
    parser.add_argument('--case', required=True, choices=_CASE_NAMES)
    args = parser.parse_args(argv)
    try:
        config = _parse_config(check.read_json(args.config))
        lease_owner = _leases(config)
        try:
            with lease_owner.campaign_leases(config.source_root, native=True):
                report = _qualify(config, args.case, lease_owner)
        except lease_owner.LeaseBusy as error:
            print(f'Selected workload: incomplete\nReason: {_reason(error)}', file=sys.stderr)
            print(f'Product source binding: {_binding_label(False)}\nPerformance: not measured\nFull cohort: not run', file=sys.stderr)
            return 2
        print(f'Selected workload: {report["case"]} correctness passed')
        print(f'Product source binding: {_binding_label(True)}')
        print('Performance: not measured')
        print('Full cohort: not run')
        return 0
    except check.owned.SignalReceived as error:
        print(f'Selected workload: cancelled\nReason: {_reason(error)}', file=sys.stderr)
        print(f'Product source binding: {_binding_label(False)}\nPerformance: not measured\nFull cohort: not run', file=sys.stderr)
        return 128 + error.signum
    except _WorkloadFailure as error:
        print(f'Selected workload: failed\nReason: {_reason(error)}', file=sys.stderr)
        print(f'Product source binding: {_binding_label(False)}\nPerformance: not measured\nFull cohort: not run', file=sys.stderr)
        return 1
    except (ValueError, RuntimeError, OSError, subprocess.SubprocessError) as error:
        print(f'Selected workload: incomplete\nReason: {_reason(error)}', file=sys.stderr)
        print(f'Product source binding: {_binding_label(False)}\nPerformance: not measured\nFull cohort: not run', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
