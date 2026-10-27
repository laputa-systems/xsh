from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from enum import Enum
import errno
import os
from pathlib import Path
from pathlib import PurePosixPath
import re


__all__ = (
    "Availability",
    "Unavailable",
    "GroupScope",
    "ResourceWindowError",
    "CgroupIdentity",
    "CgroupSnapshot",
    "CaseResources",
    "capture_current_cgroup",
    "validate_case_window",
)


class Availability(str, Enum):
    UNSUPPORTED = "unsupported"
    INCOMPLETE = "incomplete"


@dataclass(frozen=True)
class Unavailable:
    availability: Availability
    reason: str


_Counter = int | Unavailable
_Counters = dict[str, int] | Unavailable


class GroupScope(str, Enum):
    FRESH_CASE_CONTAINER = "fresh_case_container_including_driver_init_workers"
    SHARED_CONTAINER = "shared_container_aggregate"


class ResourceWindowError(ValueError):
    pass


@dataclass(frozen=True)
class CgroupIdentity:
    device: int
    inode: int
    namespace_inode: int
    membership: str


@dataclass(frozen=True)
class _CgroupLocation:
    path: Path
    memory_events_hierarchical: bool | Unavailable


@dataclass(frozen=True)
class CgroupSnapshot:
    identity: CgroupIdentity | Unavailable
    memory_events_hierarchical: bool | Unavailable
    pressure_accounting_enabled: bool | Unavailable
    memory_peak_bytes: _Counter
    memory_current_bytes: _Counter
    memory_max: str | Unavailable
    memory_stat: _Counters
    memory_events: _Counters
    cpu_stat: _Counters
    cpu_max: str | Unavailable
    cpuset_cpus_effective: str | Unavailable
    pids_current: _Counter
    pids_peak: _Counter
    pressure: dict[str, _Counters]
    observer_pid: int
    observer_rss_bytes: _Counter
    observer_peak_rss_bytes: _Counter


@dataclass(frozen=True)
class CaseResources:
    scope: GroupScope
    identity: CgroupIdentity
    memory_events_hierarchical: bool | Unavailable
    pressure_accounting_enabled: bool | Unavailable
    memory_peak_sample_bytes: _Counter
    cpu_usage_ns: _Counter
    cpu_throttled_ns: _Counter
    cpu_throttled_periods: _Counter
    cpu_max: str | Unavailable
    memory_max: str | Unavailable
    cpuset_cpus_effective: str | Unavailable
    memory_events_delta: _Counters
    pressure_stall_us: dict[str, _Counters]
    declared_workers: tuple[tuple[str, int], ...]
    observed_cgroup_tasks_peak: _Counter
    observed_cgroup_tasks_current: _Counter
    observer_peak_rss_bytes: _Counter
    memory_stat_boundary: _Counters
    violations: tuple[str, ...]
    telemetry_complete: bool


def _missing(reason: str, *, incomplete: bool = False) -> Unavailable:
    return Unavailable(Availability.INCOMPLETE if incomplete else Availability.UNSUPPORTED, reason)


def _read(path: Path) -> str | Unavailable:
    try:
        return path.read_text(encoding="ascii").strip()
    except OSError as error:
        unsupported = error.errno in {errno.ENOENT, errno.EACCES, errno.EPERM, errno.ENODEV, errno.EOPNOTSUPP}
        return _missing(f"{path.name}: {error.strerror}", incomplete=not unsupported)
    except UnicodeError:
        return _missing(f"{path.name}: non-ASCII kernel counter", incomplete=True)


def _integer(path: Path) -> _Counter:
    value = _read(path)
    if isinstance(value, Unavailable):
        return value
    if not value.isascii() or not value.isdecimal():
        return _missing(f"{path.name}: expected a nonnegative integer", incomplete=True)
    return int(value)


def _toggle(path: Path) -> bool | Unavailable:
    value = _read(path)
    if isinstance(value, Unavailable):
        return value
    if value not in {"0", "1"}:
        return _missing(f"{path.name}: expected 0 or 1", incomplete=True)
    return value == "1"


def _valid_memory_limit(value: str) -> bool:
    return value == "max" or value.isascii() and value.isdecimal()


def _valid_cpu_limit(value: str) -> bool:
    fields = value.split()
    return (
        len(fields) == 2
        and (fields[0] == "max" or fields[0].isascii() and fields[0].isdecimal() and int(fields[0]) > 0)
        and fields[1].isascii()
        and fields[1].isdecimal()
        and int(fields[1]) > 0
    )


def _limit(path: Path, *, cpu: bool = False) -> str | Unavailable:
    value = _read(path)
    if isinstance(value, Unavailable):
        return value
    valid = _valid_cpu_limit(value) if cpu else _valid_memory_limit(value)
    return value if valid else _missing(f"{path.name}: malformed resource limit", incomplete=True)


def _valid_cpuset(value: str) -> bool:
    if not re.fullmatch(r"[0-9]+(?:-[0-9]+)?(?:,[0-9]+(?:-[0-9]+)?)*", value):
        return False
    for item in value.split(","):
        bounds = item.split("-")
        if len(bounds) == 2 and int(bounds[0]) > int(bounds[1]):
            return False
    return True


def _cpuset(path: Path) -> str | Unavailable:
    value = _read(path)
    if isinstance(value, Unavailable):
        return value
    if not _valid_cpuset(value):
        return _missing(f"{path.name}: malformed effective CPU set", incomplete=True)
    return value


def _counters(path: Path) -> _Counters:
    value = _read(path)
    if isinstance(value, Unavailable):
        return value
    result = {}
    for line in value.splitlines():
        fields = line.split()
        if len(fields) != 2 or not fields[0] or not fields[0].isascii() \
                or any(character.isspace() for character in fields[0]) or fields[0] in result \
                or not fields[1].isascii() or not fields[1].isdecimal():
            return _missing(f"{path.name}: malformed or duplicate counter", incomplete=True)
        result[fields[0]] = int(fields[1])
    return result if result else _missing(f"{path.name}: empty counters", incomplete=True)


def _pressure(path: Path) -> _Counters:
    value = _read(path)
    if isinstance(value, Unavailable):
        return value
    result = {}
    for line in value.splitlines():
        fields = line.split()
        if not fields or fields[0] not in {"some", "full"} or fields[0] in result:
            return _missing(f"{path.name}: malformed or duplicate pressure row", incomplete=True)
        totals = [word[6:] for word in fields[1:] if word.startswith("total=")]
        if len(totals) != 1 or not totals[0].isascii() or not totals[0].isdecimal():
            return _missing(f"{path.name}: missing pressure total", incomplete=True)
        result[fields[0]] = int(totals[0])
    if "some" not in result or "full" not in result:
        return _missing(f"{path.name}: incomplete some/full pressure totals", incomplete=True)
    return result


def _observer_memory(proc_root: Path) -> tuple[_Counter, _Counter]:
    status = _read(proc_root / "self/status")
    if isinstance(status, Unavailable):
        return status, status
    values = {}
    for line in status.splitlines():
        fields = line.split()
        if fields and fields[0] in {"VmRSS:", "VmHWM:"}:
            if fields[0] in values or len(fields) != 3 or fields[2] != "kB" or not fields[1].isascii() or not fields[1].isdecimal():
                problem = _missing("observer status: malformed RSS", incomplete=True)
                return problem, problem
            values[fields[0]] = int(fields[1]) * 1024
    return values.get("VmRSS:", _missing("observer VmRSS unavailable")), values.get("VmHWM:", _missing("observer VmHWM unavailable"))


def _absolute_posix_path(value: str) -> PurePosixPath | Unavailable:
    if not value.startswith("/") or "\x00" in value:
        return _missing("cgroup path is not absolute", incomplete=True)
    if value != "/" and any(part in {"", ".", ".."} for part in value.split("/")[1:]):
        return _missing("cgroup path is not canonical", incomplete=True)
    return PurePosixPath(value)


def _decode_mountinfo_path(value: str) -> str | Unavailable:
    decoded = []
    index = 0
    while index < len(value):
        if value[index] != "\\":
            decoded.append(value[index])
            index += 1
            continue
        escape = value[index + 1:index + 4]
        if escape not in {"040", "011", "012", "134"}:
            return _missing("mountinfo contains an invalid path escape", incomplete=True)
        decoded.append(chr(int(escape, 8)))
        index += 4
    return "".join(decoded)


def _unified_membership(proc_root: Path) -> str | Unavailable:
    value = _read(proc_root / "self/cgroup")
    if isinstance(value, Unavailable):
        return value
    memberships = []
    for line in value.splitlines():
        fields = line.split(":", 2)
        if len(fields) != 3:
            return _missing("malformed process cgroup membership", incomplete=True)
        if fields[0] == "0" and fields[1] == "":
            memberships.append(fields[2])
    if len(memberships) != 1:
        return _missing("unified cgroup v2 membership unavailable", incomplete=len(memberships) > 1)
    path = _absolute_posix_path(memberships[0])
    if isinstance(path, Unavailable):
        return path
    return memberships[0]


def _current_cgroup_path(proc_root: Path, membership: str) -> _CgroupLocation | Unavailable:
    mountinfo = _read(proc_root / "self/mountinfo")
    if isinstance(mountinfo, Unavailable):
        return mountinfo
    membership_path = _absolute_posix_path(membership)
    if isinstance(membership_path, Unavailable):
        return membership_path
    candidates = []
    cgroup2_mount_seen = False
    for line in mountinfo.splitlines():
        before, separator, after = line.partition(" - ")
        fields = before.split()
        filesystem = after.split()
        if not separator or len(fields) < 6 or not filesystem:
            return _missing("malformed process mountinfo", incomplete=True)
        if filesystem[0] != "cgroup2":
            continue
        cgroup2_mount_seen = True
        root_text = _decode_mountinfo_path(fields[3])
        mount_text = _decode_mountinfo_path(fields[4])
        if isinstance(root_text, Unavailable) or isinstance(mount_text, Unavailable):
            return root_text if isinstance(root_text, Unavailable) else mount_text
        mount_root = _absolute_posix_path(root_text)
        mount_point = _absolute_posix_path(mount_text)
        if isinstance(mount_root, Unavailable) or isinstance(mount_point, Unavailable):
            return mount_root if isinstance(mount_root, Unavailable) else mount_point
        if len(filesystem) < 3 or not filesystem[2]:
            memory_events_hierarchical: bool | Unavailable = _missing("cgroup2 super-options unavailable", incomplete=True)
        else:
            super_options = filesystem[2].split(",")
            if any(not option for option in super_options):
                memory_events_hierarchical = _missing("cgroup2 super-options malformed", incomplete=True)
            else:
                memory_events_hierarchical = "memory_localevents" not in super_options
        try:
            relative = membership_path.relative_to(mount_root)
        except ValueError:
            continue
        candidates.append((len(mount_root.parts), Path(mount_point.as_posix()).joinpath(*relative.parts), memory_events_hierarchical))
    if not candidates:
        if not cgroup2_mount_seen:
            return _missing("no visible cgroup2 mount")
        return _missing("process cgroup membership is outside visible cgroup2 mounts", incomplete=True)
    specificity = max(candidate[0] for candidate in candidates)
    selected = [(path, hierarchy) for depth, path, hierarchy in candidates if depth == specificity]
    identities = []
    for path, _ in selected:
        try:
            info = path.stat()
        except OSError as error:
            return _missing(f"cgroup path unavailable: {error.strerror}", incomplete=True)
        identities.append((info.st_dev, info.st_ino))
    if len(set(identities)) != 1:
        return _missing("process cgroup maps to ambiguous visible mounts", incomplete=True)
    hierarchy_states = [hierarchy for _, hierarchy in selected]
    if all(state == hierarchy_states[0] for state in hierarchy_states):
        hierarchy = hierarchy_states[0]
    else:
        hierarchy = _missing("cgroup2 mount super-options disagree", incomplete=True)
    return _CgroupLocation(selected[0][0], hierarchy)


def _stable_toggle(before: bool | Unavailable, after: bool | Unavailable, label: str) -> bool | Unavailable:
    if before == after:
        return before
    return _missing(f"{label} changed or became unavailable during the case", incomplete=True)


def _capture(proc_root: Path) -> CgroupSnapshot:
    membership = _unified_membership(proc_root)
    location = _current_cgroup_path(proc_root, membership) if isinstance(membership, str) else membership
    identity: CgroupIdentity | Unavailable
    initial_cgroup_identity = None
    initial_namespace_inode = None
    if isinstance(location, Unavailable):
        identity = location
        cgroup_path = location
        memory_events_hierarchical = location
        pressure_accounting_enabled = location
    else:
        cgroup_path = location.path
        memory_events_hierarchical = location.memory_events_hierarchical
        try:
            info = cgroup_path.stat()
            namespace = (proc_root / "self/ns/cgroup").stat()
            initial_cgroup_identity = (info.st_dev, info.st_ino)
            initial_namespace_inode = namespace.st_ino
            identity = CgroupIdentity(info.st_dev, info.st_ino, namespace.st_ino, membership)
        except OSError as error:
            cgroup_path = _missing(f"cgroup identity: {error.strerror}", incomplete=True)
            identity = cgroup_path
            memory_events_hierarchical = cgroup_path
            pressure_accounting_enabled = cgroup_path
        else:
            pressure_accounting_enabled = _toggle(cgroup_path / "cgroup.pressure")
    rss, peak_rss = _observer_memory(proc_root)
    if isinstance(cgroup_path, Unavailable):
        unavailable = cgroup_path
        memory_peak = memory_current = memory_max = unavailable
        memory_stat = memory_events = cpu_stat = cpu_max = cpuset = unavailable
        pids_current = pids_peak = unavailable
        pressure = {name: unavailable for name in ("cpu", "memory", "io")}
    else:
        memory_peak = _integer(cgroup_path / "memory.peak")
        memory_current = _integer(cgroup_path / "memory.current")
        memory_max = _limit(cgroup_path / "memory.max")
        memory_stat = _counters(cgroup_path / "memory.stat")
        if isinstance(memory_events_hierarchical, Unavailable):
            memory_events = memory_events_hierarchical
        elif memory_events_hierarchical:
            memory_events = _counters(cgroup_path / "memory.events")
        else:
            memory_events = _missing("memory_localevents makes memory.events local-only", incomplete=True)
        cpu_stat = _counters(cgroup_path / "cpu.stat")
        cpu_max = _limit(cgroup_path / "cpu.max", cpu=True)
        cpuset = _cpuset(cgroup_path / "cpuset.cpus.effective")
        pids_current = _integer(cgroup_path / "pids.current")
        pids_peak = _integer(cgroup_path / "pids.peak")
        if isinstance(pressure_accounting_enabled, Unavailable):
            pressure = {name: pressure_accounting_enabled for name in ("cpu", "memory", "io")}
        elif pressure_accounting_enabled:
            pressure = {name: _pressure(cgroup_path / f"{name}.pressure") for name in ("cpu", "memory", "io")}
        else:
            unavailable = _missing("cgroup PSI accounting is disabled")
            pressure = {name: unavailable for name in ("cpu", "memory", "io")}
        try:
            final_cgroup = cgroup_path.stat()
            final_namespace = (proc_root / "self/ns/cgroup").stat()
            if (final_cgroup.st_dev, final_cgroup.st_ino) != initial_cgroup_identity or final_namespace.st_ino != initial_namespace_inode:
                identity = _missing("cgroup identity changed during snapshot", incomplete=True)
        except OSError as error:
            identity = _missing(f"cgroup identity: {error.strerror}", incomplete=True)
        final_location = _current_cgroup_path(proc_root, membership)
        if isinstance(final_location, Unavailable):
            identity = _missing("cgroup mount mapping became unavailable during snapshot", incomplete=True)
            memory_events_hierarchical = final_location
            memory_events = final_location
        elif final_location.path != cgroup_path:
            identity = _missing("cgroup mount mapping changed during snapshot", incomplete=True)
            memory_events_hierarchical = _missing("memory.events subtree attribution changed during snapshot", incomplete=True)
            memory_events = memory_events_hierarchical
        else:
            memory_events_hierarchical = _stable_toggle(
                memory_events_hierarchical,
                final_location.memory_events_hierarchical,
                "memory.events subtree attribution",
            )
            if memory_events_hierarchical is not True:
                memory_events = memory_events_hierarchical if isinstance(memory_events_hierarchical, Unavailable) else _missing("memory_localevents makes memory.events local-only", incomplete=True)
        final_pressure_accounting = _toggle(cgroup_path / "cgroup.pressure")
        pressure_accounting_enabled = _stable_toggle(
            pressure_accounting_enabled,
            final_pressure_accounting,
            "cgroup PSI accounting",
        )
        if pressure_accounting_enabled is not True:
            pressure_unavailable = pressure_accounting_enabled if isinstance(pressure_accounting_enabled, Unavailable) else _missing("cgroup PSI accounting is disabled")
            pressure = {name: pressure_unavailable for name in ("cpu", "memory", "io")}
        final_membership = _unified_membership(proc_root)
        if final_membership != membership:
            identity = _missing("process cgroup membership changed or became unavailable during snapshot", incomplete=True)
    return CgroupSnapshot(
        identity=identity, memory_events_hierarchical=memory_events_hierarchical,
        pressure_accounting_enabled=pressure_accounting_enabled,
        memory_peak_bytes=memory_peak, memory_current_bytes=memory_current, memory_max=memory_max,
        memory_stat=memory_stat, memory_events=memory_events,
        cpu_stat=cpu_stat, cpu_max=cpu_max, cpuset_cpus_effective=cpuset,
        pids_current=pids_current, pids_peak=pids_peak, pressure=pressure,
        observer_pid=os.getpid(), observer_rss_bytes=rss, observer_peak_rss_bytes=peak_rss,
    )


def capture_current_cgroup() -> CgroupSnapshot:
    return _capture(Path("/proc"))


def _delta(before: _Counters, after: _Counters, label: str) -> _Counters:
    if isinstance(before, Unavailable):
        return before
    if isinstance(after, Unavailable):
        return after
    if set(before) != set(after):
        return _missing(f"{label}: counter set changed during the case", incomplete=True)
    if any(after[name] < value for name, value in before.items()):
        raise ResourceWindowError(f"{label}: cumulative counter decreased")
    return {name: after[name] - value for name, value in before.items()}


def _counter(values: _Counters, name: str, multiplier: int = 1) -> _Counter:
    if isinstance(values, Unavailable):
        return values
    if name not in values:
        return _missing(f"required counter {name} unavailable")
    return values[name] * multiplier


def validate_case_window(
    before: CgroupSnapshot,
    after: CgroupSnapshot,
    *,
    declared_workers: Mapping[str, int],
    scope: GroupScope,
) -> CaseResources:
    if not isinstance(scope, GroupScope):
        raise ResourceWindowError("the controller must declare a typed container scope")
    if not isinstance(declared_workers, Mapping) or not declared_workers or any(not isinstance(name, str) or not name or type(count) is not int or count < 0 for name, count in declared_workers.items()) or not any(declared_workers.values()):
        raise ResourceWindowError("declared workers require names, nonnegative integer counts and at least one process")
    if isinstance(before.identity, Unavailable) or isinstance(after.identity, Unavailable):
        problem = before.identity if isinstance(before.identity, Unavailable) else after.identity
        raise ResourceWindowError(f"cgroup identity unavailable: {problem.reason}")
    if before.identity != after.identity:
        raise ResourceWindowError("cgroup identity changed during the case")
    if before.observer_pid != after.observer_pid:
        raise ResourceWindowError("observer process changed during the case")
    for name in ("memory_max", "cpu_max", "cpuset_cpus_effective"):
        initial = getattr(before, name)
        final = getattr(after, name)
        if isinstance(initial, str) and isinstance(final, str) and initial != final:
            raise ResourceWindowError(f"container {name} changed during the case")
    if not isinstance(before.memory_peak_bytes, Unavailable) and not isinstance(after.memory_peak_bytes, Unavailable) and after.memory_peak_bytes < before.memory_peak_bytes:
        raise ResourceWindowError("container lifetime memory peak decreased")
    cpu = _delta(before.cpu_stat, after.cpu_stat, "cpu.stat")
    memory_events_hierarchical = _stable_toggle(
        before.memory_events_hierarchical,
        after.memory_events_hierarchical,
        "memory.events subtree attribution",
    )
    if memory_events_hierarchical is True:
        events = _delta(before.memory_events, after.memory_events, "memory.events")
    elif memory_events_hierarchical is False:
        events = _missing("memory_localevents makes memory.events local-only", incomplete=True)
    else:
        events = memory_events_hierarchical
    pressure_accounting_state = _stable_toggle(
        before.pressure_accounting_enabled,
        after.pressure_accounting_enabled,
        "cgroup PSI accounting",
    )
    pressure_accounting_enabled = (
        _missing("cgroup PSI accounting is disabled")
        if pressure_accounting_state is False
        else pressure_accounting_state
    )
    pressure = {}
    if pressure_accounting_enabled is True:
        for name in ("cpu", "memory", "io"):
            initial = before.pressure.get(name, _missing(f"{name}.pressure unavailable"))
            final = after.pressure.get(name, _missing(f"{name}.pressure unavailable"))
            pressure[name] = _delta(initial, final, f"{name}.pressure")
    else:
        pressure_unavailable = pressure_accounting_enabled if isinstance(pressure_accounting_enabled, Unavailable) else _missing("cgroup PSI accounting is disabled")
        pressure = {name: pressure_unavailable for name in ("cpu", "memory", "io")}
    case_peak_attribution = after.memory_peak_bytes if scope is GroupScope.FRESH_CASE_CONTAINER else _missing("shared container memory peak is not a per-case peak")
    usage = _counter(cpu, "usage_usec", 1000)
    throttled = _counter(cpu, "throttled_usec", 1000)
    periods = _counter(cpu, "nr_throttled")
    violations = []
    for name in ("oom", "oom_kill", "oom_group_kill"):
        count = _counter(events, name)
        if isinstance(count, int) and count > 0:
            violations.append(f"memory.events {name} increased by {count}")
    required = [case_peak_attribution, after.memory_peak_bytes, before.memory_peak_bytes,
                before.memory_current_bytes, after.memory_current_bytes,
                memory_events_hierarchical, pressure_accounting_enabled,
                usage, throttled, periods, _counter(events, "oom"), _counter(events, "oom_kill"),
                _counter(events, "oom_group_kill"), after.pids_current, after.pids_peak,
                after.observer_peak_rss_bytes, after.memory_stat, *pressure.values(), after.cpu_max,
                before.cpu_max, after.memory_max, before.memory_max,
                after.cpuset_cpus_effective, before.cpuset_cpus_effective]
    return CaseResources(
        scope=scope, identity=after.identity,
        memory_events_hierarchical=memory_events_hierarchical,
        pressure_accounting_enabled=pressure_accounting_enabled,
        memory_peak_sample_bytes=after.memory_peak_bytes, cpu_usage_ns=usage,
        cpu_throttled_ns=throttled, cpu_throttled_periods=periods, memory_events_delta=events,
        cpu_max=after.cpu_max, memory_max=after.memory_max, cpuset_cpus_effective=after.cpuset_cpus_effective,
        pressure_stall_us=pressure, declared_workers=tuple(sorted(declared_workers.items())),
        observed_cgroup_tasks_peak=after.pids_peak, observed_cgroup_tasks_current=after.pids_current,
        observer_peak_rss_bytes=after.observer_peak_rss_bytes,
        memory_stat_boundary=after.memory_stat, violations=tuple(violations),
        telemetry_complete=all(not isinstance(value, Unavailable) for value in required),
    )
