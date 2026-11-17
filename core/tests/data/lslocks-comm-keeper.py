import ctypes
import fcntl
import json
import os
import sys

name = bytes.fromhex(sys.argv[2])
assert ctypes.CDLL(None).prctl(15, ctypes.c_char_p(name), 0, 0, 0) == 0
fd = os.open(sys.argv[1], os.O_CREAT | os.O_RDWR, 0o600)
fcntl.flock(fd, fcntl.LOCK_EX)
print(json.dumps({"pid": os.getpid()}), flush=True)
sys.stdin.readline()
