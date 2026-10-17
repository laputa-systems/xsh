#!/usr/bin/env python3
from __future__ import annotations

import argparse
import contextlib
import hashlib
import os
from pathlib import Path
import re
import signal
import shutil
import subprocess
import sys
import tempfile
import uuid

sys.dont_write_bytecode = True

from leases import campaign_leases, LeaseBusy, mark_cleanup_unresolved, original_root, require_native_lease


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


def protected_products() -> Path:
    inputs = ["src", "crates", "Cargo.toml", "Cargo.lock", "build.rs", "tests/helpers", "rust-toolchain.toml", ".cargo"]
    changed = subprocess.check_output(["git", "diff", "--name-only", PRODUCT_SOURCE, "--", *inputs], cwd=ROOT)
    untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard", "--", *inputs], cwd=ROOT)
    if changed or untracked:
        raise ValueError("protected products cannot execute changed Rust inputs")
    directory = original_root(ROOT) / f".work/consolidation/final-performance-gates/profile-cost-{PRODUCT_SOURCE}/opt3/target/x86_64-unknown-linux-musl/release"
    for name, expected in PRODUCT_HASHES.items():
        executable = directory / name
        if executable.is_symlink() or not executable.is_file():
            raise ValueError(f"unsafe protected executable: {executable}")
        with executable.open("rb") as stream:
            actual = hashlib.file_digest(stream, "sha256").hexdigest()
        if actual != expected:
            raise ValueError(f"protected product changed: {name}")
    return directory


PRIVATE_NAMESPACE = r'''
export XSH_UTIL_LINUX_PARENT_ipc=$(readlink /proc/self/ns/ipc)
export XSH_UTIL_LINUX_PARENT_mnt=$(readlink /proc/self/ns/mnt)
export XSH_UTIL_LINUX_PARENT_pid=$(readlink /proc/self/ns/pid)
exec /usr/bin/python3 /work/core/tests/fixtures/util-linux/contexts/identity_keeper.py outer -- unshare --mount --ipc --pid --fork --mount-proc sh -ec '
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


def native_case(selector: str, *, private: bool = False) -> bytes:
    if not re.fullmatch(r"core/tests/test-[a-z0-9-]+\.xsh::test_[a-zA-Z0-9_]+", selector):
        raise ValueError(f"invalid exact native selector: {selector}")
    products = protected_products()
    command = f"/products/xsht test --exact {selector}"
    script = "cd /work; " + ((PRIVATE_NAMESPACE.rstrip() + " " + command) if private else command)
    output = container(script, mounts=[
        f"type=bind,source={ROOT},target=/work,readonly",
        f"type=bind,source={products},target=/products,readonly",
    ], capabilities=["SYS_ADMIN"] if private else [])
    text = output.decode("utf-8")
    protected_products()
    summaries = re.findall(r"^test result: ok\. ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) skipped$", text, re.MULTILINE)
    if summaries != [("1", "0", "0")]:
        raise ValueError(f"native case did not execute exactly one passing behavior: {text}")
    print(text, end="")
    return output


def harness() -> None:
    from controls import term_during_container
    subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", "dev/util-linux", "-p", "test_*.py"],
                   cwd=ROOT, check=True, timeout=30,
                   env={**os.environ, "TMPDIR": str(ROOT / ".work/tmp"), "PYTHONDONTWRITEBYTECODE": "1"})
    term_during_container(ROOT)
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
    products = protected_products()
    output = container(BASELINE_SCRIPT, mounts=[
        f"type=bind,source={ROOT},target=/work,readonly",
        f"type=bind,source={products},target=/products,readonly",
    ])
    protected_products()
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
    parity_parser = modes.add_parser("parity")
    selectors = parity_parser.add_mutually_exclusive_group(required=True)
    selectors.add_argument("--command", choices=COHORT)
    selectors.add_argument("--commands")
    selectors.add_argument("--all", action="store_true")
    parity_parser.add_argument("--case")
    parity_parser.add_argument("--require-positive-boundaries", action="store_true")
    args = parser.parse_args()
    try:
        if args.mode not in {"inventory", "harness", "baseline", "parity"}:
            raise ValueError(f"{args.mode}: acceptance is not implemented; no result can pass")
        with cancellation_scope(), campaign_leases(ROOT, native=True):
            if args.mode == "inventory":
                inventory()
            elif args.mode == "harness":
                harness()
            elif args.mode == "baseline":
                baseline()
            else:
                parity(args)
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
