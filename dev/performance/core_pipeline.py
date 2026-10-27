import argparse
import base64
import os
from pathlib import Path
import sys
import tempfile

from dev.performance import core_cases, owned_case, results, workloads

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'bench/consolidation'))
import run


WorkloadFailure = owned_case.WorkloadFailure


def check_pipeline(xsht, scratch, lease_owner, shape='small'):
    if shape not in ('small', 'medium', 'large'):
        raise ValueError('unknown pipeline workload shape')
    if sys.platform != 'linux' or not hasattr(os, 'pidfd_open'):
        raise ValueError('pipeline tracing requires Linux pidfd ownership')
    if not xsht.is_absolute() or xsht != xsht.resolve() or not xsht.is_file():
        raise ValueError('pipeline XSHT product must be an absolute regular file')
    if not scratch.is_absolute() or scratch != scratch.resolve() or not scratch.is_dir():
        raise ValueError('pipeline scratch must be an existing absolute directory')
    with tempfile.TemporaryDirectory(prefix='core-pipeline-', dir=scratch) as directory:
        work = Path(directory)
        fixtures = work / 'fixtures'
        execution = work / 'execution'
        fixtures.mkdir()
        execution.mkdir()
        case = core_cases.prepare_case(f'core-pipeline-{shape}', fixtures=fixtures)
        trace = work / 'trace.jsonl'
        command = [str(xsht), 'trace', '--syscalls', '--trace-format', 'jsonl', '--trace-file', str(trace),
                   *case.argv(xsht)[1:]]
        environment = {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'LANG': 'C', 'TZ': 'UTC',
                       'HOME': str(execution), 'TMPDIR': str(execution)}
        row = run.invoke(command, execution, environment, 60,
                         on_launch=lambda: lease_owner.prepare_cleanup_unresolved('core pipeline trace: process launch in progress'),
                         on_cleanup_verified=lease_owner.resolve_cleanup_unresolved)
        stderr = base64.b64decode(row['stderr_base64'], validate=True)
        if type(row['status']) is not int:
            raise ValueError('pipeline trace did not report a child status')
        if row['timed_out']:
            raise ValueError('pipeline trace timed out before completion')
        if row['status'] != 0:
            raise WorkloadFailure(f'pipeline trace failed: status={row["status"]} timeout={row["timed_out"]} stderr={stderr[:4096]!r}')
        if (row['effects'] != {} or base64.b64decode(row['stdout_base64'], validate=True) != case.expected_stdout or stderr):
            raise WorkloadFailure(f'pipeline output or cleanup differs from the oracle: stderr={stderr[:4096]!r}')
        core_cases.validate_case(case, fixtures=fixtures)
        trace_rows = results._records(results._event_file(trace))
        counts = workloads.trace_counts(iter(trace_rows),
                                        expected_pipeline_segments=(2,) * core_cases.work_counts(case)['completed_pipelines'])
        if counts['subprocesses'] != case.expected_subprocesses:
            raise ValueError('pipeline helper count differs from the workload')
        return counts['subprocesses']


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--xsht', type=Path, required=True)
    parser.add_argument('--scratch', type=Path, required=True)
    parser.add_argument('--shape', choices=('small', 'medium', 'large'), default='small')
    args = parser.parse_args()
    return owned_case.execute(
        'pipeline correctness',
        lambda leases: f"{check_pipeline(args.xsht, args.scratch, leases, args.shape)} completed helpers",
    )


if __name__ == '__main__':
    raise SystemExit(main())
