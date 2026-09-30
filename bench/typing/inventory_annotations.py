#!/usr/bin/env python3
"""Freeze parser-owned annotation sites and preserve exact source correspondence."""
import argparse
from collections import Counter
import difflib
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def invoke(helper, args, cwd):
    environment = dict(os.environ)
    environment.pop('XSH_MODULE_PATH', None)
    result = subprocess.run([str(helper), *map(str, args)], cwd=cwd, env=environment,
                            capture_output=True, text=True, timeout=120)
    if result.returncode:
        raise RuntimeError(f'Parser helper failed ({result.returncode}): {args}\n{result.stderr}')
    return [json.loads(line) for line in result.stdout.splitlines()]


def build_helper(baseline_target, repo, output):
    metadata = list(baseline_target.rglob('libxsh-*.rmeta'))
    if len(metadata) != 1:
        raise RuntimeError(f'Expected one immutable baseline xsh metadata artifact: {metadata}')
    library = metadata[0].with_suffix('.rlib')
    command = ['rustc', '--edition=2024', '-Clto=thin', str(repo / 'bench/typing/inventory_annotations.rs'),
               '--extern', 'xsh=' + str(metadata[0]), '--extern', 'xsh=' + str(library), '-o', str(output)]
    dependency_dirs = {item.parent for extension in ('*.rmeta', '*.rlib', '*.dylib')
                       for item in baseline_target.rglob(extension)}
    for directory in sorted(dependency_dirs):
        command.extend(['-L', 'dependency=' + str(directory)])
    for directory in sorted((baseline_target / 'build').rglob('out')):
        command.extend(['-L', 'native=' + str(directory)])
    result = subprocess.run(command, cwd=repo, capture_output=True, text=True, timeout=120)
    if result.returncode:
        raise RuntimeError(f'Helper build failed ({result.returncode}):\n{result.stderr[-4000:]}')
    return {'rustc_command': command, 'helper_sha256': sha256(output.read_bytes()),
            'baseline_library_sha256': sha256(library.read_bytes()),
            'baseline_metadata_sha256': sha256(metadata[0].read_bytes())}


def classify(site, source, fact):
    category = site['category']
    contract_categories = {'schema-field', 'error-field', 'type-alias', 'module-contract-parameter',
                           'module-contract-value', 'module-contract-return', 'module-contract-effect', 'validation-target', 'signal-hook-effect'}
    if category in contract_categories:
        return 'protected', 'schema-or-module-contract', 'A type definition or module/error field declares a schema, nominal payload, or promised interface.'
    if site['exported'] or site.get('callable_kind') == 'cli-main' or site.get('callable_name') == 'main':
        return 'protected', 'public-or-entry-promise', 'The written exported declaration or command entry signature is a caller-visible promise.'
    if category == 'effect' and site['empty']:
        return 'protected', 'explicit-empty-effect-bound', 'The written empty effect clause forbids host/error effects independently of the implementation.'
    if site['source'].startswith('tests/fixtures/'):
        return 'protected', 'annotation-fixture', 'This annotated positive/negative checker fixture owns explicit boundary coverage.'
    if category != 'effect' and site['source'] in {'tests/xsh/local-inference.xsh', 'tests/xsh/result-contexts.xsh'}:
        return 'protected', 'annotation-fixture', 'This focused inference/Result regression module preserves written-form and contextual semantic coverage.'
    text = site['text'].strip()
    if category in {'parameter', 'return', 'producer-item'} and (text == 'Any' or text == 'Record'):
        return 'protected', 'explicit-dynamic-boundary', 'The declared erased callable boundary permits dynamic values independently of a narrower implementation.'
    if category == 'parameter' and 'UInt' in text:
        return 'protected', 'domain-range-promise', 'The required unsigned parameter domain excludes negative or out-of-range caller values.'
    if category == 'return' and text in {'Unit', 'Result[Unit]', 'Result[Unit, Error]'}:
        return 'protected', 'statement-result-semantics', 'An explicit consuming Unit or Result[Unit] body fixes assertion, propagation, and success behavior.'
    if category == 'return' and text.startswith('Result[') and fact:
        completion_types = [fact.get('tail_expression_type')] + [value['type'] for value in fact.get('explicit_completions', [])]
        if (any(value and not value.startswith('Result[') for value in completion_types)
                and not fact.get('outward_propagation_spans')
                and not any(value and value.startswith('Result[') for value in completion_types)):
            return 'protected', 'implicit-result-wrapping', 'Baseline checked completion has a non-Result payload with no independently checked outward propagation or Result-valued completion; the written Result boundary supplies success wrapping.'
    if category == 'local':
        start, end = site['initializer_span']
        initializer = source[start:end].decode('utf-8').strip()
        if text.startswith('Map[') and site.get('initializer_kind') == 'record-literal':
            return 'protected', 'record-map-classification', 'The written Map expectation classifies a brace initializer as a Map instead of a record.'
        if 'UInt' in text and site.get('initializer_kind') == 'integer-literal':
            return 'protected', 'unsigned-storage-domain', 'The explicit unsigned storage annotation checks the literal and later writes against an unsigned range.'
        if text in {'Any', 'Record'} and fact and fact.get('initializer_type') not in {None, text}:
            return 'protected', 'explicit-dynamic-erasure', 'This written local erasure explicitly broadens a concrete value to a dynamic boundary.'
        if text == 'Float' and site.get('initializer_kind') == 'integer-literal':
            return 'protected', 'numeric-context-conversion', 'The Float expectation selects the numeric representation of an integer literal.'
    return 'eligible', 'ordinary-internal-annotation', 'No independent public, domain, schema, effect-bound, fixture, or semantic purpose is established; inference difficulty does not exclude this site.'


def prepare(repo, baseline_source, helper, build_record):
    bench = repo / 'bench/typing'
    selection = json.loads((bench / 'cohort-selection.json').read_text())
    if selection['status'].startswith('frozen'):
        raise RuntimeError('Frozen cohorts are immutable; run --verify instead of regeneration.')
    original = bench / 'cohort/original'
    stabilized = bench / 'cohort/stabilized'
    parsed = {}
    pending = {workload['entry'] for workload in selection['workloads']}
    while pending:
        relative = sorted(pending)[0]
        pending.remove(relative)
        path = repo / relative
        snapshot = baseline_source / relative
        raw = path.read_bytes()
        if raw != snapshot.read_bytes():
            raise RuntimeError(f'Candidate source differs from immutable baseline: {relative}')
        target = original / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists() and target.read_bytes() != raw:
            raise RuntimeError(f'Frozen original differs: {relative}')
        target.write_bytes(raw)
        row = invoke(helper, [path], repo)[0]
        if row['parser_diagnostics']:
            raise RuntimeError(f'Unexpected source parser diagnostics: {relative}: {row["parser_diagnostics"]}')
        for imported in row['imports']:
            resolved = imported['resolved']
            if resolved:
                imported['resolved'] = Path(resolved).resolve().relative_to(repo).as_posix()
                if imported['resolved'] not in parsed:
                    pending.add(imported['resolved'])
        row['file'] = relative
        parsed[relative] = row
    def closure(relative):
        seen = set()
        def visit(path):
            if path in seen:
                return
            seen.add(path)
            for imported in parsed[path]['imports']:
                if imported['resolved']:
                    visit(imported['resolved'])
        visit(relative)
        return sorted(seen)
    for workload in selection['workloads']:
        workload['closure'] = closure(workload['entry'])
        workload['closure_status'] = 'resolved-by-baseline-loader'
        workload['frontend'] = 'expected-rejection' if workload['id'] == 'invalid-annotated' else 'valid-or-baseline-diagnostic-audited'
        workload['runtime_platform'] = 'not-applicable-macos-live-host' if workload['id'] in {'core-ifup', 'core-ifdown-producer', 'system-report', 'system-report-live', 'system-report-collect'} else 'host-safe-observation-or-native-harness'
    save(bench / 'cohort/structural-inventory.json', {'schema_version': 1, 'parser': 'immutable-baseline frontend facade Parser/AstArena/LazyCst/resolve_user_module', 'sources': list(parsed.values())})
    facts = {}
    binding_facts = {}
    fact_runs = []
    for workload in selection['workloads']:
        result = invoke(helper, ['--facts', original / workload['entry'], original, original / 'dev'], repo)[0]
        result['file'] = workload['entry']
        for fact in result['callables']:
            path = Path(fact['file'])
            try:
                relative = path.resolve().relative_to(original).as_posix()
            except ValueError:
                continue
            fact['file'] = relative
            key = (relative, tuple(fact['body_span']))
            normalized = json.loads(json.dumps(fact).replace(str(original / relative) + '.', ''))
            if key in facts and facts[key] != normalized:
                raise RuntimeError(f'Inconsistent normalized baseline checked facts across shared import closure: {key}')
            facts[key] = normalized
        result['callables'] = [f for f in result['callables'] if f.get('file') in parsed]
        for binding in result['bindings']:
            try:
                relative = Path(binding['file']).resolve().relative_to(original).as_posix()
            except ValueError:
                continue
            binding['file'] = relative
            key = (relative, tuple(binding['span']))
            normalized = json.loads(json.dumps(binding).replace(str(original / relative) + '.', ''))
            if key in binding_facts and binding_facts[key] != normalized:
                raise RuntimeError(f'Inconsistent normalized baseline binding facts: {key}')
            binding_facts[key] = normalized
        result['bindings'] = [b for b in result['bindings'] if b.get('file') in parsed]
        fact_runs.append(result)
    save(bench / 'cohort/baseline-signatures.json', {'schema_version': 1, 'runs': fact_runs})
    patches = []
    stabilizers = []
    correspondence = []
    sources = []
    sites = []
    for relative, row in sorted(parsed.items()):
        raw = (original / relative).read_bytes()
        text = raw.decode('utf-8')
        additions = []
        for declaration in row['declarations']:
            if declaration['kind'] not in {'proc', 'cli-main'} or not declaration['omitted_return']:
                continue
            fact = facts.get((relative, tuple(declaration['body_span'])))
            if not fact or fact['return_type'] not in {'Result[Unit]', 'Result[Unit, Error]'}:
                raise RuntimeError(f'No checked historical omitted-proc Result[Unit] fact for {relative} {declaration["owner"]}: {fact}')
            position = declaration['body_span'][0]
            addition = '-> Result[Unit] '
            additions.append((position, addition, declaration))
        rewritten = raw
        for position, addition, _ in sorted(additions, reverse=True):
            rewritten = rewritten[:position] + addition.encode('utf-8') + rewritten[position:]
        target = stabilized / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(rewritten)
        def offset(position):
            return position + sum(len(addition.encode('utf-8')) for at, addition, _ in additions if at <= position)
        for position, addition, declaration in sorted(additions):
            insertion = position + sum(len(value.encode('utf-8')) for at, value, _ in additions if at < position)
            stabilizers.append({'id': f'{relative}:stabilizer:{position}', 'source': relative, 'owner': declaration['owner'],
                                'original_insertion_byte': position, 'stabilized_span': [insertion + 3, insertion + 15],
                                'text': 'Result[Unit]', 'classification': 'protected', 'reason': 'historical-omitted-proc-success',
                                'evidence': 'The baseline checked return is Result[Unit]; a written return preserves assertion, propagation, and success convention after omitted-return inference changes.'})
        for site in row['sites']:
            site['source'] = relative
            site['source_sha256'] = sha256(raw)
            site['id'] = f'{relative}:{site["span"][0]}:{site["span"][1]}:{site["category"]}'
            owner_fact = binding_facts.get((relative, tuple(site['span']))) if site['category'] == 'local' else facts.get((relative, tuple(site.get('body_span', []))))
            classification, reason, evidence = classify(site, raw, owner_fact)
            site['classification'] = classification
            site['reason'] = reason
            site['evidence'] = evidence
            site['contract_owner'] = f'{relative}:{site["owner"]}'
            site['stabilized_span'] = [offset(v) for v in site['span']]
            site['joint_removal'] = 'not-run'
            sites.append(site)
        correspondence.append({'source': relative, 'original_sha256': sha256(raw), 'stabilized_sha256': sha256(rewritten),
                               'original_bytes': len(raw), 'stabilized_bytes': len(rewritten),
                               'insertions': [{'original_byte': p, 'utf8_text': a, 'owner': d['owner']} for p, a, d in sorted(additions)]})
        sources.append({'path': relative, 'original_sha256': sha256(raw), 'stabilized_sha256': sha256(rewritten),
                        'original_bytes': len(raw), 'stabilized_bytes': len(rewritten),
                        'imports': row['imports'], 'workload_ids': [w['id'] for w in selection['workloads'] if relative in w['closure']]})
        if rewritten != raw:
            patches.extend(difflib.unified_diff(text.splitlines(True), rewritten.decode('utf-8').splitlines(True), fromfile='original/' + relative, tofile='stabilized/' + relative))
    (bench / 'cohort/stabilization.patch').write_text(''.join(patches))
    save(bench / 'cohort/source-correspondence.json', {'schema_version': 1, 'sources': correspondence})
    counts = Counter((s['classification'], s['category']) for s in sites)
    internal_categories = {'parameter', 'return', 'producer-item', 'effect'}
    denominators = {'eligible_local': counts['eligible', 'local'],
                    'eligible_internal': sum(counts['eligible', c] for c in internal_categories),
                    'eligible_parameters': counts['eligible', 'parameter'], 'eligible_returns': counts['eligible', 'return'],
                    'eligible_producer_items': counts['eligible', 'producer-item'], 'eligible_effects': counts['eligible', 'effect']}
    annotations = {'schema_version': 1, 'status': 'classification-pending-stabilized-parser-and-fact-reconciliation',
                   'snapshot_commit': selection['snapshot_commit'], 'site_identity': 'original source path + SHA256 + parser syntactic byte span + category',
                   'counting_policy': {'shared_imports': 'one source site once, regardless of closure multiplicity',
                                       'effect_clauses': 'one explicit syntactic clause once, regardless of effect names',
                                       'fixture_program_strings': 'excluded automatically by parser AST ownership',
                                       'stabilizers': 'protected separately, excluded from original eligible denominators',
                                       'implicit_default_types': 'not explicit annotation sites',
                                       'schemas': 'protected field/type-definition count separate from local/internal eligible targets'},
                   'denominators': denominators, 'original_site_count': len(sites), 'original_source_count': len(parsed),
                   'counts_by_classification_category': [{'classification': a, 'category': b, 'count': n} for (a,b),n in sorted(counts.items())],
                   'protected_by_reason': dict(sorted(Counter(s['reason'] for s in sites if s['classification'] == 'protected').items())),
                   'sites': sites, 'stabilizers': stabilizers, 'source_correspondence': 'cohort/source-correspondence.json',
                   'joint_removal_results': None, 'retained_eligible_explanations': None, 'build': build_record}
    save(bench / 'annotations.json', annotations)
    selection['sources'] = sources
    selection['original_unique_source_count'] = len(sources)
    selection['original_unique_bytes'] = sum(s['original_bytes'] for s in sources)
    selection['status'] = 'source-closure-frozen-annotations-pending-reconciliation'
    selection['module_roots'] = ['.', 'dev']
    selection['module_roots_authority'] = 'xsht-config.ini module_path'
    selection['source_closure_sha256'] = sha256(json.dumps(sources, sort_keys=True, separators=(',', ':')).encode('utf-8'))
    save(bench / 'cohort-selection.json', selection)
    print(json.dumps({'source_count': len(sources), 'source_bytes': selection['original_unique_bytes'], 'site_count': len(sites), 'stabilizers': len(stabilizers), 'denominators_pending': denominators}))



def reconcile(repo, helper):
    bench = repo / 'bench/typing'
    selection = json.loads((bench / 'cohort-selection.json').read_text())
    annotations = json.loads((bench / 'annotations.json').read_text())
    correspondence = json.loads((bench / 'cohort/source-correspondence.json').read_text())
    originals = bench / 'cohort/original'
    stabilized = bench / 'cohort/stabilized'
    source_map = {source['source']: source for source in correspondence['sources']}
    original_runs = json.loads((bench / 'cohort/baseline-signatures.json').read_text())['runs']
    def original_offset(relative, position):
        shift = 0
        for insertion in source_map[relative]['insertions']:
            at = insertion['original_byte']
            size = len(insertion['utf8_text'].encode('utf-8'))
            if position >= at + shift + size:
                shift += size
            elif position > at + shift:
                raise RuntimeError(f'Existing fact entered inserted annotation: {relative} {position}')
        return position - shift
    def normalize_fact(fact, root, map_spans):
        fact = json.loads(json.dumps(fact))
        file = Path(fact['file'])
        relative = file.resolve().relative_to(root).as_posix() if file.is_absolute() else file.as_posix()
        fact['file'] = relative
        def visit(value, key=''):
            if isinstance(value, str):
                return value.replace(str(root) + '/', '<cohort>/')
            if isinstance(value, dict):
                return {name: visit(item, name) for name, item in value.items()}
            if isinstance(value, list):
                if map_spans and ('span' in key) and len(value) == 2 and all(isinstance(item, int) for item in value):
                    return [original_offset(relative, item) for item in value]
                return [visit(item, key) for item in value]
            return value
        return visit(fact)
    source_checks = []
    for relative, source in sorted(source_map.items()):
        raw = (originals / relative).read_bytes()
        rewritten = (stabilized / relative).read_bytes()
        if sha256(raw) != source['original_sha256'] or sha256(rewritten) != source['stabilized_sha256']:
            raise RuntimeError(f'Source hash mismatch: {relative}')
        parsed = invoke(helper, [stabilized / relative], stabilized)[0]
        if parsed['parser_diagnostics']:
            raise RuntimeError(f'Stabilized parser diagnostics: {relative}')
        expected = [(site['category'], site['owner'], tuple(site['stabilized_span']), site['text'])
                    for site in annotations['sites'] if site['source'] == relative]
        expected += [('return', item['owner'], tuple(item['stabilized_span']), item['text'])
                     for item in annotations['stabilizers'] if item['source'] == relative]
        actual = [(site['category'], site['owner'], tuple(site['span']), site['text']) for site in parsed['sites']]
        if Counter(actual) != Counter(expected):
            raise RuntimeError(f'Annotation correspondence mismatch: {relative}; extra={Counter(actual)-Counter(expected)} missing={Counter(expected)-Counter(actual)}')
        source_checks.append({'source': relative, 'status': 'passed', 'original_sites': len(expected)-len(source['insertions']), 'stabilizers': len(source['insertions'])})
    stable_runs = []
    root_checks = []
    for workload, original_run in zip(selection['workloads'], original_runs, strict=True):
        run = invoke(helper, ['--facts', stabilized / workload['entry'], stabilized, stabilized / 'dev'], repo)[0]
        run['file'] = workload['entry']
        for category in ['callables', 'bindings']:
            run[category] = [fact for fact in run[category] if Path(fact['file']).resolve().is_relative_to(stabilized)]
            original_facts = sorted([normalize_fact(fact, originals, False) for fact in original_run[category]], key=lambda fact:(fact['file'],fact.get('name',''),fact.get('span',fact.get('body_span',[]))))
            stable_facts = sorted([normalize_fact(fact, stabilized, True) for fact in run[category]], key=lambda fact:(fact['file'],fact.get('name',''),fact.get('span',fact.get('body_span',[]))))
            if category == 'callables':
                for original_fact, stable_fact in zip(original_facts,stable_facts,strict=True):
                    if original_fact['omitted_return'] != stable_fact['omitted_return']:
                        relative = original_fact['file']
                        insertion = original_fact['body_span'][0]
                        if not any(item['original_byte']==insertion for item in source_map[relative]['insertions']):
                            raise RuntimeError(f'Unexpected omitted-return change: {workload["id"]} {original_fact["name"]}')
                        stable_fact['omitted_return'] = original_fact['omitted_return']
            if original_facts != stable_facts:
                first = next(((a,b) for a,b in zip(original_facts,stable_facts) if a != b), None)
                raise RuntimeError(f'Baseline checked fact changed: {workload["id"]} {category}: {first}')
        original_effects = json.dumps(original_run['callable_effects'],sort_keys=True).replace(str(originals)+'/', '<cohort>/')
        stable_effects = json.dumps(run['callable_effects'],sort_keys=True).replace(str(stabilized)+'/', '<cohort>/')
        if original_effects != stable_effects:
            raise RuntimeError(f'Baseline callable effects changed: {workload["id"]}')
        original_diagnostics = json.dumps(original_run['diagnostics'],sort_keys=True).replace(str(originals)+'/', '<cohort>/')
        stable_diagnostics = json.dumps(run['diagnostics'],sort_keys=True).replace(str(stabilized)+'/', '<cohort>/')
        if original_diagnostics != stable_diagnostics:
            raise RuntimeError(f'Baseline diagnostics changed: {workload["id"]}')
        if workload['id'] != 'invalid-annotated' and run['diagnostics']:
            raise RuntimeError(f'Unexpected valid workload diagnostics: {workload["id"]}')
        stable_runs.append(run)
        root_checks.append({'workload_id': workload['id'], 'status': 'passed', 'check': 'baseline-original-stabilized-types-assertions-propagation-effects-diagnostics', 'diagnostics':len(run['diagnostics'])})
    counts = Counter((site['classification'],site['category']) for site in annotations['sites'])
    if sum(counts.values()) != annotations['original_site_count'] or len({site['id'] for site in annotations['sites']}) != annotations['original_site_count']:
        raise RuntimeError('Original annotation totals/unique IDs do not reconcile')
    for site in annotations['sites']:
        if site['classification'] not in {'eligible','protected'} or not site['reason'] or not site['evidence']:
            raise RuntimeError(f'Annotation lacks frozen classification: {site["id"]}')
    for variant in ['original','stabilized']:
        config = bench / 'cohort' / variant / 'xsht-config.ini'
        config.write_bytes((repo / 'xsht-config.ini').read_bytes())
    selection['configuration'] = {'path':'xsht-config.ini','sha256':sha256((repo / 'xsht-config.ini').read_bytes()), 'variants':['original','stabilized']}
    annotations['status'] = 'frozen-eligibility-and-correspondence'
    annotations['reconciliation'] = {'status':'passed', 'original_unique_sites':len(annotations['sites']), 'protected_original_sites':sum(n for (classification,_),n in counts.items() if classification=='protected'), 'eligible_original_sites':sum(n for (classification,_),n in counts.items() if classification=='eligible'), 'added_protected_stabilizers':len(annotations['stabilizers']), 'source_checks':source_checks, 'workload_checks':root_checks, 'runtime_observations':'owned-by-campaign-observation-harness; checked facts are not execution evidence'}
    frozen = {'denominators':annotations['denominators'],'sites':[{key:site[key] for key in ['id','source_sha256','classification','reason']} for site in annotations['sites']]}
    annotations['eligibility_sha256'] = sha256(json.dumps(frozen,sort_keys=True,separators=(',',':')).encode('utf-8'))
    selection['status'] = 'frozen-source-closures-and-stabilization'
    selection['snapshot_commit'] = subprocess.check_output(['git','rev-parse',selection['snapshot_commit']],cwd=repo,text=True).strip()
    annotations['snapshot_commit'] = selection['snapshot_commit']
    save(bench / 'annotations.json', annotations)
    save(bench / 'cohort-selection.json', selection)
    save(bench / 'cohort/stabilized-signatures.json', {'schema_version':1,'runs':stable_runs})
    print(json.dumps({'status':'passed','sources':len(source_map),'workloads':len(root_checks),'sites':len(annotations['sites']),'stabilizers':len(annotations['stabilizers']),'denominators':annotations['denominators'],'eligibility_sha256':annotations['eligibility_sha256']}))


def verify(repo):
    selection = json.loads((repo / 'bench/typing/cohort-selection.json').read_text())
    for source in selection['sources']:
        for variant in ('original', 'stabilized'):
            path = repo / 'bench/typing/cohort' / variant / source['path']
            if sha256(path.read_bytes()) != source[variant + '_sha256']:
                raise RuntimeError(f'Frozen {variant} source changed: {source["path"]}')
    for variant in selection['configuration']['variants']:
        config = repo / 'bench/typing/cohort' / variant / selection['configuration']['path']
        if sha256(config.read_bytes()) != selection['configuration']['sha256']:
            raise RuntimeError(f'Frozen module configuration changed: {variant}')
    annotations = json.loads((repo / 'bench/typing/annotations.json').read_text())
    frozen = {'denominators': annotations['denominators'], 'sites': [{key: site[key] for key in ['id', 'source_sha256', 'classification', 'reason']} for site in annotations['sites']]}
    fingerprint = sha256(json.dumps(frozen, sort_keys=True, separators=(',', ':')).encode('utf-8'))
    if fingerprint != annotations['eligibility_sha256']:
        raise RuntimeError('Frozen eligibility or denominators changed')
    sources = {source['path']: source for source in selection['sources']}
    for site in annotations['sites']:
        raw = (repo / 'bench/typing/cohort/original' / site['source']).read_bytes()
        start, end = site['span']
        if raw[start:end].decode('utf-8') != site['text'] or site['source_sha256'] != sources[site['source']]['original_sha256']:
            raise RuntimeError(f'Original source/site identity mismatch: {site["id"]}')
    print(json.dumps({'sources': len(selection['sources']), 'original_sites': len(annotations['sites']), 'status': 'passed', 'check': 'exact-source-config-site-and-eligibility-hashes'}))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--baseline-source', type=Path)
    parser.add_argument('--baseline-target', type=Path)
    parser.add_argument('--helper', type=Path, default=Path('/tmp/xsh-typing-inventory'))
    parser.add_argument('--build-helper', action='store_true')
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--verify', action='store_true')
    parser.add_argument('--reconcile', action='store_true')
    args = parser.parse_args()
    build = build_helper(args.baseline_target.resolve(), args.repo.resolve(), args.helper) if args.build_helper else None
    if args.prepare:
        prepare(args.repo.resolve(), args.baseline_source.resolve(), args.helper, build)
    if args.reconcile:
        reconcile(args.repo.resolve(), args.helper)
    if args.verify:
        verify(args.repo.resolve())


if __name__ == '__main__':
    main()
