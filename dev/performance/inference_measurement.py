import base64
import argparse
import hashlib
import json
import math
import os
import statistics
from pathlib import Path
import sys
from types import SimpleNamespace


PAIR_COUNT = 100
RUNTIME_PAIR_COUNT = 5
METRIC_FIELDS = (
    'fresh_variables', 'generated_constraints', 'representative_hops', 'compression_writes',
    'unifications', 'occurs_visits', 'row_field_visits', 'scheme_instantiations',
    'successful_generic_call_resolutions', 'worklist_wakeups', 'host_candidate_trials',
    'host_obligation_requeues', 'rollbacks', 'rollback_changes', 'level_adjustments',
    'effect_edge_visits', 'candidate_tests', 'diagnostic_work', 'output_type_nodes',
    'evidence_nodes',
)
SCALING_METRICS = tuple(name for name in METRIC_FIELDS
                        if name not in ('output_type_nodes', 'evidence_nodes')) + ('function_body_check_entries',)
VARIANTS = ('annotated', 'reduced')
MEASURES = ('cpu_ns', 'wall_ns', 'max_rss_bytes')
ALLOCATION_STAGES = ('tokens', 'cst', 'ast_check', 'lower', 'after_drop')
PHASES = (
    'preparation_inclusive', 'project_parse_load_exclusive',
    'check_solve_fact_build_exclusive', 'declaration_publication_exclusive',
    'lower_verify_install_exclusive', 'preparation_other_exclusive',
    'execution_finish_exclusive', 'diagnostic_finish_exclusive',
)


def canonical_inference_metrics(value):
    record = frontend_record(value)
    metrics = record.get('inference_work')
    if not isinstance(metrics, dict):
        raise ValueError('canonical inference counters unavailable: entry-root frontend-stats lacks inference_work')
    missing = set(METRIC_FIELDS) - metrics.keys()
    if missing:
        raise ValueError(f'canonical inference counters unavailable: inference_work lacks {sorted(missing)}')
    if any(type(metrics[name]) is not int or metrics[name] < 0 for name in METRIC_FIELDS):
        raise ValueError('canonical inference counters unavailable: work and shape counts must be nonnegative integers')
    retained = record.get('semantic_retained_bytes')
    if type(retained) is not int or retained < 0:
        raise ValueError('canonical inference counters unavailable: entry-root semantic_retained_bytes is missing')
    body_checks = record.get('function_body_check_entries')
    if type(body_checks) is not int or body_checks < 0:
        raise ValueError('canonical inference counters unavailable: function body check count is missing')
    return {**{name: metrics[name] for name in METRIC_FIELDS},
            'function_body_check_entries': body_checks, 'retained_typing_bytes': retained}


def frontend_record(value):
    if not isinstance(value, dict):
        raise ValueError('canonical frontend-stats result is not an object')
    files = value.get('files')
    if not isinstance(files, list) or len(files) != 1 or not isinstance(files[0], dict):
        raise ValueError('canonical frontend-stats expected one entry-root record')
    return files[0]


def canonical_retained_bytes(value):
    record = frontend_record(value)
    retained = record.get('semantic_retained_bytes')
    if type(retained) is not int or retained < 0:
        raise ValueError('canonical reference retained-typing metric is missing')
    return retained


def canonical_allocation_traffic(value):
    record = frontend_record(value)
    stages = record.get('stages')
    if not isinstance(stages, list):
        raise ValueError('canonical allocation traffic is unavailable: frontend-stats stages are missing')
    by_name = {stage.get('name'): stage for stage in stages if isinstance(stage, dict)}
    if set(by_name) != set(ALLOCATION_STAGES):
        raise ValueError('canonical allocation traffic is unavailable: frontend-stats stage set changed')
    result = {}
    for name in ALLOCATION_STAGES:
        stage = by_name[name]
        if any(type(stage.get(field)) is not int or stage[field] < 0
               for field in ('alloc_count', 'alloc_bytes', 'retained_bytes', 'peak_bytes', 'item_count')):
            raise ValueError(f'canonical allocation traffic is malformed for {name}')
        if type(stage.get('tracking_active')) is not bool:
            raise ValueError(f'canonical allocation tracking state is missing for {name}')
        result[name] = {field: stage[field] for field in
                        ('alloc_count', 'alloc_bytes', 'retained_bytes', 'peak_bytes', 'item_count')}
        result[name]['tracking_active'] = stage['tracking_active']
    return result


def validate_observation(row, expected_stdout, *, expected_status=0, expected_depth_rejection=False,
                         require_useful=False):
    status = row.get('status')
    if row.get('timed_out') is not False or type(status) is not int:
        raise ValueError('inference sample did not complete with an observed process status')
    stderr = base64.b64decode(row.get('stderr_base64', ''))
    stdout = base64.b64decode(row.get('stdout_base64', ''))
    if expected_depth_rejection:
        diagnostic = stderr.decode('utf-8', errors='replace').lower()
        if status != 2 or b'parse.nesting-depth' in stderr or 'type depth limit' not in diagnostic:
            raise ValueError('512-level deep-record control did not reach the semantic type-depth rejection')
    elif status != expected_status or stdout != expected_stdout or stderr or row.get('effects') != {}:
        raise ValueError(f'inference output/status/effects mismatch: status={status} stdout={stdout!r} stderr={stderr!r} effects={row.get("effects")!r}')
    for metric in MEASURES:
        if type(row.get(metric)) is not int or row[metric] < 0:
            raise ValueError(f'inference sample lacks valid {metric}')
    useful_time = row.get('first_useful_stdout_ns')
    if require_useful and (row.get('useful_stdout_observed') is not True or type(useful_time) is not int or useful_time < 0):
        raise ValueError('runtime sample lacks first useful output timing')
    return {
        **{name: row[name] for name in MEASURES},
        'status': status,
        'stdout_bytes': len(stdout),
        'stdout_sha256': hashlib.sha256(stdout).hexdigest(),
        'stderr_bytes': len(stderr),
        'stderr_sha256': hashlib.sha256(stderr).hexdigest(),
        'effects': row['effects'],
        'first_useful_stdout_ns': useful_time,
    }


def percentile(values, percent):
    if not values or not 0 < percent <= 100:
        raise ValueError('percentile requires values and a rank in (0, 100]')
    ordered = sorted(values)
    return ordered[max(0, math.ceil(percent * len(ordered) / 100) - 1)]


def summarize(values):
    if not values:
        raise ValueError('cannot summarize an empty distribution')
    low, high = min(values), max(values)
    median = statistics.median(values)
    return {
        'samples': len(values),
        'median': median,
        'p95': percentile(values, 95),
        'range': [low, high],
        'spread': high - low,
    }


def alternate_pairs(count, invoke_pair, arms=VARIANTS):
    if type(count) is not int or count < 1:
        raise ValueError('pair count must be a positive integer')
    if len(arms) != 2 or len(set(arms)) != 2 or any(not isinstance(arm, str) for arm in arms):
        raise ValueError('paired comparison requires two distinct named arms')
    results = {name: {measure: [] for measure in MEASURES} for name in arms}
    samples = {name: [] for name in arms}
    paired_differences = {measure: [] for measure in MEASURES}
    order = []
    for index in range(count):
        variants = arms if index % 2 == 0 else tuple(reversed(arms))
        pair = {}
        for variant in variants:
            observation = invoke_pair(variant)
            sample = validate_observation(observation['row'], observation['expected_stdout'],
                                          require_useful=observation.get('require_useful', False))
            pair[variant] = sample
            samples[variant].append(sample)
            order.append(variant)
        for variant in arms:
            for measure in MEASURES:
                results[variant][measure].append(pair[variant][measure])
        for measure in MEASURES:
            paired_differences[measure].append(pair[arms[1]][measure] - pair[arms[0]][measure])
    return {
        'pairs': count,
        'order': order,
        'arms': {arm: {
            'summaries': {measure: summarize(values) for measure, values in metrics.items()},
            'samples': samples[arm],
        } for arm, metrics in results.items()},
        'paired_differences': {
            measure: {'summary': summarize(values), 'samples': values}
            for measure, values in paired_differences.items()
        },
    }


def bracketed_measurement(ready, identity, operation, *, discard=None):
    ready(False)
    primary = None
    result = None
    operation_started = False
    try:
        identity()
        operation_started = True
        result = operation()
    except BaseException as error:
        primary = error
    discarded = False
    for finalize in (identity, lambda: ready(True)):
        try:
            finalize()
        except BaseException as error:
            if operation_started and not discarded and discard is not None:
                try:
                    discard()
                except BaseException as discard_error:
                    error.add_note(f'timing invalidation reporting failed: {discard_error}')
                discarded = True
            if primary is None:
                primary = error
            else:
                primary.add_note(f'post-measurement validation failed: {error}')
    if primary is not None:
        raise primary
    return result


def source_maps(workloads, root):
    root = Path(root)
    root.mkdir(parents=True, exist_ok=False)
    materialized = {}
    for workload in workloads:
        materialized[workload.name] = {}
        for variant, sources in (('annotated', workload.annotated_sources), ('reduced', workload.reduced_sources)):
            directory = root / workload.name / variant
            directory.mkdir(parents=True)
            for relative, content in sources.items():
                destination = directory / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(content, encoding='utf-8')
            materialized[workload.name][variant] = directory
    return materialized


def source_map_snapshot(directory):
    root = Path(directory)
    if not root.is_dir():
        raise ValueError(f'generated source map is missing: {root}')
    rows = {}
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise ValueError(f'generated source map contains a symlink: {path}')
        if path.is_file():
            rows[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return rows


def frontend_json(stdout, entry):
    try:
        value = json.loads(stdout)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f'canonical inference counters unavailable: frontend-stats did not return JSON: {error}') from error
    files = value.get('files') if isinstance(value, dict) else None
    if (not isinstance(files, list) or len(files) != 1 or not isinstance(files[0], dict)
            or files[0].get('path') != str(entry)):
        raise ValueError('canonical inference counters unavailable: frontend-stats did not report the exact entry root')
    return value


def frontend_metrics(stdout, entry):
    return canonical_inference_metrics(frontend_json(stdout, entry))


def collect_frontend_metrics(xsht, variant_directory, sampler, environment, timeout=60):
    entry = variant_directory / 'main.xsh'
    row = sampler.invoke(
        [str(xsht), 'frontend-stats', '--json', str(entry)],
        variant_directory,
        environment,
        timeout,
    )
    if row.get('status') != 0 or row.get('timed_out') is not False or row.get('stderr_base64'):
        raise ValueError('canonical frontend-stats invocation failed before inference counters were available')
    value = frontend_json(base64.b64decode(row['stdout_base64']), entry)
    return {
        'inference_work': canonical_inference_metrics(value),
        'allocation_traffic': canonical_allocation_traffic(value),
    }


def collect_reference_frontend_metrics(xsht, variant_directory, sampler, environment, timeout=60):
    entry = variant_directory / 'main.xsh'
    row = sampler.invoke(
        [str(xsht), 'frontend-stats', '--json', str(entry)],
        variant_directory,
        environment,
        timeout,
    )
    if row.get('status') != 0 or row.get('timed_out') is not False or row.get('stderr_base64'):
        raise ValueError('reference frontend-stats invocation failed before retained typing data was available')
    value = frontend_json(base64.b64decode(row['stdout_base64']), entry)
    return {
        'retained_typing_bytes': canonical_retained_bytes(value),
        'allocation_traffic': canonical_allocation_traffic(value),
    }


def collect_runtime_phase_diagnostic(xsht, entry, report_path, sampler, environment, expected_stdout):
    row = sampler.invoke([str(xsht), 'runtime-stats', '--json', str(report_path), str(entry)],
                         Path(entry).parent, environment, 60)
    if row.get('timed_out') is not False or row.get('status') != 0:
        raise ValueError('inference runtime phase diagnostic did not complete successfully')
    stdout = base64.b64decode(row.get('stdout_base64', ''))
    stderr = base64.b64decode(row.get('stderr_base64', ''))
    if stdout != expected_stdout or stderr or row.get('effects') != {}:
        raise ValueError('inference runtime phase diagnostic changed program output or effects')
    try:
        report = json.loads(Path(report_path).read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f'inference runtime phase diagnostic report is unavailable: {error}') from error
    if (report.get('status') != 0 or report.get('stdout_bytes') != len(expected_stdout)
            or report.get('stderr_bytes') != 0 or report.get('tracking_active') is not True):
        raise ValueError('inference runtime phase diagnostic report does not describe the checked program run')
    phases = {}
    for name, clock in (('wall_time_ns', 'monotonic_wall'), ('cpu_time_ns', 'process_cpu')):
        values = report.get(name)
        if not isinstance(values, dict) or values.get('clock') != clock:
            raise ValueError(f'inference runtime phase diagnostic lacks {clock} phase data')
        if any(type(values.get(phase)) is not int or values[phase] < 0
               for phase in ('preparation_inclusive', 'preparation_other_exclusive')):
            raise ValueError(f'inference runtime phase diagnostic has invalid {clock} preparation time')
        if any(values.get(phase) is not None
               and (type(values[phase]) is not int or values[phase] < 0)
               for phase in PHASES if phase not in ('preparation_inclusive', 'preparation_other_exclusive')):
            raise ValueError(f'inference runtime phase diagnostic has invalid {clock} phase time')
        phases[name] = {phase: values[phase] for phase in PHASES}
    return {
        'status': row['status'],
        'stdout_bytes': len(stdout),
        'stdout_sha256': hashlib.sha256(stdout).hexdigest(),
        'stderr_bytes': len(stderr),
        'wall_time_ns': phases['wall_time_ns'],
        'cpu_time_ns': phases['cpu_time_ns'],
    }


def run_distribution(workload, variant_directories, products, sampler, environment, *, stage, count, arms):
    if stage not in ('check', 'runtime'):
        raise ValueError(f'unknown inference measurement stage: {stage}')
    if stage == 'runtime' and workload.frontend_expectation == 'expected-type-depth-rejection':
        raise ValueError(f'{workload.name}: expected semantic rejection has no runnable output')

    def invoke_pair(variant):
        compiler_arm, source_variant = arms[variant]
        directory = variant_directories[source_variant]
        entry = directory / workload.entry
        xsh, xsht = products[compiler_arm]['xsh'], products[compiler_arm]['xsht']
        if stage == 'check':
            argv = [str(xsht), 'check', str(entry)]
            expected = b''
            useful_prefix = None
        else:
            argv = [str(xsh), str(entry)]
            expected = workload.expected_stdout
            useful_prefix = expected.splitlines(keepends=True)[0]
        row = sampler.invoke(argv, directory, environment, 60, measured=True, useful_prefix=useful_prefix)
        return {'row': row, 'expected_stdout': expected, 'require_useful': useful_prefix is not None}

    distribution = alternate_pairs(count, invoke_pair, tuple(arms))
    return {
        'workload': workload.name,
        'stage': stage,
        'pairs': distribution['pairs'],
        'order_policy': f'{tuple(arms)[0]} then {tuple(arms)[1]} on even pairs; reversed on odd pairs',
        'arms': distribution['arms'],
        'paired_differences': distribution['paired_differences'],
        'filesystem_cache': 'warm; no host page-cache flush',
        'process_lifecycle': 'fresh owned process per sample',
    }


def check_budget(reference, candidate, *, fraction, absolute, spread):
    allowance = max(reference * fraction, absolute)
    difference = candidate - reference
    if spread is None:
        status = 'pass' if difference <= allowance else 'fail'
    elif abs(difference) <= spread:
        status = 'inconclusive'
    elif difference + spread <= allowance:
        status = 'pass'
    elif difference - spread > allowance:
        status = 'fail'
    else:
        status = 'inconclusive'
    return {
        'status': status,
        'reference': reference,
        'candidate': candidate,
        'difference': difference,
        'allowance': allowance,
        'paired_spread': spread,
    }


def pair_spread(distribution, measure):
    summary = distribution['paired_differences'][measure]['summary']
    return summary['spread']


def check_scaling(workloads):
    groups = {}
    missing = []
    required = {}
    for workload, result in workloads.items():
        metadata = (result.get('family'), result.get('shape'), result.get('size'))
        if any(value is None for value in metadata):
            missing.append({'workload': workload, 'reason': 'workload shape metadata missing'})
            continue
        if result.get('frontend_expectation') == 'expected-type-depth-rejection':
            continue
        key = (result['family'], result['shape'])
        required.setdefault(key, set()).add(result['size'])
        for variant in VARIANTS:
            wrapped = result.get('inference_work', {}).get('candidate', {}).get(variant)
            if not isinstance(wrapped, dict) or not isinstance(wrapped.get('inference_work'), dict):
                missing.append({'workload': workload, 'variant': variant, 'reason': 'candidate counters missing'})
                continue
            key = (result['family'], result['shape'], variant)
            groups.setdefault(key, {})[result['size']] = wrapped['inference_work']
    comparisons = []
    for (family, shape, variant), sizes in sorted(groups.items()):
        ordered = sorted(sizes)
        for previous_size, size in zip(ordered, ordered[1:]):
            if size != previous_size * 2:
                continue
            previous = sizes[previous_size]
            current = sizes[size]
            previous_output = previous['output_type_nodes'] + previous['evidence_nodes']
            output = current['output_type_nodes'] + current['evidence_nodes']
            if previous_output <= 0 or output <= 0:
                raise ValueError(f'{family}/{shape}/{variant}: emitted output/evidence size must be positive')
            for metric in SCALING_METRICS:
                old = previous[metric]
                new = current[metric]
                normalized_previous = old / previous_output
                normalized = new / output
                limit = 3 * normalized_previous + 128
                comparisons.append({
                    'family': family,
                    'shape': shape,
                    'variant': variant,
                    'previous_size': previous_size,
                    'size': size,
                    'metric': metric,
                    'previous_raw': old,
                    'raw': new,
                    'previous_output_nodes': previous_output,
                    'output_nodes': output,
                    'previous_normalized': normalized_previous,
                    'normalized': normalized,
                    'limit': limit,
                    'passed': new * previous_output <= (3 * old + 128 * previous_output) * output,
                })
    for (family, shape), sizes in sorted(required.items()):
        expected_sizes = []
        expected_size = min(sizes)
        while expected_size <= max(sizes):
            expected_sizes.append(expected_size)
            expected_size *= 2
        expected_pairs = list(zip(expected_sizes, expected_sizes[1:]))
        for variant in VARIANTS:
            actual = {(row['previous_size'], row['size']) for row in comparisons
                      if row['family'] == family and row['shape'] == shape and row['variant'] == variant}
            for previous_size, size in expected_pairs:
                if (previous_size, size) not in actual:
                    missing.append({'family': family, 'shape': shape, 'variant': variant,
                                    'previous_size': previous_size, 'size': size,
                                    'reason': 'required scaling comparison missing'})
    return {
        'law': 'normalized current <= 3 * normalized previous + 128 per doubling',
        'normalization': 'independently counted output_type_nodes + evidence_nodes',
        'comparisons': comparisons,
        'missing': missing,
        'passed': bool(comparisons) and not missing and all(row['passed'] for row in comparisons),
    }


def allocation_traffic_complete(workloads):
    records = []
    required_candidate_records = 0
    for workload, result in workloads.items():
        if result.get('frontend_expectation') == 'expected-type-depth-rejection':
            continue
        required_candidate_records += len(VARIANTS)
        check_result = result.get('check', {})
        for owner, variants in check_result.get('inference_work', {}).items():
            for variant, metrics in variants.items():
                traffic = metrics['allocation_traffic']
                records.append({
                    'workload': workload,
                    'owner': owner,
                    'variant': variant,
                    'tracking_active': all(stage['tracking_active'] for stage in traffic.values()),
                    'stages': traffic,
                })
    candidate_records = [row for row in records if row['owner'] == 'candidate']
    candidate_complete = (len(candidate_records) == required_candidate_records
                          and all(row['tracking_active'] for row in candidate_records))
    return {
        'records': records,
        'candidate_complete': candidate_complete,
        'complete': candidate_complete and all(row['tracking_active'] for row in records),
    }


def phase_diagnostic_valid(value):
    if not isinstance(value, dict) or value.get('status') != 0:
        return False
    for clock in ('wall_time_ns', 'cpu_time_ns'):
        phases = value.get(clock)
        if (not isinstance(phases, dict)
                or any(type(phases.get(name)) is not int or phases[name] < 0
                       for name in ('preparation_inclusive', 'preparation_other_exclusive'))):
            return False
    return True


def phase_diagnostics_complete(workloads):
    missing = []
    for workload, result in workloads.items():
        if result.get('frontend_expectation') == 'expected-type-depth-rejection':
            continue
        check_result = result.get('check')
        if not isinstance(check_result, dict):
            missing.append({'workload': workload, 'reason': 'check phase diagnostics missing'})
            continue
        required = {'candidate_annotated', 'candidate_reduced'}
        if check_result.get('reference_support', {}).get('supported') is True:
            required.add('reference_annotated')
        diagnostics = check_result.get('phase_diagnostics')
        for name in sorted(required):
            if not isinstance(diagnostics, dict) or not phase_diagnostic_valid(diagnostics.get(name)):
                missing.append({'workload': workload, 'diagnostic': name,
                                'reason': 'runtime-stats preparation phase data missing or invalid'})
    return {'complete': not missing, 'missing': missing}


def qualify_measurements(workloads, *, monomorphic_runtime, binary_sizes):
    budget_results = []
    baseline_unsupported = []
    samples_complete = True
    runtime_complete = True
    counter_records = {}
    for name, result in workloads.items():
        check_stage = result.get('check')
        counter_records[name] = {
            **{field: result.get(field) for field in
               ('family', 'shape', 'size', 'frontend_expectation')},
            'inference_work': check_stage.get('inference_work', {})
            if isinstance(check_stage, dict) else {},
        }
        if result.get('frontend_expectation') == 'expected-type-depth-rejection':
            controls = result.get('expected_rejection_control', {})
            required_controls = {'reference_annotated', 'reference_reduced',
                                 'candidate_annotated', 'candidate_reduced'}
            if (not required_controls.issubset(controls)
                    or any(not controls[name].get('valid') for name in required_controls)):
                samples_complete = False
            continue
        check_result = result.get('check')
        runtime_result = result.get('runtime')
        if not isinstance(check_result, dict) or not isinstance(runtime_result, dict):
            samples_complete = False
            runtime_complete = False
            continue
        candidate_check = check_result['distributions'].get('candidate_annotation')
        candidate_runtime = runtime_result['distributions'].get('candidate_annotation')
        if not candidate_check or candidate_check['pairs'] != PAIR_COUNT:
            samples_complete = False
        if not candidate_runtime or candidate_runtime['pairs'] != RUNTIME_PAIR_COUNT:
            samples_complete = False
            runtime_complete = False
        for stage_result in (check_result, runtime_result):
            support = stage_result.get('reference_support', {})
            if support.get('observed') is not True:
                samples_complete = False
                if stage_result['stage'] == 'runtime':
                    runtime_complete = False
            if support.get('supported') is not True:
                baseline_unsupported.append({'workload': name, 'stage': stage_result['stage'],
                                             'observed': support})
                continue
            expected_pairs = PAIR_COUNT if stage_result['stage'] == 'check' else RUNTIME_PAIR_COUNT
            comparison = stage_result['distributions'].get('reference_candidate_annotated')
            if not comparison or comparison['pairs'] != expected_pairs:
                samples_complete = False
                if stage_result['stage'] == 'runtime':
                    runtime_complete = False
                continue
            if stage_result['stage'] == 'check':
                for measure in ('cpu_ns', 'wall_ns'):
                    reference = comparison['arms']['reference']['summaries'][measure]
                    candidate = comparison['arms']['candidate']['summaries'][measure]
                    spread = pair_spread(comparison, measure)
                    for rank, fraction, absolute in (('median', .05, 1_000_000),
                                                      ('p95', .10, 2_000_000)):
                        budget_results.append({
                            'workload': name,
                            'comparison': f'unchanged_annotated_frontend_{rank}',
                            'measure': measure,
                            **check_budget(reference[rank], candidate[rank], fraction=fraction,
                                           absolute=absolute, spread=spread),
                        })
                reference_rss = comparison['arms']['reference']['summaries']['max_rss_bytes']['median']
                candidate_rss = comparison['arms']['candidate']['summaries']['max_rss_bytes']['median']
                budget_results.append({
                    'workload': name,
                    'comparison': 'unchanged_annotated_median_process_rss',
                    'measure': 'max_rss_bytes',
                    **check_budget(reference_rss, candidate_rss, fraction=.10, absolute=2 * 1024**2,
                                   spread=pair_spread(comparison, 'max_rss_bytes')),
                })
                candidate_annotated = check_result['inference_work']['candidate'].get('annotated', {}).get('inference_work')
                reference_annotated = check_result['inference_work']['reference'].get('annotated')
                candidate_reduced = check_result['inference_work']['candidate'].get('reduced', {}).get('inference_work')
                if reference_annotated is None or candidate_annotated is None or candidate_reduced is None:
                    samples_complete = False
                else:
                    for variant, metrics in (('annotated', candidate_annotated), ('reduced', candidate_reduced)):
                        budget_results.append({
                            'workload': name,
                            'comparison': f'{variant}_retained_typing_bytes',
                            'measure': 'retained_typing_bytes',
                            **check_budget(reference_annotated['retained_typing_bytes'],
                                           metrics['retained_typing_bytes'], fraction=.10,
                                           absolute=2 * 1024**2, spread=None),
                        })
                reduced_distribution = check_result['distributions'].get('candidate_annotation')
                if not reduced_distribution:
                    samples_complete = False
                    continue
                for measure in ('cpu_ns', 'wall_ns'):
                    annotated = reduced_distribution['arms']['annotated']['summaries'][measure]['median']
                    reduced = reduced_distribution['arms']['reduced']['summaries'][measure]['median']
                    budget_results.append({
                        'workload': name,
                        'comparison': 'reduced_vs_annotated_frontend_median',
                        'measure': measure,
                        **check_budget(annotated, reduced, fraction=.20, absolute=2_000_000,
                                       spread=pair_spread(reduced_distribution, measure)),
                    })
            else:
                for measure in ('cpu_ns', 'wall_ns'):
                    reference = comparison['arms']['reference']['summaries'][measure]['median']
                    candidate = comparison['arms']['candidate']['summaries'][measure]['median']
                    budget_results.append({
                        'workload': name,
                        'comparison': 'reference_to_candidate_runtime_median',
                        'measure': measure,
                        **check_budget(reference, candidate, fraction=.05, absolute=1_000_000,
                                       spread=pair_spread(comparison, measure)),
                    })
    if not isinstance(monomorphic_runtime, dict) or monomorphic_runtime.get('pairs') != RUNTIME_PAIR_COUNT:
        samples_complete = False
        runtime_complete = False
    else:
        for measure in ('cpu_ns', 'wall_ns'):
            reference = monomorphic_runtime['arms']['reference']['summaries'][measure]['median']
            candidate = monomorphic_runtime['arms']['candidate']['summaries'][measure]['median']
            budget_results.append({
                'workload': monomorphic_runtime['name'],
                'comparison': 'existing_monomorphic_runtime_median',
                'measure': measure,
                **check_budget(reference, candidate, fraction=.05, absolute=1_000_000,
                               spread=pair_spread(monomorphic_runtime, measure)),
            })
    sizes = []
    for name in ('xsh', 'xsht'):
        reference = binary_sizes['reference'][name]
        candidate = binary_sizes['candidate'][name]
        sizes.append({
            'product': name,
            'reference_bytes': reference,
            'candidate_bytes': candidate,
            'difference_bytes': candidate - reference,
        })
    allocations = allocation_traffic_complete(workloads)
    phase_diagnostics = phase_diagnostics_complete(workloads)
    scaling = check_scaling(counter_records)
    statuses = [row['status'] for row in budget_results]
    counters_complete = allocations['candidate_complete'] and not scaling['missing']
    qualification_complete = (
        samples_complete and runtime_complete and not baseline_unsupported and allocations['complete']
        and phase_diagnostics['complete']
        and scaling['passed'] and bool(statuses) and all(status == 'pass' for status in statuses)
    )
    return {
        'samples_complete': samples_complete,
        'runtime_complete': runtime_complete,
        'counters_complete': counters_complete,
        'qualification_complete': qualification_complete,
        'baseline_unsupported': baseline_unsupported,
        'budgets': budget_results,
        'binary_size_observations': sizes,
        'allocation_traffic': allocations,
        'phase_diagnostics': phase_diagnostics,
        'scaling': scaling,
        'qualification_status': 'pass' if qualification_complete else 'incomplete',
    }


def reference_support(xsht, entry, sampler, environment):
    row = sampler.invoke([str(xsht), 'check', str(entry)], entry.parent, environment, 60)
    if row.get('timed_out') is not False or type(row.get('status')) is not int:
        raise ValueError('reference support probe did not finish with an observed status')
    stdout = base64.b64decode(row.get('stdout_base64', ''))
    stderr = base64.b64decode(row.get('stderr_base64', ''))
    supported = row['status'] == 0 and not stdout and not stderr and row.get('effects') == {}
    return {
        'observed': True,
        'supported': supported,
        'status': row['status'],
        'stdout_bytes': len(stdout),
        'stdout_sha256': hashlib.sha256(stdout).hexdigest(),
        'stderr_bytes': len(stderr),
        'stderr_sha256': hashlib.sha256(stderr).hexdigest(),
        'stderr_text': stderr.decode('utf-8', errors='replace')[:1024],
        'effects': row.get('effects'),
    }


def run_monomorphic_runtime_control(candidate_root, reference_root, products, sampler, environment, *, count):
    relative = Path('bench/stdlib-port/cold_trivial.xsh')
    candidate_entry = candidate_root / relative
    reference_entry = reference_root / relative
    if candidate_entry.read_bytes() != reference_entry.read_bytes():
        raise ValueError('monomorphic runtime control differs between reference and candidate sources')
    arms = {
        'reference': ('reference', reference_entry),
        'candidate': ('candidate', candidate_entry),
    }

    def invoke_pair(arm):
        compiler, entry = arms[arm]
        row = sampler.invoke([str(products[compiler]['xsh']), str(entry)], entry.parent,
                             environment, 60, measured=True, useful_prefix=b'hello\n')
        return {'row': row, 'expected_stdout': b'hello\n', 'require_useful': True}

    result = alternate_pairs(count, invoke_pair, ('reference', 'candidate'))
    return {
        'name': 'bench/stdlib-port/cold_trivial.xsh',
        'pairs': count,
        'order_policy': 'reference then candidate on even pairs; reversed on odd pairs',
        'arms': result['arms'],
        'paired_differences': result['paired_differences'],
        'filesystem_cache': 'warm; no host page-cache flush',
        'process_lifecycle': 'fresh owned process per sample',
    }


def environment_for(directory):
    paths = {}
    for name in ('home', 'tmp', 'config', 'cache', 'data'):
        path = directory / name
        path.mkdir(exist_ok=True)
        paths[name] = path
    return {
        'PATH': '/usr/local/bin:/usr/bin:/bin',
        'LC_ALL': 'C',
        'LANG': 'C',
        'TZ': 'UTC',
        'HOME': str(paths['home']),
        'TMPDIR': str(paths['tmp']),
        'XDG_CONFIG_HOME': str(paths['config']),
        'XDG_CACHE_HOME': str(paths['cache']),
        'XDG_DATA_HOME': str(paths['data']),
        'NO_COLOR': '1',
        'PYTHONDONTWRITEBYTECODE': '1',
    }


def run_depth_rejection_control(workload, variant_directories, products, sampler, environment):
    rows = {}
    for compiler in ('reference', 'candidate'):
        for variant in VARIANTS:
            directory = variant_directories[variant]
            row = sampler.invoke([str(products[compiler]['xsht']), 'check', str(directory / workload.entry)],
                                 directory, environment, 60)
            status = row.get('status')
            stderr = base64.b64decode(row.get('stderr_base64', ''))
            stdout = base64.b64decode(row.get('stdout_base64', ''))
            diagnostic = stderr.decode('utf-8', errors='replace').lower()
            valid = (row.get('timed_out') is False and type(status) is int and status == 2
                     and not stdout and row.get('effects') == {}
                     and b'parse.nesting-depth' not in stderr
                     and 'type depth limit' in diagnostic)
            rows[f'{compiler}_{variant}'] = {
                'valid': valid,
                'status': status,
                'timed_out': row.get('timed_out'),
                'stdout_bytes': len(stdout),
                'stderr_sha256': hashlib.sha256(stderr).hexdigest(),
                'stderr_text': diagnostic[:1024],
                'rejection': 'semantic type-depth limit' if valid else 'unexpected check outcome',
            }
    return {'workload': workload.name, 'expected_rejection_control': rows}


def container_distribution(args):
    source_root = Path(args.source_root)
    sys.path.insert(0, str(source_root))
    from dev.performance import check, inference_workloads

    config = SimpleNamespace(source_root=source_root)
    sampler = check.baseline_owner(config, 'bench/consolidation/run.py', 'inference_sampler')
    resources = check.baseline_owner(config, 'dev/consolidation/winddown/gate-tools/final-performance-gates/capture-container-resources.py', 'inference_resources')
    sampler.validate_container_resources(
        args.cpu_id, 4 * 1024**3, sorted(os.sched_getaffinity(0)),
        sampler.cgroup_memory_limit(), sampler.cgroup_memory_swap_limit(),
    )
    workloads = {case.name: case for case in inference_workloads.fixed_inference_workloads()}
    workload = workloads[args.workload]
    directories = {variant: Path(args.workload_root) / variant for variant in VARIANTS}
    products = {
        'candidate': {'xsh': args.xsh, 'xsht': args.xsht},
        'reference': {'xsh': args.reference_xsh, 'xsht': args.reference_xsht},
    }
    reference_root = Path(args.reference_root)
    environment = environment_for(directories['annotated'])
    memory_before = resources.cgroup_snapshot()
    if workload.frontend_expectation == 'expected-type-depth-rejection':
        result = run_depth_rejection_control(workload, directories, products, sampler, environment)
    else:
        annotated_entry = directories['annotated'] / workload.entry
        support = reference_support(args.reference_xsht, annotated_entry, sampler, environment)
        inference_work = {'candidate': {}, 'reference': {}}
        phase_diagnostics = {}
        if args.stage == 'check':
            for variant in VARIANTS:
                inference_work['candidate'][variant] = collect_frontend_metrics(
                    args.xsht, directories[variant], sampler, environment)
                phase_path = Path(args.output).parent / f'{workload.name}-candidate-{variant}-runtime-stats.json'
                phase_diagnostics[f'candidate_{variant}'] = collect_runtime_phase_diagnostic(
                    args.xsht, directories[variant] / workload.entry, phase_path, sampler,
                    environment, workload.expected_stdout,
                )
            if support['supported']:
                inference_work['reference']['annotated'] = collect_reference_frontend_metrics(
                    args.reference_xsht, directories['annotated'], sampler, environment)
                phase_path = Path(args.output).parent / f'{workload.name}-reference-annotated-runtime-stats.json'
                phase_diagnostics['reference_annotated'] = collect_runtime_phase_diagnostic(
                    args.reference_xsht, annotated_entry, phase_path, sampler, environment,
                    workload.expected_stdout,
                )
        result = {
            'workload': workload.name,
            'family': workload.family,
            'shape': workload.shape,
            'size': workload.size,
            'frontend_expectation': workload.frontend_expectation,
            'stage': args.stage,
            'reference_support': support,
            'distributions': {},
            'inference_work': inference_work,
        }
        if args.stage == 'check':
            result['phase_diagnostics'] = phase_diagnostics
        result['distributions']['candidate_annotation'] = run_distribution(
            workload, directories, products, sampler, environment, stage=args.stage,
            count=args.pairs, arms={'annotated': ('candidate', 'annotated'),
                                     'reduced': ('candidate', 'reduced')})
        if support['supported']:
            result['distributions']['reference_candidate_annotated'] = run_distribution(
                workload, directories, products, sampler, environment, stage=args.stage,
                count=args.pairs if args.stage == 'check' else RUNTIME_PAIR_COUNT,
                arms={'reference': ('reference', 'annotated'),
                      'candidate': ('candidate', 'annotated')})
        if args.stage == 'runtime' and args.include_monomorphic:
            result['monomorphic_runtime'] = run_monomorphic_runtime_control(
                Path(args.source_root), reference_root, products, sampler, environment,
                count=RUNTIME_PAIR_COUNT)
    result['cgroup_memory_before'] = memory_before
    result['cgroup_memory_after'] = resources.cgroup_snapshot()
    for key in ('cgroup_memory_before', 'cgroup_memory_after'):
        peak = result[key].get('memory_peak')
        if not isinstance(peak, str) or not peak.isdecimal():
            raise ValueError(f'{workload.name}: cgroup memory peak unavailable inside the sample container')
    Path(args.output).write_text(json.dumps(result, sort_keys=True) + '\n')


def main(argv=None):
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--container-distribution', action='store_true')
    mode.add_argument('--run-performance', action='store_true')
    parser.add_argument('--config', type=Path)
    parser.add_argument('--source-root', type=Path)
    parser.add_argument('--workload', default='')
    parser.add_argument('--workload-root', type=Path)
    parser.add_argument('--stage', choices=('check', 'runtime'), default='check')
    parser.add_argument('--pairs', type=int, default=PAIR_COUNT)
    parser.add_argument('--cpu-id', type=int)
    parser.add_argument('--xsh', type=Path)
    parser.add_argument('--xsht', type=Path)
    parser.add_argument('--reference-root', type=Path)
    parser.add_argument('--reference-xsh', type=Path)
    parser.add_argument('--reference-xsht', type=Path)
    parser.add_argument('--include-monomorphic', action='store_true')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args(argv)
    if args.run_performance:
        if args.config is None:
            parser.error('--run-performance requires --config')
        internal_values = (args.source_root, args.workload_root, args.cpu_id, args.xsh, args.xsht,
                           args.reference_root, args.reference_xsh, args.reference_xsht, args.output)
        if (any(value is not None for value in internal_values) or args.workload
                or args.include_monomorphic or args.stage != 'check' or args.pairs != PAIR_COUNT):
            parser.error('--run-performance accepts only --config')
        from dev.performance import check

        try:
            config = check.parse_config(check.read_json(args.config))
            check.inference_performance(config)
        except check.owned.SignalReceived as error:
            print(f'inference performance cancelled: {error}', file=sys.stderr)
            for note in getattr(error, '__notes__', []):
                print(note, file=sys.stderr)
            return 128 + error.signum
        except (ValueError, RuntimeError, OSError, KeyError, TypeError, json.JSONDecodeError) as error:
            print(f'inference performance: {error}', file=sys.stderr)
            return 1
        return 0
    required = (args.source_root, args.workload_root, args.xsh, args.xsht,
                args.reference_root, args.reference_xsh, args.reference_xsht, args.output)
    if any(value is None for value in required) or args.cpu_id is None or not args.workload:
        parser.error('container distribution requires explicit source, workload, products, and output')
    container_distribution(args)


if __name__ == '__main__':
    raise SystemExit(main())
