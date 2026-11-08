from importlib.util import module_from_spec, spec_from_file_location
import fcntl
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]


def load_module(name, path):
    spec = spec_from_file_location(name, path)
    module = module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


sys.path.insert(0, str(HERE.parent / "final-performance-gates"))
compare = load_module("writer_compare", HERE / "compare.py")
owner = load_module("writer_owner", ROOT / "dev/performance/check.py")


def test_container_writer_preserves_status_and_parseable_snapshots():
    scratch = ROOT / ".work/tmp"
    scratch.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="scheduler-writer-", dir=scratch))
    common = Path(subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "--path-format=absolute", "--git-common-dir"], text=True).strip()).parent
    with (common / ".work/consolidation/infra/native-suite.lock").open("a") as native, (common / ".work/consolidation/infra/cargo.lock").open("a") as cargo:
        fcntl.flock(native, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(cargo, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for status in (0, 19):
            case = output / f"status-{status}"
            binaries = case / "bin"
            results = case / "results"
            binaries.mkdir(parents=True)
            results.mkdir()
            xsht = binaries / "xsht"
            xsht.write_text(f"#!/bin/sh\nprintf 'scheduler child status {status}\\n'\nexit {status}\n")
            xsht.chmod(0o755)
            command = [
                "docker", "run", "--rm", "--init", "--platform=linux/amd64", "--network=none",
                "--cpus=2", "--memory=4g", "--memory-swap=4g", "--user", f"{os.getuid()}:{os.getgid()}",
                "-v", f"{HERE / 'container-run.sh'}:/container-run.sh:ro", "-v", f"{binaries}:/opt/reference:ro",
                "-v", f"{results}:/results:rw", "-e", "HOME=/results", "-e", "XSH_BIN=/opt/reference/xsh",
                "-e", "XSH_MODULE_PATH=/work", compare.PINNED_IMAGE.removeprefix("xsh-test@"),
                "/bin/sh", "/container-run.sh", "list", "reference", "writer",
            ]
            observed, stdout, stderr = owner.checked_docker(command, case, timeout_seconds=20)
            (case / "stdout").write_bytes(stdout)
            (case / "stderr").write_bytes(stderr)
            (case / "status").write_text(f"{observed}\n")
            assert observed == status, (status, observed, stdout, stderr)
            assert (results / "writer.exit-code").read_text() == f"{status}\n"
            assert (results / "writer.log").read_text() == f"scheduler child status {status}\n"
    print(f"actual writer outputs: {output}", flush=True)
    for status in (0, 19):
        results = output / f"status-{status}/results"
        for boundary in ("before", "after"):
            compare.parse_cgroup_snapshot((results / f"writer.cgroup-{boundary}.txt").read_text())
    print("actual scheduler writer preserves zero/nonzero status and parseable before/after cgroups")


def test_container_writer_starts_xsht_with_interrupt_and_quit_at_their_defaults():
    scratch = ROOT / ".work/tmp"
    scratch.mkdir(parents=True, exist_ok=True)
    case = Path(tempfile.mkdtemp(prefix="scheduler-signals-", dir=scratch))
    binaries = case / "bin"
    results = case / "results"
    binaries.mkdir()
    results.mkdir()
    xsht = binaries / "xsht"
    xsht.write_text("#!/bin/sh\nexec awk '/^SigIgn:/ { print $2 }' /proc/self/status\n")
    xsht.chmod(0o755)
    command = [
        "docker", "run", "--rm", "--init", "--platform=linux/amd64", "--network=none",
        "--cpus=2", "--memory=4g", "--memory-swap=4g", "--user", f"{os.getuid()}:{os.getgid()}",
        "-v", f"{HERE / 'container-run.sh'}:/container-run.sh:ro", "-v", f"{binaries}:/opt/reference:ro",
        "-v", f"{results}:/results:rw", "-e", "HOME=/results", "-e", "XSH_BIN=/opt/reference/xsh",
        "-e", "XSH_MODULE_PATH=/work", compare.PINNED_IMAGE.removeprefix("xsh-test@"),
        "/bin/sh", "/container-run.sh", "suite", "reference", "signals",
    ]
    observed, stdout, stderr = owner.checked_docker(command, case, timeout_seconds=20)
    assert observed == 0, (observed, stdout, stderr)
    ignored = int((results / "signals.log").read_text().strip(), 16)
    for signal in (2, 3):
        assert not ignored >> (signal - 1) & 1, f"xsht starts with signal {signal} ignored: {ignored:#x}"
    print("actual scheduler writer starts xsht with SIGINT and SIGQUIT at their defaults")


if __name__ == "__main__":
    test_container_writer_preserves_status_and_parseable_snapshots()
    test_container_writer_starts_xsht_with_interrupt_and_quit_at_their_defaults()
