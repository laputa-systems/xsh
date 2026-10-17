import atexit
from contextlib import contextmanager
import ctypes
import errno
import json
import os
import select
import signal
import sys
import time
import traceback


IPC_PRIVATE = 0
IPC_CREAT = 0o1000
IPC_RMID = 0
GETVAL = 12
GETPID = 11
GETNCNT = 14
GETZCNT = 15
OWNER_UID = 10001
OWNER_GID = 10001
HANDLED_SIGNALS = {signal.SIGTERM, signal.SIGINT, signal.SIGHUP}

libc = ctypes.CDLL(None, use_errno=True)
semget = libc.semget
semget.restype = ctypes.c_int
semget.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int]
semctl = libc.semctl
semctl.restype = ctypes.c_int
semctl.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int]
semop = libc.semop
semop.restype = ctypes.c_int
semop.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_size_t]


class Sembuf(ctypes.Structure):
    _fields_ = [
        ("sem_num", ctypes.c_ushort),
        ("sem_op", ctypes.c_short),
        ("sem_flg", ctypes.c_short),
    ]


semaphore_id = None
waiter_pids = []
readiness_fds = []
cleaning = False
cleaned = False


@contextmanager
def owner_scope():
    previous_mask = signal.pthread_sigmask(signal.SIG_BLOCK, HANDLED_SIGNALS)
    previous_uid = os.geteuid()
    previous_gid = os.getegid()
    try:
        os.setegid(OWNER_GID)
        os.seteuid(OWNER_UID)
        yield
    finally:
        os.seteuid(previous_uid)
        os.setegid(previous_gid)
        signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)


def require_zero(result, operation):
    if result != 0:
        number = ctypes.get_errno()
        raise OSError(number, f"{operation}: {os.strerror(number)}")


def require_nonnegative(result, operation):
    if result < 0:
        number = ctypes.get_errno()
        raise OSError(number, f"{operation}: {os.strerror(number)}")
    return result


def semaphore_operation(identity, index, operation):
    entry = Sembuf(index, operation, 0)
    require_zero(semop(identity, ctypes.byref(entry), 1), "semop")


def semaphore_element(identity, index):
    return {
        "value": require_nonnegative(semctl(identity, index, GETVAL), "GETVAL"),
        "waiting_for_increase": require_nonnegative(semctl(identity, index, GETNCNT), "GETNCNT"),
        "waiting_for_zero": require_nonnegative(semctl(identity, index, GETZCNT), "GETZCNT"),
        "pid": require_nonnegative(semctl(identity, index, GETPID), "GETPID"),
    }


def kernel_snapshot():
    with owner_scope():
        elements = [semaphore_element(semaphore_id, index) for index in range(2)]
    return {
        "pid": os.getpid(),
        "owner_uid": OWNER_UID,
        "owner_gid": OWNER_GID,
        "semaphore_id": semaphore_id,
        "waiters": list(waiter_pids),
        "elements": elements,
    }


def child_exec(identity, index, readiness_fd, previous_mask):
    os.close(readiness_fd[0])
    for number in HANDLED_SIGNALS:
        signal.signal(number, signal.SIG_DFL)
    signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)
    os.set_inheritable(readiness_fd[1], True)
    script = os.path.realpath(__file__)
    arguments = [
        "/bin/setpriv",
        "setpriv",
        f"--reuid={OWNER_UID}",
        f"--regid={OWNER_GID}",
        "--clear-groups",
        "--bounding-set=-all",
        "--inh-caps=-all",
        "--ambient-caps=-all",
        "--no-new-privs",
        "--",
        "/usr/bin/python3",
        script,
        "--waiter",
        str(identity),
        str(index),
        str(readiness_fd[1]),
    ]
    os.execv(arguments[0], arguments)


def spawn_waiter(identity, index):
    previous_mask = signal.pthread_sigmask(signal.SIG_BLOCK, HANDLED_SIGNALS)
    read_fd, write_fd = os.pipe()
    child_pid = None
    try:
        child_pid = os.fork()
        if child_pid == 0:
            try:
                child_exec(identity, index, (read_fd, write_fd), previous_mask)
            except BaseException:
                traceback.print_exc()
                os._exit(127)
        waiter_pids.append(child_pid)
        readiness_fds.append((child_pid, read_fd))
        read_fd = None
        os.close(write_fd)
        write_fd = None
        return child_pid
    finally:
        if read_fd is not None:
            os.close(read_fd)
        if write_fd is not None:
            os.close(write_fd)
        signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)


def wait_for_readiness():
    pending = {descriptor: pid for pid, descriptor in readiness_fds}
    deadline = time.monotonic() + 5.0
    while pending:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("waiter children did not report setup readiness")
        readable, _, _ = select.select(list(pending), [], [], remaining)
        if not readable:
            raise TimeoutError("waiter children did not report setup readiness")
        for descriptor in readable:
            data = os.read(descriptor, 1)
            pid = pending.pop(descriptor)
            os.close(descriptor)
            readiness_fds.remove((pid, descriptor))
            if data != b"R":
                raise RuntimeError(f"waiter {pid} exited before setup readiness")


def require_waiters_alive():
    for pid in list(waiter_pids):
        try:
            completed, _ = os.waitpid(pid, os.WNOHANG)
        except ChildProcessError:
            waiter_pids.remove(pid)
            raise RuntimeError(f"waiter {pid} could not be reaped")
        if completed == pid:
            waiter_pids.remove(pid)
            raise RuntimeError(f"waiter {pid} exited before kernel readiness")


def wait_for_kernel_waiters():
    deadline = time.monotonic() + 5.0
    while True:
        require_waiters_alive()
        state = kernel_snapshot()
        elements = state["elements"]
        if (elements[0]["waiting_for_increase"], elements[0]["waiting_for_zero"],
                elements[1]["waiting_for_increase"], elements[1]["waiting_for_zero"]) == (1, 0, 0, 1):
            expected = [
                {"value": 0, "waiting_for_increase": 1, "waiting_for_zero": 0, "pid": waiter_pids[0]},
                {"value": 1, "waiting_for_increase": 0, "waiting_for_zero": 1, "pid": waiter_pids[1]},
            ]
            if elements != expected:
                raise RuntimeError(f"kernel waiter state differs from the registered children: {elements}")
            return state
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(f"kernel did not report both blocked semaphore operations: {elements}")
        select.select([], [], [], min(0.005, remaining))


def close_readiness_fds():
    while readiness_fds:
        _, descriptor = readiness_fds.pop()
        os.close(descriptor)


def stop_waiters():
    active = set(waiter_pids)
    for pid in active:
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + 1.0
    while active:
        for pid in list(active):
            try:
                completed, _ = os.waitpid(pid, os.WNOHANG)
            except InterruptedError:
                continue
            except ChildProcessError:
                completed = pid
            if completed == pid:
                active.remove(pid)
                waiter_pids.remove(pid)
        if not active:
            return
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        select.select([], [], [], min(0.005, remaining))
    for pid in active:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    for pid in active:
        while True:
            try:
                completed, _ = os.waitpid(pid, 0)
                if completed == pid:
                    break
            except InterruptedError:
                continue
            except ChildProcessError:
                break
        waiter_pids.remove(pid)


def cleanup():
    global cleaning, cleaned
    if cleaned or cleaning:
        return
    cleaning = True
    previous_mask = signal.pthread_sigmask(signal.SIG_BLOCK, HANDLED_SIGNALS)
    failure = None
    try:
        stop_waiters()
        close_readiness_fds()
        if semaphore_id is not None:
            with owner_scope():
                require_zero(semctl(semaphore_id, 0, IPC_RMID), "IPC_RMID")
        cleaned = True
    except BaseException as error:
        failure = error
    finally:
        cleaning = False
        signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)
    if failure is not None:
        raise failure


def stop(number, frame):
    for handled in HANDLED_SIGNALS:
        signal.signal(handled, signal.SIG_IGN)
    try:
        cleanup()
    except BaseException as error:
        print(f"fixture cleanup failed: {error}", file=sys.stderr, flush=True)
    raise SystemExit(128 + number)


def assert_waiter_identity():
    expected_uid = (OWNER_UID, OWNER_UID, OWNER_UID)
    expected_gid = (OWNER_GID, OWNER_GID, OWNER_GID)
    if os.getresuid() != expected_uid or os.getresgid() != expected_gid or os.getgroups() != []:
        raise RuntimeError("waiter did not start with the registered owner identity")
    with open("/proc/self/status", encoding="ascii") as source:
        capabilities = {
            name: value.strip()
            for line in source
            for name, _, value in [line.partition(":")]
            if name in {"CapEff", "CapPrm", "CapInh", "CapAmb", "CapBnd"}
        }
    if len(capabilities) != 5 or any(int(value, 16) != 0 for value in capabilities.values()):
        raise RuntimeError("waiter retained capabilities")


def waiter_main(arguments):
    identity, index, readiness_fd = [int(value) for value in arguments]
    assert_waiter_identity()
    if index == 0:
        semaphore_operation(identity, index, 1)
        semaphore_operation(identity, index, -1)
        blocked_operation = -1
    elif index == 1:
        semaphore_operation(identity, index, 1)
        blocked_operation = 0
    else:
        raise ValueError(f"unsupported semaphore index {index}")
    if os.write(readiness_fd, b"R") != 1:
        raise OSError("waiter readiness pipe accepted no data")
    os.close(readiness_fd)
    semaphore_operation(identity, index, blocked_operation)
    raise RuntimeError("blocked semop unexpectedly completed")


def probe_main(arguments):
    if len(arguments) != 1:
        raise ValueError("probe requires exactly one semaphore ID")
    identity = int(arguments[0])
    result = semctl(identity, 0, GETVAL)
    if result < 0:
        number = ctypes.get_errno()
        if number != errno.EINVAL:
            raise OSError(number, f"GETVAL: {os.strerror(number)}")
        print(json.dumps({"exists": False, "errno": number, "value": None}, separators=(",", ":")), flush=True)
    else:
        print(json.dumps({"exists": True, "errno": None, "value": result}, separators=(",", ":")), flush=True)
    return 0


def serve():
    global semaphore_id
    with owner_scope():
        semaphore_id = require_nonnegative(semget(IPC_PRIVATE, 2, IPC_CREAT | 0o600), "semget")
    for index in range(2):
        spawn_waiter(semaphore_id, index)
    wait_for_readiness()
    ready = wait_for_kernel_waiters()
    print(json.dumps(ready, separators=(",", ":")), flush=True)
    for request in sys.stdin:
        request = request.rstrip("\n")
        if request == "DONE":
            return
        if request == "snapshot":
            print(json.dumps(kernel_snapshot(), separators=(",", ":")), flush=True)
            continue
        raise ValueError(f"unknown fixture request {request!r}")


def keeper_main():
    for number in HANDLED_SIGNALS:
        signal.signal(number, stop)
    atexit.register(cleanup)
    try:
        serve()
    except BaseException:
        try:
            cleanup()
        except BaseException as error:
            print(f"fixture cleanup failed: {error}", file=sys.stderr, flush=True)
        raise
    cleanup()
    return 0


def main():
    if len(sys.argv) == 1:
        return keeper_main()
    if sys.argv[1] == "--waiter":
        return waiter_main(sys.argv[2:])
    if sys.argv[1] == "--probe":
        return probe_main(sys.argv[2:])
    raise ValueError(f"unknown fixture mode {sys.argv[1]!r}")


if __name__ == "__main__":
    sys.exit(main())
