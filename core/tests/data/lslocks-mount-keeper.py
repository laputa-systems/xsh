import ctypes
import fcntl
import json
import os
import signal
import subprocess
import sys

root = os.fsencode(sys.argv[1])
mode = sys.argv[2]
def stop(*args):
    raise SystemExit(1)

signal.signal(signal.SIGTERM, stop)
mounts = [root + b'/first', root + (b'/mount-\xff' if mode == 'fallback' else b'/second')]
for directory in mounts:
    os.mkdir(directory)
    subprocess.run([b'/bin/mount', b'-t', b'tmpfs', b'tmpfs', directory], check=True)
try:
    wrong = os.open(mounts[0] + b'/wrong', os.O_CREAT | os.O_RDWR, 0o600)
    locked = os.open(mounts[1] + b'/locked', os.O_CREAT | os.O_RDWR, 0o600)
    os.write(locked, b'locked data')
    assert os.fstat(wrong).st_ino == os.fstat(locked).st_ino
    control = os.dup(0)
    os.dup2(wrong, 0)
    fcntl.flock(locked, fcntl.LOCK_EX)
    if mode == 'fallback':
        assert ctypes.CDLL(None).prctl(4, 0, 0, 0, 0) == 0
    args = [b'-p', str(os.getpid()).encode(), b'-n', b'-o', b'PATH,SIZE']
    env = dict(os.environb)
    env[b'LC_ALL'] = b'C'
    ref = subprocess.run([b'/usr/bin/unshare', b'-U', b'-r', b'/usr/bin/lslocks', *args], capture_output=True, env=env, timeout=10)
    cand = subprocess.run([b'/usr/bin/unshare', b'-U', b'-r', os.fsencode(sys.argv[3]), os.fsencode(sys.argv[4]), b'--', *args], capture_output=True, env=env, timeout=10)
    print(json.dumps({'pid': os.getpid(), 'reference': ref.stdout.hex(), 'candidate': cand.stdout.hex(), 'reference_error': ref.stderr.hex(), 'candidate_error': cand.stderr.hex(), 'reference_status': ref.returncode, 'candidate_status': cand.returncode}), flush=True)
    os.read(control, 128)
finally:
    os.close(locked)
    os.close(wrong)
    os.close(0)
    for directory in reversed(mounts):
        subprocess.run([b'/bin/umount', directory], check=True)
