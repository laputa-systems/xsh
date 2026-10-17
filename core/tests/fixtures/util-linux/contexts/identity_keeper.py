import fcntl
import os
import signal
import subprocess
import sys
import tempfile


class Interrupted(Exception):
    def __init__(self, number):
        self.number = number


def interrupt(number, frame):
    raise Interrupted(number)


def stop(child):
    if child.poll() is not None:
        return
    os.killpg(child.pid, signal.SIGTERM)
    try:
        child.wait(timeout=2)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait(timeout=2)


def outer(command):
    descriptor, path = tempfile.mkstemp(prefix="util-linux-outside-", dir="/t")
    child = None
    result = 1
    try:
        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        metadata = os.fstat(descriptor)
        environment = dict(os.environ)
        environment.update({
            "XSH_UTIL_LINUX_OUTSIDE_PID": str(os.getpid()),
            "XSH_UTIL_LINUX_OUTSIDE_LOCK_PATH": path,
            "XSH_UTIL_LINUX_OUTSIDE_DEV": str(metadata.st_dev),
            "XSH_UTIL_LINUX_OUTSIDE_INO": str(metadata.st_ino),
        })
        for number in [signal.SIGTERM, signal.SIGINT, signal.SIGHUP]:
            signal.signal(number, interrupt)
        blocked = {signal.SIGTERM, signal.SIGINT, signal.SIGHUP}
        previous = signal.pthread_sigmask(signal.SIG_BLOCK, blocked)
        try:
            child = subprocess.Popen(command, env=environment, start_new_session=True)
        except BaseException:
            signal.pthread_sigmask(signal.SIG_SETMASK, previous)
            raise
        try:
            signal.pthread_sigmask(signal.SIG_SETMASK, previous)
            result = child.wait(timeout=120)
        except subprocess.TimeoutExpired:
            result = 124
        except Interrupted as interrupted:
            result = 128 + interrupted.number
        finally:
            for number in [signal.SIGTERM, signal.SIGINT, signal.SIGHUP]:
                signal.signal(number, signal.SIG_IGN)
            stop(child)
    finally:
        os.close(descriptor)
        os.unlink(path)
    return result if result >= 0 else 128 - result


if __name__ == "__main__":
    if len(sys.argv) < 4 or sys.argv[1:3] != ["outer", "--"]:
        raise SystemExit("usage: identity_keeper.py outer -- COMMAND...")
    raise SystemExit(outer(sys.argv[3:]))
