import fcntl
import json
import os
import struct
import sys
import threading
import time


METHODS = {
    "flock-sh": ("FLOCK", [(fcntl.LOCK_SH, 0, 0)], 0),
    "flock-ex": ("FLOCK", [(fcntl.LOCK_EX, 0, 0)], 0),
    "posix-r-": ("POSIX", [(fcntl.LOCK_SH, 0, 1)], 1),
    "posix--w": ("POSIX", [(fcntl.LOCK_EX, 0, 1)], 1),
    "posix-rw": ("POSIX", [(fcntl.LOCK_SH, 0, 1), (fcntl.LOCK_EX, 2, 1)], 3),
    "posix-eof-r": ("POSIX", [(fcntl.LOCK_SH, 2, 0)], 3),
    "posix-eof-w": ("POSIX", [(fcntl.LOCK_EX, 2, 0)], 3),
    "ofd-r-": ("OFDLCK", [(fcntl.F_RDLCK, 0, 1)], 1),
    "ofd--w": ("OFDLCK", [(fcntl.F_WRLCK, 0, 1)], 1),
    "ofd-rw": ("OFDLCK", [(fcntl.F_RDLCK, 0, 1), (fcntl.F_WRLCK, 2, 1)], 3),
    "ofd-eof-r": ("OFDLCK", [(fcntl.F_RDLCK, 2, 0)], 3),
    "ofd-eof-w": ("OFDLCK", [(fcntl.F_WRLCK, 2, 0)], 3),
    "lease-w": ("LEASE", [(fcntl.F_WRLCK, 0, 0)], 0),
}


def lock_request(lock_type, start, length):
    return struct.pack("@hh4xqqi4x", lock_type, os.SEEK_SET, start, length, 0)


def acquire(fd, method, blocking):
    lock_class, ranges, _ = METHODS[method]
    for lock_type, start, length in ranges:
        if lock_class == "FLOCK":
            operation = lock_type if blocking else lock_type | fcntl.LOCK_NB
            fcntl.flock(fd, operation)
        elif lock_class == "LEASE":
            fcntl.fcntl(fd, fcntl.F_SETLEASE, lock_type)
        elif lock_class == "POSIX":
            operation = lock_type if blocking else lock_type | fcntl.LOCK_NB
            fcntl.lockf(fd, operation, length, start, os.SEEK_SET)
        else:
            operation = fcntl.F_OFD_SETLKW if blocking else fcntl.F_OFD_SETLK
            fcntl.fcntl(fd, operation, lock_request(lock_type, start, length))


def parse_lock(line):
    fields = line.split()
    if len(fields) < 8:
        return None
    try:
        lock_id = int(fields[0][:-1])
    except (ValueError, IndexError):
        return None
    index = 1
    blocked = fields[index] == "->"
    if blocked:
        index += 1
    try:
        lock_class = fields[index]
        mode = fields[index + 2]
        pid = int(fields[index + 3])
        major, minor, inode = fields[index + 4].split(":")
        start = fields[index + 5]
        end = fields[index + 6]
        return {
            "lock_id": lock_id,
            "blocked": blocked,
            "class": lock_class,
            "mode": mode,
            "pid": pid,
            "device": f"{int(major, 16)}:{int(minor, 16)}",
            "inode": int(inode),
            "start": start,
            "end": end,
        }
    except (IndexError, ValueError):
        return None


def lock_rows():
    with open("/proc/locks", "r", encoding="ascii") as locks:
        return [row for line in locks if (row := parse_lock(line)) is not None]


def wait_for_row(method, expected_pid, stat, blocked=False, owner_pid=None):
    lock_class, ranges, _ = METHODS[method]
    device = f"{os.major(stat.st_dev)}:{os.minor(stat.st_dev)}"
    lock_type = ranges[0][0]
    is_read = lock_type == fcntl.LOCK_SH if lock_class in {"FLOCK", "POSIX"} else lock_type == fcntl.F_RDLCK
    expected_mode = "READ" if is_read else "WRITE"
    deadline = time.monotonic_ns() + 5_000_000_000
    while time.monotonic_ns() < deadline:
        rows = lock_rows()
        matches = [
            row for row in rows
            if row["class"] == lock_class
            and row["pid"] == expected_pid
            and row["device"] == device
            and row["inode"] == stat.st_ino
            and row["mode"] == expected_mode
            and row["blocked"] == blocked
        ]
        if matches:
            matches.sort(key=lambda row: (row["start"] == "EOF", row["start"]))
            match = matches[0]
            if not blocked or any(
                row["lock_id"] == match["lock_id"]
                and not row["blocked"]
                and row["pid"] == (-1 if lock_class == "OFDLCK" else owner_pid)
                and row["class"] == lock_class
                and row["device"] == device
                and row["inode"] == stat.st_ino
                for row in rows
            ):
                return match
        os.sched_yield()
    raise TimeoutError(f"lock record not visible for {method}")


def move_to_fixture_fd(fd, duplicate):
    if fd != 17:
        os.dup2(fd, 17)
        os.close(fd)
    duplicate_fds = []
    if duplicate:
        os.dup2(17, 18)
        duplicate_fds.append(18)
    return 17, duplicate_fds


def ready_record(method, path, fd, duplicate_fds, lock_row, stat, owner_pid=None):
    record = {
        "method": method,
        "pid": os.getpid(),
        "fd": fd,
        "duplicate_fds": duplicate_fds,
        "device": f"{os.major(stat.st_dev)}:{os.minor(stat.st_dev)}",
        "inode": stat.st_ino,
        "path": path,
        "lock_id": lock_row["lock_id"],
    }
    if owner_pid is not None:
        record["owner_pid"] = owner_pid
    sys.stdout.write(json.dumps(record, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def hold(method, target, duplicate):
    _, _, size = METHODS[method]
    path = os.path.abspath(target)
    fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_RDWR, 0o600)
    if size:
        data = memoryview(b"z" * size)
        while data:
            data = data[os.write(fd, data):]
    fd, duplicate_fds = move_to_fixture_fd(fd, duplicate)
    acquire(fd, method, False)
    stat = os.fstat(fd)
    row = wait_for_row(method, os.getpid(), stat)
    ready_record(method, path, fd, duplicate_fds, row, stat)
    release = sys.stdin.readline()
    if release not in {"DONE\n", ""}:
        raise ValueError("keeper release must be DONE")


def wait(method, target, owner_pid):
    path = os.path.abspath(target)
    fd = os.open(path, os.O_RDWR)
    fd, duplicate_fds = move_to_fixture_fd(fd, False)
    stat = os.fstat(fd)
    failures = []

    def acquire_when_released():
        try:
            acquire(fd, method, True)
        except BaseException as error:
            failures.append(error)

    acquisition = threading.Thread(target=acquire_when_released, daemon=True)
    acquisition.start()
    lock_row = wait_for_row(method, os.getpid(), stat, blocked=True, owner_pid=owner_pid)
    ready_record(method, path, fd, duplicate_fds, lock_row, stat, owner_pid)
    acquisition.join()
    if failures:
        raise failures[0]


def main():
    mode, method, target, *options = sys.argv[1:]
    if method not in METHODS:
        raise ValueError(f"unknown lock method: {method}")
    if mode == "hold" and len(options) == 1:
        hold(method, target, options[0] == "duplicate")
    elif mode == "wait" and len(options) == 1 and method in {"flock-ex", "posix--w", "ofd--w"}:
        wait(method, target, int(options[0]))
    else:
        raise ValueError("usage: keeper.py hold METHOD PATH [duplicate] | wait METHOD PATH OWNER_PID")


if __name__ == "__main__":
    main()
