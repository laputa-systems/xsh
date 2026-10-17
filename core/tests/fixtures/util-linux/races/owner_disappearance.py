import base64
import ctypes
import json
import os
import platform
from pathlib import Path
import signal
import sys
import tempfile


PTRACE_TRACEME = 0
PTRACE_PEEKDATA = 2
PTRACE_CONT = 7
PTRACE_SYSCALL = 24
PTRACE_SETOPTIONS = 0x4200
PTRACE_GETEVENTMSG = 0x4201
PTRACE_GET_SYSCALL_INFO = 0x420E
PTRACE_O_TRACESYSGOOD = 1
PTRACE_O_TRACEFORK = 2
PTRACE_O_TRACEVFORK = 4
PTRACE_O_TRACEEXEC = 16
PTRACE_O_EXITKILL = 0x100000
PTRACE_EVENT_FORK = 1
PTRACE_EVENT_VFORK = 2
PTRACE_EVENT_EXEC = 4
SYSCALL_ENTRY = 1
SYSCALL_EXIT = 2
AUDIT_ARCH_X86_64 = 0xC000003E
SYS_READ = 0
MAX_SNAPSHOT = 65536
MAX_CAPTURE = 8192
CANCELLATION = {signal.SIGTERM, signal.SIGINT, signal.SIGHUP}


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
    ctypes.set_errno(0)
    result = libc.ptrace(request, pid, address, data)
    number = ctypes.get_errno()
    if result == -1 and number:
        raise OSError(number, f"ptrace request={request} pid={pid}: {os.strerror(number)}")
    return result


def timeout(number, frame):
    raise TimeoutError("owned syscall observer exceeded its deadline")


def cancelled(number, frame):
    raise InterruptedError(f"owned syscall observer cancelled by signal {number}")


def emit(record):
    line = json.dumps(record, separators=(",", ":"))
    if len(line.encode()) > 65535:
        raise ValueError("observer reply exceeds keeper reply bound")
    print(line, flush=True)


def capture(path):
    with open(path, "rb") as source:
        data = source.read(MAX_CAPTURE + 1)
    if len(data) > MAX_CAPTURE:
        raise ValueError(f"tracee capture exceeds {MAX_CAPTURE} bytes")
    return base64.b64encode(data).decode("ascii")


def holder_row(snapshot, configuration):
    for line in snapshot.splitlines(keepends=True):
        if not line.endswith(b"\n"):
            continue
        fields = line.split()
        if len(fields) != 8 or fields[1:4] != [b"POSIX", b"ADVISORY", b"WRITE"]:
            continue
        major, minor, inode = fields[5].split(b":")
        device = (int(major, 16), int(minor, 16))
        expected_device = tuple(int(part) for part in configuration["device"].split(":"))
        if int(fields[4]) == configuration["holder_pid"] and int(inode) == configuration["inode"] and device == expected_device:
            return line.decode("ascii")
    return None


def read_memory(pid, address, count):
    words = []
    aligned = address & ~7
    prefix = address - aligned
    for offset in range(0, prefix + count, 8):
        word = ptrace(PTRACE_PEEKDATA, pid, aligned + offset) & ((1 << 64) - 1)
        words.append(word.to_bytes(8, "little"))
    return b"".join(words)[prefix:prefix + count]


def fork_tracee(words, output, errors, children):
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
                    if descriptor != target:
                        os.dup2(descriptor, target)
                        os.close(descriptor)
                ptrace(PTRACE_TRACEME, 0)
                os.kill(os.getpid(), signal.SIGSTOP)
                os.execvpe(words[0], words, os.environ)
            except BaseException as failure:
                os.write(2, f"tracee setup: {failure}\n".encode("utf-8", errors="backslashreplace"))
                os._exit(125)
        children.add(pid)
        return pid
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)


def wait_owned(pid, children):
    observed, status = os.waitpid(pid, 0)
    if observed != pid:
        raise RuntimeError("waitpid returned an unowned process")
    if os.WIFEXITED(status) or os.WIFSIGNALED(status):
        children.remove(pid)
    return status


def unexpected_fork(pid, children):
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, CANCELLATION | {signal.SIGALRM})
    try:
        message = ctypes.c_ulong()
        ptrace(PTRACE_GETEVENTMSG, pid, data=ctypes.byref(message))
        children.add(message.value)
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)
    raise RuntimeError("lslocks observer encountered an unexpected process fork")


def pause_after_holder_read(pid, errors, configuration, children):
    status = wait_owned(pid, children)
    if not os.WIFSTOPPED(status) or os.WSTOPSIG(status) != signal.SIGSTOP:
        diagnostic = Path(errors).read_bytes().decode("utf-8", errors="backslashreplace")
        raise RuntimeError(f"tracee failed before PTRACE_TRACEME stop: {diagnostic}")
    options = PTRACE_O_TRACESYSGOOD | PTRACE_O_TRACEFORK | PTRACE_O_TRACEVFORK | PTRACE_O_TRACEEXEC | PTRACE_O_EXITKILL
    ptrace(PTRACE_SETOPTIONS, pid, data=options)
    pending_read = None
    snapshot = b""
    delivered = 0
    while True:
        ptrace(PTRACE_SYSCALL, pid, data=delivered)
        delivered = 0
        status = wait_owned(pid, children)
        if not os.WIFSTOPPED(status):
            raise RuntimeError("tracee exited before returning the owned /proc/locks row")
        event = status >> 16
        stopped = os.WSTOPSIG(status)
        if event in {PTRACE_EVENT_FORK, PTRACE_EVENT_VFORK}:
            unexpected_fork(pid, children)
        if event == PTRACE_EVENT_EXEC:
            pending_read = None
            continue
        if stopped != (signal.SIGTRAP | 0x80):
            delivered = stopped
            continue
        information = SyscallInfo()
        available = ptrace(PTRACE_GET_SYSCALL_INFO, pid, ctypes.sizeof(information), ctypes.byref(information))
        if information.arch != AUDIT_ARCH_X86_64:
            raise RuntimeError("syscall observer requires x86_64 Linux tracees")
        if information.op == SYSCALL_ENTRY:
            if available < 80:
                raise RuntimeError("kernel returned a truncated syscall entry record")
            pending_read = (information.data[1], information.data[2]) if information.data[0] == SYS_READ else None
        elif information.op == SYSCALL_EXIT:
            if available < 33:
                raise RuntimeError("kernel returned a truncated syscall exit record")
            count = ctypes.c_int64(information.data[0]).value
            if pending_read is None or count <= 0:
                continue
            descriptor, address = pending_read
            if os.readlink(f"/proc/{pid}/fd/{descriptor}") != "/proc/locks":
                continue
            if count > MAX_SNAPSHOT or len(snapshot) + count > MAX_SNAPSHOT:
                raise ValueError("owned procfs lock snapshot exceeds 64 KiB")
            snapshot += read_memory(pid, address, count)
            row = holder_row(snapshot, configuration)
            if row is not None:
                return row


def finish_tracee(pid, children):
    delivered = 0
    while True:
        ptrace(PTRACE_CONT, pid, data=delivered)
        status = wait_owned(pid, children)
        if os.WIFEXITED(status) or os.WIFSIGNALED(status):
            return os.waitstatus_to_exitcode(status)
        event = status >> 16
        if event in {PTRACE_EVENT_FORK, PTRACE_EVENT_VFORK}:
            unexpected_fork(pid, children)
        delivered = 0 if event == PTRACE_EVENT_EXEC else os.WSTOPSIG(status)


def stop_children(children):
    for number in CANCELLATION:
        signal.signal(number, signal.SIG_IGN)
    signal.setitimer(signal.ITIMER_REAL, 2)
    try:
        for pid in list(children):
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        for pid in list(children):
            status = wait_owned(pid, children)
            if not (os.WIFEXITED(status) or os.WIFSIGNALED(status)):
                raise RuntimeError("owned tracee did not terminate after SIGKILL")
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)


def observe(configuration):
    if sys.platform != "linux" or platform.machine() != "x86_64" or ctypes.sizeof(ctypes.c_long) != 8:
        raise RuntimeError("owner disappearance observer supports x86_64 Linux only")
    signal.signal(signal.SIGCHLD, signal.SIG_DFL)
    signal.signal(signal.SIGALRM, timeout)
    for number in CANCELLATION:
        signal.signal(number, cancelled)
    children = set()
    with tempfile.TemporaryDirectory(prefix="owner-disappearance-", dir=os.environ["TMPDIR"]) as directory:
        primary = None
        try:
            signal.setitimer(signal.ITIMER_REAL, 12)
            tracees = {}
            rows = {}
            for label in ["reference", "candidate"]:
                output = os.path.join(directory, label + ".stdout")
                errors = os.path.join(directory, label + ".stderr")
                pid = fork_tracee(configuration[label], output, errors, children)
                tracees[label] = (pid, output, errors)
                rows[label] = pause_after_holder_read(pid, errors, configuration, children)
            emit({"holder_pid": configuration["holder_pid"], "inode": configuration["inode"],
                  "reference_row": rows["reference"], "candidate_row": rows["candidate"]})
            signal.setitimer(signal.ITIMER_REAL, 5)
            if sys.stdin.readline(16) != "RESUME\n":
                raise ValueError("observer expected RESUME after the holder was reaped")
            if os.path.exists(f"/proc/{configuration['holder_pid']}"):
                raise RuntimeError("holder still exists at the collection barrier")
            if holder_row(Path("/proc/locks").read_bytes(), configuration) is not None:
                raise RuntimeError("owned lock still exists at the collection barrier")
            captures = {}
            for label, (pid, output, errors) in tracees.items():
                captures[label] = {"returncode": finish_tracee(pid, children),
                                   "stdout_b64": capture(output), "stderr_b64": capture(errors)}
            emit(captures)
        except BaseException as failure:
            primary = failure
            raise
        finally:
            try:
                stop_children(children)
            except BaseException as cleanup:
                if primary is not None:
                    raise RuntimeError(f"{primary}; owned tracee cleanup also failed: {cleanup}") from primary
                raise


try:
    observe(json.loads(sys.argv[1]))
except BaseException as failure:
    emit({"error": str(failure)})
    raise SystemExit(1)
