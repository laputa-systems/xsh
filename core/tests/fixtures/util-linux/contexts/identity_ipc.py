import ctypes
import json
import os
import signal
import sys


libc = ctypes.CDLL(None, use_errno=True)
libc.shmget.argtypes = [ctypes.c_int, ctypes.c_size_t, ctypes.c_int]
libc.shmget.restype = ctypes.c_int
libc.shmctl.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
libc.shmctl.restype = ctypes.c_int
identity = None


def cleanup():
    global identity
    if identity is not None:
        if libc.shmctl(identity, 0, None) != 0:
            number = ctypes.get_errno()
            raise OSError(number, os.strerror(number))
        identity = None


def stop(number, frame):
    for blocked in [signal.SIGTERM, signal.SIGINT, signal.SIGHUP]:
        signal.signal(blocked, signal.SIG_IGN)
    cleanup()
    raise SystemExit(128 + number)


for number in [signal.SIGTERM, signal.SIGINT, signal.SIGHUP]:
    signal.signal(number, stop)
assert os.getresuid() == (10002, 10002, 10002)
assert os.getresgid() == (10002, 10002, 10002)
assert os.getgroups() == []
with open("/proc/self/status", encoding="ascii") as source:
    capabilities = {name: value.strip() for line in source
                    for name, _, value in [line.partition(":")]
                    if name in {"CapEff", "CapPrm", "CapInh", "CapAmb", "CapBnd"}}
assert len(capabilities) == 5 and all(int(value, 16) == 0 for value in capabilities.values())
try:
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM, signal.SIGINT, signal.SIGHUP})
    identity = libc.shmget(0, 4096, 0o1600)
    if identity < 0:
        identity = None
        number = ctypes.get_errno()
        raise OSError(number, os.strerror(number))
    signal.pthread_sigmask(signal.SIG_SETMASK, previous)
    print(json.dumps({"pid": os.getpid(), "id": identity, "uid": os.geteuid(),
                      "capabilities": capabilities}, separators=(",", ":")), flush=True)
    if sys.stdin.readline() != "DONE\n":
        raise ValueError("identity IPC keeper expected DONE")
finally:
    cleanup()
