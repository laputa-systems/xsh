from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

from dev.performance import check, focused, tokei_corpus


CASE_NAME = 'tokei-rust-correctness'
TOKEI_VERSION = 'tokei 14.0.0 compiled with serialization support: json'
TOKEI_COMMIT = '8cdd6fa3a54f8cd69442d2f00effb29aa3110353'
_DRIVER = r'''
import json, os, selectors, signal, subprocess, sys, time
from pathlib import Path
xsh, script, projection, tokei, xsh_report, tokei_report, work = sys.argv[1:]
env = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "LANG": "C", "TZ": "UTC",
       "HOME": work, "TMPDIR": work, "PYTHONDONTWRITEBYTECODE": "1",
       "RAYON_NUM_THREADS": "2"}
report_limit = 16 * 1024 * 1024
stderr_limit = 64 * 1024
def stop(child):
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        child.wait(timeout=1)
    except subprocess.TimeoutExpired as error:
        raise RuntimeError("could not reap scanner child") from error
def run(argv, output=None):
    child = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             env=env, start_new_session=True, bufsize=0)
    selector = None
    try:
        selector = selectors.DefaultSelector()
        buffers = {"stdout": bytearray(), "stderr": bytearray()}
        limits = {"stdout": report_limit, "stderr": stderr_limit}
        deadline = time.monotonic() + 60
        for name, stream in (("stdout", child.stdout), ("stderr", child.stderr)):
            os.set_blocking(stream.fileno(), False)
            selector.register(stream, selectors.EVENT_READ, name)
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"command exceeded deadline: {argv[0]}")
            for key, _ in selector.select(remaining):
                name = key.data
                buffer = buffers[name]
                chunk = os.read(key.fileobj.fileno(), min(65536, limits[name] - len(buffer) + 1))
                if not chunk:
                    selector.unregister(key.fileobj)
                    key.fileobj.close()
                    continue
                if len(buffer) + len(chunk) > limits[name]:
                    raise RuntimeError(f"command {name} exceeded its {limits[name]}-byte limit: {argv[0]}")
                buffer.extend(chunk)
        child.wait(timeout=max(0, deadline - time.monotonic()))
    except BaseException as primary:
        try:
            stop(child)
        except BaseException as cleanup:
            primary.add_note(f"scanner child cleanup failed: {cleanup}")
        raise
    finally:
        if selector is not None:
            selector.close()
        for stream in (child.stdout, child.stderr):
            if stream is not None and not stream.closed:
                stream.close()
    stdout = bytes(buffers["stdout"])
    stderr = bytes(buffers["stderr"])
    if child.returncode != 0 or stderr:
        raise RuntimeError(f"command failed: {argv[0]} status={child.returncode} stderr={stderr!r}")
    if output is not None:
        Path(output).write_bytes(stdout)
    return stdout
version = run([tokei, "--version"]).decode("utf-8").strip()
if version != "tokei 14.0.0 compiled with serialization support: json":
    raise RuntimeError(f"unexpected Tokei version: {version!r}")
run([xsh, script, "--root", projection, "--json"], xsh_report)
run([tokei, "--output", "json", "--types", "Rust", "--no-ignore", projection], tokei_report)
print(json.dumps({"tokei_version": version, "xsh_report": xsh_report,
                  "tokei_report": tokei_report}, sort_keys=True))
'''


@dataclass(frozen=True)
class _Config:
    source_root: Path
    corpus_root: Path
    commit: str
    tree: str
    xsh: Path
    tokei: Path
    scratch: Path
    output: Path
    image_id: str
    platform: str
    source_digest: str
    xsh_digest: str
    xsh_mode: int
    tokei_digest: str
    tokei_mode: int


def _parse_config(value):
    check.fields(value, ('schema', 'image_id', 'platform', 'source_root', 'corpus_root', 'commit', 'tree',
                         'xsh', 'tokei', 'scratch', 'output', 'gate_binding'), 'Tokei scenario config')
    check.require(type(value['schema']) is int and value['schema'] == 1,
                  'unsupported Tokei scenario config schema')
    for name in ('commit', 'tree'):
        check.require(isinstance(value[name], str) and re.fullmatch('[0-9a-f]{40}', value[name]),
                      f'Tokei scenario {name} must be a full lowercase Git ID')
    paths = {name: check.canonical(value[name]) for name in
             ('source_root', 'corpus_root', 'xsh', 'tokei', 'scratch', 'output')}
    check.require(paths['scratch'].is_relative_to(check.ADAPTER_ROOT / '.work/tmp'),
                  'Tokei scenario scratch must be under adapter .work/tmp')
    check.require(paths['scratch'].is_dir() and paths['scratch'].resolve() == paths['scratch'],
                  'Tokei scenario scratch is unavailable or linked')
    check.require(paths['output'].is_relative_to(paths['scratch']),
                  'Tokei scenario output must be under owned scratch')
    check.require(paths['output'].parent.is_dir() and paths['output'].parent.resolve() == paths['output'].parent,
                  'Tokei scenario report parent is unavailable or linked')
    check.require(isinstance(value['image_id'], str)
                  and re.fullmatch('sha256:[0-9a-f]{64}', value['image_id']),
                  'Tokei scenario image ID must be a full sha256 digest')
    check.require(isinstance(value['platform'], str) and re.fullmatch('linux/(amd64|arm64)', value['platform']),
                  'Tokei scenario platform must be linux/amd64 or linux/arm64')
    binding = value['gate_binding']
    check.fields(binding, ('source_digest', 'xsh_digest', 'xsh_mode', 'tokei_digest', 'tokei_mode'),
                 'Tokei scenario gate binding')
    for name in ('source_digest', 'xsh_digest', 'tokei_digest'):
        check.require(isinstance(binding[name], str) and re.fullmatch('[0-9a-f]{64}', binding[name]),
                      f'Tokei scenario {name} must be a full SHA-256 digest')
    for name in ('xsh_mode', 'tokei_mode'):
        check.require(type(binding[name]) is int and 0 <= binding[name] <= 0o7777,
                      f'Tokei scenario {name} must be mode bits')
    return _Config(
        source_root=paths['source_root'], corpus_root=paths['corpus_root'],
        commit=value['commit'], tree=value['tree'], xsh=paths['xsh'], tokei=paths['tokei'],
        scratch=paths['scratch'], output=paths['output'], image_id=value['image_id'],
        platform=value['platform'], source_digest=binding['source_digest'], xsh_digest=binding['xsh_digest'],
        xsh_mode=binding['xsh_mode'], tokei_digest=binding['tokei_digest'],
        tokei_mode=binding['tokei_mode'],
    )


def _product(path, digest, mode, label):
    check.require(path == path.resolve(), f'{label} product must not be linked')
    info = path.lstat()
    check.require(stat.S_ISREG(info.st_mode) and os.access(path, os.X_OK),
                  f'{label} product must be a regular executable')
    record = {'path': str(path), 'sha256': check.owned_digest(path),
              'bytes': info.st_size, 'mode': stat.S_IMODE(info.st_mode)}
    check.require(record['sha256'] == digest and record['mode'] == mode,
                  f'{label} product differs from gate binding')
    return {'record': record, 'identity': (info.st_dev, info.st_ino, info.st_mode)}


def _mount(path, readonly):
    value = os.fspath(path)
    check.require(':' not in value and ',' not in value,
                  f'Docker bind mount path contains unsupported delimiter: {path}')
    return f'type=bind,src={value},dst={value}' + (',readonly' if readonly else '')


def _observe(config, projection, xsh_report, tokei_report, work, lease_owner):
    command = [
        'docker', 'run', '--rm', '--init', '--platform', config.platform, '--network', 'none',
        '--read-only', '--cpus=2', '--memory=4g', '--memory-swap=4g',
        '--user', f'{os.getuid()}:{os.getgid()}',
    ]
    for path in dict.fromkeys((config.source_root, config.xsh, config.tokei, projection, work)):
        command += ['--mount', _mount(path, path != work)]
    command += [
        '-w', str(config.source_root), config.image_id, '/usr/bin/python3', '-B', '-c', _DRIVER,
        str(config.xsh), str(config.source_root / 'showcase/tokei.xsh'), str(projection), str(config.tokei),
        str(xsh_report), str(tokei_report), str(work),
    ]
    admission = focused._admission(config)
    focused._verify_image_id(config.image_id)
    lease_owner.prepare_cleanup_unresolved(f'{CASE_NAME}: container launch in progress')
    status, stdout, stderr = check.checked_docker(
        command, config.scratch, timeout_seconds=240,
        on_cleanup_verified=lease_owner.resolve_cleanup_unresolved,
    )
    check.require(status == 0 and stderr == b'',
                  f'Tokei scenario observation failed: status={status}, stderr={stderr!r}')
    try:
        result = json.loads(stdout, object_pairs_hook=check.unique_object)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise check.CheckError('Tokei scenario returned malformed observation JSON') from error
    check.require(result == {'tokei_version': TOKEI_VERSION,
                             'xsh_report': str(xsh_report), 'tokei_report': str(tokei_report)},
                  'Tokei scenario observation binding mismatch')
    return admission


def _publish(path, report):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='wb', prefix='.tokei-scenario-', dir=path.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write((json.dumps(report, indent=2, sort_keys=True) + '\n').encode())
            stream.flush()
            os.fsync(stream.fileno())
        os.link(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink()


def _line_counts(xsh_report, tokei_report):
    counts = {}
    for name, path in (('xsh', xsh_report), ('tokei', tokei_report)):
        rust = tokei_corpus._read_report(path).get('Rust')
        check.require(isinstance(rust, dict) and all(type(rust.get(key)) is int and rust[key] >= 0
                      for key in ('code', 'comments', 'blanks')),
                      f'{name} Rust line counts are unavailable or invalid')
        counts[name] = {key: rust[key] for key in ('code', 'comments', 'blanks')}
    counts['xsh_minus_tokei'] = {key: counts['xsh'][key] - counts['tokei'][key]
                                 for key in ('code', 'comments', 'blanks')}
    return counts


def _qualify(config, lease_owner):
    lease_owner.require_native_lease()
    check.require(config.output.parent.is_dir() and not config.output.exists() and not config.output.is_symlink(),
                  'Tokei scenario report path must be fresh under owned scratch')
    revision = focused._source_revision(config)
    source = focused._source_snapshot(config.source_root)
    source_digest = focused._gate_source_digest(config.source_root)
    check.require(source_digest == config.source_digest, 'source differs from gate selection')
    xsh_product = _product(config.xsh, config.xsh_digest, config.xsh_mode, 'XSH')
    tokei_product = _product(config.tokei, config.tokei_digest, config.tokei_mode, 'Tokei')
    with tempfile.TemporaryDirectory(prefix='tokei-scenario-', dir=config.scratch) as directory:
        work = Path(directory) / 'work'
        work.mkdir()
        projection = Path(directory) / 'uutils-rust'
        xsh_report = work / 'xsh.json'
        tokei_report = work / 'tokei.json'
        projection_summary = tokei_corpus.project_commit(
            config.corpus_root, projection, tokei_corpus.UUTILS_COMMIT,
        )
        admission = _observe(config, projection, xsh_report, tokei_report, work, lease_owner)
        reconciliation = tokei_corpus.reconcile(
            config.corpus_root, projection, xsh_report, tokei_report,
        )
        check.require(reconciliation['exact'],
                      'XSH and Tokei selected file sets differ from pinned Rust corpus')
        line_counts = _line_counts(xsh_report, tokei_report)
    check.require(focused._source_revision(config) == revision,
                  'XSH source revision changed during Tokei scenario')
    check.require(focused._source_snapshot(config.source_root) == source,
                  'XSH source changed during Tokei scenario')
    check.require(focused._gate_source_digest(config.source_root) == source_digest,
                  'gate source selection changed during Tokei scenario')
    check.require(_product(config.xsh, config.xsh_digest, config.xsh_mode, 'XSH') == xsh_product,
                  'XSH product changed during Tokei scenario')
    check.require(_product(config.tokei, config.tokei_digest, config.tokei_mode, 'Tokei') == tokei_product,
                  'Tokei product changed during Tokei scenario')
    report = {
        'schema': 1, 'mode': 'focused-correctness', 'case': CASE_NAME,
        'correctness_complete': True, 'cleanup_complete': True, 'full_cohort_run': False,
        'performance_complete': False, 'timing_complete': False, 'performance_status': 'not-measured',
        'source_commit': revision['commit'], 'source_tree': revision['tree'],
        'source_digest': source_digest, 'image_id': config.image_id,
        'product_bindings': {
            'xsh': 'gate-prepared-and-bound-to-selected-source-snapshot',
            'tokei': 'version-checked-binary-digest-and-mode-snapshot',
        },
        'products': {'xsh': xsh_product['record'], 'tokei': tokei_product['record']},
        'resource_admission': admission,
        'tokei_reference': {
            'version': TOKEI_VERSION, 'expected_source_commit': TOKEI_COMMIT,
            'source_provenance': 'unqualified', 'binary_sha256': tokei_product['record']['sha256'],
        },
        'p0_qualification': 'unqualified',
        'corpus': projection_summary, 'reconciliation': reconciliation, 'rust_line_counts': line_counts,
    }
    _publish(config.output, report)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        config = _parse_config(check.read_json(args.config))
        lease_owner = check.baseline_owner(config, 'dev/util-linux/leases.py', 'tokei_scenario_leases')
        try:
            with lease_owner.campaign_leases(config.source_root, native=True):
                report = _qualify(config, lease_owner)
        except lease_owner.LeaseBusy as error:
            print(f'{CASE_NAME}: incomplete\nReason: {error}', file=sys.stderr)
            print('Performance: not measured\nFull cohort: not run', file=sys.stderr)
            return 2
        check.require(report['correctness_complete'] and report['reconciliation']['exact'],
                      'Tokei scenario did not qualify the exact file set')
        print(f'{CASE_NAME}: selected file sets passed')
        print('Product binding: XSH source-bound; Tokei binary snapshot verified')
        print('Tokei source provenance: unqualified; P0: not qualified')
        print('Performance: not measured')
        print('Full cohort: not run')
        return 0
    except check.owned.SignalReceived as error:
        print(f'{CASE_NAME}: cancelled\nReason: {error}', file=sys.stderr)
        return 128 + error.signum
    except (check.CheckError, tokei_corpus.CorpusError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f'{CASE_NAME}: incomplete\nReason: {error}', file=sys.stderr)
        print('Performance: not measured\nFull cohort: not run', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
