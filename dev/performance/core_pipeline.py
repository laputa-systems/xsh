import argparse
import base64
import os
from pathlib import Path
import signal
import sys
import tempfile
from types import SimpleNamespace

from dev.performance import check, core_cases, results, workloads

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'bench/consolidation'))
import run


class WorkloadFailure(ValueError):
    pass


def _report_cancellation(error):
    print('pipeline correctness cancelled')
    if isinstance(error, run.ProcessCleanupUnresolved):
        print(f'Detail: {error}')
    for note in getattr(error, '__notes__', ()):
        print(f'Detail: {note}')


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
        if row['status'] == 130:
            raise SystemExit(130)
        if row['timed_out'] or row['status'] == 2:
            raise ValueError(f'pipeline trace incomplete: status={row["status"]} timeout={row["timed_out"]} stderr={stderr[:4096]!r}')
        if row['status'] != 0:
            raise WorkloadFailure(f'pipeline trace failed: status={row["status"]} stderr={stderr[:4096]!r}')
        if (row['effects'] != {} or base64.b64decode(row['stdout_base64'], validate=True) != case.expected_stdout or stderr):
            raise WorkloadFailure(f'pipeline output or cleanup differs from the oracle: stderr={stderr[:4096]!r}')
        core_cases.validate_case(case, fixtures=fixtures)
        trace_rows = results._records(results._event_file(trace))
        counts = workloads.trace_counts(iter(trace_rows),
                                        expected_pipeline_segments=(2,) * core_cases.work_counts(case)['completed_pipelines'])
        if counts['subprocesses'] != case.expected_subprocesses:
            raise ValueError('pipeline helper count differs from the workload')
        return counts['subprocesses']


def _main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--xsht', type=Path, required=True)
    parser.add_argument('--scratch', type=Path, required=True)
    parser.add_argument('--shape', choices=('small', 'medium', 'large'), default='small')
    args = parser.parse_args()
    if sys.platform != 'linux' or not hasattr(os, 'pidfd_open'):
        print('pipeline correctness incomplete: Linux pidfd ownership is unavailable')
        return 2
    owner = Path(__file__).resolve().parents[2]
    signals = {signal.SIGINT, signal.SIGTERM}
    mask = signal.pthread_sigmask(signal.SIG_BLOCK, signals)
    previous = None
    try:
        previous = check.owned.install_signal_handlers()
        signal.pthread_sigmask(signal.SIG_SETMASK, mask)
        leases = check.baseline_owner(SimpleNamespace(source_root=owner), 'dev/util-linux/leases.py', 'core_pipeline_leases')
        try:
            with leases.campaign_leases(owner, native=True, cargo=True):
                count = check_pipeline(args.xsht, args.scratch, leases, args.shape)
        except leases.LeaseBusy as error:
            print(f'pipeline correctness incomplete: {error}')
            return 2
    except WorkloadFailure as error:
        print(f'pipeline correctness failed: {error}')
        return 1
    except run.ProcessCleanupUnresolved as error:
        if error.child_status == 130:
            _report_cancellation(error)
            return 130
        if (type(error.child_status) is int and
                (error.child_status > 0 and error.child_status != 2
                 or error.child_status < 0 and error.child_status != -signal.SIGKILL)):
            print(f'pipeline correctness failed: {error}')
            return 1
        print(f'pipeline correctness incomplete: {error}')
        return 2
    except (check.owned.SignalReceived, KeyboardInterrupt) as error:
        _report_cancellation(error)
        return 130
    except SystemExit as error:
        if error.code in (130, 143):
            _report_cancellation(error)
            return 130
        raise
    except (ValueError, OSError, RuntimeError) as error:
        print(f'pipeline correctness incomplete: {error}')
        return 2
    finally:
        interrupted = None
        while True:
            try:
                signal.pthread_sigmask(signal.SIG_BLOCK, signals)
                break
            except (check.owned.SignalReceived, KeyboardInterrupt) as error:
                interrupted = error
        if previous is not None:
            check.owned.restore_signal_handlers(previous)
        signal.pthread_sigmask(signal.SIG_SETMASK, mask)
        if interrupted is not None:
            raise interrupted
    print(f'pipeline correctness passed; {count} completed helpers')
    return 0


def main():
    try:
        return _main()
    except (check.owned.SignalReceived, KeyboardInterrupt) as error:
        _report_cancellation(error)
        return 130


if __name__ == '__main__':
    raise SystemExit(main())
