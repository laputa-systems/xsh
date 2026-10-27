import base64
from enum import Enum
import hashlib
from pathlib import Path

from dev.performance import inference_measurement as measurement
from dev.performance.inference_workloads import fixed_inference_workloads


class FrontendTier(str, Enum):
    EDIT = 'edit'
    CHANGE = 'change'


_EDIT_INPUTS = ('dependency-chain-chain-64', 'generic-stream-items-64')
_CHANGE_INPUTS = _EDIT_INPUTS + (
    'alias-chain-chain-64', 'records-wide-64', 'higher-order-composition-chain-64',
    'recursive-effect-component-component-64', 'module-diamond-diamonds-64',
    'finite-overload-chain-chain-64',
)
_DISPOSAL_CONTROLS = (
    'runtime::eval::indexed::full::tests::user_schemes_execute_after_frontend_drop_on_both_routes',
    'runtime::eval::indexed::full::tests::compact_entry_executes_after_all_frontend_and_adapter_scratch_is_dropped',
)


def representatives(tier):
    tier = FrontendTier(tier)
    selected = _EDIT_INPUTS if tier == FrontendTier.EDIT else _CHANGE_INPUTS
    available = {workload.name: workload for workload in fixed_inference_workloads()}
    return tuple(available[name] for name in selected)


def prepare_cases(tier, *, root):
    workloads = representatives(tier)
    root = Path(root)
    if root.parent != root.parent.resolve() or not root.parent.is_dir() or root.is_symlink():
        raise ValueError('frontend input parent must be an existing nonsymlink directory')
    return measurement.source_maps(workloads, root)


def _canonical_workload(workload):
    available = {item.name: item for item in representatives(FrontendTier.CHANGE)}
    if workload.name not in available or workload != available[workload.name]:
        raise ValueError('frontend workload does not match its existing generator')


def validate_sources(workload, directories):
    _canonical_workload(workload)
    if set(directories) != set(measurement.VARIANTS):
        raise ValueError('frontend case requires both annotation variants')
    for variant, sources in (('annotated', workload.annotated_sources), ('reduced', workload.reduced_sources)):
        directory = Path(directories[variant])
        if directory != directory.resolve():
            raise ValueError('frontend source root must not be linked')
        expected = {relative: hashlib.sha256(content.encode()).hexdigest() for relative, content in sources.items()}
        if measurement.source_map_snapshot(directory) != expected:
            raise ValueError('frontend source map changed')


def diagnostic_inputs(workload, directory, *, xsh, xsht, phase_report):
    _canonical_workload(workload)
    entry = Path(directory) / workload.entry
    return {
        'required_profile': 'verification',
        'frontend_argv': (str(xsht), 'frontend-stats', '--json', str(entry)),
        'runtime_argv': (str(xsh), str(entry)),
        'phase_argv': (str(xsht), 'runtime-stats', '--json', str(phase_report), str(entry)),
        'required_disposal_controls': _DISPOSAL_CONTROLS,
        'input_units': workload.expected_units,
        'source_files': len(workload.annotated_sources),
        'logical_output_shape': workload.logical_output_shape,
        'logical_evidence_shape': workload.logical_evidence_shape,
        'expected_stdout': workload.expected_stdout,
    }


def _frontend_evidence(row, entry):
    stdout = base64.b64decode(row.get('stdout_base64', ''), validate=True)
    process = measurement.validate_output(row, stdout)
    value = measurement.frontend_json(stdout, entry)
    record = measurement.frontend_record(value)
    metrics = measurement.canonical_inference_metrics(value)
    traffic = measurement.canonical_allocation_traffic(value)
    if len(record['stages']) != len(measurement.ALLOCATION_STAGES):
        raise ValueError('frontend allocation stage duplicated')
    for name in ('diagnostics', 'lowered_blocker_events', 'reconcile_delta'):
        if type(record.get(name)) is not int or record[name] != 0:
            raise ValueError(f'frontend preparation did not complete cleanly: {name}')
    for name in ('function_body_check_entries', 'output_type_nodes', 'evidence_nodes'):
        if metrics[name] == 0:
            raise ValueError(f'frontend checked evidence unavailable: {name}')
    if not all(stage['tracking_active'] for stage in traffic.values()):
        raise ValueError('frontend allocation tracking incomplete')
    retained = record.get('retained_after_drop_bytes')
    constructed = record.get('lowered_constructed_functions')
    if (type(retained) is not int or retained < 0 or retained != traffic['after_drop']['retained_bytes']
            or type(constructed) is not int or constructed <= 0
            or traffic['after_drop']['item_count'] != constructed):
        raise ValueError('frontend disposal accounting disagrees with retained executable')
    return {'process': process, 'inference_work': metrics, 'allocation_traffic': traffic,
            'retained_after_frontend_drop_bytes': retained}


def validate_case_evidence(workload, *, directories, frontend_rows, runtime_rows, phase_diagnostics):
    validate_sources(workload, directories)
    for rows in (frontend_rows, runtime_rows, phase_diagnostics):
        if set(rows) != set(measurement.VARIANTS):
            raise ValueError('frontend evidence requires both annotation variants')
    variants = {}
    for variant in measurement.VARIANTS:
        entry = Path(directories[variant]) / workload.entry
        frontend = _frontend_evidence(frontend_rows[variant], entry)
        runtime = measurement.validate_output(runtime_rows[variant], workload.expected_stdout)
        phases = phase_diagnostics[variant]
        if (not measurement.phase_diagnostic_valid(phases) or type(phases.get('status')) is not int
                or phases.get('stdout_bytes') != runtime['stdout_bytes']
                or phases.get('stdout_sha256') != runtime['stdout_sha256']
                or phases.get('stderr_bytes') != 0):
            raise ValueError('frontend preparation phases do not describe the expected runtime work')
        variants[variant] = {'frontend': frontend, 'runtime': runtime, 'phases': phases}
    return {
        'workload': workload.name,
        'input_units': workload.expected_units,
        'source_files_per_variant': len(workload.annotated_sources),
        'logical_output_shape': workload.logical_output_shape,
        'logical_evidence_shape': workload.logical_evidence_shape,
        'variants': variants,
        'frontend_disposal_execution_complete': False,
        'required_disposal_controls': _DISPOSAL_CONTROLS,
        'scaling_complete': False,
    }
