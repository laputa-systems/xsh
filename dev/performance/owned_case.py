import os
from pathlib import Path
import signal
import sys
from types import SimpleNamespace

from dev.performance import check

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'bench/consolidation'))
import run


class WorkloadFailure(ValueError):
    pass


def _report_cancellation(label, error):
    print(f'{label} cancelled')
    if isinstance(error, run.ProcessCleanupUnresolved):
        print(f'Detail: {error}')
    for note in getattr(error, '__notes__', ()):
        print(f'Detail: {note}')


def _execute(label, operation):
    if sys.platform != 'linux' or not hasattr(os, 'pidfd_open'):
        print(f'{label} incomplete: Linux pidfd ownership is unavailable')
        return 2
    owner = Path(__file__).resolve().parents[2]
    signals = {signal.SIGINT, signal.SIGTERM}
    mask = signal.pthread_sigmask(signal.SIG_BLOCK, signals)
    previous = None
    try:
        previous = check.owned.install_signal_handlers()
        signal.pthread_sigmask(signal.SIG_SETMASK, mask)
        leases = check.baseline_owner(SimpleNamespace(source_root=owner), 'dev/util-linux/leases.py', 'owned_case_leases')
        try:
            with leases.campaign_leases(owner, native=True):
                detail = operation(leases)
        except leases.LeaseBusy as error:
            print(f'{label} incomplete: {error}')
            return 2
    except WorkloadFailure as error:
        print(f'{label} failed: {error}')
        return 1
    except run.ProcessCleanupUnresolved as error:
        if (type(error.child_status) is int and
                (error.child_status > 0
                 or error.child_status < 0 and error.child_status != -signal.SIGKILL)):
            print(f'{label} failed: {error}')
            return 1
        print(f'{label} incomplete: {error}')
        return 2
    except (check.owned.SignalReceived, KeyboardInterrupt) as error:
        _report_cancellation(label, error)
        return 130
    except SystemExit as error:
        if error.code in (130, 143):
            _report_cancellation(label, error)
            return 130
        raise
    except (ValueError, OSError, RuntimeError) as error:
        print(f'{label} incomplete: {error}')
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
    print(f'{label} passed; {detail}')
    return 0


def execute(label, operation):
    try:
        return _execute(label, operation)
    except (check.owned.SignalReceived, KeyboardInterrupt) as error:
        _report_cancellation(label, error)
        return 130
