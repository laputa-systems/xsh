#!/usr/bin/env python3
"""Reconcile relocated diagnostic identities without changing raw observations."""
import datetime
import hashlib
import json
from pathlib import Path

OUT = Path(__file__).resolve().parent
ROOT = OUT.parents[3]
BENCH = ROOT / 'bench/typing'
OLD_ROOT = '/Users/josh/d/laputa-systems/xsh-typing-campaign'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_text())


def save(path, data):
    temp = path.with_suffix('.tmp')
    temp.write_text(json.dumps(data, indent=2) + '\n')
    temp.replace(path)


semantic = load(OUT / 'baseline-semantic-matrix-preflight-v2.json')
matrix = load(BENCH / 'semantic-cases.json')
products = semantic['binary_identities']
helpers = {path.name: {'path': str(path.relative_to(ROOT)), 'sha256': sha(path.read_bytes())}
           for path in [OUT / 'verify.py', Path(__file__).resolve()]}
identity_replacements = {str(ROOT): '{checkout}', OLD_ROOT: '{checkout}'}
for name in ['xsh', 'xsht']:
    identity_replacements[products[name]['path']] = '{product:' + name + '}'
    identity_replacements[matrix['baseline_products'][name]] = '{product:' + name + '}'


def normalize(text):
    for original, replacement in identity_replacements.items():
        text = text.replace(original, replacement)
    return text


old_cases = {case['id']: case for case in matrix['cases']}
for case in semantic['cases']:
    old = old_cases[case['id']]['baseline_observed']
    run, check = case['run'], case['check']
    for observation in [run, check]:
        for stream in ['stdout', 'stderr']:
            raw = (ROOT / observation[stream + '_raw']['path']).read_bytes()
            assert sha(raw) == observation[stream + '_sha256']
            assert raw.decode('utf-8', errors='replace') == observation[stream]
    reproduced = (not run['timeout'] and not check['timeout']
                  and run['exit_status'] == old['status'] and run['stdout'] == old['stdout']
                  and normalize(run['stderr']) == normalize(old['stderr'])
                  and check['exit_status'] == old['check_status']
                  and normalize(check['stderr']) == normalize(old['check_stderr']))
    case['initial_checkout_only_comparison_reproduced'] = case['baseline_observation_reproduced']
    case['baseline_observation_reproduced'] = reproduced
    case['baseline_status'] = 'passed' if reproduced else 'failed'
    case['target_status'] = ('baseline-preserved-pass' if reproduced else 'baseline-preserved-failed') if case['contract'] == 'preserved' else 'candidate-pending'
semantic['comparison_identity_replacements'] = identity_replacements
semantic['comparison_note'] = 'Only explicit checkout and product path identities are normalized; raw diagnostic bytes, operation, error, source positions, and call path are retained.'
semantic['counts']['baseline_observations_reproduced'] = sum(c['baseline_observation_reproduced'] for c in semantic['cases'])
assert len(semantic['cases']) == 38 and semantic['counts']['preserved'] == 24 and semantic['counts']['candidate_pending'] == 14
semantic['status'] = 'passed' if all(c['baseline_observation_reproduced'] for c in semantic['cases']) and semantic['preserved_native_module_passed'] else 'failed'

manifest = load(BENCH / 'manifest.json')
changed = [item['path'] for item in manifest['source']['files']
           if sha((BENCH / item['path']).read_bytes()) != item['sha256']]
changed += [case['source'] for case in matrix['cases']
            if sha((ROOT / case['source']).read_bytes()) != case['source_sha256']]
binary_unchanged = all(sha(Path(product['path']).read_bytes()) == product['sha256'] for product in products.values())
annotation_unchanged = sha((BENCH / 'annotations.json').read_bytes()) == semantic['provenance']['frozen_annotation_inventory_sha256']
assert not changed and binary_unchanged and annotation_unchanged

extra = load(OUT / 'baseline-runtime-extra-preflight.json')
tokei = [run for run in extra['runs'] if run['workload_id'] == 'showcase-tokei-tiny']
assert len(tokei) == 2 and tokei[0]['stdout_sha256'] == tokei[1]['stdout_sha256']
assert all(run['checkout_path_normalized_historical_bytes_equal'] for run in tokei)
extra['workloads'][0]['historical_stdout_sha256'] = extra['workloads'][0]['expected']['stdout_sha256']
extra['workloads'][0]['expected']['stdout_sha256'] = tokei[0]['stdout_sha256']
extra['workloads'][0]['expectation_provenance'] = 'Fresh raw bytes verified against frozen accepted historical bytes after replacing only the checkout root in reported filenames.'

summaries = {
    'baseline-frontend-preflight.json': load(OUT / 'baseline-frontend-preflight.json'),
    'baseline-runtime-preflight.json': load(OUT / 'baseline-runtime-preflight.json'),
    'baseline-runtime-extra-preflight.json': extra,
    'baseline-semantic-matrix-preflight-v2.json': semantic,
}
completed = datetime.datetime.now(datetime.timezone.utc).isoformat()
for name, data in summaries.items():
    data.update(verification_helpers=helpers, inputs_unchanged=True, binary_unchanged=True,
                frozen_annotation_inventory_unchanged=True, completed_utc=completed)
    save(OUT / name, data)
    save(BENCH / name, data)
initial = load(OUT / 'verification.json')
save(OUT / 'verification-initial.json', initial)
result = {**initial, 'statuses': {name: data['status'] for name, data in summaries.items()},
          'verification_helpers': helpers, 'completed_utc': completed,
          'initial_comparison': {'status': initial['status'], 'reason': 'Propagation traceback executable field used the explicit relocated baseline product path.'}}
result['status'] = 'passed' if all(data['status'] == 'passed' for data in summaries.values()) and not changed and binary_unchanged and annotation_unchanged else 'failed'
save(OUT / 'verification.json', result)
print(json.dumps({'status': result['status'], 'semantic_counts': semantic['counts'],
                  'native_runs': extra['accepted_native_runs'], 'tokei_runs': extra['accepted_tokei_runs'],
                  'preserved_native_tests': 24, 'frozen_inputs_changed': changed}, indent=2))
