import argparse
import atexit
from contextlib import contextmanager
import ctypes
import errno
import json
import os
import secrets
import signal
import stat
import struct
import sys


IPC_PRIVATE = 0
IPC_CREAT = 0o1000
IPC_RMID = 0
IPC_SET = 1
O_CREAT = 0o100
O_EXCL = 0o200
O_RDWR = 2
O_RDONLY = 0
GETVAL = 12
OWNER_FIELD = 4
GROUP_FIELD = 8
MODE_FIELD = 20
MESSAGE_TYPE_SIZE = ctypes.sizeof(ctypes.c_long)

libc = ctypes.CDLL(None, use_errno=True)
resources = []
cleaning = False


def bind(name, result, arguments):
    function = getattr(libc, name)
    function.restype = result
    function.argtypes = arguments
    return function


msgget = bind("msgget", ctypes.c_int, [ctypes.c_int, ctypes.c_int])
msgctl = bind("msgctl", ctypes.c_int, [ctypes.c_int, ctypes.c_int, ctypes.c_void_p])
msgsnd = bind("msgsnd", ctypes.c_int, [ctypes.c_int, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int])
msgrcv = bind("msgrcv", ctypes.c_ssize_t, [ctypes.c_int, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_long, ctypes.c_int])
shmget = bind("shmget", ctypes.c_int, [ctypes.c_int, ctypes.c_size_t, ctypes.c_int])
shmctl = bind("shmctl", ctypes.c_int, [ctypes.c_int, ctypes.c_int, ctypes.c_void_p])
shmat = bind("shmat", ctypes.c_void_p, [ctypes.c_int, ctypes.c_void_p, ctypes.c_int])
shmdt = bind("shmdt", ctypes.c_int, [ctypes.c_void_p])
semget = bind("semget", ctypes.c_int, [ctypes.c_int, ctypes.c_int, ctypes.c_int])
semctl = getattr(libc, "semctl")
semctl.restype = ctypes.c_int
semctl.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int]
semop = bind("semop", ctypes.c_int, [ctypes.c_int, ctypes.c_void_p, ctypes.c_size_t])
shm_open = bind("shm_open", ctypes.c_int, [ctypes.c_char_p, ctypes.c_int, ctypes.c_uint])
shm_unlink = bind("shm_unlink", ctypes.c_int, [ctypes.c_char_p])
mq_open = getattr(libc, "mq_open")
mq_open.restype = ctypes.c_int
mq_open.argtypes = [ctypes.c_char_p, ctypes.c_int, ctypes.c_uint, ctypes.c_void_p]
mq_close = bind("mq_close", ctypes.c_int, [ctypes.c_int])
mq_getattr = bind("mq_getattr", ctypes.c_int, [ctypes.c_int, ctypes.c_void_p])
mq_unlink = bind("mq_unlink", ctypes.c_int, [ctypes.c_char_p])
mq_send = bind("mq_send", ctypes.c_int, [ctypes.c_int, ctypes.c_char_p, ctypes.c_size_t, ctypes.c_uint])
mq_receive = bind("mq_receive", ctypes.c_ssize_t, [ctypes.c_int, ctypes.c_void_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_uint)])
sem_open = getattr(libc, "sem_open")
sem_open.restype = ctypes.c_void_p
sem_open.argtypes = [ctypes.c_char_p, ctypes.c_int, ctypes.c_uint, ctypes.c_uint]
sem_close = bind("sem_close", ctypes.c_int, [ctypes.c_void_p])
sem_getvalue = bind("sem_getvalue", ctypes.c_int, [ctypes.c_void_p, ctypes.POINTER(ctypes.c_int)])
sem_unlink = bind("sem_unlink", ctypes.c_int, [ctypes.c_char_p])
sem_post = bind("sem_post", ctypes.c_int, [ctypes.c_void_p])


class Sembuf(ctypes.Structure):
    _fields_ = [("sem_num", ctypes.c_ushort), ("sem_op", ctypes.c_short), ("sem_flg", ctypes.c_short)]


class MqAttr(ctypes.Structure):
    _fields_ = [
        ("mq_flags", ctypes.c_long),
        ("mq_maxmsg", ctypes.c_long),
        ("mq_msgsize", ctypes.c_long),
        ("mq_curmsgs", ctypes.c_long),
        ("reserved", ctypes.c_long * 4),
    ]


def fail_errno(operation):
    code = ctypes.get_errno()
    raise OSError(code, f"{operation}: {os.strerror(code)}")


def require_zero(result, operation):
    if result != 0:
        fail_errno(operation)


def require_id(result, operation):
    if result < 0:
        fail_errno(operation)
    return result


def require_pointer(result, operation):
    if result is None or result == ctypes.c_void_p(-1).value:
        fail_errno(operation)
    return result


def track(kind, identity, name, lookup_name, owner_uid, owner_gid, creator_uid, creator_gid, mode, size=0):
    resource = {
        "kind": kind,
        "id": identity,
        "name": name,
        "lookup_name": lookup_name,
        "owner_uid": owner_uid,
        "owner_gid": owner_gid,
        "creator_uid": creator_uid,
        "creator_gid": creator_gid,
        "creator_pid": os.getpid(),
        "mode": mode,
        "size": size,
    }
    resources.append(resource)
    return resource


def set_sysv_owner(function, identity, owner_uid, owner_gid, mode):
    data = ctypes.create_string_buffer(256)
    struct.pack_into("=I", data, OWNER_FIELD, owner_uid)
    struct.pack_into("=I", data, GROUP_FIELD, owner_gid)
    struct.pack_into("=I", data, MODE_FIELD, mode & 0o777)
    require_zero(function(identity, IPC_SET, ctypes.byref(data)), "IPC_SET")


def privileged_ids():
    os.setegid(0)
    os.seteuid(0)


def creator_ids(creator_uid, creator_gid):
    os.setegid(creator_gid)
    os.seteuid(creator_uid)


@contextmanager
def creator_scope(creator_uid, creator_gid):
    blocked = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM, signal.SIGINT, signal.SIGHUP})
    try:
        creator_ids(creator_uid, creator_gid)
        yield
    finally:
        privileged_ids()
        signal.pthread_sigmask(signal.SIG_SETMASK, blocked)


def send_sysv_message(identity, message):
    buffer = ctypes.create_string_buffer(MESSAGE_TYPE_SIZE + len(message))
    ctypes.c_long.from_buffer(buffer).value = 1
    ctypes.memmove(ctypes.addressof(buffer) + MESSAGE_TYPE_SIZE, message, len(message))
    require_zero(msgsnd(identity, buffer, len(message), 0), "msgsnd")


def receive_sysv_message(identity):
    buffer = ctypes.create_string_buffer(MESSAGE_TYPE_SIZE + 256)
    if msgrcv(identity, buffer, 256, 0, 0) < 0:
        fail_errno("msgrcv")


def send_posix_messages(descriptor):
    for message in [b"xsh-ipc-first", b"xsh-ipc-second"]:
        require_zero(mq_send(descriptor, message, len(message), 0), "mq_send")
    buffer = ctypes.create_string_buffer(8192)
    priority = ctypes.c_uint()
    if mq_receive(descriptor, buffer, len(buffer), ctypes.byref(priority)) < 0:
        fail_errno("mq_receive")


def make_sysv_queue(owner_uid, owner_gid, creator_uid, creator_gid, mode):
    with creator_scope(creator_uid, creator_gid):
        identity = require_id(msgget(IPC_PRIVATE, IPC_CREAT | mode), "msgget")
        resource = track("sysv_queue", identity, None, None, owner_uid, owner_gid, creator_uid, creator_gid, mode)
    send_sysv_message(identity, b"xsh-ipc-first")
    send_sysv_message(identity, b"xsh-ipc-second")
    receive_sysv_message(identity)
    set_sysv_owner(msgctl, identity, owner_uid, owner_gid, mode)
    return resource


def make_sysv_shmem(owner_uid, owner_gid, creator_uid, creator_gid, mode, size):
    with creator_scope(creator_uid, creator_gid):
        identity = require_id(shmget(IPC_PRIVATE, size, IPC_CREAT | mode), "shmget")
        resource = track("sysv_shmem", identity, None, None, owner_uid, owner_gid, creator_uid, creator_gid, mode, size)
    pointer = require_pointer(shmat(identity, None, 0), "shmat")
    resource["attachment"] = pointer
    ctypes.memset(pointer, 1, 1)
    require_zero(shmdt(pointer), "shmdt")
    resource["attachment"] = None
    pointer = require_pointer(shmat(identity, None, 0), "shmat")
    resource["attachment"] = pointer
    set_sysv_owner(shmctl, identity, owner_uid, owner_gid, mode)
    return resource


def make_sysv_semaphore(owner_uid, owner_gid, creator_uid, creator_gid, mode):
    with creator_scope(creator_uid, creator_gid):
        identity = require_id(semget(IPC_PRIVATE, 2, IPC_CREAT | mode), "semget")
        resource = track("sysv_semaphore", identity, None, None, owner_uid, owner_gid, creator_uid, creator_gid, mode)
    operation = Sembuf(0, 1, 0)
    require_zero(semop(identity, ctypes.byref(operation), 1), "semop")
    operation = Sembuf(0, -1, 0)
    require_zero(semop(identity, ctypes.byref(operation), 1), "semop")
    set_sysv_owner(lambda identity, command, data: semctl(identity, 0, command, data), identity, owner_uid, owner_gid, mode)
    return resource


def create_posix_mqueue(name, owner_uid, owner_gid, creator_uid, creator_gid, mode):
    encoded_name = name.encode()
    with creator_scope(creator_uid, creator_gid):
        descriptor = mq_open(encoded_name, O_CREAT | O_EXCL | O_RDWR, mode, None)
        if descriptor < 0:
            fail_errno("mq_open")
        resource = track("posix_mqueue", None, name, name, owner_uid, owner_gid, creator_uid, creator_gid, mode)
    send_posix_messages(descriptor)
    os.chown(f"/dev/mqueue/{name[1:]}", owner_uid, owner_gid)
    os.chmod(f"/dev/mqueue/{name[1:]}", mode)
    require_zero(mq_close(descriptor), "mq_close")
    return resource


def create_posix_shmem(name, owner_uid, owner_gid, creator_uid, creator_gid, mode, size):
    encoded_name = name.encode()
    with creator_scope(creator_uid, creator_gid):
        descriptor = shm_open(encoded_name, O_CREAT | O_EXCL | O_RDWR, mode)
        if descriptor < 0:
            fail_errno("shm_open")
        resource = track("posix_shmem", None, name, name[1:], owner_uid, owner_gid, creator_uid, creator_gid, mode, size)
    os.ftruncate(descriptor, size)
    os.fchown(descriptor, owner_uid, owner_gid)
    os.fchmod(descriptor, mode)
    os.close(descriptor)
    return resource


def create_posix_semaphore(name, owner_uid, owner_gid, creator_uid, creator_gid, mode):
    encoded_name = name.encode()
    with creator_scope(creator_uid, creator_gid):
        semaphore = sem_open(encoded_name, O_CREAT | O_EXCL, mode, 0)
        require_pointer(semaphore, "sem_open")
        resource = track("posix_semaphore", None, name, name[1:], owner_uid, owner_gid, creator_uid, creator_gid, mode)
    require_zero(sem_post(semaphore), "sem_post")
    os.chown(f"/dev/shm/sem.{name[1:]}", owner_uid, owner_gid)
    os.chmod(f"/dev/shm/sem.{name[1:]}", mode)
    require_zero(sem_close(semaphore), "sem_close")
    return resource


def create_class(kind, token, owner_uid, owner_gid, observer_uid, observer_gid):
    if kind == "sysv_queue":
        make_sysv_queue(owner_uid, owner_gid, owner_uid, owner_gid, 0o644)
        make_sysv_queue(owner_uid, owner_gid, owner_uid, owner_gid, 0o600)
        make_sysv_queue(owner_uid, owner_gid, observer_uid, observer_gid, 0o600)
    elif kind == "sysv_shmem":
        make_sysv_shmem(owner_uid, owner_gid, owner_uid, owner_gid, 0o644, 4096)
        make_sysv_shmem(owner_uid, owner_gid, owner_uid, owner_gid, 0o600, 4096)
        make_sysv_shmem(owner_uid, owner_gid, observer_uid, observer_gid, 0o600, 4096)
    elif kind == "sysv_semaphore":
        make_sysv_semaphore(owner_uid, owner_gid, owner_uid, owner_gid, 0o644)
        make_sysv_semaphore(owner_uid, owner_gid, owner_uid, owner_gid, 0o600)
        make_sysv_semaphore(owner_uid, owner_gid, observer_uid, observer_gid, 0o600)
    elif kind == "posix_mqueue":
        create_posix_mqueue(f"/xi{token}r", owner_uid, owner_gid, owner_uid, owner_gid, 0o644)
        create_posix_mqueue(f"/xi{token}p", owner_uid, owner_gid, owner_uid, owner_gid, 0o600)
    elif kind == "posix_shmem":
        create_posix_shmem(f"/xs{token}r", owner_uid, owner_gid, owner_uid, owner_gid, 0o644, 4096)
        create_posix_shmem(f"/xs{token}p", owner_uid, owner_gid, owner_uid, owner_gid, 0o600, 4096)
    elif kind == "posix_semaphore":
        create_posix_semaphore(f"/xe{token}r", owner_uid, owner_gid, owner_uid, owner_gid, 0o644)
        create_posix_semaphore(f"/xe{token}p", owner_uid, owner_gid, owner_uid, owner_gid, 0o600)
    else:
        raise ValueError(f"unsupported IPC resource class: {kind}")


def ignored_missing(error):
    return error.errno in [errno.EIDRM, errno.EINVAL, errno.ENOENT]


def sysv_snapshot(resource):
    table, id_column = {
        "sysv_queue": ("msg", "msqid"),
        "sysv_shmem": ("shm", "shmid"),
        "sysv_semaphore": ("sem", "semid"),
    }[resource["kind"]]
    with open(f"/proc/sysvipc/{table}", encoding="ascii") as source:
        lines = source.read().splitlines()
    columns = lines[0].split()
    id_index = columns.index(id_column)
    for line in lines[1:]:
        values = line.split()
        if values[id_index] == str(resource["id"]):
            state = dict(zip(columns, values))
            if resource["kind"] == "sysv_semaphore":
                state["values"] = [
                    require_nonnegative(semctl(resource["id"], index, GETVAL), "GETVAL")
                    for index in range(2)
                ]
            return state
    raise RuntimeError(f"registered System V IPC id disappeared: {resource['id']}")


def require_nonnegative(result, operation):
    if result < 0:
        fail_errno(operation)
    return result


def stat_snapshot(path):
    metadata = os.stat(path)
    return {
        "uid": metadata.st_uid,
        "gid": metadata.st_gid,
        "mode": stat.S_IMODE(metadata.st_mode),
        "size": metadata.st_size,
        "mtime_ns": metadata.st_mtime_ns,
        "ctime_ns": metadata.st_ctime_ns,
    }


def posix_snapshot(resource):
    name = resource["name"]
    if resource["kind"] == "posix_shmem":
        return stat_snapshot(f"/dev/shm/{name[1:]}")
    if resource["kind"] == "posix_semaphore":
        state = stat_snapshot(f"/dev/shm/sem.{name[1:]}")
        semaphore = require_pointer(sem_open(name.encode(), 0, 0, 0), "sem_open")
        try:
            value = ctypes.c_int()
            require_zero(sem_getvalue(semaphore, ctypes.byref(value)), "sem_getvalue")
            state["value"] = value.value
        finally:
            require_zero(sem_close(semaphore), "sem_close")
        return state
    if resource["kind"] == "posix_mqueue":
        state = stat_snapshot(f"/dev/mqueue/{name[1:]}")
        descriptor = mq_open(name.encode(), O_RDONLY, 0, None)
        if descriptor < 0:
            fail_errno("mq_open")
        try:
            attributes = MqAttr()
            require_zero(mq_getattr(descriptor, ctypes.byref(attributes)), "mq_getattr")
            state["attributes"] = {
                "maxmsg": attributes.mq_maxmsg,
                "msgsize": attributes.mq_msgsize,
                "curmsgs": attributes.mq_curmsgs,
            }
        finally:
            require_zero(mq_close(descriptor), "mq_close")
        return state
    raise RuntimeError(f"unsupported POSIX IPC resource kind: {resource['kind']}")


def snapshot():
    states = []
    for resource in resources:
        state = sysv_snapshot(resource) if resource["kind"].startswith("sysv_") else posix_snapshot(resource)
        states.append({"kind": resource["kind"], "id": resource["id"], "name": resource["name"], "state": state})
    return {"resources": states}


def remove_resource(resource):
    kind = resource["kind"]
    identity = resource["id"]
    name = resource["name"]
    if kind == "sysv_queue":
        result = msgctl(identity, IPC_RMID, None)
    elif kind == "sysv_shmem":
        pointer = resource.get("attachment")
        if pointer is not None:
            require_zero(shmdt(pointer), "shmdt")
            resource["attachment"] = None
        result = shmctl(identity, IPC_RMID, None)
    elif kind == "sysv_semaphore":
        result = semctl(identity, 0, IPC_RMID)
    elif kind == "posix_mqueue":
        result = mq_unlink(name.encode())
    elif kind == "posix_shmem":
        result = shm_unlink(name.encode())
    elif kind == "posix_semaphore":
        result = sem_unlink(name.encode())
    else:
        raise RuntimeError(f"unknown IPC resource kind: {kind}")
    if result != 0:
        code = ctypes.get_errno()
        error = OSError(code, os.strerror(code))
        if not ignored_missing(error):
            raise error


def cleanup():
    global cleaning
    if cleaning:
        return
    cleaning = True
    privileged_ids()
    failures = []
    remaining = []
    for resource in reversed(resources):
        try:
            remove_resource(resource)
        except Exception as failure:
            identity = resource["id"] if resource["id"] is not None else resource["name"]
            failures.append(f"{resource['kind']} {identity}: {failure}")
            remaining.append(resource)
    if failures:
        resources[:] = reversed(remaining)
        cleaning = False
        raise RuntimeError("IPC fixture cleanup failed: " + "; ".join(failures))
    resources.clear()


def stop(signum, frame):
    cleanup()
    raise SystemExit(128 + signum)


def run(arguments):
    if os.geteuid() != 0:
        raise PermissionError("IPC fixture keeper must start with effective UID 0")
    if min(arguments.owner_uid, arguments.owner_gid, arguments.observer_uid, arguments.observer_gid) < 1:
        raise ValueError("IPC fixture identities must be non-root")
    if arguments.owner_uid == arguments.observer_uid:
        raise ValueError("owner and observer UIDs must differ")

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGHUP, stop)
    atexit.register(cleanup)

    token = secrets.token_hex(4)
    owner_uid = arguments.owner_uid
    owner_gid = arguments.owner_gid
    observer_uid = arguments.observer_uid
    observer_gid = arguments.observer_gid
    for kind in arguments.resources:
        create_class(kind, token, owner_uid, owner_gid, observer_uid, observer_gid)

    ready = {
        "pid": os.getpid(),
        "owner_uid": owner_uid,
        "observer_uid": observer_uid,
        "resources": [
            {key: value for key, value in resource.items() if key != "attachment"}
            for resource in resources
        ],
    }
    print(json.dumps(ready, separators=(",", ":")), flush=True)
    while True:
        request = sys.stdin.readline()
        if request in ["", "release\n", "release"]:
            cleanup()
            return
        if request == "snapshot\n":
            print(json.dumps(snapshot(), separators=(",", ":")), flush=True)
            continue
        raise ValueError("IPC fixture keeper expected snapshot or release on stdin")


def arguments():
    parser = argparse.ArgumentParser()
    parser.add_argument("--owner-uid", type=int, required=True)
    parser.add_argument("--owner-gid", type=int, required=True)
    parser.add_argument("--observer-uid", type=int, required=True)
    parser.add_argument("--observer-gid", type=int, required=True)
    parser.add_argument("--resource", dest="resources", choices=[
        "sysv_queue",
        "sysv_shmem",
        "sysv_semaphore",
        "posix_mqueue",
        "posix_shmem",
        "posix_semaphore",
    ], action="append", required=True)
    result = parser.parse_args()
    if len(set(result.resources)) != len(result.resources):
        raise ValueError("each IPC resource class may be selected only once")
    return result


try:
    run(arguments())
except Exception as failure:
    print(f"IPC fixture keeper: {failure}", file=sys.stderr, flush=True)
    try:
        cleanup()
    except Exception as cleanup_failure:
        print(f"IPC fixture keeper: {cleanup_failure}", file=sys.stderr, flush=True)
    raise SystemExit(1)
