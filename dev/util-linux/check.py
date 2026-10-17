#!/usr/bin/env python3
from __future__ import annotations

import argparse
import contextlib
import contextvars
import hashlib
import os
from pathlib import Path
import re
import signal
import shutil
import shlex
import subprocess
import sys
import tempfile
import uuid

sys.dont_write_bytecode = True

from leases import campaign_leases, LeaseBusy, mark_cleanup_unresolved, original_root, require_native_lease, require_cargo_lease


ROOT = Path(__file__).resolve().parents[2]
IMAGE = "xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
COHORT = ("lslocks", "lsipc", "lsmem", "mountpoint", "namei")
PACKAGE_COMMIT = "087ea48145e24307010ba91f403c5ff8f402568c"
PRODUCT_SOURCE = "57a0f63145a4ea3b1db6cfc38b4744a5c7ab02c1"
PRODUCT_HASHES = {
    "xsh": "e79fd4820b23b840c0bbface75a8b24780eae8fb7aedc34a01d76045f9aeae6b",
    "xsht": "daa6c47667f65e8a2fe06a04ff179ee99e6286496c6d441b193889b3b9fec7fa",
    "xsh-test-helper": "cf8005649749a43cc22aa5e40092a8f90365efc370b857fce5de2004adb2e621",
}

RUNTIME_SOURCE = "b65669bf76ae79a1ea2015cb9e2cef752f43620b"
RUNTIME_HASHES = {
    "xsh": "6b0747950bba3147c4b2aa0aa010b7a3057a82b280f766c25be0f0acf4049af2",
    "xsht": "10bdbcb16ef16f56a481a27bc105fecd16d45ab682781606c815354d4f9c01a9",
    "xsh-test-helper": "4414efbc1a0091f0dba080a445db865561524fa4ac91b5f8beb834d2341c6c64",
}


class Cancelled(Exception):
    def __init__(self, signum):
        super().__init__(f"cancelled by signal {signum}")
        self.signum = signum


@contextlib.contextmanager
def cancellation_scope():
    def cancel(signum, _frame):
        raise Cancelled(signum)
    previous = {number: signal.signal(number, cancel) for number in (signal.SIGINT, signal.SIGTERM)}
    try:
        yield
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)


@cancellation_scope()
def container(script: str, *, mounts: list[str] = (), capabilities: list[str] = (), timeout: int = 30, cpus: int = 1, memory_gib: int = 1) -> bytes:
    require_native_lease()
    if (cpus, memory_gib) not in {(1, 1), (1, 4), (2, 8)}:
        raise ValueError("unsupported campaign container allocation")
    if (cpus, memory_gib) == (2, 8):
        require_cargo_lease()
    name = "xsh-util-linux-" + uuid.uuid4().hex
    argv = ["docker", "run", "--rm", "--name", name, "--network=none", f"--cpus={cpus}",
            f"--memory={memory_gib}g", f"--memory-swap={memory_gib}g", "--pids-limit=128", "--user=0:0",
            "-e", "LC_ALL=C", "-e", "TZ=UTC", "-e", "TMPDIR=/t"]
    for mount in mounts:
        argv.extend(["--mount", mount])
    for capability in capabilities:
        argv.append("--cap-add=" + capability)
    argv.extend([IMAGE, "sh", "-ec", script])
    primary = None
    scratch = tempfile.mkdtemp(dir=ROOT / ".work/tmp")
    os.chmod(scratch, 0o755)
    argv[3:3] = ["--mount", f"type=bind,source={scratch},target=/t"]
    try:
        result = subprocess.run(argv, capture_output=True, timeout=timeout)
        if result.returncode:
            raise RuntimeError(f"reference container exited {result.returncode}: stdout={result.stdout.decode(errors='backslashreplace')} stderr={result.stderr.decode(errors='backslashreplace')}")
        return result.stdout
    except BaseException as error:
        primary = error
        raise
    finally:
        removed = False
        try:
            cleanup = subprocess.run(["docker", "rm", "--force", name], capture_output=True, timeout=10)
            if cleanup.returncode and b"No such container" not in cleanup.stderr:
                raise RuntimeError(f"container cleanup failed for {name}; retain {scratch}: {cleanup.stderr!r}")
            removed = True
            shutil.rmtree(scratch)
        except BaseException as error:
            if not removed:
                try:
                    mark_cleanup_unresolved(f"container={name} scratch={scratch} cause={error}")
                except BaseException as marker_error:
                    error.add_note(f"cannot retain unresolved cleanup: {marker_error}")
            if primary is None:
                raise
            primary.add_note(f"cleanup also failed for {name}: {error}")
            for note in getattr(error, "__notes__", ()):
                primary.add_note(note)


INVENTORY_SCRIPT = r'''
packages=$(awk -F: '$1=="P"{p=$2}$1=="V"{v=$2}$1=="o"{o=$2}$1=="c"{c=$2}NF==0{if(o=="util-linux"){if(v!="2.42.4-r3" || c!="087ea48145e24307010ba91f403c5ff8f402568c")exit 2;print p}p=v=o=c=""}' /lib/apk/db/installed)
test -n "$packages"
for package in $packages; do
  apk info -L "$package" | while IFS= read -r path; do
    case "$path" in bin/*|sbin/*|usr/bin/*|usr/sbin/*)
      case "${path#*/}" in */*) case "$path" in usr/bin/*|usr/sbin/*) ;; *) continue;; esac;; esac
      test -f "/$path" && test -x "/$path" || continue
      printf '%s\t/%s\t%s\n' "${path##*/}" "$path" "$package"
    esac
  done
done | sort
for command in lslocks lsipc lsmem mountpoint namei; do
  test "$("$command" --version)" = "$command from util-linux 2.42.4"
done
'''


def inventory() -> None:
    actual = container(INVENTORY_SCRIPT).decode("utf-8")
    installed = {}
    for line in actual.splitlines():
        command, path, package = line.split("\t")
        if command in installed:
            raise ValueError(f"duplicate installed command: {command}")
        installed[command] = (path, package)
    if not set(COHORT).issubset(installed):
        raise ValueError("missing cohort reference command")
    expected = {}
    path = ROOT / "dev/util-linux/inventory.tsv"
    for line in path.read_text().splitlines()[1:]:
        command, executable, package, scope, disposition = line.split("\t")
        if command in expected or scope not in {"cohort", "outside-cohort"}:
            raise ValueError(f"invalid inventory classification: {line}")
        if disposition not in {"partial", "absent", "complete"}:
            raise ValueError(f"invalid implementation disposition: {line}")
        if (scope == "cohort") != (command in COHORT):
            raise ValueError(f"incorrect cohort: {command}")
        expected[command] = (executable, package)
    if expected != installed:
        raise ValueError(f"installed inventory changed; added={sorted(installed.keys()-expected.keys())}, removed={sorted(expected.keys()-installed.keys())}")
    print(f"qualified util-linux 2.42.4-r3: {len(installed)} installed commands, five cohort references")



def source_identity():
    import acceptance
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=normal"], cwd=ROOT)
    return acceptance.SourceIdentity(commit, tree, not dirty)


def regression_selectors(source):
    import acceptance
    if source_identity() != source:
        raise ValueError("source changed before regression selection")
    return acceptance.declared_regression_selectors(ROOT)

def verify_product_hashes(directory: Path, expected_hashes: dict[str, str]) -> Path:
    for name, expected in expected_hashes.items():
        executable = directory / name
        if executable.is_symlink() or not executable.is_file():
            raise ValueError(f"unsafe qualified executable: {executable}")
        with executable.open("rb") as stream:
            actual = hashlib.file_digest(stream, "sha256").hexdigest()
        if actual != expected:
            raise ValueError(f"qualified product changed: {name}")
    return directory


def protected_products() -> Path:
    inputs = ["src", "crates", "third_party", "Cargo.toml", "Cargo.lock", "build.rs", "tests/helpers", "rust-toolchain.toml", ".cargo"]
    changed = subprocess.check_output(["git", "diff", "--name-only", RUNTIME_SOURCE, "--", *inputs], cwd=ROOT)
    untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard", "--", *inputs], cwd=ROOT)
    if changed or untracked:
        raise ValueError("qualified products cannot execute changed Rust inputs")
    directory = original_root(ROOT) / ".work/wt/util-linux-foundation/.work/tmp/ipc-api-target/x86_64-unknown-linux-musl/verification"
    return verify_product_hashes(directory, RUNTIME_HASHES)


def candidate_products():
    import acceptance
    import release
    active = release.active_products(sys.modules[__name__])
    if active is not None:
        return active
    source = source_identity()
    if not source.clean:
        raise ValueError("resource measurements require committed candidate sources")
    directory = protected_products()
    context = acceptance.BuildContext(source=f"{source.commit}:{source.tree}", profile="verification", target="x86_64-unknown-linux-musl", image=IMAGE, allocator="mimalloc", allocation="1cpu/1GiB")
    return directory, context


def baseline_products():
    import acceptance
    directory = original_root(ROOT) / f".work/consolidation/final-performance-gates/profile-cost-{PRODUCT_SOURCE}/opt3/target/x86_64-unknown-linux-musl/release"
    context = acceptance.BuildContext(source=PRODUCT_SOURCE, profile="release", target="x86_64-unknown-linux-musl", image=IMAGE, allocator="mimalloc", allocation="1cpu/1GiB")
    return verify_product_hashes(directory, PRODUCT_HASHES), context


def measure_resources():
    import release
    import resources
    runtime = sys.modules[__name__]
    return release.with_release_products(runtime, source_identity(), lambda: resources.measure(runtime))


def measure_timing(source):
    import resources
    require_cargo_lease()
    import release
    runtime = sys.modules[__name__]
    return release.with_release_products(runtime, source, lambda: resources.measure_timing(runtime, source))


def run_full_native(source):
    import resources
    return resources.run_full_native(sys.modules[__name__], source)


def run_host_boundaries(source):
    import resources
    return resources.run_host_boundaries(sys.modules[__name__], source)


def run_repository_contract(source):
    import resources
    return resources.run_repository_contract(sys.modules[__name__], source)


def run_docs(source):
    import resources
    return resources.run_docs(sys.modules[__name__], source)


_review_result = contextvars.ContextVar("util_linux_independent_review", default=None)
_review_scope_active = contextvars.ContextVar("util_linux_review_scope_active", default=False)


@contextlib.contextmanager
def independent_review(result):
    import acceptance
    if _review_scope_active.get():
        raise ValueError("independent review scopes cannot be nested")
    if not isinstance(result, acceptance.GateResult) or result.name != "adversarial-review":
        raise ValueError("independent review requires an actual adversarial-review gate result")
    if (result.status != 0 or result.failed or result.skipped or result.accounted_skips
            or result.executed <= 0 or result.passed != result.executed or not result.selectors):
        raise ValueError("independent review did not pass every reviewed item")
    if not result.source.clean or source_identity() != result.source:
        raise ValueError("independent review does not match the current clean source")
    token = _review_result.set(result)
    scope_token = _review_scope_active.set(True)
    try:
        yield
    finally:
        _review_result.reset(token)
        _review_scope_active.reset(scope_token)


def adversarial_review(source):
    result = _review_result.get()
    if result is None:
        raise ValueError("close requires an independent review for this invocation")
    if result.source != source or source_identity() != source or not source.clean:
        raise ValueError("independent review does not match the current clean source")
    _review_result.set(None)
    return result


def owned_cleanup(source):
    import acceptance
    require_native_lease()
    if source_identity() != source or not source.clean:
        raise ValueError("cleanup requires the current clean source")
    containers = subprocess.check_output(["docker", "ps", "--all", "--format", "{{.Names}}"], timeout=10)
    owned_containers = tuple(name for name in containers.splitlines() if name.startswith(b"xsh-util-linux-"))
    if owned_containers:
        raise ValueError(f"owned containers remain: {owned_containers!r}")
    processes = subprocess.check_output(["ps", "-eo", "pid=,comm=,args="], text=True, errors="surrogateescape", timeout=10)
    if not processes.strip():
        raise ValueError("process inventory is empty during cleanup")
    for line in processes.splitlines():
        fields = line.split(None, 2)
        if len(fields) != 3:
            raise ValueError(f"cannot classify process during cleanup: {line}")
        pid, command, arguments = fields
        if int(pid) == os.getpid():
            continue
        executable = Path(command).name
        if executable in {"xsh", "xsht", "xsh-test-helper", "cargo", "rustc", "python3", "docker"} and (
            "/.work/wt/util-linux-" in arguments or "/core/tests/fixtures/util-linux/" in arguments
        ):
            raise ValueError(f"owned process remains: {line}")
    infra = original_root(ROOT) / ".work/consolidation/infra"
    for name in ("final-performance.lock", "native-suite.lock", "cargo.lock"):
        path = infra / name
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
        try:
            unresolved = os.pread(descriptor, 19, 0) == b"cleanup-unresolved:"
        finally:
            os.close(descriptor)
        marker = path.with_suffix(".cleanup-unresolved")
        if unresolved or marker.exists() or marker.is_symlink():
            raise ValueError(f"unresolved cleanup remains: {path}")
    listing = subprocess.check_output(["git", "worktree", "list", "--porcelain", "-z"], cwd=ROOT)
    worktrees = tuple(Path(os.fsdecode(field.removeprefix(b"worktree ")))
                      for field in listing.split(b"\0") if field.startswith(b"worktree "))
    if ROOT not in worktrees:
        raise ValueError("worktree inventory omitted the current source")
    for worktree in worktrees:
        if not worktree.name.startswith("util-linux-"):
            continue
        changes = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=normal"], cwd=worktree)
        if changes:
            raise ValueError(f"uncommitted campaign work remains: {worktree}")
    if source_identity() != source:
        raise ValueError("source changed during cleanup inspection")
    observations = ("remaining-owned-processes:0", "unresolved-markers:0", "lost-work:false")
    return acceptance.GateResult("cleanup", source, 0, len(observations), len(observations), 0, (), (), observations, ())


def close_gates():
    import release
    import resources
    runtime = sys.modules[__name__]
    return release.with_release_products(runtime, source_identity(), lambda: resources.close(runtime))


PRIVATE_NAMESPACE = r'''
export XSH_UTIL_LINUX_PARENT_ipc=$(readlink /proc/self/ns/ipc)
export XSH_UTIL_LINUX_PARENT_mnt=$(readlink /proc/self/ns/mnt)
export XSH_UTIL_LINUX_PARENT_pid=$(readlink /proc/self/ns/pid)
exec /usr/bin/python3 /work/core/tests/fixtures/util-linux/contexts/identity_keeper.py outer -- unshare --mount --ipc --pid --fork --kill-child=KILL --mount-proc sh -ec '
  mount --make-rprivate /
  mount -t tmpfs -o size=32m tmpfs /dev/shm
  mount -t mqueue mqueue /dev/mqueue
  for ns in ipc mnt pid; do
    eval "parent=\$XSH_UTIL_LINUX_PARENT_$ns"
    test "$(readlink /proc/self/ns/$ns)" != "$parent"
  done
  export XSH_UTIL_LINUX_PRIVATE=1
  exec "$@"
' private
'''


def _execution_products():
    import release
    active = release.active_products(sys.modules[__name__])
    return active[0] if active is not None else protected_products()


@contextlib.contextmanager
def _qualified_products():
    products = _execution_products()
    failure = None
    try:
        yield products
    except BaseException as error:
        failure = error
        raise
    finally:
        try:
            if _execution_products() != products:
                raise ValueError("execution product provider changed during native dispatch")
        except BaseException as error:
            if failure is None:
                raise
            failure.add_note(f"product verification also failed: {error}")


def native_source(paths: list[str]) -> bytes:
    if not paths or len(paths) > 8:
        raise ValueError("source checks require one to eight explicit XSH files")
    for name in paths:
        if not re.fullmatch(r"(?:core|tests/xsh|dev)/[A-Za-z0-9_./-]+\.xsh", name) or ".." in Path(name).parts or not (ROOT / name).is_file():
            raise ValueError(f"invalid source check path: {name}")
    with _qualified_products() as products:
        output = container("cd /work; " + shlex.join(["/products/xsht", "check", *paths]), mounts=[
            f"type=bind,source={ROOT},target=/work,readonly",
            f"type=bind,source={products},target=/products,readonly",
        ], timeout=30)
    print(output.decode("utf-8"), end="")
    print("source check passed: " + " ".join(paths))
    return output


def native_case(selector: str, *, private: bool = False, timeout: float = 30) -> bytes:
    if not 0 < timeout <= 30:
        raise ValueError("exact native execution requires a positive timeout at most 30 seconds")
    if not re.fullmatch(r"(?:core/tests/test-[a-z0-9-]+|tests/xsh/(?:[a-z0-9_-]+/)*[a-z0-9_-]+)\.xsh::test_[a-zA-Z0-9_]+", selector):
        raise ValueError(f"invalid exact native selector: {selector}")
    with _qualified_products() as products:
        command = f"/products/xsht test --exact {selector}"
        script = "cd /work; " + ((PRIVATE_NAMESPACE.rstrip() + " " + command) if private else command)
        output = container(script, mounts=[
            f"type=bind,source={ROOT},target=/work,readonly",
            f"type=bind,source={products},target=/products,readonly",
        ], capabilities=["SYS_ADMIN"] if private else [], timeout=timeout)
    text = output.decode("utf-8")
    summaries = re.findall(r"^test result: ok\. ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) skipped$", text, re.MULTILINE)
    cases = re.findall(r"^(\S+) \.\.\. ok \S+$", text, re.MULTILINE)
    if summaries != [("1", "0", "0")] or cases != [selector]:
        raise ValueError(f"native case did not execute exactly one passing behavior: {text}")
    print(text, end="")
    return output


def native_batch(selectors):
    from batches import run_exact_batch
    known = tuple(selector for selector in regression_selectors(source_identity())
                  if not selector.endswith("test_util_linux_lsns_selects_and_releases_a_live_user_namespace"))
    with _qualified_products() as products:
        summaries = run_exact_batch(selectors, known_selectors=known, root=ROOT,
                                    products=products, container=container)
    for summary in summaries:
        print(f"{summary.selector}: {summary.passed} passed; {summary.failed} failed; {summary.skipped} skipped")
    return summaries


@contextlib.contextmanager
def _frozen_source():
    source = source_identity()
    if not source.clean:
        raise ValueError("private batch requires a clean frozen source")
    failure = None
    try:
        yield source
    except BaseException as error:
        failure = error
        raise
    finally:
        try:
            if source_identity() != source:
                raise ValueError("source changed during private batch")
        except BaseException as error:
            if failure is None:
                raise
            failure.add_note(f"source verification also failed: {error}")


def native_private_batch(selectors):
    import acceptance
    from batches import run_private_exact_batch
    with _frozen_source(), _qualified_products() as products:
        known = tuple(acceptance._case_selector(command, name)
                      for command in COHORT for name in acceptance._case_names(ROOT, command))
        summaries = run_private_exact_batch(selectors, known_selectors=known, root=ROOT,
                                            products=products, private_namespace=PRIVATE_NAMESPACE,
                                            container=container)
    for summary in summaries:
        print(f"{summary.selector}: {summary.passed} passed; {summary.failed} failed; {summary.skipped} skipped")
    return summaries


def harness() -> None:
    from controls import term_during_container, writable_target_bind
    subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", "dev/util-linux", "-p", "test_*.py"],
                   cwd=ROOT, check=True, timeout=30,
                   env={**os.environ, "TMPDIR": str(ROOT / ".work/tmp"), "PYTHONDONTWRITEBYTECODE": "1"})
    term_during_container(ROOT)
    writable_target_bind(ROOT)
    for name in ("comparison_rejects_observable_mutants", "reference_preserves_raw_path_arguments"):
        native_case("core/tests/test-util-linux-support.xsh::test_util_linux_" + name)
    native_case("core/tests/test-util-linux-support.xsh::test_util_linux_observation_distinguishes_signal_from_shell_exit")
    native_case("core/tests/test-util-linux-support.xsh::test_util_linux_private_namespace_and_unprivileged_identity", private=True)


BASELINE_SCRIPT = r'''
cd /work
python3 - <<'PY'
import subprocess
for command in ("lslocks", "lsipc", "lsmem", "mountpoint", "namei"):
    reference = subprocess.run([command, "--version"], capture_output=True, timeout=5)
    candidate = subprocess.run(["/usr/bin/time", "-f", "%M", "-o", "/t/rss", "/products/xsh", f"/work/core/{command}.xsh", "--", "--version"], capture_output=True, timeout=5)
    with open("/t/rss") as resource:
        rss = resource.read().splitlines()[-1]
    print("\t".join([command, str(reference.returncode), reference.stdout.hex(), reference.stderr.hex(), str(candidate.returncode), candidate.stdout.hex(), candidate.stderr.hex(), rss]))
PY
'''


def baseline() -> None:
    with _qualified_products() as products:
        output = container(BASELINE_SCRIPT, mounts=[
            f"type=bind,source={ROOT},target=/work,readonly",
            f"type=bind,source={products},target=/products,readonly",
        ])
    rows = output.decode("ascii").splitlines()
    if len(rows) != 5:
        raise ValueError("baseline did not execute five command pairs")
    for expected, line in zip(COHORT, rows):
        command, ref_status, ref_out, ref_err, status, stdout, stderr, rss = line.split("\t")
        if command != expected or ref_status != "0" or bytes.fromhex(ref_err) or bytes.fromhex(ref_out) != f"{command} from util-linux 2.42.4\n".encode():
            raise ValueError(f"unqualified baseline reference: {command}")
        if not rss.isdecimal() or int(rss) <= 0:
            raise ValueError(f"missing peak RSS sample: {command}")
        source = ROOT / f"core/{command}.xsh"
        if source.is_file():
            if status != "0" or bytes.fromhex(stderr) or bytes.fromhex(stdout) != f"{command} (XSH core) 0.0.1\n".encode():
                raise ValueError(f"candidate version baseline differs: {command}")
            disposition = "implemented; ordinary behaviors require parity selectors"
        else:
            diagnostic = bytes.fromhex(stderr)
            if status != "2" or bytes.fromhex(stdout) or f"core/{command}.xsh".encode() not in diagnostic:
                raise ValueError(f"unaccounted missing-command baseline: {command}: {status}: {diagnostic!r}")
            disposition = "absent; ordinary behaviors remain open"
        print(f"{command}: {disposition}; reference=0 candidate={status} peak_rss_kib={rss}")
    print("five command pairs executed within the 30-second selector bound; timing gates remain separate")


def parity(args) -> None:
    import acceptance
    acceptance.parity(args, sys.modules[__name__])


def main() -> int:
    parser = argparse.ArgumentParser()
    modes = parser.add_subparsers(dest="mode", required=True)
    modes.add_parser("inventory").add_argument("--check", action="store_true", required=True)
    modes.add_parser("harness").add_argument("--self-test", action="store_true", required=True)
    for mode in ("baseline", "resources", "close"):
        modes.add_parser(mode).add_argument("--check", action="store_true", required=True)
    source_parser = modes.add_parser("source")
    source_parser.add_argument("--check", action="store_true", required=True)
    source_parser.add_argument("paths", nargs="+")
    parity_parser = modes.add_parser("parity")
    selectors = parity_parser.add_mutually_exclusive_group(required=True)
    selectors.add_argument("--command", choices=COHORT)
    selectors.add_argument("--commands")
    selectors.add_argument("--all", action="store_true")
    parity_parser.add_argument("--case")
    parity_parser.add_argument("--require-positive-boundaries", action="store_true")
    args = parser.parse_args()
    try:
        with cancellation_scope(), campaign_leases(ROOT, native=True, cargo=args.mode in {"resources", "close"}):
            if args.mode == "inventory":
                inventory()
            elif args.mode == "harness":
                harness()
            elif args.mode == "baseline":
                baseline()
            elif args.mode == "source":
                native_source(args.paths)
            elif args.mode == "parity":
                parity(args)
            else:
                import acceptance
                getattr(acceptance, args.mode)(sys.modules[__name__])
        return 0
    except Cancelled as error:
        print(str(error), file=sys.stderr)
        for note in getattr(error, "__notes__", ()):
            print(note, file=sys.stderr)
        return 128 + error.signum
    except LeaseBusy as error:
        print(str(error), file=sys.stderr)
        return 75
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        for note in getattr(error, "__notes__", ()):
            print(note, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
