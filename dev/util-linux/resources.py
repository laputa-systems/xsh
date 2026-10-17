from __future__ import annotations

from collections.abc import Callable, Sequence
from pathlib import Path
import hashlib
import os
import shlex
import shutil
import stat
import subprocess
from typing import Protocol

import acceptance
from acceptance import (
    AppletSourceSize,
    BinarySize,
    BuildContext,
    DeferredTiming,
    MemorySample,
    MeasurementKind,
    CloseReport,
    GateResult,
    MeasuredTiming,
    POSITIVE_BOUNDARIES,
    ResourceReport,
    Readiness,
    SourceIdentity,
)


GitRunner = Callable[..., subprocess.CompletedProcess[str]]
PRODUCTS = ("xsh", "xsht", "xsh-test-helper")
POSITIVE_SELECTORS = {
    "lslocks": "test_lslocks_blocker_flock",
    "lsipc": "test_lsipc_visibility_matches_for_owner_and_unprivileged_observer",
    "lsmem": "test_lsmem_rooted_sparse_memory_attributes_and_output_modes",
    "mountpoint": "test_mountpoint_private_same_device_bind_mount",
    "namei": "test_namei_private_same_device_bind_mount_matches_reference",
}


class ResourceRuntime(Protocol):
    ROOT: Path
    COHORT: tuple[str, ...]
    PRIVATE_NAMESPACE: str
    IMAGE: str
    PACKAGE_COMMIT: str
    PRODUCT_SOURCE: str

    def container(
        self,
        script: str,
        *,
        mounts: list[str],
        capabilities: list[str],
        timeout: int,
    ) -> bytes: ...

    def source_identity(self) -> SourceIdentity: ...

    def regression_selectors(self, source: SourceIdentity) -> tuple[str, ...]: ...

    def candidate_products(self) -> tuple[Path, BuildContext]: ...

    def baseline_products(self) -> tuple[Path, BuildContext]: ...

    def inventory(self) -> None: ...

    def harness(self) -> None: ...

    def baseline(self) -> None: ...

    def native_case(self, selector: str, *, private: bool = False) -> bytes: ...

    def run_full_native(self, source: SourceIdentity) -> GateResult: ...

    def run_host_boundaries(self, source: SourceIdentity) -> GateResult: ...

    def run_repository_contract(self, source: SourceIdentity) -> GateResult: ...

    def run_docs(self, source: SourceIdentity) -> GateResult: ...

    def adversarial_review(self, source: SourceIdentity) -> GateResult: ...

    def owned_cleanup(self, source: SourceIdentity) -> GateResult: ...

    def measure_timing(self, source: SourceIdentity) -> MeasuredTiming: ...


def _git_text(root: Path, args: Sequence[str], run: GitRunner) -> str:
    result = run(
        ["git", *args],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.rstrip("\n")


def source_identity(
    root: Path,
    *,
    run: GitRunner = subprocess.run,
) -> SourceIdentity:
    commit = _git_text(root, ("rev-parse", "--verify", "HEAD^{commit}"), run)
    tree = _git_text(root, ("rev-parse", "--verify", "HEAD^{tree}"), run)
    status = _git_text(root, ("status", "--porcelain=v1", "--untracked-files=all"), run)
    if not commit or not tree:
        raise ValueError("git returned an incomplete source identity")
    return SourceIdentity(commit=commit, tree=tree, clean=not status)


def regression_selectors(
    root: Path,
    source: SourceIdentity,
    *,
    identify: Callable[[Path], SourceIdentity] | None = None,
) -> tuple[str, ...]:
    identify_source = identify or source_identity
    before = identify_source(root)
    if before != source or not before.clean:
        raise ValueError("regression selectors require the current clean source")
    selectors = acceptance.declared_regression_selectors(root)
    after = identify_source(root)
    if after != before:
        raise ValueError("source changed while reading regression selectors")
    if not selectors:
        raise ValueError("regression sources declare no behaviors")
    return selectors


ProductSignature = tuple[tuple[str, int, int, int, int, int, int, bytes], ...]


def _product_signature(directory: Path) -> ProductSignature:
    directory_fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        opened_directory = os.fstat(directory_fd)
        named_directory = directory.lstat()
        if (
            not stat.S_ISDIR(opened_directory.st_mode)
            or (opened_directory.st_dev, opened_directory.st_ino)
            != (named_directory.st_dev, named_directory.st_ino)
        ):
            raise ValueError(f"product directory is not a real directory: {directory}")
        result = []
        for name in PRODUCTS:
            info = os.stat(name, dir_fd=directory_fd, follow_symlinks=False)
            if not stat.S_ISREG(info.st_mode) or not info.st_mode & 0o111:
                raise ValueError(f"product is not a regular executable: {directory / name}")
            descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=directory_fd)
            try:
                opened = os.fstat(descriptor)
                if (opened.st_dev, opened.st_ino) != (info.st_dev, info.st_ino):
                    raise ValueError(f"product changed while opening: {directory / name}")
                if not stat.S_ISREG(opened.st_mode) or not opened.st_mode & 0o111:
                    raise ValueError(f"product stopped being an executable: {directory / name}")
                with os.fdopen(descriptor, "rb", closefd=False) as stream:
                    digest = hashlib.file_digest(stream, "sha256").digest()
                finished = os.fstat(descriptor)
                if (
                    (finished.st_dev, finished.st_ino, finished.st_size, finished.st_mtime_ns, finished.st_ctime_ns)
                    != (opened.st_dev, opened.st_ino, opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns)
                ):
                    raise ValueError(f"product changed while hashing: {directory / name}")
            finally:
                os.close(descriptor)
            result.append((
                name,
                opened.st_dev,
                opened.st_ino,
                opened.st_size,
                opened.st_mtime_ns,
                opened.st_ctime_ns,
                opened.st_mode,
                digest,
            ))
        current_directory = directory.lstat()
        if (current_directory.st_dev, current_directory.st_ino) != (opened_directory.st_dev, opened_directory.st_ino):
            raise ValueError(f"product directory changed while hashing: {directory}")
        return tuple(result)
    finally:
        os.close(directory_fd)


def _product_pair(runtime: ResourceRuntime, source: SourceIdentity):
    baseline_path, baseline_context = runtime.baseline_products()
    candidate_path, candidate_context = runtime.candidate_products()
    if baseline_context.source != runtime.PRODUCT_SOURCE:
        raise ValueError("protected products do not identify the frozen baseline source")
    if candidate_context.source != source.key:
        raise ValueError("candidate products do not identify the current clean source")
    if baseline_context.image != runtime.IMAGE or candidate_context.image != runtime.IMAGE:
        raise ValueError("products do not identify the pinned acceptance image")
    if baseline_context.target != candidate_context.target:
        raise ValueError("baseline and candidate products use different targets")
    if baseline_context.allocator != candidate_context.allocator:
        raise ValueError("baseline and candidate products use different allocators")
    if baseline_context.allocation != candidate_context.allocation:
        raise ValueError("baseline and candidate products use different allocations")
    baseline_signature = _product_signature(baseline_path)
    candidate_signature = _product_signature(candidate_path)
    return (
        baseline_path,
        baseline_context,
        baseline_signature,
        candidate_path,
        candidate_context,
        candidate_signature,
    )


def _source_sizes(root: Path, cohort: tuple[str, ...]) -> tuple[AppletSourceSize, ...]:
    rows = []
    for command in cohort:
        source = root / "core" / f"{command}.xsh"
        info = source.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_size <= 0:
            raise ValueError(f"missing or invalid applet source: {source}")
        rows.append(AppletSourceSize(command, info.st_size))
    return tuple(rows)


def _selectors(runtime: ResourceRuntime, source: SourceIdentity) -> tuple[str, ...]:
    selectors = regression_selectors(runtime.ROOT, source, identify=lambda _root: runtime.source_identity())
    if runtime.regression_selectors(source) != selectors:
        raise ValueError("regression selector producer differs from native declarations")
    return selectors


_MEASUREMENT_DRIVER = r'''import contextlib
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

REGRESSION_SELECTORS = @REGRESSION_SELECTORS@
POSITIVE = @POSITIVE@
ROOT = Path("/work")
BASELINE = Path("/baseline")
CANDIDATE = Path("/candidate")
ENV = {"LC_ALL": "C", "TZ": "UTC", "XSH_MODULE_PATH": "/work/core", "PYTHONDONTWRITEBYTECODE": "1", "TMPDIR": "/t", "PATH": os.environ["PATH"]}


def measured(argv, cwd, *, preexec_fn=None, timeout=30, env=None):
    with tempfile.TemporaryFile(dir="/t") as stdout, tempfile.TemporaryFile(dir="/t") as stderr:
        child = subprocess.Popen(argv, cwd=cwd, env={**ENV, **(env or {})}, stdin=subprocess.DEVNULL,
                                 stdout=stdout, stderr=stderr, start_new_session=True,
                                 preexec_fn=preexec_fn)
        deadline = time.monotonic() + timeout
        try:
            while True:
                try:
                    pid, status, usage = os.wait4(child.pid, os.WNOHANG)
                except InterruptedError:
                    continue
                if pid:
                    child.returncode = os.waitstatus_to_exitcode(status)
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError("resource child exceeded its bounded runtime")
                time.sleep(0.005)
        except BaseException:
            try:
                os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            time.sleep(0.1)
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                os.wait4(child.pid, 0)
            except ChildProcessError:
                pass
            child.returncode = 128 + signal.SIGKILL
            raise
        stdout.seek(0)
        stderr.seek(0)
        return child.returncode, stdout.read(), stderr.read(), usage.ru_maxrss * 1024, int((usage.ru_utime + usage.ru_stime) * 1_000_000_000)


def applet_argv(products, command, args):
    if products == "reference":
        executable = "/bin/mountpoint" if command == "mountpoint" else f"/usr/bin/{command}"
        return [executable, *args]
    return [str(products / "xsh"), f"/work/core/{command}.xsh", "--", *args]


def fixture_tree(root):
    rows = []
    for directory, names, files in os.walk(root, followlinks=False):
        names.sort()
        files.sort()
        base = Path(directory)
        for name in names + files:
            path = base / name
            info = path.lstat()
            relative = path.relative_to(root).as_posix()
            if path.is_symlink():
                rows.append((relative, "link", os.readlink(path)))
            elif path.is_file():
                rows.append((relative, "file", hashlib.sha256(path.read_bytes()).hexdigest()))
            else:
                rows.append((relative, "dir", info.st_mode & 0o7777))
    return tuple(rows)


def start_keeper(argv):
    child = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, text=True, bufsize=1,
                             env=ENV, start_new_session=True)
    line = child.stdout.readline()
    if not line:
        detail = child.stderr.read()
        raise RuntimeError(f"fixture keeper failed before readiness: {detail}")
    return child, json.loads(line)


def finish_keeper(child, request):
    if child.poll() is None:
        child.stdin.write(request + "\n")
        child.stdin.flush()
    try:
        stdout, stderr = child.communicate(timeout=8)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            stdout, stderr = child.communicate(timeout=1)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = child.communicate(timeout=1)
        raise RuntimeError(f"fixture keeper exceeded its cleanup bound: stdout={stdout!r} stderr={stderr!r}")
    if child.returncode != 0:
        raise RuntimeError(f"fixture keeper failed to clean up: stdout={stdout!r} stderr={stderr!r}")


@contextlib.contextmanager
def lock_fixture(root):
    root.mkdir(parents=True)
    path = root / "blocked-flock"
    keeper = "/work/core/tests/fixtures/util-linux/locks/keeper.py"
    owner, owner_ready = start_keeper([sys.executable, keeper, "hold", "flock-ex", str(path), "none"])
    waiter = None
    try:
        waiter, waiter_ready = start_keeper([sys.executable, keeper, "wait", "flock-ex", str(path), str(owner_ready["pid"])])
        args = ["--json", "--pid", str(waiter_ready["pid"]), "--output", "PID,TYPE,MODE,BLOCKER,PATH,INODE,MAJ:MIN"]

        def snapshot():
            with open("/proc/locks", encoding="ascii") as source:
                locks = tuple(line.strip() for line in source if f"{owner_ready['lock_id']}:" in line)
            info = path.stat()
            return locks, info.st_dev, info.st_ino, info.st_size

        yield args, snapshot
    finally:
        try:
            finish_keeper(owner, "DONE")
        finally:
            if waiter is not None:
                try:
                    stdout, stderr = waiter.communicate(timeout=8)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(waiter.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    stdout, stderr = waiter.communicate(timeout=1)
                    raise RuntimeError(f"lock waiter exceeded its cleanup bound: stdout={stdout!r} stderr={stderr!r}")
                if waiter.returncode != 0:
                    raise RuntimeError(f"lock waiter failed to exit: stdout={stdout!r} stderr={stderr!r}")


@contextlib.contextmanager
def ipc_fixture(root):
    root.mkdir(parents=True)
    keeper = "/work/core/tests/fixtures/util-linux/ipc/keeper.py"
    args = [sys.executable, keeper, "--owner-uid", "10001", "--owner-gid", "10001",
            "--observer-uid", "10002", "--observer-gid", "10002"]
    for kind in ("sysv_queue", "sysv_shmem", "sysv_semaphore", "posix_mqueue", "posix_shmem", "posix_semaphore"):
        args.extend(["--resource", kind])
    child, ready = start_keeper(args)
    queue = next(row for row in ready["resources"] if row["kind"] == "sysv_queue")
    command_args = ["--queues", "--id", str(queue["id"]), "--output", "ID"]

    def snapshot():
        child.stdin.write("snapshot\n")
        child.stdin.flush()
        line = child.stdout.readline()
        if not line:
            raise RuntimeError("IPC keeper omitted its state snapshot")
        return line

    def owner():
        os.setgroups([])
        os.setgid(10001)
        os.setuid(10001)

    try:
        yield command_args, snapshot, owner
    finally:
        finish_keeper(child, "release")


@contextlib.contextmanager
def lsmem_fixture(root):
    shutil.copytree("/work/core/tests/fixtures/util-linux/memory/sparse", root)
    args = ["--sysroot", str(root)]
    yield args, lambda: fixture_tree(root), None


@contextlib.contextmanager
def bind_fixture(root, command):
    source = root / "source"
    target = root / "target"
    source.mkdir(parents=True)
    target.mkdir()
    fixture = Path("/work/core/tests/fixtures/util-linux/paths/bind")
    shutil.copyfile(fixture / "source/entry", source / "entry")
    shutil.copyfile(fixture / "target/sentinel", target / "sentinel")
    if source.stat().st_dev != target.stat().st_dev:
        raise RuntimeError("bind fixture paths do not share a device")
    status, stdout, stderr, _, _ = measured(["mount", "--bind", str(source), str(target)], root)
    if status or stdout or stderr:
        raise RuntimeError("could not establish the private bind-mount fixture")
    args = ["--show", "--", str(target)] if command == "mountpoint" else ["-x", "--", str(target)]

    def snapshot():
        with open("/proc/self/mountinfo", encoding="utf-8") as source_file:
            rows = tuple(line.strip() for line in source_file if str(target) in line)
        return rows, tuple(sorted(path.name for path in target.iterdir())), (target / "entry").exists(), (target / "sentinel").exists()

    try:
        yield args, snapshot, None
    finally:
        status, stdout, stderr, _, _ = measured(["umount", str(target)], root)
        if status or stdout or stderr:
            raise RuntimeError(f"could not remove private bind-mount fixture: {stderr!r}")


FIXTURES = {
    "lslocks": lock_fixture,
    "lsipc": ipc_fixture,
    "lsmem": lsmem_fixture,
    "mountpoint": lambda root: bind_fixture(root, "mountpoint"),
    "namei": lambda root: bind_fixture(root, "namei"),
}


def positive(command, selector):
    with tempfile.TemporaryDirectory(dir="/t", prefix=f"resource-{command}-") as temporary:
        root = Path(temporary) / command
        fixture = FIXTURES[command]
        with fixture(root) as prepared:
            args, snapshot = prepared[0], prepared[1]
            preexec = prepared[2] if len(prepared) > 2 else None
            before = snapshot()
            reference = measured(applet_argv("reference", command, args), root, preexec_fn=preexec)
            after_reference = snapshot()
            candidate = measured(applet_argv(CANDIDATE, command, args), root, preexec_fn=preexec)
            after_candidate = snapshot()
            if (
                reference[0] != 0
                or candidate[0] != 0
                or reference[1] != candidate[1]
                or reference[2]
                or candidate[2]
            ):
                raise RuntimeError(f"positive fixture failed output/status parity: {command} {selector}")
            if before != after_reference or before != after_candidate:
                raise RuntimeError(f"positive fixture state changed: {command} {selector}")
            print("\t".join(("positive", command, selector, str(reference[3]), str(candidate[3]),
                             str(reference[0]), str(candidate[0]), "1", "1")))


def exact_native(products, selector):
    with tempfile.TemporaryDirectory(dir="/t", prefix="native-regression-") as scratch:
        before = fixture_tree(Path(scratch))
        status, stdout, stderr, peak, cpu = measured(
            [str(products / "xsht"), "test", "--exact", selector],
            ROOT,
            timeout=90,
            env={"TMPDIR": scratch},
        )
        text = stdout.decode("utf-8")
        lines = text.splitlines()
        running = [line for line in lines if line.startswith("running ") and line.endswith(" tests")]
        summaries = [line for line in lines if line.startswith("test result: ")]
        results = [line for line in lines if " ... " in line]
        if status or stderr or running != ["running 1 tests"] or len(summaries) != 1:
            raise RuntimeError(f"regression selector did not complete exactly: {selector}: {text} {stderr!r}")
        if summaries[0] != "test result: ok. 1 passed; 0 failed; 0 skipped" or len(results) != 1:
            raise RuntimeError(f"regression selector failed, skipped, or overselected: {selector}: {text}")
        selected, outcome = results[0].split(" ... ", 1)
        if selected != selector or outcome.split()[0] != "ok":
            raise RuntimeError(f"regression runner executed a different behavior: {selector}: {results[0]}")
        if fixture_tree(Path(scratch)) != before:
            raise RuntimeError(f"native regression leaked temporary resources: {selector}")
        return peak, cpu, status, stdout


for command, selector in POSITIVE:
    positive(command, selector)

for selector in REGRESSION_SELECTORS:
    baseline = exact_native(BASELINE, selector)
    candidate = exact_native(CANDIDATE, selector)
    if baseline[2] or candidate[2] or baseline[3] != candidate[3]:
        raise RuntimeError(f"regression selector differs across protected and candidate products: {selector}")
    print("\t".join(("regression", selector, str(baseline[0]), str(candidate[0]),
                     str(baseline[2]), str(candidate[2]), "1", "1")))
'''


def _driver_source(selectors: tuple[str, ...], cohort: tuple[str, ...]) -> str:
    positives = tuple((command, POSITIVE_SELECTORS[command]) for command in cohort)
    return (
        _MEASUREMENT_DRIVER
        .replace("@REGRESSION_SELECTORS@", repr(selectors))
        .replace("@POSITIVE@", repr(positives))
    )


def _parse_measurements(
    output: bytes,
    cohort: tuple[str, ...],
    regression_selectors: tuple[str, ...],
) -> tuple[tuple[str, str, int, int], ...]:
    try:
        lines = output.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise ValueError("resource child output was not UTF-8") from error
    expected_positive = {
        (command, POSITIVE_SELECTORS[command])
        for command in cohort
    }
    positive_rows: dict[tuple[str, str], tuple[int, ...]] = {}
    regression_rows: dict[str, tuple[int, ...]] = {}
    for line in lines:
        fields = tuple(line.split("\t"))
        if fields[0] == "positive" and len(fields) == 9:
            key = (fields[1], fields[2])
            if key in positive_rows:
                raise ValueError(f"resource child repeated positive fixture: {key}")
            positive_rows[key] = _integers(fields[3:9], line)
            continue
        if fields[0] == "regression" and len(fields) == 8:
            selector = fields[1]
            if selector in regression_rows:
                raise ValueError(f"resource child repeated regression selector: {selector}")
            regression_rows[selector] = _integers(fields[2:8], line)
            continue
        raise ValueError(f"resource child emitted an unrecognized measurement: {line}")
    if set(positive_rows) != expected_positive:
        raise ValueError("resource child did not measure each positive boundary exactly once")
    if set(regression_rows) != set(regression_selectors):
        raise ValueError("resource child did not measure the frozen regression selectors exactly once")
    samples = []
    for (command, selector), values in sorted(positive_rows.items()):
        baseline_rss, candidate_rss, baseline_status, candidate_status, equal_output, equal_state = values
        if min(baseline_rss, candidate_rss) <= 0 or baseline_status != 0 or candidate_status != 0:
            raise ValueError(f"positive resource sample did not complete: {command}")
        if (equal_output, equal_state) != (1, 1):
            raise ValueError(f"positive resource sample changed output or fixture state: {command}")
        samples.append((command, selector, baseline_rss, candidate_rss))
    for selector, values in sorted(regression_rows.items()):
        baseline_rss, candidate_rss, baseline_status, candidate_status, equal_output, equal_state = values
        if min(baseline_rss, candidate_rss) <= 0 or baseline_status != 0 or candidate_status != 0:
            raise ValueError(f"regression resource sample did not complete: {selector}")
        if (equal_output, equal_state) != (1, 1):
            raise ValueError(f"regression resource sample changed behavior or state: {selector}")
        command = selector.partition("::")[0].rsplit("/", 1)[-1].removesuffix(".xsh")
        samples.append((command, selector, baseline_rss, candidate_rss))
    return tuple(samples)


def _integers(values: tuple[str, ...], line: str) -> tuple[int, ...]:
    if any(not value.isascii() or not value.isdecimal() for value in values):
        raise ValueError(f"resource child emitted a nonnumeric measurement: {line}")
    return tuple(int(value) for value in values)


def _readiness(program: Path, arguments: tuple[str, ...], run=subprocess.run) -> Readiness:
    result = run([str(program), *arguments], capture_output=True, check=False, timeout=30)
    host_quiet = result.returncode == 0 and not result.stdout and not result.stderr
    return Readiness(result.returncode, result.stdout, result.stderr, host_quiet)


def _resource_rows(
    runtime: ResourceRuntime,
    source: SourceIdentity,
) -> tuple[tuple[BinarySize, ...], tuple[AppletSourceSize, ...], tuple[MemorySample, ...]]:
    selectors = _selectors(runtime, source)
    products = _product_pair(runtime, source)
    baseline_path, baseline_context, baseline_signature, candidate_path, candidate_context, candidate_signature = products
    source_sizes = _source_sizes(runtime.ROOT, runtime.COHORT)
    baseline_sizes = {row[0]: row[3] for row in baseline_signature}
    candidate_sizes = {row[0]: row[3] for row in candidate_signature}
    binary_sizes = tuple(
        BinarySize(name, baseline_sizes[name], candidate_sizes[name], baseline_context, candidate_context)
        for name in PRODUCTS
    )
    driver = _driver_source(selectors, runtime.COHORT)
    script = "cd /work; " + runtime.PRIVATE_NAMESPACE.rstrip() + " python3 -c " + shlex.quote(driver)
    output = runtime.container(
        script,
        mounts=[
            f"type=bind,source={runtime.ROOT},target=/work,readonly",
            f"type=bind,source={baseline_path},target=/baseline,readonly",
            f"type=bind,source={candidate_path},target=/candidate,readonly",
        ],
        capabilities=["SYS_ADMIN"],
        timeout=600,
    )
    measured = _parse_measurements(output, runtime.COHORT, selectors)
    before = runtime.source_identity()
    current_products = _product_pair(runtime, source)
    after = runtime.source_identity()
    if before != source or after != source:
        raise ValueError("source changed during resource measurement")
    if current_products[2] != baseline_signature or current_products[5] != candidate_signature:
        raise ValueError("products changed during resource measurement")
    positive_keys = {
        (command, POSITIVE_SELECTORS[command])
        for command in runtime.COHORT
    }
    memory = tuple(
        MemorySample(
            command=command,
            selector=selector,
            kind=MeasurementKind.NEW_REFERENCE if (command, selector) in positive_keys else MeasurementKind.EXISTING_BASELINE,
            baseline_peak_bytes=baseline_bytes,
            candidate_peak_bytes=candidate_bytes,
            baseline=(
                BuildContext(
                    source=runtime.PACKAGE_COMMIT,
                    profile="apk-package-2.42.4-r3",
                    target=candidate_context.target,
                    image=runtime.IMAGE,
                    allocator=candidate_context.allocator,
                    allocation=candidate_context.allocation,
                )
                if (command, selector) in positive_keys
                else baseline_context
            ),
            candidate=candidate_context,
        )
        for command, selector, baseline_bytes, candidate_bytes in measured
    )
    report_source = runtime.source_identity()
    if report_source != source:
        raise ValueError("resource source changed before report construction")
    return binary_sizes, source_sizes, memory


def measure(runtime: ResourceRuntime) -> ResourceReport:
    if tuple(runtime.COHORT) != tuple(POSITIVE_SELECTORS):
        raise ValueError("resource cohort does not match the five qualified positive fixtures")
    source = runtime.source_identity()
    if not source.clean:
        raise ValueError("resource acceptance requires a clean final source")
    product_pair = _product_pair(runtime, source)
    binary_sizes, source_sizes, memory = _resource_rows(runtime, source)
    program = shutil.which("perf-gate-ready")
    if program is None:
        raise RuntimeError("perf-gate-ready is unavailable; timing cannot be qualified")
    readiness = _readiness(Path(program), ())
    if not readiness.host_quiet:
        timing = DeferredTiming("host busy")
    else:
        failure = None
        try:
            measured_timing = runtime.measure_timing(source)
        except BaseException as error:
            failure = error
            measured_timing = None
        post = _readiness(Path(program), ("-1",))
        if failure is not None:
            if not post.host_quiet:
                failure.add_note("perf gate deferred: host busy after failed timing producer")
            raise failure
        if not post.host_quiet:
            timing = DeferredTiming("host busy")
        elif measured_timing is None:
            raise ValueError("timing producer returned no measured samples")
        else:
            timing = measured_timing
        current = runtime.source_identity()
        if current != source:
            raise ValueError("source changed during timed resource measurement")
    current_products = _product_pair(runtime, source)
    if current_products != product_pair:
        raise ValueError("products changed during resource acceptance")
    if runtime.source_identity() != source:
        raise ValueError("source changed during resource acceptance")
    report = ResourceReport(source, binary_sizes, source_sizes, memory, timing)
    acceptance._validate_resource_report(report, runtime, source)
    return report


def close(runtime: ResourceRuntime) -> CloseReport:
    source = runtime.source_identity()
    if not source.clean:
        raise ValueError("close acceptance requires a clean final source")
    gates = []

    for name in ("inventory", "harness", "baseline"):
        _require_current(runtime, source, name)
        getattr(runtime, name)()
        _require_current(runtime, source, name)
        gates.append(_gate_result(name, source, 1, selectors=()))

    all_selectors = []
    for command in runtime.COHORT:
        _require_current(runtime, source, "parity")
        names = acceptance._case_names(runtime.ROOT, command)
        result = acceptance._run_group(runtime, command, names)
        if result.passed != len(names) or result.failed or result.skipped:
            raise ValueError(f"parity did not pass every declared behavior: {command}")
        all_selectors.extend(acceptance._case_selector(command, name) for name in result.selectors)
        _require_current(runtime, source, "parity")
    all_selectors = tuple(sorted(all_selectors))
    gates.append(_gate_result("parity", source, len(all_selectors), selectors=all_selectors, private=all_selectors))

    positive_selectors = tuple(sorted(
        acceptance._case_selector(command, POSITIVE_BOUNDARIES[command])
        for command in runtime.COHORT
    ))
    positive_names = tuple(selector.rsplit("::", 1)[-1] for selector in positive_selectors)
    for selector, name in zip(positive_selectors, positive_names, strict=True):
        _require_current(runtime, source, "positive-boundaries")
        result = acceptance._parse_native_group(
            selector.split("/", 1)[1].split(".xsh::", 1)[0].removeprefix("test-"),
            (name,),
            runtime.native_case(selector, private=True),
        )
        if result.selectors != (name,) or result.passed != 1:
            raise ValueError(f"positive boundary did not execute exactly once: {selector}")
        _require_current(runtime, source, "positive-boundaries")
    gates.append(_gate_result("positive-boundaries", source, len(positive_selectors), selectors=positive_selectors, private=positive_selectors))

    regressions = _selectors(runtime, source)
    regression_names = tuple(selector.rsplit("::", 1)[-1] for selector in regressions)
    for selector, name in zip(regressions, regression_names, strict=True):
        _require_current(runtime, source, "focused-regressions")
        relative = selector.split("::", 1)[0]
        command = Path(relative).stem.removeprefix("test-")
        result = acceptance._parse_native_group(command, (name,), runtime.native_case(selector, private=True))
        if result.selectors != (name,) or result.passed != 1:
            raise ValueError(f"focused regression did not execute exactly once: {selector}")
        _require_current(runtime, source, "focused-regressions")
    gates.append(_gate_result("focused-regressions", source, len(regressions), selectors=regressions))

    for callback_name, gate_name in (
        ("run_full_native", "full-native"),
        ("run_host_boundaries", "host-boundaries"),
        ("run_repository_contract", "repository-contract"),
        ("run_docs", "docs"),
    ):
        _require_current(runtime, source, gate_name)
        gate = getattr(runtime, callback_name)(source)
        _validate_returned_gate(gate, gate_name, source)
        _require_current(runtime, source, gate_name)
        gates.append(gate)

    _require_current(runtime, source, "resources")
    resource_report = runtime.measure_resources()
    _require_current(runtime, source, "resources")
    acceptance._validate_resource_report(resource_report, runtime, source)
    if isinstance(resource_report.timing, DeferredTiming):
        raise ValueError(f"perf gate deferred: {resource_report.timing.reason}")
    measured_selectors = tuple(sorted(
        tuple(f"binary:{row.name}" for row in resource_report.binary_sizes)
        + tuple(f"source:{row.command}" for row in resource_report.applet_sources)
        + tuple(f"rss:{row.selector}" for row in resource_report.memory)
    ))
    gates.append(_gate_result(
        "resources",
        source,
        len(measured_selectors),
        selectors=measured_selectors,
    ))

    _require_current(runtime, source, "adversarial-review")
    review = runtime.adversarial_review(source)
    _validate_returned_gate(review, "adversarial-review", source)
    _require_current(runtime, source, "adversarial-review")
    gates.append(review)

    _require_current(runtime, source, "cleanup")
    cleanup = runtime.owned_cleanup(source)
    _validate_returned_gate(cleanup, "cleanup", source)
    _require_current(runtime, source, "cleanup")
    gates.append(cleanup)

    after = runtime.source_identity()
    if after != source:
        raise ValueError("source changed during close acceptance")
    return CloseReport(
        source=source,
        gates=tuple(gates),
        resources=resource_report,
        remaining_owned_processes=0,
        lost_work=False,
    )


def _require_current(runtime: ResourceRuntime, source: SourceIdentity, gate: str) -> None:
    current = runtime.source_identity()
    if current != source or not current.clean:
        raise ValueError(f"source changed during {gate} acceptance")


def _gate_result(
    name: str,
    source: SourceIdentity,
    executed: int,
    *,
    selectors: tuple[str, ...],
    private: tuple[str, ...] = (),
) -> GateResult:
    if executed <= 0:
        raise ValueError(f"close gate executed no checks: {name}")
    return GateResult(
        name=name,
        source=source,
        status=0,
        executed=executed,
        passed=executed,
        failed=0,
        skipped=(),
        accounted_skips=(),
        selectors=selectors,
        private_selectors=private,
    )


def _validate_returned_gate(gate: GateResult, name: str, source: SourceIdentity) -> None:
    if not isinstance(gate, GateResult):
        raise TypeError(f"{name} producer did not return gate evidence")
    if gate.name != name or gate.source != source:
        raise ValueError(f"{name} producer returned mismatched gate evidence")
    acceptance._validate_gate_result(gate, source)
