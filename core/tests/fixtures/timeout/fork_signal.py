import argparse
import ctypes
import os
from pathlib import Path
import platform
import select
import signal
import sys
import tempfile


PTRACE_TRACEME = 0
PTRACE_SYSCALL = 24
PTRACE_SETOPTIONS = 0x4200
PTRACE_GETEVENTMSG = 0x4201
PTRACE_GET_SYSCALL_INFO = 0x420E
PTRACE_O_TRACESYSGOOD = 1
PTRACE_O_TRACEFORK = 2
PTRACE_O_TRACEVFORK = 4
PTRACE_O_TRACECLONE = 8
PTRACE_O_TRACEEXEC = 16
PTRACE_O_EXITKILL = 0x100000
PTRACE_FORK_EVENTS = {1, 2, 3}
SYSCALL_ENTRY = 1
SYSCALL_EXIT = 2
AUDIT_ARCH_X86_64 = 0xC000003E
SYS_READ = 0
SYS_FORKS = {56, 57, 58}
SYS_KILL = 62
PR_SET_CHILD_SUBREAPER = 36
CANCELLATION = {signal.SIGINT, signal.SIGTERM, signal.SIGHUP}
MAX_CAPTURE = 4096


class SyscallInfo(ctypes.Structure):
    _fields_ = [
        ("op", ctypes.c_uint8), ("reserved", ctypes.c_uint8),
        ("flags", ctypes.c_uint16), ("arch", ctypes.c_uint32),
        ("instruction_pointer", ctypes.c_uint64), ("stack_pointer", ctypes.c_uint64),
        ("data", ctypes.c_uint64 * 8),
    ]


libc = ctypes.CDLL(None, use_errno=True)
libc.ptrace.argtypes = [ctypes.c_uint, ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p]
libc.ptrace.restype = ctypes.c_long


def ptrace(request, pid, address=0, data=0):
    result = libc.ptrace(request, pid, address, data)
    if result == -1:
        number = ctypes.get_errno()
        raise OSError(number, f"ptrace request={request} pid={pid}: {os.strerror(number)}")
    return result


def deadline(number, frame):
    raise TimeoutError("timeout signal schedule exceeded its safety bound")


def cancelled(number, frame):
    raise InterruptedError(f"timeout signal schedule cancelled by signal {number}")


def record_process(pid, processes):
    status = Path(f"/proc/{pid}/status").read_text()
    tgid = next(int(line.split()[1]) for line in status.splitlines() if line.startswith("Tgid:"))
    if tgid == pid and pid not in processes:
        processes[pid] = os.pidfd_open(pid)


def fork_wrapper(words, output, errors, children, processes):
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, CANCELLATION | {signal.SIGALRM})
    try:
        pid = os.fork()
        if pid == 0:
            try:
                for number in CANCELLATION | {signal.SIGALRM, signal.SIGCHLD}:
                    signal.signal(number, signal.SIG_DFL)
                signal.pthread_sigmask(signal.SIG_SETMASK, previous)
                os.setpgid(0, 0)
                for target, path in [(0, "/dev/null"), (1, output), (2, errors)]:
                    flags = os.O_RDONLY if target == 0 else os.O_WRONLY | os.O_CREAT | os.O_EXCL
                    descriptor = os.open(path, flags, 0o600)
                    os.dup2(descriptor, target)
                    if descriptor != target:
                        os.close(descriptor)
                ptrace(PTRACE_TRACEME, 0)
                os.kill(os.getpid(), signal.SIGSTOP)
                os.execv(words[0], words)
            except BaseException as failure:
                os.write(2, f"tracee setup: {failure}\n".encode(errors="backslashreplace"))
                os._exit(125)
        children.add(pid)
        record_process(pid, processes)
        return pid
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)


def trace_schedule(wrapper, ready, control, children, processes):
    observed, status = os.waitpid(wrapper, 0)
    if observed != wrapper or not os.WIFSTOPPED(status) or os.WSTOPSIG(status) != signal.SIGSTOP:
        raise RuntimeError("wrapper failed before its owned PTRACE_TRACEME stop")
    options = (PTRACE_O_TRACESYSGOOD | PTRACE_O_TRACEFORK | PTRACE_O_TRACEVFORK
               | PTRACE_O_TRACECLONE | PTRACE_O_TRACEEXEC | PTRACE_O_EXITKILL)
    ptrace(PTRACE_SETOPTIONS, wrapper, data=options)
    ptrace(PTRACE_SYSCALL, wrapper)
    pending = {}
    unannounced = set()
    held_shell = None
    injected = False
    forwarded = False
    post_signal_forks = 0
    wrapper_status = None
    stops = 0
    while children:
        pid, status = os.waitpid(-1, 0)
        if os.WIFEXITED(status) or os.WIFSIGNALED(status):
            children.discard(pid)
            if pid == wrapper:
                wrapper_status = os.waitstatus_to_exitcode(status)
            continue
        if pid not in children:
            status_fields = Path(f"/proc/{pid}/status").read_text().splitlines()
            tracer = next(int(line.split()[1]) for line in status_fields if line.startswith("TracerPid:"))
            if tracer != os.getpid():
                raise RuntimeError(f"observer encountered an unowned tracee {pid}")
            children.add(pid)
            record_process(pid, processes)
            unannounced.add(pid)
            continue
        stops += 1
        if stops > 10000:
            raise RuntimeError("observer exceeded its syscall stop bound")
        event = status >> 16
        stopped = os.WSTOPSIG(status)
        if event in PTRACE_FORK_EVENTS:
            previous = signal.pthread_sigmask(signal.SIG_BLOCK, CANCELLATION | {signal.SIGALRM})
            try:
                child = ctypes.c_ulong()
                ptrace(PTRACE_GETEVENTMSG, pid, data=ctypes.byref(child))
                children.add(child.value)
                if len(children) > 16:
                    raise RuntimeError("observer exceeded its owned tracee bound")
                record_process(child.value, processes)
                if child.value in unannounced:
                    unannounced.remove(child.value)
                    ptrace(PTRACE_SYSCALL, child.value)
            finally:
                signal.pthread_sigmask(signal.SIG_SETMASK, previous)
            if forwarded and Path(f"/proc/{pid}/exe").resolve().name == "bash":
                post_signal_forks += 1
        if stopped == (signal.SIGTRAP | 0x80):
            info = SyscallInfo()
            available = ptrace(PTRACE_GET_SYSCALL_INFO, pid, ctypes.sizeof(info), ctypes.byref(info))
            if info.arch != AUDIT_ARCH_X86_64:
                raise RuntimeError("observer requires Linux x86-64 tracees")
            if info.op == SYSCALL_ENTRY:
                if available < 80:
                    raise RuntimeError("kernel returned a truncated syscall entry")
                number, *arguments = info.data
                pending[pid] = (number, arguments)
                boundary = (number in SYS_FORKS if control == "foreground-fork"
                            else number == SYS_READ and arguments[0] == 0)
                if (not injected and boundary and ready.exists()
                        and Path(f"/proc/{pid}/exe").resolve().name == "bash"):
                    held_shell = pid
                    injected = True
                    signal.pidfd_send_signal(processes[wrapper], signal.SIGINT)
                    continue
            elif info.op == SYSCALL_EXIT:
                if available < 33:
                    raise RuntimeError("kernel returned a truncated syscall exit")
                number, arguments = pending.pop(pid, (None, []))
                if (held_shell is not None and number == SYS_KILL
                        and ctypes.c_int64(arguments[0]).value == -held_shell
                        and arguments[1] == signal.SIGINT):
                    if ctypes.c_int64(info.data[0]).value != 0:
                        raise RuntimeError("XSH failed to forward SIGINT to its owned shell group")
                    forwarded = True
                    ptrace(PTRACE_SYSCALL, held_shell)
                    held_shell = None
        delivered = 0 if event or stopped in {signal.SIGTRAP | 0x80, signal.SIGSTOP} else stopped
        ptrace(PTRACE_SYSCALL, pid, data=delivered)
    if not injected or not forwarded:
        raise AssertionError("controlled signal schedule did not reach its forwarding barrier")
    expected_forks = 1 if control == "foreground-fork" else 0
    if post_signal_forks != expected_forks:
        raise AssertionError(f"expected {expected_forks} shell forks after forwarding, got {post_signal_forks}")
    return wrapper_status


def require_exited(processes):
    for pid, descriptor in processes.items():
        exited = select.poll()
        exited.register(descriptor, select.POLLIN)
        if not exited.poll(0):
            raise RuntimeError(f"owned process survived: {pid}")


def stop_children(children, processes):
    for number in CANCELLATION:
        signal.signal(number, signal.SIG_IGN)
    signal.setitimer(signal.ITIMER_REAL, 2)
    try:
        for descriptor in processes.values():
            try:
                signal.pidfd_send_signal(descriptor, signal.SIGKILL)
            except ProcessLookupError:
                pass
        for pid in children:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        while True:
            try:
                pid, status = os.waitpid(-1, 0)
            except ChildProcessError:
                break
            if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                children.discard(pid)
            elif os.WIFSTOPPED(status):
                os.kill(pid, signal.SIGKILL)
        if children:
            raise RuntimeError(f"owned tracees were not reaped: {sorted(children)}")
        require_exited(processes)
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        for descriptor in processes.values():
            os.close(descriptor)


def capture(path):
    with path.open("rb") as source:
        data = source.read(MAX_CAPTURE + 1)
    if len(data) > MAX_CAPTURE:
        raise RuntimeError(f"tracee output exceeded {MAX_CAPTURE} bytes")
    return data


def main():
    parser = argparse.ArgumentParser(
        description="Prove the timeout fixture's fork-after-SIGINT race with owned Linux x86-64 ptrace children.",
        epilog=("Run once with --control foreground-fork (negative: status 130, empty stdout), "
                "then --control builtin-read (positive: status 130, stdout forwarded). "
                "Example: python3 core/tests/fixtures/timeout/fork_signal.py --xsh /products/xsh "
                "--timeout-script core/timeout.xsh --scratch .work/tmp --control builtin-read. "
                "Requires /bin/sh to be Bash, procfs, pidfds, PTRACE_GET_SYSCALL_INFO, "
                "and permission to trace owned children; Docker needs SYS_PTRACE. "
                "Each run has a 5-second execution and 2-second cleanup safety bound."))
    parser.add_argument("--xsh", required=True, type=Path)
    parser.add_argument("--timeout-script", required=True, type=Path)
    parser.add_argument("--scratch", required=True, type=Path)
    parser.add_argument("--control", required=True, choices=["foreground-fork", "builtin-read"])
    args = parser.parse_args()
    if sys.platform != "linux" or platform.machine() != "x86_64" or ctypes.sizeof(ctypes.c_long) != 8:
        raise RuntimeError("timeout fork signal observer supports Linux x86-64 only")
    if Path("/bin/sh").resolve().name != "bash":
        raise RuntimeError("timeout fork signal controls require /bin/sh to be Bash")
    xsh = args.xsh.resolve(strict=True)
    script = args.timeout_script.resolve(strict=True)
    scratch = args.scratch.resolve(strict=True)
    if not os.access(xsh, os.X_OK) or not scratch.is_dir():
        raise RuntimeError("--xsh must be executable and --scratch must be an existing directory")
    if libc.prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) != 0:
        raise OSError(ctypes.get_errno(), "cannot own orphaned fixture descendants")
    signal.signal(signal.SIGCHLD, signal.SIG_DFL)
    signal.signal(signal.SIGALRM, deadline)
    for number in CANCELLATION:
        signal.signal(number, cancelled)
    children = set()
    processes = {}
    with tempfile.TemporaryDirectory(prefix="timeout-signal-", dir=scratch) as directory:
        root = Path(directory)
        ready = root / "ready"
        fifo = root / "input"
        os.mkfifo(fifo, 0o600)
        body = "trap 'printf forwarded; exit 42' INT; printf ready > \"$1\"; while :; do sleep 1; done"
        if args.control == "builtin-read":
            body = "exec 3<> \"$2\"; trap 'printf forwarded; exit 42' INT; printf ready > \"$1\"; while :; do read -r ignored <&3; done"
        words = [str(xsh), str(script), "10", "sh", "-c", body, "sh", str(ready), str(fifo)]
        primary = None
        try:
            signal.setitimer(signal.ITIMER_REAL, 5)
            wrapper = fork_wrapper(words, root / "stdout", root / "stderr", children, processes)
            status = trace_schedule(wrapper, ready, args.control, children, processes)
            stdout = capture(root / "stdout")
            stderr = capture(root / "stderr")
            expected = b"" if args.control == "foreground-fork" else b"forwarded"
            if status != 130 or stdout != expected or stderr != b"":
                raise AssertionError(f"{args.control}: status={status}, stdout={stdout!r}, stderr={stderr!r}")
            if ready.read_bytes() != b"ready":
                raise AssertionError("shell readiness marker was not written")
            require_exited(processes)
        except BaseException as failure:
            primary = failure
            raise
        finally:
            try:
                stop_children(children, processes)
            except BaseException as cleanup:
                if primary is not None:
                    raise RuntimeError(f"{primary}; owned child cleanup also failed: {cleanup}") from primary
                raise
        print(f"{args.control}: status={status}, stdout={stdout!r}, stderr={stderr!r}; no owned descendants remain")


if __name__ == "__main__":
    main()
