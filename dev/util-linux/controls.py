from pathlib import Path
import os
import select
import signal
import subprocess
import sys
import tempfile
import uuid

from leases import _native_descriptor, mark_cleanup_unresolved


def term_during_container(root: Path) -> None:
    held = _native_descriptor.get()
    if held is None:
        raise RuntimeError("TERM control requires the native lease")
    descriptor, lock_path = held
    token = uuid.uuid4().hex
    name = "xsh-util-linux-" + token
    child_source = '''
import pathlib, sys, types
import check, leases
leases._native_descriptor.set((int(sys.argv[1]), pathlib.Path(sys.argv[2])))
check.uuid.uuid4 = lambda: types.SimpleNamespace(hex=sys.argv[4])
try:
    check.container("printf ready >/probe/ready; cat /probe/release", mounts=[f"type=bind,source={sys.argv[3]},target=/probe"])
except check.Cancelled as error:
    for note in getattr(error, "__notes__", ()):
        print(note, file=sys.stderr)
    raise SystemExit(128 + error.signum)
'''
    with tempfile.TemporaryDirectory(dir=root / ".work/tmp") as directory:
        scratch = Path(directory)
        os.mkfifo(scratch / "ready", 0o600)
        os.mkfifo(scratch / "release", 0o600)
        ready = os.open(scratch / "ready", os.O_RDONLY | os.O_NONBLOCK)
        worker = subprocess.Popen([sys.executable, "-c", child_source, str(descriptor), str(lock_path), directory, token],
                                  cwd=root / "dev/util-linux", pass_fds=(descriptor,), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            if not select.select([ready], [], [], 10)[0] or os.read(ready, 5) != b"ready":
                raise RuntimeError("TERM control container did not reach its readiness barrier")
            worker.send_signal(signal.SIGTERM)
            stdout, stderr = worker.communicate(timeout=15)
            if worker.returncode != 143 or stderr:
                raise RuntimeError(f"TERM control failed: status={worker.returncode}, stdout={stdout!r}, stderr={stderr!r}")
            inspect = subprocess.run(["docker", "container", "inspect", name], capture_output=True, timeout=5)
            if inspect.returncode == 0 or b"No such container" not in inspect.stderr:
                raise RuntimeError(f"TERM control left an owned container: {name}: {inspect.stderr!r}")
            print("TERM control: owned container removed before guard release")
        finally:
            os.close(ready)
            if worker.poll() is None:
                worker.kill()
                worker.wait(timeout=5)
            cleanup = subprocess.run(["docker", "rm", "--force", name], capture_output=True, timeout=10)
            if cleanup.returncode and b"No such container" not in cleanup.stderr:
                mark_cleanup_unresolved(f"TERM-control container={name} cause={cleanup.stderr!r}")
                raise RuntimeError(f"TERM control cleanup unresolved: {name}")


def writable_target_bind(root: Path) -> None:
    if _native_descriptor.get() is None:
        raise RuntimeError("writable target control requires the native lease")
    import check
    import release

    value = b"xsh-writable-target"
    with tempfile.TemporaryDirectory(dir=root / ".work/tmp", prefix="writable-target-") as directory:
        target = Path(directory)
        output = check.container(
            "printf xsh-writable-target >/cargo-target/probe; cat /cargo-target/probe",
            mounts=[f"type=bind,source={target},target=/cargo-target"],
            timeout=30,
        )
        marker = target / "probe"
        if output != value or marker.read_bytes() != value:
            raise RuntimeError("writable target bind did not preserve container writes on the host")
        release._remove_target(check, target)
        if any(target.iterdir()):
            raise RuntimeError("release target cleanup left the writable bind probe")
    print("Writable target bind control: host-visible write and release cleanup passed")
