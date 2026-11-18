import base64
import hashlib
import json
import re
from pathlib import Path


METRIC_FIELDS = (
    'fresh_variables', 'generated_constraints', 'representative_hops', 'compression_writes',
    'unifications', 'occurs_visits', 'row_field_visits', 'scheme_instantiations',
    'successful_generic_call_resolutions', 'worklist_wakeups', 'host_candidate_trials',
    'host_obligation_requeues', 'rollbacks', 'rollback_changes', 'level_adjustments',
    'effect_edge_visits', 'candidate_tests', 'diagnostic_work', 'output_type_nodes',
    'evidence_nodes',
    'generalization_type_visits',
    'instantiation_type_visits',
    'schema_argument_visits',
    'constructor_generalizations',
    'constructor_template_binding_visits',
    'generalization_binding_clones',
)
VARIANTS = ('annotated', 'reduced')
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


def check_diagnostics(stderr):
    if not stderr.endswith(b'\n'):
        return stderr
    lines = stderr.splitlines(keepends=True)
    summary = lines[-1]
    seconds = rb'[0-9]+\.[0-9]{2}s'
    stages = b', '.join(name + b' ' + seconds for name in (b'discover', b'load', b'check', b'lower'))
    match = re.fullmatch(rb'xsht check: ([1-9][0-9]*) (file|files) in ' + seconds
                         + rb' \(thread time by stage: ' + stages + rb'\)\n', summary)
    if match is None or (match[1] == b'1') != (match[2] == b'file'):
        return stderr
    return b''.join(lines[:-1])


def validate_output(row, expected_stdout, *, expected_status=0, expected_depth_rejection=False, stage=None):
    status = row.get('status')
    if row.get('timed_out') is not False or type(status) is not int:
        raise ValueError('inference sample did not complete with an observed process status')
    stderr = base64.b64decode(row.get('stderr_base64', ''))
    stdout = base64.b64decode(row.get('stdout_base64', ''))
    diagnostics = check_diagnostics(stderr) if stage == 'check' else stderr
    if expected_depth_rejection:
        diagnostic = stderr.decode('utf-8', errors='replace').lower()
        if status != 2 or b'parse.nesting-depth' in stderr or 'type depth limit' not in diagnostic:
            raise ValueError('512-level deep-record control did not reach the semantic type-depth rejection')
    elif status != expected_status or stdout != expected_stdout or diagnostics or row.get('effects') != {}:
        raise ValueError(f'inference output/status/effects mismatch: status={status} stdout={stdout!r} stderr={stderr!r} effects={row.get("effects")!r}')
    return {'status': status, 'stdout_bytes': len(stdout), 'stdout_sha256': hashlib.sha256(stdout).hexdigest(),
            'stderr_bytes': len(stderr), 'stderr_sha256': hashlib.sha256(stderr).hexdigest(), 'effects': row['effects']}


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
