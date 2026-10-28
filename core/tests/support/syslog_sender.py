import array
import errno
import json
import os
import socket
import sys


def connect(path):
    directory, name = os.path.split(path)
    handle = os.open(directory, os.O_DIRECTORY)
    sender = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
    sender.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1 << 20)
    sender.settimeout(5)
    sender.connect(f"/proc/self/fd/{handle}/{name}")
    os.close(handle)
    return sender


def send(sender, data, descriptors=0):
    opened = [os.open("/dev/null", os.O_RDONLY) for _ in range(descriptors)]
    ancillary = []
    if opened:
        ancillary = [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", opened))]
    try:
        sender.sendmsg([data], ancillary)
        return "sent"
    except BlockingIOError:
        return "again"
    except socket.timeout:
        return "again"
    except OSError as error:
        return errno.errorcode.get(error.errno, str(error.errno))
    finally:
        for descriptor in opened:
            os.close(descriptor)


def main():
    scenario, path = sys.argv[1], sys.argv[2]
    sender = connect(path)
    outcomes = {"sent": 0, "again": 0, "other": 0}

    def note(outcome):
        outcomes[outcome if outcome in outcomes else "other"] += 1

    if scenario == "rights":
        count, descriptors = int(sys.argv[3]), int(sys.argv[4])
        for index in range(count):
            note(send(sender, b"<13>rights: carrier %d" % index, descriptors))
    elif scenario == "flood":
        count, size = int(sys.argv[3]), int(sys.argv[4])
        if len(sys.argv) > 5 and sys.argv[5] == "nonblocking":
            sender.settimeout(0)
        filler = b"x" * max(0, size - 40)
        for index in range(count):
            note(send(sender, b"<13>flood[%d]: %d " % (os.getpid(), index) + filler))
    elif scenario == "numbered":
        count = int(sys.argv[3])
        for index in range(count):
            note(send(sender, b"<13>numbered[%d]: n%d" % (os.getpid(), index)))
    elif scenario == "datagram":
        note(send(sender, sys.stdin.buffer.read()))
    print(json.dumps({"pid": os.getpid(), **outcomes}))


main()
