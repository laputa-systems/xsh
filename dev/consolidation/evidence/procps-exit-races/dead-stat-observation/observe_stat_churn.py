import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import time

MAX_SECONDS = 22.0
WORKER_COUNT = 8
I64_MIN = -(1 << 63)
I64_MAX = (1 << 63) - 1
INTEGER = re.compile(r"[+-]?[0-9]+\Z")
WORKER = """child=; stop_worker() { trap - TERM INT HUP; if [ -n \"$child\" ]; then kill -TERM \"$child\" 2>/dev/null || :; wait \"$child\" 2>/dev/null || :; fi; exit 0; }; trap stop_worker TERM INT HUP; while :; do /bin/true & child=$!; wait \"$child\" 2>/dev/null || :; child=; done"""


def checked_integer(fields, index, label):
    if len(fields) <= index:
        return None, f"missing {label} at field index {index}"
    token = fields[index]
    if not INTEGER.fullmatch(token):
        return None, f"invalid integer {label} at field index {index}: {token!r}"
    value = int(token, 10)
    if value < I64_MIN or value > I64_MAX:
        return None, f"out-of-range integer {label} at field index {index}: {token!r}"
    return value, None


def parser_failure(raw):
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        return f"invalid UTF-8 at byte {error.start}"
    open_index = text.find("(")
    if open_index < 0:
        return "missing first open parenthesis"
    close_index = text.rfind(") ")
    if close_index < 0:
        return "missing final close-parenthesis delimiter"
    fields = text[close_index + 2 :].split()
    if not fields:
        return "missing status field"
    values = {}
    for index, label in ((1, "parent pid"), (19, "start ticks"), (11, "user ticks"), (12, "system ticks"), (21, "rss pages")):
        value, error = checked_integer(fields, index, label)
        if error:
            return error
        values[index] = value
    rss_bytes = values[21] * os.sysconf("SC_PAGE_SIZE")
    if rss_bytes < I64_MIN or rss_bytes > I64_MAX:
        return "overflow rss bytes"
    user_system_ticks = values[11] + values[12]
    if user_system_ticks < I64_MIN or user_system_ticks > I64_MAX:
        return "overflow user + system ticks"
    for index, label in ((20, "virtual bytes"), (2, "process group"), (3, "session"), (4, "terminal"), (16, "priority"), (15, "nice"), (17, "thread count"), (36, "exit signal")):
        value, error = checked_integer(fields, index, label)
        if error:
            return error
        values[index] = value
    counters = [
        values[11], values[12], user_system_ticks, values[19], 0,
        rss_bytes, values[20], values[2], values[3], values[4], values[16],
        values[15], values[17], values[36],
    ]
    negative = [index for index in (0, 1, 2, 3, 5, 6, 7, 8, 12, 13) if counters[index] < 0]
    if negative:
        return f"negative selected counters {negative}: {counters!r}"
    return None


def observe_entry(pid):
    base = f"/proc/{pid}"
    observation = {"path": base}
    try:
        entry = os.stat(base)
        observation["directory_stat"] = {
            "inode": entry.st_ino,
            "mode": entry.st_mode,
            "uid": entry.st_uid,
            "gid": entry.st_gid,
        }
    except OSError as error:
        observation["directory_stat_error"] = {
            "errno": error.errno,
            "strerror": error.strerror,
        }
    try:
        with open(base + "/status", "rb") as status_file:
            status_bytes = status_file.read()
        observation["status_bytes_hex"] = status_bytes.hex()
        observation["status_fields"] = {
            line.split(b":", 1)[0].decode("ascii", "backslashreplace"): line.split(b":", 1)[1].strip().decode("ascii", "backslashreplace")
            for line in status_bytes.splitlines()
            if b":" in line and line.split(b":", 1)[0] in {b"Name", b"State", b"Tgid", b"Pid", b"PPid", b"NSpid", b"Uid"}
        }
    except OSError as error:
        observation["status_error"] = {
            "errno": error.errno,
            "strerror": error.strerror,
        }
    return observation


def write_result(result):
    with open("/scratch/procps-stat-observe/standalone-stat-probe.json", "w", encoding="utf-8") as output:
        json.dump(result, output, indent=2, sort_keys=True)
        output.write("\n")
    print(json.dumps(result, sort_keys=True))


workers = []
start = time.monotonic()
scans = 0
entries_seen = 0
stat_reads = 0
read_errors = {}
result = None
try:
    for _ in range(WORKER_COUNT):
        workers.append(subprocess.Popen(["/bin/bash", "-c", WORKER], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    deadline = start + MAX_SECONDS
    while time.monotonic() < deadline:
        scans += 1
        try:
            names = os.listdir("/proc")
        except OSError as error:
            result = {"outcome": "proc-list-error", "errno": error.errno, "strerror": error.strerror}
            break
        pids = [name for name in names if name.isascii() and name.isdecimal()]
        entries_seen += len(pids)
        for pid_text in pids:
            try:
                with open(f"/proc/{pid_text}/stat", "rb") as stat_file:
                    raw = stat_file.read(65537)
            except OSError as error:
                read_errors[error.errno] = read_errors.get(error.errno, 0) + 1
                continue
            stat_reads += 1
            if len(raw) > 65536:
                result = {"outcome": "stat-over-64k", "pid": int(pid_text), "byte_count": len(raw)}
                break
            failure = parser_failure(raw)
            if failure is not None:
                pid = int(pid_text)
                with open("/scratch/procps-stat-observe/standalone-stat-malformed.bin", "wb") as raw_output:
                    raw_output.write(raw)
                result = {
                    "outcome": "first-parser-rejection",
                    "pid": pid,
                    "parser_failure": failure,
                    "stat_byte_count": len(raw),
                    "stat_hex": raw.hex(),
                    "stat_repr": repr(raw),
                    "process_entry_observation": observe_entry(pid),
                    "scan_number": scans,
                    "entries_seen": entries_seen,
                    "stat_reads": stat_reads,
                }
                break
        if result is not None:
            break
    if result is None:
        result = {"outcome": "deadline-no-parser-rejection"}
finally:
    for worker in workers:
        try:
            worker.send_signal(signal.SIGTERM)
        except ProcessLookupError:
            pass
    for worker in workers:
        try:
            worker.wait(timeout=2)
        except subprocess.TimeoutExpired:
            worker.kill()
            worker.wait()

result.update({
    "elapsed_seconds": round(time.monotonic() - start, 6),
    "hard_deadline_seconds": MAX_SECONDS,
    "workers": WORKER_COUNT,
    "max_churn_children": WORKER_COUNT,
    "worker_pids": [worker.pid for worker in workers],
    "worker_return_codes": [worker.returncode for worker in workers],
    "scans": scans,
    "entries_seen": entries_seen,
    "stat_reads": stat_reads,
    "read_errors_by_errno": read_errors,
    "same_parser_source": "src/modules/process.rs::parse_linux_stat field selection and signed i64 checks",
    "interpreter": sys.executable,
    "interpreter_sha256": hashlib.sha256(open(sys.executable, "rb").read()).hexdigest(),
})
write_result(result)
