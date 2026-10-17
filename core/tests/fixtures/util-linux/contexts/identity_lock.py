import fcntl
import json
import os
import sys


status = {}
with open("/proc/self/status", encoding="ascii") as source:
    for line in source:
        name, _, value = line.partition(":")
        if name in {"Uid", "Gid", "CapEff", "CapPrm", "CapInh", "CapAmb", "CapBnd"}:
            status[name] = value.strip()
assert status["Uid"].split() == ["10002"] * 4
assert status["Gid"].split() == ["10002"] * 4
assert all(int(status[name], 16) == 0 for name in ["CapEff", "CapPrm", "CapInh", "CapAmb", "CapBnd"])
assert os.getgroups() == []
fd = os.open(sys.argv[1], os.O_CREAT | os.O_EXCL | os.O_RDWR, 0o600)
try:
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    identity = os.fstat(fd)
    print(json.dumps({"pid": os.getpid(), "inode": identity.st_ino,
                      "uid": identity.st_uid, "status": status}, separators=(",", ":")), flush=True)
    if sys.stdin.readline() != "DONE\n":
        raise ValueError("identity holder expected DONE")
finally:
    os.close(fd)
