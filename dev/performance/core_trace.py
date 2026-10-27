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
_CASE_NAMES = ('core-pipeline-small', 'core-pipeline-medium', 'core-pipeline-large', 'core-sort-small', 'core-wc-small', 'core-wc-medium', 'core-awk-small', 'core-traversal-small', 'core-traversal-medium', 'core-traversal-large', 'core-spawn-small', 'core-spawn-medium', 'core-spawn-large')


def check_case(xsht, scratch, lease_owner, name='core-pipeline-small'):
    if name not in _CASE_NAMES:
        raise ValueError('unknown core trace workload')
    if sys.platform != 'linux' or not hasattr(os, 'pidfd_open'):
        raise ValueError('pipeline tracing requires Linux pidfd ownership')
    if not xsht.is_absolute() or xsht != xsht.resolve() or not xsht.is_file():
        raise ValueError('pipeline XSHT product must be an absolute regular file')
    if not scratch.is_absolute() or scratch != scratch.resolve() or not scratch.is_dir():
        raise ValueError('pipeline scratch must be an existing absolute directory')
    with tempfile.TemporaryDirectory(prefix='core-trace-', dir=scratch) as directory:
        work = Path(directory)
        fixtures = work / 'fixtures'
        execution = work / 'execution'
        fixtures.mkdir()
        execution.mkdir()
        case = core_cases.prepare_case(name, fixtures=fixtures)
        trace = work / 'trace.jsonl'
        command = [str(xsht), 'trace', '--syscalls', '--trace-format', 'jsonl', '--trace-file', str(trace),
                   *case.argv(xsht)[1:]]
        environment = {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'LANG': 'C', 'TZ': 'UTC',
                       'HOME': str(execution), 'TMPDIR': str(execution)}
        row = run.invoke(command, execution, environment, 60,
                         on_launch=lambda: lease_owner.prepare_cleanup_unresolved('core trace: process launch in progress'),
                         on_cleanup_verified=lease_owner.resolve_cleanup_unresolved)
        stderr = base64.b64decode(row['stderr_base64'], validate=True)
        if type(row['status']) is not int:
            raise ValueError('core trace did not report a child status')
        if row['timed_out']:
            raise ValueError('core trace timed out before completion')
        if row['status'] != 0:
            raise WorkloadFailure(f'core trace failed: status={row["status"]} timeout={row["timed_out"]} stderr={stderr[:4096]!r}')
        if (row['effects'] != {} or base64.b64decode(row['stdout_base64'], validate=True) != case.expected_stdout or stderr):
            raise WorkloadFailure(f'core output or cleanup differs from the oracle: stderr={stderr[:4096]!r}')
        core_cases.validate_case(case, fixtures=fixtures)
        trace_rows = results.trace_records(trace)
        segments = (2,) * core_cases.work_counts(case)['completed_pipelines'] if name.startswith('core-pipeline-') else None
        counts = workloads.trace_counts(iter(trace_rows), expected_pipeline_segments=segments)
        if counts['subprocesses'] != case.expected_subprocesses:
            raise WorkloadFailure('core helper count differs from the workload')
        return counts


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--xsht', type=Path, required=True)
    parser.add_argument('--scratch', type=Path, required=True)
    parser.add_argument('--case', choices=_CASE_NAMES, default='core-pipeline-small')
    args = parser.parse_args()
    def execute(leases):
        counts = check_case(args.xsht, args.scratch, leases, args.case)
        return f"{counts['subprocesses']} completed helpers; {counts['syscalls']} syscalls observed"
    return owned_case.execute('core trace correctness', execute)


if __name__ == '__main__':
    raise SystemExit(main())
