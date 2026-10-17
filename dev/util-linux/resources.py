from __future__ import annotations

from collections.abc import Callable, Sequence
import contextlib
from dataclasses import replace
from pathlib import Path
import hashlib
import os
import re
import shlex
import shutil
import stat
import subprocess
import tempfile
import time
from typing import Protocol

import acceptance
from acceptance import (
    AppletSourceSize,
    BinarySize,
    BuildCost,
    BuildPair,
    BuildContext,
    CommandTiming,
    DeferredTiming,
    MemorySample,
    MeasurementKind,
    CloseReport,
    GateResult,
    MeasuredTiming,
    PairOrder,
    POSITIVE_BOUNDARIES,
    ResourceMetric,
    ResourceReport,
    Readiness,
    SourceIdentity,
    TimedPair,
)


GitRunner = Callable[..., subprocess.CompletedProcess[str]]
PRODUCTS = ("xsh", "xsht", "xsh-test-helper")
LAPUTA_CORPUS = Path("/home/josh/d/laputa-systems/laputa")
MOLD_PATH = Path("/usr/local/bin/mold")
POSITIVE_SELECTORS = {
    "lslocks": "test_lslocks_blocker_flock",
    "lsipc": "test_lsipc_visibility_matches_for_owner_and_unprivileged_observer",
    "lsmem": "test_lsmem_rooted_sparse_memory_attributes_and_output_modes",
    "mountpoint": "test_mountpoint_private_same_device_bind_mount",
    "namei": "test_namei_private_same_device_bind_mount_matches_reference",
}
TARGET = "x86_64-unknown-linux-musl"
HOST_NATIVE_BUDGET_SECONDS = 1200
FULL_NATIVE_TIMEOUT_SECONDS = 3600
RUST_GATE_TIMEOUT_SECONDS = 3600
REPOSITORY_GATE_TIMEOUT_SECONDS = 900
DOCS_GATE_TIMEOUT_SECONDS = 3600
_CARGO_LISTED_TEST = re.compile(r"^(?P<selector>\S+): test$")
_CARGO_RESULT = re.compile(
    r"^test (?P<selector>\S+) \.\.\. (?P<result>ok|ignored|FAILED)(?P<detail>(?:, .*)?)$"
)
_CARGO_SUMMARY = re.compile(
    r"^test result: (?P<result>ok|FAILED)\. (?P<passed>\d+) passed; "
    r"(?P<failed>\d+) failed; (?P<ignored>\d+) ignored; "
    r"(?P<measured>\d+) measured; (?P<filtered>\d+) filtered out; finished in .+$"
)
_RUST_GATE_CASES = (
    (
        "core_compat_boundaries",
        ("cargo", "test", "--locked", "--profile", "verification", "--target", TARGET,
         "-p", "xsh", "--test", "core_compat_boundaries"),
        ("--", "--skip", "split::"),
    ),
    (
        "core_compat_extra",
        ("cargo", "test", "--locked", "--profile", "verification", "--target", TARGET,
         "-p", "xsh", "--test", "core_compat_extra"),
        ("--", "--skip", "split::"),
    ),
    (
        "libxsh_api",
        ("cargo", "test", "--locked", "--profile", "verification", "--target", TARGET,
         "-p", "xsh", "--test", "integration", "libxsh_api"),
        (),
    ),
    (
        "xsh_registry",
        ("cargo", "test", "--locked", "-p", "xsh-registry", "--lib"),
        (),
    ),
    (
        "xsh_signature",
        ("cargo", "test", "--locked", "-p", "xsh", "--lib", "modules::signature"),
        (),
    ),
    (
        "xsht_api",
        ("cargo", "test", "--locked", "--profile", "verification", "--target", TARGET,
         "-p", "xsht", "--test", "integration", "api::"),
        (),
    ),
)


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
        cpus: int = 1,
        memory_gib: int = 1,
    ) -> bytes: ...

    def source_identity(self) -> SourceIdentity: ...

    def regression_selectors(self, source: SourceIdentity) -> tuple[str, ...]: ...

    def candidate_products(self) -> tuple[Path, BuildContext]: ...

    def baseline_products(self) -> tuple[Path, BuildContext]: ...

    def inventory(self) -> None: ...

    def harness(self) -> None: ...

    def baseline(self) -> None: ...

    def native_case(
        self,
        selector: str,
        *,
        private: bool = False,
        timeout: float = 30,
    ) -> bytes: ...

    def run_full_native(self, source: SourceIdentity) -> GateResult: ...

    def run_host_boundaries(self, source: SourceIdentity) -> GateResult: ...

    def run_repository_contract(self, source: SourceIdentity) -> GateResult: ...

    def run_docs(self, source: SourceIdentity) -> GateResult: ...

    def adversarial_review(self, source: SourceIdentity) -> GateResult: ...

    def owned_cleanup(self, source: SourceIdentity) -> GateResult: ...

    def measure_timing(self, source: SourceIdentity) -> MeasuredTiming | DeferredTiming: ...


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
    if baseline_context.profile != "release" or candidate_context.profile != "release":
        raise ValueError("resource acceptance requires final release products on both sides")
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
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile
import time

REGRESSION_SELECTORS = @REGRESSION_SELECTORS@
POSITIVE = @POSITIVE@
MODE = @MODE@
TIMING_COMMAND = @TIMING_COMMAND@
TIMING_SELECTOR = @TIMING_SELECTOR@
TIMING_ORDER = @TIMING_ORDER@
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
                             stderr=subprocess.PIPE, bufsize=0,
                             env=ENV, start_new_session=True)
    selector = selectors.DefaultSelector()
    output = bytearray()
    errors = bytearray()
    deadline = time.monotonic() + 8
    succeeded = False
    try:
        os.set_blocking(child.stdout.fileno(), False)
        os.set_blocking(child.stderr.fileno(), False)
        selector.register(child.stdout, selectors.EVENT_READ, output)
        selector.register(child.stderr, selectors.EVENT_READ, errors)
        while time.monotonic() < deadline:
            if b"\n" in output:
                line, extra = output.split(b"\n", 1)
                if extra:
                    raise RuntimeError("fixture keeper emitted data after readiness")
                ready = json.loads(line)
                os.set_blocking(child.stdout.fileno(), True)
                os.set_blocking(child.stderr.fileno(), True)
                succeeded = True
                return child, ready
            if child.poll() is not None and not selector.get_map():
                break
            for key, _ in selector.select(min(0.1, deadline - time.monotonic())):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    selector.unregister(key.fileobj)
                else:
                    key.data.extend(chunk)
        raise RuntimeError(f"fixture keeper did not report readiness: {bytes(errors)!r}")
    except BaseException:
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=1)
        except subprocess.TimeoutExpired:
            pass
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        if child.poll() is None:
            child.wait(timeout=1)
        raise
    finally:
        selector.close()
        if not succeeded:
            for pipe in (child.stdin, child.stdout, child.stderr):
                if pipe is not None:
                    pipe.close()


def finish_keeper(child, request):
    failure = None
    try:
        if child.poll() is not None:
            raise RuntimeError("fixture keeper exited before cleanup was requested")
        child.stdin.write((request + "\n").encode())
        child.stdin.flush()
    except BaseException as error:
        failure = error
    if failure is None:
        try:
            stdout, stderr = child.communicate(timeout=8)
        except subprocess.TimeoutExpired:
            failure = RuntimeError("fixture keeper exceeded its cleanup bound")
        except BaseException as error:
            failure = error
    if failure is not None:
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            stdout, stderr = child.communicate(timeout=1)
        except subprocess.TimeoutExpired:
            stdout, stderr = b"", b""
        except BaseException as error:
            failure.add_note(f"keeper cleanup also failed: {error}")
            stdout, stderr = b"", b""
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        if child.poll() is None:
            try:
                child.wait(timeout=1)
            except BaseException as error:
                failure.add_note(f"keeper reap also failed: {error}")
        failure.add_note(f"keeper cleanup output: stdout={stdout!r} stderr={stderr!r}")
        for pipe in (child.stdin, child.stdout, child.stderr):
            if pipe is not None and not pipe.closed:
                try:
                    pipe.close()
                except BaseException as error:
                    failure.add_note(f"keeper pipe close also failed: {error}")
        raise failure
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
    try:
        queue = next(row for row in ready["resources"] if row["kind"] == "sysv_queue")
        command_args = ["--queues", "--id", str(queue["id"]), "--output", "ID"]

        def snapshot():
            child.stdin.write(b"snapshot\n")
            child.stdin.flush()
            line = child.stdout.readline()
            if not line:
                raise RuntimeError("IPC keeper omitted its state snapshot")
            return line

        def owner():
            os.setgroups([])
            os.setgid(10001)
            os.setuid(10001)

        yield command_args, snapshot, owner
    finally:
        finish_keeper(child, "release")


def copy_lsmem_fixture(source, root):
    shutil.copytree(source, root, symlinks=True)


@contextlib.contextmanager
def lsmem_fixture(root):
    copy_lsmem_fixture("/work/core/tests/fixtures/util-linux/memory/sparse", root)
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
                or not reference[1]
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


def normalized_native_output(raw):
    lines = raw.decode("utf-8").splitlines()
    normalized = []
    for line in lines:
        if " ... ok " in line:
            selector, duration = line.rsplit(" ... ok ", 1)
            if not duration or any(character.isspace() for character in duration):
                raise RuntimeError("native runner emitted an invalid case duration")
            line = selector + " ... ok"
        normalized.append(line)
    return "\n".join(normalized).encode("utf-8")


def positive_timing(command, selector):
    with tempfile.TemporaryDirectory(dir="/t", prefix=f"timing-{command}-") as temporary:
        root = Path(temporary) / command
        with FIXTURES[command](root) as prepared:
            args, snapshot = prepared[0], prepared[1]
            preexec = prepared[2] if len(prepared) > 2 else None
            before = snapshot()
            commands = (("reference", "baseline"), (CANDIDATE, "candidate"))
            if TIMING_ORDER == "candidate-first":
                commands = tuple(reversed(commands))
            results = {}
            for products, side in commands:
                results[side] = measured(applet_argv(products, command, args), root, preexec_fn=preexec)
                if snapshot() != before:
                    raise RuntimeError(f"positive fixture changed after {side}: {command} {selector}")
            baseline, candidate = results["baseline"], results["candidate"]
            equal_output = baseline[1] == candidate[1] and baseline[2] == candidate[2]
            equal_state = snapshot() == before
            if baseline[0] or candidate[0] or not baseline[1] or not equal_output or not equal_state:
                raise RuntimeError(f"timed positive fixture failed parity: {command} {selector}")
            print("\t".join(("timing", command, selector, str(baseline[4]), str(candidate[4]),
                             str(baseline[0]), str(candidate[0]), str(int(equal_output)), str(int(equal_state)))))


def regression_timing(selector):
    commands = ((BASELINE, "baseline"), (CANDIDATE, "candidate"))
    if TIMING_ORDER == "candidate-first":
        commands = tuple(reversed(commands))
    results = {}
    for products, side in commands:
        results[side] = exact_native(products, selector)
    baseline, candidate = results["baseline"], results["candidate"]
    equal_output = normalized_native_output(baseline[3]) == normalized_native_output(candidate[3])
    if baseline[2] or candidate[2] or not equal_output:
        raise RuntimeError(f"regression selector differs across protected and candidate products: {selector}")
    print("\t".join(("timing", "native-regression", selector, str(baseline[1]), str(candidate[1]),
                     str(baseline[2]), str(candidate[2]), str(int(equal_output)), "1")))


def startup_timing(command, selector):
    with tempfile.TemporaryDirectory(dir="/t", prefix=f"startup-{command}-") as temporary:
        root = Path(temporary)
        before = fixture_tree(root)
        commands = ((BASELINE, "baseline"), (CANDIDATE, "candidate"))
        if TIMING_ORDER == "candidate-first":
            commands = tuple(reversed(commands))
        results = {}
        for products, side in commands:
            argv = [str(products / "xsh"), f"/work/core/{command}.xsh", "--version"]
            results[side] = measured(argv, ROOT, timeout=30, env={"TMPDIR": str(root)})
        baseline, candidate = results["baseline"], results["candidate"]
        equal_output = baseline[1] == candidate[1] and baseline[2] == candidate[2]
        equal_state = fixture_tree(root) == before
        if baseline[0] or candidate[0] or not (baseline[1] or baseline[2]) or not equal_output or not equal_state:
            raise RuntimeError(f"startup command failed output or state parity: {command} {selector}")
        print("\t".join(("timing", command, selector, str(baseline[4]), str(candidate[4]),
                         str(baseline[0]), str(candidate[0]), str(int(equal_output)), str(int(equal_state)))))


if MODE == "resources":
    for command, selector in POSITIVE:
        positive(command, selector)
    for selector in REGRESSION_SELECTORS:
        baseline = exact_native(BASELINE, selector)
        candidate = exact_native(CANDIDATE, selector)
        if baseline[2] or candidate[2] or normalized_native_output(baseline[3]) != normalized_native_output(candidate[3]):
            raise RuntimeError(f"regression selector differs across protected and candidate products: {selector}")
        print("\t".join(("regression", selector, str(baseline[0]), str(candidate[0]),
                         str(baseline[2]), str(candidate[2]), "1", "1")))
elif MODE == "command":
    if TIMING_SELECTOR in REGRESSION_SELECTORS:
        regression_timing(TIMING_SELECTOR)
    else:
        positive_timing(TIMING_COMMAND, TIMING_SELECTOR)
elif MODE == "startup":
    startup_timing(TIMING_COMMAND, TIMING_SELECTOR)
else:
    raise RuntimeError("unknown resource driver mode")
'''


def _driver_source(
    selectors: tuple[str, ...],
    cohort: tuple[str, ...],
    *,
    mode: str = "resources",
    command: str = "",
    selector: str = "",
    order: str = "baseline-first",
) -> str:
    positives = tuple((command, POSITIVE_SELECTORS[command]) for command in cohort)
    return (
        _MEASUREMENT_DRIVER
        .replace("@REGRESSION_SELECTORS@", repr(selectors))
        .replace("@POSITIVE@", repr(positives))
        .replace("@MODE@", repr(mode))
        .replace("@TIMING_COMMAND@", repr(command))
        .replace("@TIMING_SELECTOR@", repr(selector))
        .replace("@TIMING_ORDER@", repr(order))
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
        samples.append(("native-regression", selector, baseline_rss, candidate_rss))
    return tuple(samples)


def _parse_timed_pair(
    output: bytes,
    command: str,
    selector: str,
    order: PairOrder,
    before: Readiness,
    after: Readiness,
) -> TimedPair:
    try:
        lines = output.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise ValueError("timing child output was not UTF-8") from error
    if len(lines) != 1:
        raise ValueError("timing child must report exactly one paired sample")
    fields = tuple(lines[0].split("\t"))
    if len(fields) != 9 or fields[:3] != ("timing", command, selector):
        raise ValueError("timing child reported an unexpected measurement target")
    values = _integers(fields[3:], lines[0])
    baseline_cpu, candidate_cpu, baseline_status, candidate_status, output_equal, state_equal = values
    if min(baseline_cpu, candidate_cpu) <= 0:
        raise ValueError("timing child reported no positive CPU sample")
    if (baseline_status, candidate_status) != (0, 0):
        raise ValueError("timing child reported a failed process")
    if (output_equal, state_equal) != (1, 1):
        raise ValueError("timing child reported changed output or state")
    return TimedPair(
        baseline_cpu,
        candidate_cpu,
        before,
        after,
        order,
        baseline_status,
        candidate_status,
        True,
        True,
    )


_BUILD_DRIVER = r'''import hashlib
import os
from pathlib import Path
import signal
import stat
import subprocess
import tempfile
import time

EXPECTED = @EXPECTED@
TARGET = "x86_64-unknown-linux-musl"
JOBS = 2
RUSTFLAGS = "-C target-feature=+crt-static -C link-arg=-Wl,--defsym=__isoc23_sscanf=sscanf -C link-arg=-Wl,--defsym=__isoc23_strtol=strtol"
BUILD = ["/root/.cargo/bin/cargo", "build", "--locked", "--offline", "--release", "--target", TARGET,
         "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht", "-p", "xsh", "--bin", "xsh-test-helper"]


def build(source, side, sample):
    target = Path(f"/cargo-target/{side}-{sample}")
    target.mkdir(parents=True)
    temporary = target / "tmp"
    temporary.mkdir()
    env = dict(os.environ)
    env.update({
        "CARGO_HOME": "/root/.cargo",
        "RUSTUP_HOME": "/root/.rustup",
        "CARGO_NET_OFFLINE": "true",
        "CARGO_TARGET_DIR": str(target),
        "CARGO_BUILD_JOBS": str(JOBS),
        "CARGO_PROFILE_RELEASE_OPT_LEVEL": "3",
        "CARGO_PROFILE_RELEASE_LTO": "thin",
        "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "16",
        "RUSTC_WRAPPER": "/usr/local/bin/kache",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER": "/usr/local/bin/xsh-mold-linker",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS": RUSTFLAGS,
        "XSH_LAPUTA_CORPUS": "/home/josh/d/laputa-systems/laputa",
        "CC": "/usr/bin/clang-23",
        "AR": "/usr/local/bin/llvm-ar",
        "LD_PRELOAD": "/usr/lib/libjemalloc.so.2",
        "TMPDIR": str(temporary),
        "PATH": "/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin",
    })
    with tempfile.TemporaryFile(dir="/t") as stdout, tempfile.TemporaryFile(dir="/t") as stderr:
        child = subprocess.Popen(BUILD, cwd=source, env=env, stdin=subprocess.DEVNULL,
                                 stdout=stdout, stderr=stderr, start_new_session=True)
        deadline = time.monotonic() + 1800
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
                    raise TimeoutError("release build exceeded its bounded runtime")
                time.sleep(0.01)
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
            raise
        stdout.seek(0)
        stderr.seek(0)
        out, err = stdout.read(), stderr.read()
    elapsed = int((usage.ru_utime + usage.ru_stime) * 1_000_000_000)
    valid = child.returncode == 0
    if valid:
        product_root = target / TARGET / "release"
        for name, digest in EXPECTED[side]:
            product = product_root / name
            try:
                info = product.lstat()
                if not stat.S_ISREG(info.st_mode) or not info.st_mode & 0o111:
                    valid = False
                    break
                with product.open("rb") as stream:
                    if hashlib.file_digest(stream, "sha256").hexdigest() != digest:
                        valid = False
                        break
            except OSError:
                valid = False
                break
    return child.returncode, elapsed, valid, out, err


baseline_source = Path("/baseline-source")
subprocess.run(["tar", "-xf", "/baseline.tar", "-C", str(baseline_source)], check=True)
order = @ORDER@
sample = @SAMPLE@
commands = ((baseline_source, "baseline"), (Path("/work"), "candidate"))
if order == "candidate-first":
    commands = tuple(reversed(commands))
results = {}
for source, side in commands:
    results[side] = build(source, side, sample)
baseline, candidate = results["baseline"], results["candidate"]
print("\t".join(("build", str(baseline[1]), str(candidate[1]), str(baseline[0]),
                 str(candidate[0]), str(int(baseline[2])), str(int(candidate[2])))))
if baseline[0] or candidate[0] or not baseline[2] or not candidate[2]:
    raise RuntimeError(f"release build or product qualification failed: {baseline[3]!r} {baseline[4]!r} {candidate[3]!r} {candidate[4]!r}")
'''


_BUILD_CLEANUP_DRIVER = r'''import shutil
import stat
from pathlib import Path


def clear_build_directory(root):
    if not stat.S_ISDIR(root.lstat().st_mode):
        raise RuntimeError(f"build scratch root is not a directory: {root}")
    for child in root.iterdir():
        if stat.S_ISDIR(child.lstat().st_mode):
            shutil.rmtree(child)
        else:
            child.unlink()


clear_build_directory(Path("/baseline-source"))
clear_build_directory(Path("/cargo-target"))
'''


def _build_driver_source(
    expected: dict[str, tuple[tuple[str, str], ...]],
    order: PairOrder,
    sample: int,
) -> str:
    return (
        _BUILD_DRIVER
        .replace("@EXPECTED@", repr(expected))
        .replace("@ORDER@", repr(order.value))
        .replace("@SAMPLE@", repr(sample))
    )


def _integers(values: tuple[str, ...], line: str) -> tuple[int, ...]:
    if any(not value.isascii() or not value.isdecimal() for value in values):
        raise ValueError(f"resource child emitted a nonnumeric measurement: {line}")
    return tuple(int(value) for value in values)


def _readiness(program: Path, arguments: tuple[str, ...], run=subprocess.run) -> Readiness:
    result = run(
        [str(program), *arguments],
        capture_output=True,
        check=False,
        env={**os.environ, "PERF_GATE_ATTEMPTS": "1", "PERF_GATE_WAIT": "0"},
        timeout=30,
    )
    host_quiet = result.returncode == 0 and not result.stdout and not result.stderr
    return Readiness(result.returncode, result.stdout, result.stderr, host_quiet)


def _require_readiness_result(readiness: Readiness, phase: str) -> bool:
    if readiness.status == 75:
        return False
    if readiness.status != 0 or readiness.stdout or readiness.stderr or not readiness.host_quiet:
        raise RuntimeError(
            f"perf-gate-ready {phase} failed: status={readiness.status} "
            f"stdout={readiness.stdout!r} stderr={readiness.stderr!r}"
        )
    return True


def _readied_container(
    runtime: ResourceRuntime,
    program: Path,
    script: str,
    *,
    mounts: list[str],
    capabilities: list[str],
    timeout: int,
    cpus: int = 1,
    memory_gib: int = 1,
) -> tuple[bytes, Readiness, Readiness] | None:
    before = _readiness(program, ())
    if not _require_readiness_result(before, "before measurement"):
        return None
    failure = None
    output = None
    try:
        output = runtime.container(
            script,
            mounts=mounts,
            capabilities=capabilities,
            timeout=timeout,
            cpus=cpus,
            memory_gib=memory_gib,
        )
    except BaseException as error:
        failure = error
    try:
        after = _readiness(program, ("-1",))
    except BaseException as error:
        if failure is None:
            raise
        failure.add_note(f"post-measurement readiness failed: {error}")
        raise failure
    try:
        after_quiet = _require_readiness_result(after, "after measurement")
    except BaseException as error:
        if failure is None:
            raise
        failure.add_note(f"post-measurement readiness failed: {error}")
        raise failure
    if failure is not None:
        if not after_quiet:
            failure.add_note("perf gate deferred: host busy after failed measurement")
        raise failure
    if not after_quiet:
        return None
    if output is None:
        raise ValueError("timed container returned no output")
    return output, before, after


def _readied_timing_driver(
    runtime: ResourceRuntime,
    program: Path,
    source: SourceIdentity,
    selectors: tuple[str, ...],
    products: tuple[Path, BuildContext, ProductSignature, Path, BuildContext, ProductSignature],
    *,
    mode: str,
    command: str,
    selector: str,
    order: PairOrder,
) -> tuple[TimedPair, Readiness, Readiness] | None:
    _require_current(runtime, source, "timed resource sample")
    driver = _driver_source(
        selectors,
        runtime.COHORT,
        mode=mode,
        command=command,
        selector=selector,
        order=order.value,
    )
    script = "cd /work; " + runtime.PRIVATE_NAMESPACE.rstrip() + " python3 -c " + shlex.quote(driver)
    result = _readied_container(
        runtime,
        program,
        script,
        mounts=[
            f"type=bind,source={runtime.ROOT},target=/work,readonly",
            f"type=bind,source={products[0]},target=/baseline,readonly",
            f"type=bind,source={products[3]},target=/candidate,readonly",
        ],
        capabilities=["SYS_ADMIN"],
        timeout=120,
    )
    if result is None:
        return None
    output, before, after = result
    _require_current(runtime, source, "timed resource sample")
    pair = _parse_timed_pair(output, command, selector, order, before, after)
    return pair, before, after


def _build_result(output: bytes, order: PairOrder, before: Readiness, after: Readiness) -> BuildPair:
    try:
        lines = output.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise ValueError("build child output was not UTF-8") from error
    if len(lines) != 1:
        raise ValueError("build child must report exactly one paired sample")
    fields = tuple(lines[0].split("\t"))
    if len(fields) != 7 or fields[0] != "build":
        raise ValueError("build child reported an unexpected measurement")
    baseline_cpu, candidate_cpu, baseline_status, candidate_status, baseline_valid, candidate_valid = _integers(fields[1:], lines[0])
    if min(baseline_cpu, candidate_cpu) <= 0:
        raise ValueError("build child reported no positive CPU sample")
    if (baseline_status, candidate_status) != (0, 0) or (baseline_valid, candidate_valid) != (1, 1):
        raise ValueError("build child failed or produced stale products")
    return BuildPair(
        baseline_cpu,
        candidate_cpu,
        before,
        after,
        order,
        baseline_status,
        candidate_status,
        True,
        True,
    )


def _git_archive(root: Path, commit: str, destination: Path) -> Path:
    with destination.open("wb") as archive:
        result = subprocess.run(
            ["git", "archive", "--format=tar", commit],
            cwd=root,
            stdout=archive,
            stderr=subprocess.PIPE,
            check=False,
        )
    if result.returncode != 0:
        raise RuntimeError(f"cannot archive the frozen build source: {result.stderr.decode(errors='replace')}")
    if destination.stat().st_size <= 0:
        raise ValueError("frozen build source archive is empty")
    return destination


def _expected_build_products(products: tuple[Path, BuildContext, ProductSignature, Path, BuildContext, ProductSignature]):
    return {
        "baseline": tuple((row[0], row[-1].hex()) for row in products[2]),
        "candidate": tuple((row[0], row[-1].hex()) for row in products[5]),
    }


@contextlib.contextmanager
def _build_source_scratch(scratch: Path):
    temporary = Path(tempfile.mkdtemp(prefix="util-linux-build-source-", dir=scratch))
    primary = None
    try:
        yield temporary
    except BaseException as error:
        primary = error
        raise
    finally:
        try:
            shutil.rmtree(temporary)
        except BaseException as cleanup_error:
            if primary is None:
                raise
            primary.add_note(f"build-source scratch retained at {temporary}: {cleanup_error}")


def _readied_build_pair(
    runtime: ResourceRuntime,
    program: Path,
    source: SourceIdentity,
    archive: Path,
    expected: dict[str, tuple[tuple[str, str], ...]],
    order: PairOrder,
    sample: int,
) -> BuildPair | None:
    _require_current(runtime, source, "build-cost sample")
    driver = _build_driver_source(expected, order, sample)
    jemalloc = Path("/usr/lib/libjemalloc.so.2")
    if not jemalloc.is_file():
        raise RuntimeError("build-cost measurement requires the pinned jemalloc preload")
    baseline_source = archive.parent / f"baseline-source-{sample}"
    target = archive.parent / f"build-target-{sample}"
    created_roots = []
    build_started = False
    primary = None
    try:
        baseline_source.mkdir()
        created_roots.append(baseline_source)
        target.mkdir()
        created_roots.append(target)
        mounts = _cargo_mounts(runtime, target)
        mounts.extend((
            f"type=bind,source={archive},target=/baseline.tar,readonly",
            f"type=bind,source={baseline_source},target=/baseline-source",
            f"type=bind,source={jemalloc},target=/usr/lib/libjemalloc.so.2,readonly",
        ))
        build_started = True
        result = _readied_container(
            runtime,
            program,
            "cd /work; python3 -c " + shlex.quote(driver),
            mounts=mounts,
            capabilities=[],
            timeout=3600,
            cpus=2,
            memory_gib=8,
        )
        if result is None:
            return None
        output, before, after = result
        _require_current(runtime, source, "build-cost sample")
        return _build_result(output, order, before, after)
    except BaseException as error:
        primary = error
        raise
    finally:
        try:
            if build_started and (any(baseline_source.iterdir()) or any(target.iterdir())):
                runtime.container(
                    "python3 -c " + shlex.quote(_BUILD_CLEANUP_DRIVER),
                    mounts=[
                        f"type=bind,source={baseline_source},target=/baseline-source",
                        f"type=bind,source={target},target=/cargo-target",
                    ],
                    capabilities=[],
                    timeout=30,
                )
            for root in created_roots:
                if any(root.iterdir()):
                    raise RuntimeError(f"build-cost cleanup left files in {root}")
                root.rmdir()
        except BaseException as cleanup_error:
            if primary is None:
                raise
            primary.add_note(f"build-cost scratch cleanup failed: {cleanup_error}")


_PAIR_ORDERS = (
    PairOrder.BASELINE_FIRST,
    PairOrder.CANDIDATE_FIRST,
    PairOrder.BASELINE_FIRST,
    PairOrder.CANDIDATE_FIRST,
    PairOrder.BASELINE_FIRST,
)


def _assert_timing_inputs(
    runtime: ResourceRuntime,
    source: SourceIdentity,
    expected: tuple[Path, BuildContext, ProductSignature, Path, BuildContext, ProductSignature],
) -> None:
    _require_current(runtime, source, "resource timing")
    current = _product_pair(runtime, source)
    if current[2] != expected[2] or current[5] != expected[5]:
        raise ValueError("products changed during timed resource acceptance")


def measure_timing(runtime: ResourceRuntime, source: SourceIdentity) -> MeasuredTiming | DeferredTiming:
    _require_current(runtime, source, "resource timing")
    if tuple(runtime.COHORT) != tuple(POSITIVE_SELECTORS):
        raise ValueError("timing cohort does not match the five qualified positive fixtures")
    products = _product_pair(runtime, source)
    baseline_path, baseline_context = products[0], products[1]
    candidate_path, candidate_context = products[3], products[4]
    selectors = _selectors(runtime, source)
    acceptance._validate_startup_targets(set(acceptance.STARTUP_TARGETS), selectors)
    program_name = shutil.which("perf-gate-ready")
    if program_name is None:
        raise RuntimeError("perf-gate-ready is unavailable; timing cannot be qualified")
    program = Path(program_name)
    reference_context = BuildContext(
        source=runtime.PACKAGE_COMMIT,
        profile=acceptance.REFERENCE_PROFILE,
        target=candidate_context.target,
        image=runtime.IMAGE,
        allocator=acceptance.REFERENCE_ALLOCATOR,
        allocation=candidate_context.allocation,
    )
    command_rows = []

    def collect_command(command: str, selector: str, kind: MeasurementKind, mode: str) -> CommandTiming | None:
        samples = []
        for order in _PAIR_ORDERS:
            result = _readied_timing_driver(
                runtime,
                program,
                source,
                selectors,
                products,
                mode=mode,
                command=command,
                selector=selector,
                order=order,
            )
            if result is None:
                _assert_timing_inputs(runtime, source, products)
                return None
            pair, _, _ = result
            samples.append(pair)
            _assert_timing_inputs(runtime, source, products)
        return CommandTiming(
            command=command,
            selector=selector,
            kind=kind,
            metric=ResourceMetric.COMMAND,
            baseline=reference_context if kind is MeasurementKind.NEW_REFERENCE else baseline_context,
            candidate=candidate_context,
            samples=tuple(samples),
        )

    for command in runtime.COHORT:
        selector = POSITIVE_SELECTORS[command]
        row = collect_command(command, selector, MeasurementKind.NEW_REFERENCE, "command")
        if row is None:
            return DeferredTiming("host busy")
        command_rows.append(row)
    for selector in selectors:
        command = "native-regression"
        row = collect_command(command, selector, MeasurementKind.EXISTING_BASELINE, "command")
        if row is None:
            return DeferredTiming("host busy")
        command_rows.append(row)

    startup_rows = []
    for command, selector in acceptance.STARTUP_TARGETS:
        samples = []
        for order in _PAIR_ORDERS:
            result = _readied_timing_driver(
                runtime,
                program,
                source,
                selectors,
                products,
                mode="startup",
                command=command,
                selector=selector,
                order=order,
            )
            if result is None:
                _assert_timing_inputs(runtime, source, products)
                return DeferredTiming("host busy")
            pair, _, _ = result
            samples.append(pair)
            _assert_timing_inputs(runtime, source, products)
        startup_rows.append(CommandTiming(
            command=command,
            selector=selector,
            kind=MeasurementKind.EXISTING_BASELINE,
            metric=ResourceMetric.STARTUP,
            baseline=baseline_context,
            candidate=candidate_context,
            samples=tuple(samples),
        ))

    scratch = runtime.ROOT / ".work" / "tmp"
    scratch.mkdir(parents=True, exist_ok=True)
    with _build_source_scratch(scratch) as temporary:
        archive = _git_archive(runtime.ROOT, runtime.PRODUCT_SOURCE, temporary / "baseline.tar")
        expected = _expected_build_products(products)
        warmup = _readied_build_pair(
            runtime,
            program,
            source,
            archive,
            expected,
            PairOrder.BASELINE_FIRST,
            -1,
        )
        if warmup is None:
            _assert_timing_inputs(runtime, source, products)
            return DeferredTiming("host busy")
        _assert_timing_inputs(runtime, source, products)
        build_samples = []
        for index, order in enumerate(_PAIR_ORDERS):
            pair = _readied_build_pair(runtime, program, source, archive, expected, order, index)
            if pair is None:
                _assert_timing_inputs(runtime, source, products)
                return DeferredTiming("host busy")
            build_samples.append(pair)
            _assert_timing_inputs(runtime, source, products)

    build_cache = "warm:xsh-consolidation-kache"
    build_baseline = replace(
        baseline_context,
        allocation="2cpu/8GiB",
        cache_state=build_cache,
        compile_jobs=2,
    )
    build_candidate = replace(
        candidate_context,
        allocation="2cpu/8GiB",
        cache_state=build_cache,
        compile_jobs=2,
    )
    _assert_timing_inputs(runtime, source, products)
    return MeasuredTiming(
        commands=tuple(command_rows),
        startup=tuple(startup_rows),
        build_cost=BuildCost(build_baseline, build_candidate, tuple(build_samples)),
    )


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
                    profile=acceptance.REFERENCE_PROFILE,
                    target=candidate_context.target,
                    image=runtime.IMAGE,
                    allocator=acceptance.REFERENCE_ALLOCATOR,
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
    timing = runtime.measure_timing(source)
    if timing is None:
        raise ValueError("timing producer returned no result")
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
    _product_pair(runtime, source)
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
    gates.append(_gate_result(
        "focused-regressions",
        source,
        len(regressions),
        selectors=regressions,
        private=regressions,
    ))

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
    remaining_owned_processes, lost_work = _cleanup_state(cleanup, source)
    _require_current(runtime, source, "cleanup")
    gates.append(cleanup)

    after = runtime.source_identity()
    if after != source:
        raise ValueError("source changed during close acceptance")
    return CloseReport(
        source=source,
        gates=tuple(gates),
        resources=resource_report,
        remaining_owned_processes=remaining_owned_processes,
        lost_work=lost_work,
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


def _cleanup_state(gate: GateResult, source: SourceIdentity) -> tuple[int, bool]:
    _validate_returned_gate(gate, "cleanup", source)
    expected = {
        "remaining-owned-processes:0",
        "unresolved-markers:0",
        "lost-work:false",
    }
    if len(gate.selectors) != len(expected) or set(gate.selectors) != expected:
        raise ValueError("cleanup gate omitted current-source process, marker, or handoff observations")
    if gate.executed != len(expected):
        raise ValueError("cleanup gate did not execute each required observation exactly once")
    observations = dict(selector.split(":", 1) for selector in gate.selectors)
    remaining = int(observations["remaining-owned-processes"])
    unresolved = int(observations["unresolved-markers"])
    lost_work = observations["lost-work"] == "true"
    if remaining or unresolved or lost_work:
        raise ValueError("cleanup gate found owned processes, unresolved markers, or lost work")
    return remaining, lost_work


def _qualified_candidate(
    runtime: ResourceRuntime,
    source: SourceIdentity,
) -> tuple[Path, BuildContext, ProductSignature]:
    _require_current(runtime, source, "close products")
    products, context = runtime.candidate_products()
    if context.source != source.key or context.profile != "release":
        raise ValueError("close gates require final release products for the current source")
    if context.image != runtime.IMAGE or context.target != TARGET:
        raise ValueError("close products do not identify the pinned Linux target and image")
    signature = _product_signature(products)
    _require_current(runtime, source, "close products")
    return products, context, signature


def _recheck_candidate(
    runtime: ResourceRuntime,
    source: SourceIdentity,
    expected: tuple[Path, BuildContext, ProductSignature],
    gate: str,
) -> None:
    current = _qualified_candidate(runtime, source)
    if current != expected:
        raise ValueError(f"candidate products changed during {gate}")


def _candidate_mounts(runtime: ResourceRuntime, products: Path) -> list[str]:
    return [
        f"type=bind,source={runtime.ROOT},target=/work,readonly",
        f"type=bind,source={products},target=/products,readonly",
    ]


def _framed_script(script: str) -> str:
    return "cd /work; python3 -c " + shlex.quote(script)


def _parse_frames(output: bytes, labels: tuple[str, ...]) -> dict[str, bytes]:
    remaining = output
    frames: dict[str, bytes] = {}
    for label in labels:
        header, separator, body = remaining.partition(b"\n")
        prefix = f"@@xsh-util-linux:{label}:".encode()
        if not separator or not header.startswith(prefix) or not header.endswith(b"@@"):
            raise ValueError(f"container omitted the {label} observation frame")
        raw_size = header[len(prefix):-2]
        if not raw_size.isdigit():
            raise ValueError(f"container returned a malformed {label} frame size")
        size = int(raw_size)
        if len(body) < size + 1 or body[size:size + 1] != b"\n":
            raise ValueError(f"container truncated the {label} observation frame")
        frames[label] = body[:size]
        remaining = body[size + 1:]
    if remaining:
        raise ValueError("container returned unaccounted output after its observation frames")
    return frames


_FULL_NATIVE_DRIVER = r'''import subprocess
import sys

REQUIRED = @REQUIRED@

def emit(label, content):
    sys.stdout.buffer.write(b"@@xsh-util-linux:" + label.encode() + b":" + str(len(content)).encode() + b"@@\n")
    sys.stdout.buffer.write(content)
    sys.stdout.buffer.write(b"\n")
    sys.stdout.buffer.flush()

for label, command in (
    ("list", ["/products/xsht", "test", "--list"]),
    ("result", ["/products/xsht", "test", "--jobs", "1"]),
):
    child = subprocess.run(command, cwd="/work", stdin=subprocess.DEVNULL,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=3500)
    if child.returncode != 0:
        sys.stderr.buffer.write(child.stdout)
        sys.stderr.buffer.write(child.stderr)
        raise SystemExit(child.returncode or 1)
    if child.stderr:
        sys.stderr.buffer.write(child.stderr)
        raise SystemExit(1)
    if label == "list":
        listed = set(child.stdout.decode("utf-8").splitlines())
        missing = set(REQUIRED) - listed
        if missing:
            sys.stderr.write("full native list omitted required selectors: " + ", ".join(sorted(missing)) + "\n")
            raise SystemExit(1)
    emit(label, child.stdout)
'''


def run_full_native(runtime: ResourceRuntime, source: SourceIdentity) -> GateResult:
    products = _qualified_candidate(runtime, source)
    mandatory = {
        acceptance._case_selector(command, name)
        for command in runtime.COHORT
        for name in acceptance._case_names(runtime.ROOT, command)
    }
    mandatory.update(acceptance.declared_native_selectors(runtime.ROOT, acceptance.HOST_BOUNDARY_FILES))
    mandatory.update(_selectors(runtime, source))
    driver = _FULL_NATIVE_DRIVER.replace("@REQUIRED@", repr(tuple(sorted(mandatory))))
    output = runtime.container(
        "cd /work; " + runtime.PRIVATE_NAMESPACE.rstrip() + " "
        + "python3 -c " + shlex.quote(driver),
        mounts=_candidate_mounts(runtime, products[0]),
        capabilities=["SYS_ADMIN"],
        timeout=FULL_NATIVE_TIMEOUT_SECONDS,
        cpus=1,
        memory_gib=4,
    )
    _require_current(runtime, source, "full native suite")
    _recheck_candidate(runtime, source, products, "full native suite")
    frames = _parse_frames(output, ("list", "result"))
    expected = acceptance._parse_native_selector_list("full native suite", frames["list"])
    if not mandatory.issubset(expected):
        raise ValueError("full native selector list omitted required cohort, host, or regression behavior")
    result = acceptance._parse_native_suite_result("full native suite", expected, frames["result"])
    skip_evidence = []
    for selector, reason in result.skipped:
        if selector in mandatory:
            raise ValueError(f"full native suite skipped mandatory behavior: {selector}: {reason}")
        if not acceptance.case_declares_skip(runtime.ROOT, selector):
            raise ValueError(f"full native suite skipped behavior without a source-declared condition: {selector}")
        skip_evidence.append(f"{selector}: {reason}")
    _require_current(runtime, source, "full native skip accounting")
    _recheck_candidate(runtime, source, products, "full native skip accounting")
    skips = tuple(sorted(skip_evidence))
    return GateResult(
        name="full-native",
        source=source,
        status=0,
        executed=len(expected),
        passed=result.passed,
        failed=0,
        skipped=skips,
        accounted_skips=skips,
        selectors=result.selectors,
        private_selectors=result.selectors,
    )


def _native_file(
    runtime: ResourceRuntime,
    source: SourceIdentity,
    relative: str,
    products: tuple[Path, BuildContext, ProductSignature],
) -> acceptance.NativeResult:
    expected = acceptance.declared_native_selectors(runtime.ROOT, (relative,))
    output = runtime.container(
        "cd /work; " + shlex.join(["/products/xsht", "test", relative]),
        mounts=_candidate_mounts(runtime, products[0]),
        capabilities=[],
        timeout=600,
    )
    _require_current(runtime, source, f"native boundary {relative}")
    _recheck_candidate(runtime, source, products, f"native boundary {relative}")
    return acceptance._parse_native_suite(relative, expected, output)


def _run_private_native(
    runtime: ResourceRuntime,
    source: SourceIdentity,
    selectors: tuple[str, ...],
    *,
    monotonic=time.monotonic,
) -> tuple[str, ...]:
    if not selectors or len(selectors) != len(set(selectors)):
        raise ValueError("host boundary selectors must be nonempty and distinct")
    deadline = monotonic() + HOST_NATIVE_BUDGET_SECONDS
    passed = []
    for selector in selectors:
        remaining = deadline - monotonic()
        if remaining <= 0:
            raise TimeoutError(f"private host boundaries exceeded {HOST_NATIVE_BUDGET_SECONDS}s")
        _require_current(runtime, source, "private host boundary")
        output = runtime.native_case(
            selector,
            private=True,
            timeout=min(30.0, remaining),
        )
        result = acceptance._parse_native_suite(selector, (selector,), output)
        passed.extend(result.selectors)
        _require_current(runtime, source, "private host boundary")
    if monotonic() > deadline:
        raise TimeoutError(f"private host boundaries exceeded {HOST_NATIVE_BUDGET_SECONDS}s")
    return tuple(passed)


def _rust_test_list(output: bytes, label: str) -> tuple[str, ...]:
    try:
        lines = output.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise ValueError(f"{label} test list was not UTF-8") from error
    selectors = tuple(match.group("selector") for line in lines if (match := _CARGO_LISTED_TEST.fullmatch(line)))
    if not selectors or len(selectors) != len(set(selectors)):
        raise ValueError(f"{label} returned no unique Rust test selectors")
    return selectors


def _rust_test_results(
    label: str,
    listed: tuple[str, ...],
    output: bytes,
    *,
    filtered: tuple[str, ...] = (),
) -> tuple[int, int, tuple[str, ...], tuple[str, ...]]:
    try:
        lines = output.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise ValueError(f"{label} test output was not UTF-8") from error
    results = [match for line in lines if (match := _CARGO_RESULT.fullmatch(line))]
    summaries = [match for line in lines if (match := _CARGO_SUMMARY.fullmatch(line))]
    if not results or not summaries:
        raise ValueError(f"{label} did not report completed Rust tests")
    omitted = tuple(selector for selector in listed if selector in filtered)
    if len(omitted) != len(filtered) or len(set(filtered)) != len(filtered):
        raise ValueError(f"{label} has an invalid conditional skip set")
    expected = tuple(selector for selector in listed if selector not in set(filtered))
    actual = tuple(match.group("selector") for match in results)
    if len(actual) != len(set(actual)) or tuple(sorted(actual)) != tuple(sorted(expected)):
        raise ValueError(f"{label} did not execute its listed test selectors exactly once")
    passed = sum(match.group("result") == "ok" for match in results)
    ignored = []
    for match in results:
        if match.group("result") != "ignored":
            continue
        reason = match.group("detail").removeprefix(", ").strip()
        if not reason:
            raise ValueError(f"{label} ignored Rust test omitted its reason: {match.group('selector')}")
        ignored.append(f"cargo:{label}::{match.group('selector')}: {reason}")
    ignored = tuple(ignored)
    failed = sum(match.group("result") == "FAILED" for match in results)
    summary_passed = sum(int(match.group("passed")) for match in summaries)
    summary_failed = sum(int(match.group("failed")) for match in summaries)
    summary_ignored = sum(int(match.group("ignored")) for match in summaries)
    summary_measured = sum(int(match.group("measured")) for match in summaries)
    summary_filtered = sum(int(match.group("filtered")) for match in summaries)
    if (summary_passed, summary_failed, summary_ignored) != (passed, failed, len(ignored)):
        raise ValueError(f"{label} Rust summaries differ from their observed test results")
    if summary_failed or any(match.group("result") != "ok" for match in summaries):
        raise ValueError(f"{label} Rust tests failed")
    if summary_measured or summary_filtered != len(filtered):
        raise ValueError(f"{label} did not account for each selected and filtered test")
    accounted_skips = tuple(sorted(
        (*ignored, *(f"cargo:{label}::{selector}: filtered by --skip split::" for selector in filtered))
    ))
    selectors = tuple(f"cargo:{label}::{selector}" for selector in listed)
    return passed, len(listed), selectors, accounted_skips


_CARGO_GATE_DRIVER = r'''import subprocess
import sys

CASES = @CASES@

def emit(label, content):
    sys.stdout.buffer.write(b"@@xsh-util-linux:" + label.encode() + b":" + str(len(content)).encode() + b"@@\n")
    sys.stdout.buffer.write(content)
    sys.stdout.buffer.write(b"\n")
    sys.stdout.buffer.flush()

failed = False
for label, command, run_suffix in CASES:
    observations = (
        (label + ".list", command + ("--", "--list")),
        (label + ".run", command + run_suffix),
    )
    for name, argv in observations:
        try:
            child = subprocess.run(argv, cwd="/work", stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   timeout=1800, check=False)
        except subprocess.TimeoutExpired as error:
            sys.stderr.write(label + " exceeded its 1800 second command limit\n")
            if error.stdout:
                sys.stderr.buffer.write(error.stdout)
            if error.stderr:
                sys.stderr.buffer.write(error.stderr)
            raise SystemExit(124)
        emit(name, child.stdout + child.stderr)
        if child.returncode != 0:
            failed = True
if failed:
    raise SystemExit(1)
'''


def _rust_gate_script() -> str:
    cases = tuple((label, command, suffix) for label, command, suffix in _RUST_GATE_CASES)
    return _framed_script(_CARGO_GATE_DRIVER.replace("@CASES@", repr(cases)))


def _cargo_mounts(runtime: ResourceRuntime, target: Path) -> list[str]:
    kache = shutil.which("kache")
    linker = runtime.ROOT / "dev" / "musl-linker.sh"
    if (
        kache is None
        or not MOLD_PATH.is_file()
        or not os.access(MOLD_PATH, os.X_OK)
        or not linker.is_file()
        or not os.access(linker, os.X_OK)
    ):
        raise RuntimeError("host and documentation gates require kache, mold, and the repository musl linker")
    if not LAPUTA_CORPUS.is_dir():
        raise RuntimeError("host and documentation gates require the qualified Laputa corpus")
    return [
        f"type=bind,source={runtime.ROOT},target=/work,readonly",
        f"type=bind,source={target},target=/cargo-target",
        f"type=bind,source={kache},target=/usr/local/bin/kache,readonly",
        f"type=bind,source={MOLD_PATH},target=/usr/local/bin/mold,readonly",
        f"type=bind,source={linker},target=/usr/local/bin/xsh-mold-linker,readonly",
        f"type=bind,source={LAPUTA_CORPUS},target={LAPUTA_CORPUS},readonly",
        "type=volume,source=xsh-cargo-registry,target=/root/.cargo/registry",
        "type=volume,source=xsh-consolidation-kache,target=/root/.cache/kache",
    ]


def _cargo_environment() -> str:
    return (
        "export CARGO_TARGET_DIR=/cargo-target CARGO_NET_OFFLINE=true "
        "RUSTC_WRAPPER=/usr/local/bin/kache TARGET=" + TARGET + " "
        "XSH_LAPUTA_CORPUS=" + str(LAPUTA_CORPUS) + " "
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker "
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS='-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol' "
    )


def _parse_rust_gate(
    output: bytes,
) -> tuple[int, tuple[str, ...], tuple[str, ...]]:
    labels = tuple(
        label + suffix
        for label, _, _ in _RUST_GATE_CASES
        for suffix in (".list", ".run")
    )
    frames = _parse_frames(output, labels)
    filtered = {
        label: tuple(
            selector
            for selector in _rust_test_list(frames[label + ".list"], label)
            if "split::" in selector
        )
        for label, _, suffix in _RUST_GATE_CASES
        if suffix == ("--", "--skip", "split::")
    }
    total = 0
    selectors: list[str] = []
    skips: list[str] = []
    for label, _, _ in _RUST_GATE_CASES:
        listed = _rust_test_list(frames[label + ".list"], label)
        passed, executed, observed, accounted = _rust_test_results(
            label,
            listed,
            frames[label + ".run"],
            filtered=filtered.get(label, ()),
        )
        total += executed
        selectors.extend(observed)
        skips.extend(accounted)
        if passed + len(accounted) != executed:
            raise ValueError(f"{label} Rust result does not account for all listed tests")
    return total, tuple(selectors), tuple(sorted(skips))


def run_host_boundaries(runtime: ResourceRuntime, source: SourceIdentity) -> GateResult:
    products = _qualified_candidate(runtime, source)
    private_files = tuple(
        relative
        for relative in acceptance.HOST_BOUNDARY_FILES
        if relative != "tests/xsh/api-tool.xsh"
    )
    private_selectors = acceptance.declared_native_selectors(runtime.ROOT, private_files)
    passed_private = _run_private_native(runtime, source, private_selectors)
    _recheck_candidate(runtime, source, products, "private host boundaries")
    api_file = "tests/xsh/api-tool.xsh"
    api_result = _native_file(runtime, source, api_file, products)
    if api_result.passed != len(api_result.selectors):
        raise ValueError("live API native suite did not pass every declared selector")

    target = products[0].parent.parent
    cargo_output = runtime.container(
        _cargo_environment() + _rust_gate_script(),
        mounts=_cargo_mounts(runtime, target),
        capabilities=[],
        timeout=RUST_GATE_TIMEOUT_SECONDS,
        cpus=2,
        memory_gib=8,
    )
    _require_current(runtime, source, "host boundary Rust gates")
    _recheck_candidate(runtime, source, products, "host boundary Rust gates")
    rust_executed, rust_selectors, rust_skips = _parse_rust_gate(cargo_output)

    runtime.container(
        "/products/xsht check docs/snippets/api",
        mounts=_candidate_mounts(runtime, products[0]),
        capabilities=[],
        timeout=120,
    )
    _require_current(runtime, source, "API documentation snippets")
    _recheck_candidate(runtime, source, products, "API documentation snippets")

    native_selectors = tuple(sorted((*passed_private, *api_result.selectors)))
    selectors = tuple(sorted((*native_selectors, *rust_selectors, "command:xsht-check-api-snippets")))
    skipped = tuple(sorted(rust_skips))
    executed = len(native_selectors) + rust_executed + 1
    passed = executed - len(skipped)
    return GateResult(
        name="host-boundaries",
        source=source,
        status=0,
        executed=executed,
        passed=passed,
        failed=0,
        skipped=skipped,
        accounted_skips=skipped,
        selectors=selectors,
        private_selectors=tuple(sorted(passed_private)),
    )


def run_repository_contract(runtime: ResourceRuntime, source: SourceIdentity) -> GateResult:
    products = _qualified_candidate(runtime, source)
    relative = "dev/tests/test-repository-contracts.xsh"
    expected = acceptance.declared_native_selectors(runtime.ROOT, (relative,))
    output = runtime.container(
        shlex.join(["/products/xsht", "test", "-j", "1", relative]),
        mounts=_candidate_mounts(runtime, products[0]),
        capabilities=[],
        timeout=REPOSITORY_GATE_TIMEOUT_SECONDS,
    )
    _require_current(runtime, source, "repository contract")
    _recheck_candidate(runtime, source, products, "repository contract")
    result = acceptance._parse_native_suite("repository contract", expected, output)
    return _gate_result(
        "repository-contract",
        source,
        result.passed,
        selectors=result.selectors,
    )


def run_docs(runtime: ResourceRuntime, source: SourceIdentity) -> GateResult:
    products = _qualified_candidate(runtime, source)
    target = products[0].parent.parent
    script = (
        "cd /work; "
        + _cargo_environment().replace("CARGO_TARGET_DIR=/cargo-target", "CARGO_TARGET_DIR=/target")
        + "cargo dev docs check"
    )
    runtime.container(
        script,
        mounts=[
            f"type=bind,source={runtime.ROOT},target=/work,readonly",
            f"type=bind,source={target},target=/target",
            f"type=bind,source={products[0]},target=/products,readonly",
            *[mount for mount in _cargo_mounts(runtime, target) if "/cargo-target" not in mount and "target=/work" not in mount],
        ],
        capabilities=[],
        timeout=DOCS_GATE_TIMEOUT_SECONDS,
        cpus=2,
        memory_gib=8,
    )
    _require_current(runtime, source, "documentation checks")
    _recheck_candidate(runtime, source, products, "documentation checks")
    return _gate_result(
        "docs",
        source,
        1,
        selectors=("command:cargo-dev-docs-check",),
    )
