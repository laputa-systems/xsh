#!/usr/bin/env python3
"""Observe frozen sources with the corrected immutable baseline products."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[4]
BENCH = ROOT / 'bench/typing'
OUT = Path(__file__).resolve().parent
BASELINE = Path('/Users/josh/d/laputa-systems/xsh-typing-baseline-v2')
OLD_ROOT = '/Users/josh/d/laputa-systems/xsh-typing-campaign'
REVISION = 'd6f09bc54305515b4c34d3b872d7f9f13b874061'
TIMEOUT = 60
EMPTY = hashlib.sha256(b'').hexdigest()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_text())


def save(path, data):
    temp = path.with_suffix('.tmp')
    temp.write_text(json.dumps(data, indent=2) + '\n')
    temp.replace(path)


def normalized(text):
    return text.replace(str(ROOT), '{checkout}').replace(OLD_ROOT, '{checkout}')


PRODUCTS = {name: {'path': str(BASELINE / 'target/release' / name),
                   'sha256': sha((BASELINE / 'target/release' / name).read_bytes())}
            for name in ['xsh', 'xsht']}
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=BASELINE).decode().strip() == REVISION
SELECTION = load(BENCH / 'cohort-selection.json')
MANIFEST = load(BENCH / 'manifest.json')
SEMANTIC = load(BENCH / 'semantic-cases.json')
OLD_FRONTEND = load(BENCH / 'history/877a114d/baseline-frontend-preflight.json')
OLD_EXTRA = load(BENCH / 'history/877a114d/baseline-runtime-extra-preflight.json')
FROZEN = {item['path']: item['sha256'] for item in MANIFEST['source']['files']}
FROZEN_SEMANTIC = {case['source']: case['source_sha256'] for case in SEMANTIC['cases']}
ANNOTATIONS_HASH = sha((BENCH / 'annotations.json').read_bytes())
PROVENANCE = {
    'baseline_commit': REVISION,
    'candidate_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT).decode().strip(),
    'frozen_source_snapshot_commit': SELECTION['snapshot_commit'],
    'binary_identities': PRODUCTS,
    'build_settings': {'toolchain': 'nightly-2026-09-15', 'profile': 'release',
                       'default_features': ['native-tests', 'net', 'tools'],
                       'build_observation': 'Products supplied after successful optimized build; this verifier performs no build.'},
    'timeout_seconds_per_process': TIMEOUT,
    'annotation_denominators': MANIFEST['annotation_denominators'],
    'frozen_annotation_inventory_sha256': ANNOTATIONS_HASH,
    'source_closure_sha256': SELECTION['source_closure_sha256'],
    'original_unique_source_count': SELECTION['original_unique_source_count'],
    'original_unique_bytes': SELECTION['original_unique_bytes'],
}


def unchanged():
    errors = []
    for path, expected in FROZEN.items():
        if sha((BENCH / path).read_bytes()) != expected:
            errors.append(path)
    for path, expected in FROZEN_SEMANTIC.items():
        if sha((ROOT / path).read_bytes()) != expected:
            errors.append(path)
    for name, identity in PRODUCTS.items():
        if sha(Path(identity['path']).read_bytes()) != identity['sha256']:
            errors.append('binary:' + name)
    if sha((BENCH / 'annotations.json').read_bytes()) != ANNOTATIONS_HASH:
        errors.append('annotations.json')
    return errors


assert not unchanged(), unchanged()


def environment(cwd):
    return {'XSH_MODULE_PATH': str(cwd) + ':' + str(cwd / 'dev'),
            'CARGO_BIN_EXE_xsh': PRODUCTS['xsh']['path'],
            'CARGO_BIN_EXE_xsht': PRODUCTS['xsht']['path'],
            'PATH': str(BASELINE / 'target/release') + ':/usr/bin:/bin', 'NO_COLOR': '1'}


def execute(group, name, command, cwd, stdin=''):
    overlay = environment(cwd)
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    start = time.perf_counter_ns()
    process = subprocess.Popen(command, cwd=cwd, env={**os.environ, **overlay},
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, start_new_session=True)
    timeout = False
    try:
        stdout, stderr = process.communicate(stdin.encode(), timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        timeout = True
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
    directory = OUT / group
    directory.mkdir(parents=True, exist_ok=True)
    row = {'command': command, 'cwd': str(cwd), 'environment_overlay': overlay,
           'started_utc': started, 'elapsed_ns': time.perf_counter_ns() - start,
           'exit_status': process.returncode, 'timeout': timeout,
           'children_killed_and_reaped_on_timeout': timeout,
           'stdin_sha256': sha(stdin.encode())}
    for stream, data in [('stdout', stdout), ('stderr', stderr)]:
        path = directory / (name + '.' + stream)
        path.write_bytes(data)
        row[stream + '_raw'] = {'path': str(path.relative_to(ROOT)), 'bytes': len(data), 'sha256': sha(data)}
        row[stream] = data.decode('utf-8', errors='replace')
        row[stream + '_sha256'] = sha(data)
    row['wall_seconds'] = row['elapsed_ns'] / 1e9
    print(group, name, process.returncode, f"{row['wall_seconds']:.3f}s", flush=True)
    return row


frontend = {'schema_version': 1, 'source_revision': REVISION, 'binary_identities': PRODUCTS, 'provenance': PROVENANCE, 'rows': [], 'original_rows': [], 'comparisons': []}
old_rows = {row['id']: row for row in OLD_FRONTEND['rows']}
for workload in SELECTION['workloads']:
    pair = []
    for variant in ['original', 'stabilized']:
        cwd = BENCH / 'cohort' / variant
        source = cwd / workload['entry']
        row = execute('frontend', workload['id'] + '-' + variant,
                      [PRODUCTS['xsht']['path'], 'check', str(source)], cwd)
        row.update(id=workload['id'], source=workload['entry'], variant=variant,
                   source_sha256=sha(source.read_bytes()))
        expected = old_rows[workload['id']]
        row['expected_exit_status'] = expected['exit_status']
        row['preserved_signature'] = (not row['timeout'] and row['exit_status'] == expected['exit_status']
                                     and row['stdout'] == expected['stdout']
                                     and normalized(row['stderr']) == normalized(expected['stderr']).replace('/stabilized/', '/' + variant + '/'))
        row['status'] = 'passed' if row['preserved_signature'] else 'failed'
        frontend['rows' if variant == 'stabilized' else 'original_rows'].append(row)
        pair.append(row)
    frontend['comparisons'].append({'id': workload['id'], 'status': 'passed' if all(r['preserved_signature'] for r in pair) else 'failed',
                                    'original_stabilized_signatures_equal': pair[0]['exit_status'] == pair[1]['exit_status']
                                    and pair[0]['stdout'] == pair[1]['stdout']
                                    and normalized(pair[0]['stderr']).replace('/original/', '/{variant}/') == normalized(pair[1]['stderr']).replace('/stabilized/', '/{variant}/')})
    save(OUT / 'baseline-frontend-preflight.json', frontend)
frontend['status'] = 'passed' if all(r['preserved_signature'] for r in frontend['rows'] + frontend['original_rows']) else 'failed'
frontend['counts'] = {'roots_per_variant': 23, 'valid_per_variant': 22, 'intentional_invalid_per_variant': 1, 'observations': 46}
save(OUT / 'baseline-frontend-preflight.json', frontend)
save(BENCH / 'baseline-frontend-preflight.json', frontend)

runtime = {'schema_version': 1, 'source_revision': REVISION, 'binary_identities': PRODUCTS, 'provenance': PROVENANCE, 'rows': [], 'comparisons': []}
runtime_ids = {row['id'] for row in load(BENCH / 'history/877a114d/baseline-runtime-preflight.json')['rows']}
for workload in [w for w in MANIFEST['workloads'] if w['kind'] == 'runtime' and w['id'] in runtime_ids]:
    entry = str(Path(workload['source']).relative_to('cohort/stabilized'))
    pair = []
    for variant in ['original', 'stabilized']:
        cwd = BENCH / 'cohort' / variant
        source = cwd / entry
        values = {'xsh': PRODUCTS['xsh']['path'], 'source': str(source), 'root': str(BENCH)}
        command = [part.format(**values) for part in workload['commandTemplate']]
        row = execute('runtime', workload['id'] + '-' + variant, command, cwd, workload.get('stdin_utf8', ''))
        row.update(id=workload['id'], source=entry, variant=variant, source_sha256=sha(source.read_bytes()))
        expected = workload['expected']
        row['preserved_signature'] = not row['timeout'] and row['exit_status'] == expected['exit_status'] and all(row[k] == expected[k] for k in ['stdout_sha256', 'stderr_sha256'])
        row['expected'] = expected
        row['status'] = 'passed' if row['preserved_signature'] else 'failed'
        runtime['rows'].append(row); pair.append(row)
    runtime['comparisons'].append({'id': workload['id'], 'status': 'passed' if all(r['preserved_signature'] for r in pair) else 'failed'})
    save(OUT / 'baseline-runtime-preflight.json', runtime)
runtime['status'] = 'passed' if all(r['preserved_signature'] for r in runtime['rows']) else 'failed'
save(OUT / 'baseline-runtime-preflight.json', runtime)
save(BENCH / 'baseline-runtime-preflight.json', runtime)

extra = {'schema_version': 1, 'source_revision': REVISION, 'provenance': PROVENANCE, 'binary_identities': PRODUCTS,
         'timeout_seconds_per_process': TIMEOUT, 'scope': 'Frozen cohort supplemental native observations and tiny tokei fixture',
         'compiler_source_changed': False, 'frozen_cohort_sources_or_denominators_changed': False,
         'cli_contract': OLD_EXTRA['cli_contract'], 'selected_native_cases': OLD_EXTRA['selected_native_cases'],
         'observation_workloads': MANIFEST['observation_workloads'], 'workloads': OLD_EXTRA['workloads'],
         'runs': [], 'comparisons': [], 'unresolved_failures': []}
for workload in MANIFEST['observation_workloads']:
    pair = []
    for variant in ['original', 'stabilized']:
        cwd = BENCH / 'cohort' / variant
        source = cwd / str(Path(workload['source']).relative_to('cohort/stabilized'))
        command = [part.format(xsht=PRODUCTS['xsht']['path']) for part in workload['commandTemplate']]
        row = execute('native', workload['id'] + '-' + variant, command, cwd)
        expected = workload['expected']
        passed = not row['timeout'] and row['exit_status'] == 0 and row['stderr_sha256'] == EMPTY and re.fullmatch(expected['stdout_pattern'], row['stdout']) is not None
        row.update(workload_id=workload['id'], variant=variant, kind='native-compatibility-observation',
                   status='passed' if passed else 'failed', source_sha256=sha(source.read_bytes()), expected=expected)
        extra['runs'].append(row); pair.append(row)
    extra['comparisons'].append({'workload_id': workload['id'], 'status': 'passed' if all(r['status'] == 'passed' for r in pair) else 'failed',
                                 'oracle': 'Exactly one selected native test passes with no stderr; native assertions own semantic observations.'})
    save(OUT / 'baseline-runtime-extra-preflight.json', extra)

tokei_rows = []
for variant in ['original', 'stabilized']:
    cwd = BENCH / 'cohort' / variant
    source = cwd / 'showcase/tokei.xsh'
    row = execute('runtime-extra', 'tokei-tiny-' + variant,
                  [PRODUCTS['xsh']['path'], str(source), '--', '--json', str(BENCH / 'runtime-fixtures/tokei-input')], cwd)
    historical = next(r for r in reversed(OLD_EXTRA['runs']) if r['workload_id'] == 'showcase-tokei-tiny' and r['variant'] == variant and r['status'] == 'passed')
    historical_bytes = (ROOT / historical['stdout']['path']).read_bytes().decode()
    matches = normalized(row['stdout']) == normalized(historical_bytes)
    decoded = json.loads(row['stdout']) if row['exit_status'] == 0 else None
    row.update(workload_id='showcase-tokei-tiny', variant=variant, kind='runtime-program', source_sha256=sha(source.read_bytes()),
               status='passed' if matches and row['exit_status'] == 0 and row['stderr_sha256'] == EMPTY and not row['timeout'] else 'failed',
               historical_stdout_sha256=historical['stdout']['sha256'], checkout_path_normalized_historical_bytes_equal=matches,
               oracle='Frozen accepted historical bytes after replacing only the checkout root embedded in report filenames.', decoded_stdout=decoded)
    extra['runs'].append(row); tokei_rows.append(row)
extra['comparisons'].append({'workload_id': 'showcase-tokei-tiny', 'status': 'passed' if all(r['status'] == 'passed' for r in tokei_rows) else 'failed',
                             'original_stabilized_stdout_equal': tokei_rows[0]['stdout'] == tokei_rows[1]['stdout'],
                             'semantic_assertions': OLD_EXTRA['workloads'][0]['semantic_assertions']})
extra['unresolved_failures'] = [r['workload_id'] + ':' + r['variant'] for r in extra['runs'] if r['status'] != 'passed']
extra['status'] = 'passed' if not extra['unresolved_failures'] else 'failed'
extra['accepted_native_runs'] = sum(r['status'] == 'passed' for r in extra['runs'] if r['kind'] == 'native-compatibility-observation')
extra['accepted_tokei_runs'] = sum(r['status'] == 'passed' for r in tokei_rows)
save(OUT / 'baseline-runtime-extra-preflight.json', extra)
save(BENCH / 'baseline-runtime-extra-preflight.json', extra)

semantic = {'schema_version': 1, 'source_revision': REVISION, 'binary_identities': PRODUCTS, 'provenance': PROVENANCE, 'cases': [], 'future_target_policy': 'Intentional acceptance and rejection cases remain candidate-pending even if their old-baseline rejection is reproduced.'}
for case in SEMANTIC['cases']:
    source = ROOT / case['source']
    run = execute('semantic', case['id'] + '-run', [PRODUCTS['xsh']['path'], str(source)], ROOT)
    check = execute('semantic', case['id'] + '-check', [PRODUCTS['xsht']['path'], 'check', str(source)], ROOT)
    old = case['baseline_observed']
    reproduced = not run['timeout'] and not check['timeout'] and run['exit_status'] == old['status'] and run['stdout'] == old['stdout'] and normalized(run['stderr']) == normalized(old['stderr']) and check['exit_status'] == old['check_status'] and normalized(check['stderr']) == normalized(old['check_stderr'])
    semantic['cases'].append({'id': case['id'], 'source': case['source'], 'source_sha256': sha(source.read_bytes()), 'contract': case['contract'],
                              'baseline_observation_reproduced': reproduced,
                              'target_status': ('baseline-preserved-pass' if reproduced else 'baseline-preserved-failed') if case['contract'] == 'preserved' else 'candidate-pending',
                              'baseline_status': 'passed' if reproduced else 'failed', 'run': run, 'check': check})
    save(OUT / 'baseline-semantic-matrix-preflight-v2.json', semantic)
semantic['counts'] = {'cases': len(semantic['cases']), 'preserved': sum(c['contract'] == 'preserved' for c in semantic['cases']),
                      'candidate_pending': sum(c['target_status'] == 'candidate-pending' for c in semantic['cases']),
                      'baseline_observations_reproduced': sum(c['baseline_observation_reproduced'] for c in semantic['cases'])}
semantic['status'] = 'passed' if all(c['baseline_observation_reproduced'] for c in semantic['cases']) else 'failed'
native = execute('native', 'typing-inference-preserved-module', [PRODUCTS['xsht']['path'], 'test', '--jobs', '1', 'tests/xsh/typing-inference-preserved.xsh::'], ROOT)
semantic['preserved_native_module'] = native
semantic['preserved_native_module_passed'] = native['exit_status'] == 0 and native['stderr_sha256'] == EMPTY and '24 passed; 0 failed; 0 skipped' in native['stdout']
semantic['frozen_inputs_changed'] = unchanged()
save(OUT / 'baseline-semantic-matrix-preflight-v2.json', semantic)
save(BENCH / 'baseline-semantic-matrix-preflight-v2.json', semantic)
result = {'schema_version': 1, 'provenance': PROVENANCE,
          'statuses': {name: data['status'] for name, data in [('frontend', frontend), ('runtime', runtime), ('runtime_extra', extra), ('semantic', semantic)]},
          'preserved_native_module_passed': semantic['preserved_native_module_passed'], 'frozen_inputs_changed': unchanged(),
          'completed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}
result['status'] = 'passed' if all(s == 'passed' for s in result['statuses'].values()) and result['preserved_native_module_passed'] and not result['frozen_inputs_changed'] else 'failed'
save(OUT / 'verification.json', result)
print(json.dumps(result, indent=2), flush=True)
