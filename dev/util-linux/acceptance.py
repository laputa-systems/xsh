from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from pathlib import Path
import re
import time
from typing import Protocol


POSITIVE_BOUNDARIES = {
    "lslocks": "test_lslocks_blocker_flock",
    "lsipc": "test_lsipc_visibility_matches_for_owner_and_unprivileged_observer",
    "lsmem": "test_lsmem_rooted_sparse_memory_attributes_and_output_modes",
    "mountpoint": "test_mountpoint_private_same_device_bind_mount",
    "namei": "test_namei_private_same_device_bind_mount_matches_reference",
}
STARTUP_TARGETS = (
    ("flock", "core/tests/test-util-linux-process-regressions.xsh::test_util_linux_flock_keeps_lock_through_waited_new_session_child"),
    ("setsid", "core/tests/test-util-linux-process-regressions.xsh::test_util_linux_flock_keeps_lock_through_waited_new_session_child"),
    ("rev", "core/tests/test-util-linux-process-regressions.xsh::test_util_linux_rev_reverses_utf8_split_across_read_chunks"),
    ("chrt", "core/tests/test-util-linux-process-regressions.xsh::test_util_linux_chrt_preserves_signal_termination"),
    ("lsblk", "core/tests/test-util-linux-storage-regressions.xsh::test_util_linux_rooted_storage_commands_join_block_and_mount_identity"),
    ("findmnt", "core/tests/test-util-linux-storage-regressions.xsh::test_util_linux_rooted_storage_commands_join_block_and_mount_identity"),
    ("lscpu", "core/tests/test-util-linux-storage-regressions.xsh::test_util_linux_lscpu_json_summary_keeps_field_values_as_strings"),
    ("lsns", "core/tests/test-util-linux-storage-regressions.xsh::test_util_linux_lsns_selects_and_releases_a_live_user_namespace"),
)
REFERENCE_PROFILE = "apk-package-2.42.4-r3"
REFERENCE_ALLOCATOR = "musl-libc"
GROUP_BUDGET_SECONDS = 120
REGRESSION_FILES = (
    "core/tests/test-util-linux-storage-regressions.xsh",
    "core/tests/test-util-linux-process-regressions.xsh",
)
HOST_BOUNDARY_FILES = (
    "tests/xsh/stdlib/linux_ipc.xsh",
    "tests/xsh/stdlib/linux_namespaces.xsh",
    "tests/xsh/api-tool.xsh",
    "core/tests/test-util-linux-ipc-waiters.xsh",
    "core/tests/test-util-linux-contexts.xsh",
    "core/tests/test-util-linux-io-boundaries.xsh",
    "core/tests/test-util-linux-race-boundaries.xsh",
)

_CASE_DECLARATION = re.compile(r"^test (test_[A-Za-z0-9_]+)\s*\{", re.MULTILINE)
_NATIVE_SELECTOR = re.compile(
    r"^(?P<path>(?:core/tests|dev/tests|tests|showcase/tests)/[A-Za-z0-9_./-]+\.xsh)::"
    r"(?P<name>test_[A-Za-z0-9_]+)$"
)
_SUMMARY = re.compile(
    r"test result: (?P<result>ok|FAILED)\. (?P<passed>\d+) passed; "
    r"(?P<failed>\d+) failed; (?P<skipped>\d+) skipped"
)
_RUNNING = re.compile(r"running (?P<count>\d+) tests")


class AcceptanceRuntime(Protocol):
    ROOT: Path
    COHORT: tuple[str, ...]
    PACKAGE_COMMIT: str
    PRODUCT_SOURCE: str

    def native_case(
        self,
        selector: str,
        *,
        private: bool = False,
        timeout: float = 30,
    ) -> bytes: ...

    def source_identity(self) -> SourceIdentity: ...

    def regression_selectors(self, source: SourceIdentity) -> tuple[str, ...]: ...

    def measure_resources(self) -> ResourceReport: ...

    def close_gates(self) -> CloseReport: ...


@dataclass(frozen=True)
class NativeResult:
    passed: int
    failed: int
    skipped: int
    selectors: tuple[str, ...]


def _selected_commands(args, cohort: tuple[str, ...]) -> tuple[str, ...]:
    if args.all:
        commands = cohort
    elif args.command is not None:
        commands = (args.command,)
    elif args.commands is not None:
        commands = tuple(args.commands.split(","))
    else:
        raise ValueError("parity requires a command selector")
    if not commands or any(not command for command in commands):
        raise ValueError("commands must name a nonempty subset of the cohort")
    if len(set(commands)) != len(commands):
        raise ValueError("commands must be distinct")
    unknown = sorted(set(commands) - set(cohort))
    if unknown:
        raise ValueError(f"commands are outside the cohort: {', '.join(unknown)}")
    if args.case and len(commands) != 1:
        raise ValueError("a case selector requires one command")
    return tuple(commands)


def _case_names(root: Path, command: str) -> tuple[str, ...]:
    relative = f"core/tests/test-{command}.xsh"
    source = root / relative
    if not source.is_file():
        raise ValueError(f"missing native behavior source: {relative}")
    names = tuple(_CASE_DECLARATION.findall(source.read_text(encoding="utf-8")))
    if not names:
        raise ValueError(f"selector matched no behavior: {command}")
    if len(set(names)) != len(names):
        raise ValueError(f"native behavior selectors are duplicated: {command}")
    return names


def declared_native_selectors(root: Path, relative_files: tuple[str, ...]) -> tuple[str, ...]:
    selectors: list[str] = []
    for relative in relative_files:
        path = root / relative
        if not path.is_file():
            raise ValueError(f"missing required native behavior source: {relative}")
        names = tuple(_CASE_DECLARATION.findall(path.read_text(encoding="utf-8")))
        if not names:
            raise ValueError(f"native behavior source declares no tests: {relative}")
        if len(names) != len(set(names)):
            raise ValueError(f"native behavior source duplicates selectors: {relative}")
        selectors.extend(f"{relative}::{name}" for name in names)
    if len(selectors) != len(set(selectors)):
        raise ValueError("native behavior sources declare duplicate selectors")
    return tuple(sorted(selectors))


def _case_selector(command: str, name: str) -> str:
    return f"core/tests/test-{command}.xsh::{name}"


def declared_regression_selectors(root: Path) -> tuple[str, ...]:
    selectors: list[str] = []
    for relative in REGRESSION_FILES:
        path = root / relative
        if not path.is_file():
            raise ValueError(f"missing required regression source: {relative}")
        names = tuple(_CASE_DECLARATION.findall(path.read_text(encoding="utf-8")))
        if not names:
            raise ValueError(f"regression source declares no behaviors: {relative}")
        if len(set(names)) != len(names):
            raise ValueError(f"regression source duplicates behavior selectors: {relative}")
        selectors.extend(f"{relative}::{name}" for name in names)
    if len(set(selectors)) != len(selectors):
        raise ValueError("regression sources declare duplicate selectors")
    return tuple(sorted(selectors))


def _expected_regression_selectors(
    runtime: AcceptanceRuntime,
    source: SourceIdentity,
) -> tuple[str, ...]:
    before = _current_source(runtime)
    if before != source:
        raise ValueError("regression selectors use a stale source")
    produced = runtime.regression_selectors(source)
    declared = declared_regression_selectors(runtime.ROOT)
    after = _current_source(runtime)
    if after != before:
        raise ValueError("source changed while resolving regression selectors")
    if produced != declared:
        raise ValueError("regression selector producer differs from actual native declarations")
    return declared


def _selected_case(args, command: str, names: tuple[str, ...]) -> str | None:
    if args.case is None:
        return None
    relative = f"core/tests/test-{command}.xsh::"
    name = args.case.removeprefix(relative)
    if name not in names:
        raise ValueError(f"selector matched no behavior: {command}::{args.case}")
    return name


def _parse_native_suite(
    label: str,
    expected: tuple[str, ...],
    output: bytes,
) -> NativeResult:
    result = _parse_native_suite_result(label, expected, output)
    if result.skipped:
        raise ValueError(f"native runner skipped a mandatory behavior for {label}: {result.skipped}")
    return NativeResult(result.passed, 0, 0, result.selectors)


def _parse_native_group(command: str, expected: tuple[str, ...], output: bytes) -> NativeResult:
    selectors = tuple(_case_selector(command, name) for name in expected)
    result = _parse_native_suite(command, selectors, output)
    names = tuple(selector.rsplit("::", 1)[-1] for selector in result.selectors)
    return NativeResult(result.passed, result.failed, result.skipped, names)


def _parse_native_selector_list(label: str, output: bytes) -> tuple[str, ...]:
    try:
        text = output.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError(f"{label} selector list was not UTF-8") from error
    selectors = tuple(text.splitlines())
    if not selectors:
        raise ValueError(f"{label} selector list is empty or malformed")
    for selector in selectors:
        match = _NATIVE_SELECTOR.fullmatch(selector)
        if match is None:
            raise ValueError(f"{label} selector list contains a malformed test identity: {selector}")
        if any(part in ("", ".", "..") for part in match.group("path").split("/")):
            raise ValueError(f"{label} selector list contains an unsafe source path: {selector}")
    if len(selectors) != len(set(selectors)):
        raise ValueError(f"{label} selector list contains duplicates")
    return tuple(sorted(selectors))


@dataclass(frozen=True)
class NativeSuiteResult:
    passed: int
    skipped: tuple[tuple[str, str], ...]
    selectors: tuple[str, ...]


_NATIVE_CASE_OUTCOME = re.compile(
    r"^(?P<outcome>ok|FAILED|TIMEOUT|skipped: .+) "
    r"(?P<duration>\d+(?:\.\d+)?ms|\d+(?:\.\d+)?s|\d+m(?:\d+s)?|\d+h(?:\d+m)?)$"
)
_DECLARED_SKIP = re.compile(r"\btest\.skip\s*\(")


def case_declares_skip(root: Path, selector: str) -> bool:
    match = _NATIVE_SELECTOR.fullmatch(selector)
    if match is None or any(part in ("", ".", "..") for part in match.group("path").split("/")):
        raise ValueError(f"invalid native selector for skip accounting: {selector}")
    source = (root / match.group("path")).read_text(encoding="utf-8")
    declaration = re.search(
        rf"^test {re.escape(match.group('name'))}\s*\{{",
        source,
        re.MULTILINE,
    )
    if declaration is None:
        raise ValueError(f"native skip selector is not declared by its source: {selector}")
    following = re.search(r"^test test_[A-Za-z0-9_]+\s*\{", source[declaration.end():], re.MULTILINE)
    end = declaration.end() + following.start() if following is not None else len(source)
    return _DECLARED_SKIP.search(source[declaration.start():end]) is not None


def _parse_native_suite_result(
    label: str,
    expected: tuple[str, ...],
    output: bytes,
) -> NativeSuiteResult:
    if not expected:
        raise ValueError(f"selector matched no behavior: {label}")
    if len(expected) != len(set(expected)):
        raise ValueError(f"native behavior selectors are duplicated: {label}")
    try:
        text = output.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError(f"native result was not UTF-8: {label}") from error
    lines = text.splitlines()
    running = [match for line in lines if (match := _RUNNING.fullmatch(line))]
    if len(running) != 1 or int(running[0].group("count")) != len(expected):
        raise ValueError(f"native runner selected an unexpected test count for {label}: {text}")
    summaries = [match for line in lines if (match := _SUMMARY.fullmatch(line))]
    if len(summaries) != 1 or not lines or _SUMMARY.fullmatch(lines[-1]) is None:
        raise ValueError(f"native runner omitted its final result summary for {label}: {text}")
    actual: list[str] = []
    skipped: list[tuple[str, str]] = []
    failed = 0
    for line in lines:
        if " ... " not in line:
            continue
        selector, raw_outcome = line.split(" ... ", 1)
        if selector not in expected:
            raise ValueError(f"native runner reported an unselected behavior: {line}")
        outcome = _NATIVE_CASE_OUTCOME.fullmatch(raw_outcome)
        if outcome is None:
            raise ValueError(f"native runner reported a malformed behavior result: {line}")
        actual.append(selector)
        status = outcome.group("outcome")
        if status == "ok":
            continue
        if status.startswith("skipped: "):
            reason = status.removeprefix("skipped: ").strip()
            if not reason:
                raise ValueError(f"native skip omitted its reason: {line}")
            skipped.append((selector, reason))
            continue
        failed += 1
    if len(actual) != len(set(actual)) or tuple(sorted(actual)) != tuple(sorted(expected)):
        raise ValueError(f"native runner did not execute the selected behaviors exactly once: {text}")
    summary = summaries[0]
    passed, summary_failed, summary_skipped = (
        int(summary.group(field)) for field in ("passed", "failed", "skipped")
    )
    if (
        summary.group("result") != "ok"
        or (passed, summary_failed, summary_skipped) != (len(expected) - len(skipped) - failed, failed, len(skipped))
        or failed
    ):
        raise ValueError(f"native runner did not pass every non-skipped behavior: {text}")
    return NativeSuiteResult(passed, tuple(skipped), tuple(actual))


def _run_group(
    runtime: AcceptanceRuntime,
    command: str,
    names: tuple[str, ...],
    *,
    monotonic=time.monotonic,
) -> NativeResult:
    if not names:
        raise ValueError(f"selector matched no behavior: {command}")
    if len(names) != len(set(names)):
        raise ValueError(f"native behavior selectors are duplicated: {command}")
    deadline = monotonic() + GROUP_BUDGET_SECONDS
    results = []
    for name in names:
        remaining = deadline - monotonic()
        if remaining <= 0:
            raise TimeoutError(f"private native acceptance exceeded {GROUP_BUDGET_SECONDS}s: {command}")
        selector = _case_selector(command, name)
        output = runtime.native_case(selector, private=True, timeout=min(30.0, remaining))
        result = _parse_native_group(command, (name,), output)
        results.extend(result.selectors)
    if monotonic() > deadline:
        raise TimeoutError(f"private native acceptance exceeded {GROUP_BUDGET_SECONDS}s: {command}")
    return NativeResult(passed=len(results), failed=0, skipped=0, selectors=tuple(results))


def parity(args, runtime: AcceptanceRuntime) -> None:
    commands = _selected_commands(args, runtime.COHORT)
    cases = {
        command: _case_names(runtime.ROOT, command)
        for command in commands
    }
    selected_cases = {
        command: _selected_case(args, command, cases[command])
        for command in commands
    }
    if args.require_positive_boundaries:
        for command in commands:
            boundary = POSITIVE_BOUNDARIES.get(command)
            if boundary is None or boundary not in cases[command]:
                raise ValueError(f"required positive boundary is absent: {command}")
    for command in commands:
        names = cases[command]
        selected = selected_cases[command]
        if selected is None:
            result = _run_group(runtime, command, names)
            print(f"{command}: {result.passed} native behaviors passed")
        else:
            selector = _case_selector(command, selected)
            _parse_native_group(command, (selected,), runtime.native_case(selector, private=True))
    if args.require_positive_boundaries:
        for command in commands:
            boundary = POSITIVE_BOUNDARIES[command]
            selector = _case_selector(command, boundary)
            if selected_cases[command] != boundary:
                _parse_native_group(command, (boundary,), runtime.native_case(selector, private=True))


class MeasurementKind(Enum):
    EXISTING_BASELINE = "existing-baseline"
    NEW_REFERENCE = "new-reference"


class ResourceMetric(Enum):
    COMMAND = "command"
    STARTUP = "startup"


class PairOrder(Enum):
    BASELINE_FIRST = "baseline-first"
    CANDIDATE_FIRST = "candidate-first"


@dataclass(frozen=True)
class SourceIdentity:
    commit: str
    tree: str
    clean: bool

    @property
    def key(self) -> str:
        return f"{self.commit}:{self.tree}"


@dataclass(frozen=True)
class BuildContext:
    source: str
    profile: str
    target: str
    image: str
    allocator: str
    allocation: str
    cache_state: str = ""
    compile_jobs: int = 0


@dataclass(frozen=True)
class Readiness:
    status: int
    stdout: bytes
    stderr: bytes
    host_quiet: bool


@dataclass(frozen=True)
class TimedPair:
    baseline_cpu_ns: int
    candidate_cpu_ns: int
    before: Readiness
    after: Readiness
    order: PairOrder
    baseline_status: int
    candidate_status: int
    output_equal: bool
    state_equal: bool


@dataclass(frozen=True)
class BuildPair:
    baseline_cpu_ns: int
    candidate_cpu_ns: int
    before: Readiness
    after: Readiness
    order: PairOrder
    baseline_status: int
    candidate_status: int
    baseline_products_valid: bool
    candidate_products_valid: bool


@dataclass(frozen=True)
class BinarySize:
    name: str
    baseline_bytes: int
    candidate_bytes: int
    baseline: BuildContext
    candidate: BuildContext


@dataclass(frozen=True)
class AppletSourceSize:
    command: str
    bytes: int


@dataclass(frozen=True)
class MemorySample:
    command: str
    selector: str
    kind: MeasurementKind
    baseline_peak_bytes: int
    candidate_peak_bytes: int
    baseline: BuildContext
    candidate: BuildContext


@dataclass(frozen=True)
class CommandTiming:
    command: str
    selector: str
    kind: MeasurementKind
    metric: ResourceMetric
    baseline: BuildContext
    candidate: BuildContext
    samples: tuple[TimedPair, ...]


@dataclass(frozen=True)
class BuildCost:
    baseline: BuildContext
    candidate: BuildContext
    samples: tuple[BuildPair, ...]


@dataclass(frozen=True)
class MeasuredTiming:
    commands: tuple[CommandTiming, ...]
    startup: tuple[CommandTiming, ...]
    build_cost: BuildCost


@dataclass(frozen=True)
class DeferredTiming:
    reason: str


@dataclass(frozen=True)
class ResourceReport:
    source: SourceIdentity
    binary_sizes: tuple[BinarySize, ...]
    applet_sources: tuple[AppletSourceSize, ...]
    memory: tuple[MemorySample, ...]
    timing: MeasuredTiming | DeferredTiming


@dataclass(frozen=True)
class GateResult:
    name: str
    source: SourceIdentity
    status: int
    executed: int
    passed: int
    failed: int
    skipped: tuple[str, ...]
    accounted_skips: tuple[str, ...]
    selectors: tuple[str, ...]
    private_selectors: tuple[str, ...]


@dataclass(frozen=True)
class CloseReport:
    source: SourceIdentity
    gates: tuple[GateResult, ...]
    resources: ResourceReport
    remaining_owned_processes: int
    lost_work: bool


REQUIRED_CLOSE_GATES = frozenset({
    "inventory",
    "harness",
    "baseline",
    "parity",
    "positive-boundaries",
    "focused-regressions",
    "full-native",
    "host-boundaries",
    "repository-contract",
    "docs",
    "resources",
    "adversarial-review",
    "cleanup",
})

_PRODUCT_NAMES = ("xsh", "xsht", "xsh-test-helper")
_SAMPLE_COUNT = 5
_MIB = 1024 * 1024


def _require_source_identity(source: SourceIdentity) -> None:
    if not source.commit or not source.tree or not source.clean:
        raise ValueError("acceptance requires a clean source identity")


def _current_source(runtime: AcceptanceRuntime) -> SourceIdentity:
    identity = runtime.source_identity()
    _require_source_identity(identity)
    return identity


def _validate_contexts(
    kind: MeasurementKind,
    baseline: BuildContext,
    candidate: BuildContext,
    source: SourceIdentity,
    package_commit: str,
    product_source: str,
) -> None:
    for context in (baseline, candidate):
        if not all((context.source, context.profile, context.target, context.image,
                    context.allocator, context.allocation)):
            raise ValueError("resource comparison has incomplete build context")
    if candidate.source != source.key:
        raise ValueError("candidate resource sample names stale source")
    if baseline.target != candidate.target or baseline.image != candidate.image:
        raise ValueError("resource comparison uses different targets or images")
    if baseline.allocation != candidate.allocation:
        raise ValueError("resource comparison uses different allocations")
    if baseline.source == candidate.source:
        raise ValueError("resource comparison does not identify a distinct baseline")
    if kind is MeasurementKind.EXISTING_BASELINE and baseline.source != product_source:
        raise ValueError("existing-command comparison does not use the protected product source")
    if kind is MeasurementKind.EXISTING_BASELINE:
        if baseline.profile != candidate.profile:
            raise ValueError("existing-command comparison uses different profiles")
        if baseline.allocator != candidate.allocator:
            raise ValueError("existing-command comparison uses different allocators")
    if kind is MeasurementKind.NEW_REFERENCE and baseline.source != package_commit:
        raise ValueError("new-command comparison does not use the qualified util-linux source")
    if kind is MeasurementKind.NEW_REFERENCE:
        if baseline.profile != REFERENCE_PROFILE:
            raise ValueError("new-command comparison does not use the qualified util-linux profile")
        if baseline.allocator != REFERENCE_ALLOCATOR:
            raise ValueError("new-command comparison does not identify the util-linux allocator")


def _context_core(context: BuildContext) -> tuple[str, str, str, str, str, str]:
    return (
        context.source,
        context.profile,
        context.target,
        context.image,
        context.allocator,
        context.allocation,
    )


def _require_stable_contexts(
    pairs: list[tuple[MeasurementKind, BuildContext, BuildContext]],
) -> None:
    grouped: dict[MeasurementKind, tuple[tuple[str, ...], tuple[str, ...]]] = {}
    for kind, baseline, candidate in pairs:
        current = (_context_core(baseline), _context_core(candidate))
        if kind in grouped and grouped[kind] != current:
            raise ValueError(f"{kind.value} resource rows use inconsistent source or build contexts")
        grouped[kind] = current


def _require_readiness(readiness: Readiness) -> None:
    if readiness.status != 0 or readiness.stdout or readiness.stderr or not readiness.host_quiet:
        raise ValueError("timing sample overlapped host load or failed readiness")


def _validate_timed_pairs(samples: tuple[TimedPair, ...]) -> None:
    if len(samples) != _SAMPLE_COUNT:
        raise ValueError(f"timing gate requires exactly {_SAMPLE_COUNT} interleaved pairs")
    for sample in samples:
        if min(sample.baseline_cpu_ns, sample.candidate_cpu_ns) <= 0:
            raise ValueError("timing sample has no positive CPU-time measurement")
        if sample.baseline_status != 0 or sample.candidate_status != 0:
            raise ValueError("timing sample did not complete both commands")
        if not sample.output_equal or not sample.state_equal:
            raise ValueError("timed reference and candidate results or fixture state differ")
        _require_readiness(sample.before)
        _require_readiness(sample.after)
    expected_orders = (
        PairOrder.BASELINE_FIRST,
        PairOrder.CANDIDATE_FIRST,
        PairOrder.BASELINE_FIRST,
        PairOrder.CANDIDATE_FIRST,
        PairOrder.BASELINE_FIRST,
    )
    if tuple(sample.order for sample in samples) != expected_orders:
        raise ValueError("timing pairs are not interleaved baseline/candidate samples")


def _timing_summary(samples: tuple[TimedPair, ...]) -> tuple[int, int, int, int, int]:
    baseline = sorted(sample.baseline_cpu_ns for sample in samples)
    candidate = sorted(sample.candidate_cpu_ns for sample in samples)
    deltas = [sample.candidate_cpu_ns - sample.baseline_cpu_ns for sample in samples]
    spread = max(deltas) - min(deltas)
    baseline_median = baseline[len(baseline) // 2]
    candidate_median = candidate[len(candidate) // 2]
    return baseline[0], candidate[0], baseline_median, candidate_median, spread


def _require_cpu_threshold(samples: tuple[TimedPair, ...], *, startup: bool = False) -> None:
    _, _, baseline_median, candidate_median, spread = _timing_summary(samples)
    allowance = max(baseline_median // 10, spread) + (5_000_000 if startup else 0)
    if candidate_median - baseline_median > allowance:
        suffix = " including the startup allowance" if startup else ""
        raise ValueError(f"candidate CPU time exceeds the 10%-and-spread limit{suffix}")


def _require_peak_rss_threshold(row: MemorySample, comparison: str) -> None:
    allowance = max(row.baseline_peak_bytes // 10, 8 * _MIB)
    if row.candidate_peak_bytes - row.baseline_peak_bytes > allowance:
        raise ValueError(f"candidate peak RSS exceeds the {comparison} limit: {row.command}")


def _require_build_threshold(samples: tuple[BuildPair, ...]) -> None:
    if len(samples) != _SAMPLE_COUNT:
        raise ValueError(f"build-cost gate requires exactly {_SAMPLE_COUNT} interleaved pairs")
    for sample in samples:
        if min(sample.baseline_cpu_ns, sample.candidate_cpu_ns) <= 0:
            raise ValueError("build sample has no positive CPU-time measurement")
        if sample.baseline_status != 0 or sample.candidate_status != 0:
            raise ValueError("baseline or candidate build failed")
        if not sample.baseline_products_valid or not sample.candidate_products_valid:
            raise ValueError("build sample produced an unqualified product")
        _require_readiness(sample.before)
        _require_readiness(sample.after)
    expected_orders = (
        PairOrder.BASELINE_FIRST,
        PairOrder.CANDIDATE_FIRST,
        PairOrder.BASELINE_FIRST,
        PairOrder.CANDIDATE_FIRST,
        PairOrder.BASELINE_FIRST,
    )
    if tuple(sample.order for sample in samples) != expected_orders:
        raise ValueError("build pairs are not interleaved baseline/candidate samples")
    paired = tuple(TimedPair(
        sample.baseline_cpu_ns,
        sample.candidate_cpu_ns,
        sample.before,
        sample.after,
        sample.order,
        sample.baseline_status,
        sample.candidate_status,
        True,
        True,
    ) for sample in samples)
    _require_cpu_threshold(paired)


def _validate_resource_report(
    report: ResourceReport,
    runtime: AcceptanceRuntime,
    source: SourceIdentity,
) -> None:
    if report.source != source:
        raise ValueError("resource report is bound to a stale source")
    if tuple(sorted(row.name for row in report.binary_sizes)) != tuple(sorted(_PRODUCT_NAMES)):
        raise ValueError("resource report must compare all three product binaries exactly once")
    if len({row.name for row in report.binary_sizes}) != len(report.binary_sizes):
        raise ValueError("resource report repeats a product binary")
    context_pairs: list[tuple[MeasurementKind, BuildContext, BuildContext]] = []
    for row in report.binary_sizes:
        if min(row.baseline_bytes, row.candidate_bytes) <= 0:
            raise ValueError(f"missing binary-size sample: {row.name}")
        _validate_contexts(MeasurementKind.EXISTING_BASELINE, row.baseline, row.candidate,
                           source, runtime.PACKAGE_COMMIT, runtime.PRODUCT_SOURCE)
        context_pairs.append((MeasurementKind.EXISTING_BASELINE, row.baseline, row.candidate))
        if row.candidate_bytes * 100 > row.baseline_bytes * 101:
            raise ValueError(f"product binary grew by more than 1%: {row.name}")
    source_sizes = {row.command: row.bytes for row in report.applet_sources}
    if set(source_sizes) != set(runtime.COHORT) or len(source_sizes) != len(report.applet_sources):
        raise ValueError("resource report must include each cohort applet source exactly once")
    if any(size <= 0 for size in source_sizes.values()):
        raise ValueError("resource report has a missing applet source-size sample")
    expected_memory = {
        command: POSITIVE_BOUNDARIES[command]
        for command in runtime.COHORT
    }
    memory_keys = [(row.command, row.selector) for row in report.memory]
    positive_memory = {
        (row.command, row.selector)
        for row in report.memory
        if row.kind is MeasurementKind.NEW_REFERENCE
    }
    if positive_memory != set(expected_memory.items()):
        raise ValueError("resource report must sample each positive boundary fixture exactly once")
    if len(set(memory_keys)) != len(memory_keys):
        raise ValueError("resource report repeats a peak RSS fixture")
    existing_memory: dict[tuple[str, str], MemorySample] = {}
    for row in report.memory:
        if min(row.baseline_peak_bytes, row.candidate_peak_bytes) <= 0:
            raise ValueError(f"missing peak RSS sample: {row.command}")
        if not row.selector:
            raise ValueError(f"peak RSS sample has no native selector: {row.command}")
        _validate_contexts(row.kind, row.baseline, row.candidate, source,
                           runtime.PACKAGE_COMMIT, runtime.PRODUCT_SOURCE)
        context_pairs.append((row.kind, row.baseline, row.candidate))
        if row.kind is MeasurementKind.NEW_REFERENCE:
            if expected_memory.get(row.command) != row.selector:
                raise ValueError(f"new-command RSS sample does not use its positive fixture: {row.command}")
            _require_peak_rss_threshold(row, "new-command")
            continue
        if row.command in runtime.COHORT:
            raise ValueError(f"new cohort command has an existing-command RSS baseline: {row.command}")
        existing_memory[(row.command, row.selector)] = row
        if row.kind is MeasurementKind.EXISTING_BASELINE:
            _require_peak_rss_threshold(row, "existing-command")
    if not existing_memory:
        raise ValueError("resource report omits existing-command peak RSS measurements")
    expected_regressions = _expected_regression_selectors(runtime, source)
    if tuple(sorted(row.selector for row in existing_memory.values())) != expected_regressions:
        raise ValueError("existing-command RSS does not cover the frozen regression selectors")

    _require_stable_contexts(context_pairs)
    if isinstance(report.timing, DeferredTiming):
        if report.timing.reason != "host busy":
            raise ValueError("timing can be deferred only when the host remains busy")
        return
    if not isinstance(report.timing, MeasuredTiming):
        raise ValueError("resource report has an unknown timing result")

    cohort_rows: dict[str, CommandTiming] = {}
    existing_rows: dict[tuple[str, str], CommandTiming] = {}
    for row in report.timing.commands:
        if row.metric is not ResourceMetric.COMMAND:
            raise ValueError("command timing row has the wrong metric")
        _validate_contexts(row.kind, row.baseline, row.candidate, source,
                           runtime.PACKAGE_COMMIT, runtime.PRODUCT_SOURCE)
        context_pairs.append((row.kind, row.baseline, row.candidate))
        _validate_timed_pairs(row.samples)
        if row.kind is MeasurementKind.NEW_REFERENCE:
            if row.command in cohort_rows or expected_memory.get(row.command) != row.selector:
                raise ValueError("new-command timing does not use its positive boundary fixture")
            cohort_rows[row.command] = row
        else:
            key = (row.command, row.selector)
            if key in existing_rows:
                raise ValueError("resource report repeats an existing regression target")
            existing_rows[key] = row
            _require_cpu_threshold(row.samples)
    if set(cohort_rows) != set(runtime.COHORT):
        raise ValueError("resource report must time each new command on its positive fixture")
    if not existing_rows:
        raise ValueError("resource report omits existing-command regression measurements")
    if tuple(sorted(row.selector for row in existing_rows.values())) != expected_regressions:
        raise ValueError("existing-command CPU does not cover the frozen regression selectors")
    if set(existing_memory) != set(existing_rows):
        raise ValueError("existing-command RSS and CPU samples use different regression fixtures")

    startup_rows: dict[tuple[str, str], CommandTiming] = {}
    for row in report.timing.startup:
        if row.kind is not MeasurementKind.EXISTING_BASELINE or row.metric is not ResourceMetric.STARTUP:
            raise ValueError("startup timing must target an existing command")
        key = (row.command, row.selector)
        if key in startup_rows:
            raise ValueError("resource report repeats a startup measurement")
        _validate_contexts(row.kind, row.baseline, row.candidate, source,
                           runtime.PACKAGE_COMMIT, runtime.PRODUCT_SOURCE)
        context_pairs.append((row.kind, row.baseline, row.candidate))
        _validate_timed_pairs(row.samples)
        startup_rows[key] = row
        _require_cpu_threshold(row.samples, startup=True)
    _validate_startup_targets(set(startup_rows), expected_regressions)

    build = report.timing.build_cost
    _validate_contexts(MeasurementKind.EXISTING_BASELINE, build.baseline, build.candidate,
                       source, runtime.PACKAGE_COMMIT, runtime.PRODUCT_SOURCE)
    if not build.baseline.cache_state or build.baseline.cache_state != build.candidate.cache_state:
        raise ValueError("build-cost samples use different cache states")
    if build.baseline.compile_jobs <= 0 or build.baseline.compile_jobs != build.candidate.compile_jobs:
        raise ValueError("build-cost samples use different compile concurrency")
    _require_build_threshold(build.samples)
    _require_stable_contexts(context_pairs)
    _require_stable_contexts([(MeasurementKind.EXISTING_BASELINE, build.baseline, build.candidate)])


def _validate_startup_targets(
    actual: set[tuple[str, str]],
    regression_selectors: tuple[str, ...],
) -> None:
    if actual != set(STARTUP_TARGETS):
        raise ValueError("startup measurements must cover the fixed command targets exactly")
    if {selector for _, selector in STARTUP_TARGETS} != set(regression_selectors):
        raise ValueError("startup command mapping differs from the actual regression declarations")


def resources(runtime: AcceptanceRuntime) -> None:
    before = _current_source(runtime)
    report = runtime.measure_resources()
    after = _current_source(runtime)
    if after != before:
        raise ValueError("source changed during resource acceptance")
    _validate_resource_report(report, runtime, before)
    for row in report.binary_sizes:
        print(f"binary {row.name}: baseline={row.baseline_bytes} candidate={row.candidate_bytes} bytes")
    for row in report.applet_sources:
        print(f"source {row.command}: {row.bytes} bytes")
    for row in report.memory:
        overhead = row.candidate_peak_bytes - row.baseline_peak_bytes
        print(
            f"RSS {row.kind.value} {row.command} {row.selector}: "
            f"baseline={row.baseline_peak_bytes} candidate={row.candidate_peak_bytes} "
            f"overhead={overhead} bytes"
        )
    if isinstance(report.timing, DeferredTiming):
        print(f"perf gate deferred: {report.timing.reason}")
        raise ValueError(f"perf gate deferred: {report.timing.reason}")
    for row in (*report.timing.commands, *report.timing.startup):
        baseline_min, candidate_min, baseline_median, candidate_median, spread = _timing_summary(row.samples)
        comparison = (
            f"reference_median_ns={baseline_median} xsh_median_ns={candidate_median} "
            f"overhead_ns={candidate_median - baseline_median}"
            if row.kind is MeasurementKind.NEW_REFERENCE
            else f"baseline_median_ns={baseline_median} candidate_median_ns={candidate_median}"
        )
        print(
            f"CPU {row.kind.value} {row.metric.value} {row.command} {row.selector}: "
            f"min={baseline_min}/{candidate_min}ns {comparison} "
            f"paired-spread={spread}ns"
        )
    baseline_min, candidate_min, baseline_median, candidate_median, spread = _timing_summary(tuple(
        TimedPair(
            sample.baseline_cpu_ns,
            sample.candidate_cpu_ns,
            sample.before,
            sample.after,
            sample.order,
            sample.baseline_status,
            sample.candidate_status,
            True,
            True,
        ) for sample in report.timing.build_cost.samples
    ))
    print(
        f"CPU build: min={baseline_min}/{candidate_min}ns "
        f"median={baseline_median}/{candidate_median}ns paired-spread={spread}ns"
    )


def _all_parity_selectors(runtime: AcceptanceRuntime) -> tuple[str, ...]:
    return tuple(sorted(
        _case_selector(command, name)
        for command in runtime.COHORT
        for name in _case_names(runtime.ROOT, command)
    ))


def _validate_gate_result(gate: GateResult, source: SourceIdentity) -> None:
    if gate.source != source:
        raise ValueError(f"close gate used stale source: {gate.name}")
    if gate.status != 0 or gate.executed <= 0 or gate.failed != 0:
        raise ValueError(f"close gate did not pass: {gate.name}")
    if gate.passed + gate.failed + len(gate.skipped) != gate.executed:
        raise ValueError(f"close gate does not account for every selected behavior: {gate.name}")
    if len(set(gate.skipped)) != len(gate.skipped):
        raise ValueError(f"close gate repeats a skipped behavior: {gate.name}")
    if set(gate.skipped) != set(gate.accounted_skips):
        raise ValueError(f"close gate has unaccounted skips: {gate.name}")
    if gate.name in {"parity", "positive-boundaries", "focused-regressions"}:
        if len(set(gate.selectors)) != len(gate.selectors) or gate.executed != len(gate.selectors):
            raise ValueError(f"close gate does not account for each selector once: {gate.name}")
        if gate.name in {"parity", "positive-boundaries"} and set(gate.private_selectors) != set(gate.selectors):
            raise ValueError(f"close gate did not use private contexts for every selector: {gate.name}")
    if gate.name in {"parity", "positive-boundaries", "focused-regressions"} and gate.skipped:
        raise ValueError(f"mandatory close gate skipped behavior: {gate.name}")


def close(runtime: AcceptanceRuntime) -> CloseReport:
    before = _current_source(runtime)
    report = runtime.close_gates()
    after = _current_source(runtime)
    if after != before or report.source != before:
        raise ValueError("close results are not bound to the current source")
    validate_close_report(runtime, report)
    all_selectors = _all_parity_selectors(runtime)
    print(f"util-linux close: {len(all_selectors)} parity selectors, {len(runtime.COHORT)} positive boundaries, {len(report.gates)} required gates passed")
    return report


def validate_close_report(runtime: AcceptanceRuntime, report: CloseReport) -> None:
    before = _current_source(runtime)
    if not isinstance(report, CloseReport) or report.source != before:
        raise ValueError("close results are not bound to the current source")
    if report.remaining_owned_processes != 0:
        raise ValueError("owned util-linux processes remain after close gates")
    if report.lost_work:
        raise ValueError("close gates report lost uncommitted work")
    gates = {gate.name: gate for gate in report.gates}
    if len(gates) != len(report.gates) or set(gates) != REQUIRED_CLOSE_GATES:
        missing = sorted(REQUIRED_CLOSE_GATES - set(gates))
        extra = sorted(set(gates) - REQUIRED_CLOSE_GATES)
        raise ValueError(f"close gate set is incomplete: missing={missing}, extra={extra}")
    for gate in report.gates:
        _validate_gate_result(gate, before)
    all_selectors = _all_parity_selectors(runtime)
    regression_selectors = _expected_regression_selectors(runtime, before)
    if tuple(sorted(gates["parity"].selectors)) != all_selectors:
        raise ValueError("close parity did not execute every declared cohort behavior")
    if tuple(sorted(gates["focused-regressions"].selectors)) != regression_selectors:
        raise ValueError("close focused regressions differ from the frozen actual selectors")
    positive_selectors = tuple(sorted(
        _case_selector(command, POSITIVE_BOUNDARIES[command])
        for command in runtime.COHORT
    ))
    if tuple(sorted(gates["positive-boundaries"].selectors)) != positive_selectors:
        raise ValueError("close did not execute each positive boundary in its required context")
    _validate_resource_report(report.resources, runtime, before)
    if isinstance(report.resources.timing, DeferredTiming):
        raise ValueError(f"perf gate deferred: {report.resources.timing.reason}")
    if _current_source(runtime) != before:
        raise ValueError("close source changed during report validation")
