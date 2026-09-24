"""Compare benchmark behavior and persistent filesystem effects.

The timed runner imports compare_workload and stores this evidence next to its
samples. Standalone runs use the same workload manifest and can write JSON.
"""

import argparse
import base64
import hashlib
import io
import json
import os
import re
import stat
import subprocess
import sys
import tarfile
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../.."))
SELF_TIMED = {
    "linux_uptime",
    "linux_meminfo",
    "linux_memory",
    "linux_modules_full",
    "linux_modules_partial",
    "linux_os_release",
}
SELF_TIMED_FIELD = re.compile(rb"\b\d+ ms\b")


def snapshot_paths(roots):
    """Fingerprint persistent files in the workload's writable source areas."""
    result = {}
    for root in roots:
        for directory, dirs, files in os.walk(root, followlinks=False):
            for name in dirs + files:
                path = os.path.join(directory, name)
                relative = os.path.relpath(path, ROOT)
                info = os.lstat(path)
                mode = stat.S_IMODE(info.st_mode)
                if stat.S_ISLNK(info.st_mode):
                    result[relative] = ["link", mode, os.readlink(path)]
                elif stat.S_ISDIR(info.st_mode):
                    result[relative] = ["directory", mode]
                elif stat.S_ISREG(info.st_mode):
                    result[relative] = ["file", mode, sha256_file(path)]
                else:
                    result[relative] = ["special", mode]
    return result


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def changed_paths(before, after):
    return {
        path: {"before": before.get(path), "after": after.get(path)}
        for path in sorted(before.keys() | after.keys())
        if before.get(path) != after.get(path)
    }


def run_host(binary, script):
    roots = [HERE, os.path.join(ROOT, "core")]
    before = snapshot_paths(roots)
    completed = subprocess.run(
        [binary, script],
        cwd=HERE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    effects = changed_paths(before, snapshot_paths(roots))
    return completed.returncode, completed.stdout, completed.stderr, effects


def docker(args, check=True):
    completed = subprocess.run(
        ["docker", *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    if check and completed.returncode:
        raise RuntimeError(
            f"docker {args[0]} failed: {completed.stderr.decode(errors='replace')[-1000:]}"
        )
    return completed


def tar_fingerprint(data):
    entries = []
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:*") as archive:
        for member in archive:
            digest = None
            if member.isfile():
                contents = archive.extractfile(member)
                digest = hashlib.sha256(contents.read()).hexdigest()
            entries.append(
                [member.name, member.type.decode("ascii"), member.mode, member.linkname, digest]
            )
    return hashlib.sha256(
        json.dumps(entries, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def container_effects(container):
    lines = docker(["diff", container]).stdout.decode().splitlines()
    changes = {}
    for line in lines:
        action, path = line.split(" ", 1)
        if path == "/work" or path.startswith("/work/"):
            continue  # The repository is mounted read-only.
        if path == "/bench" or path.startswith("/bench/"):
            continue  # The executable is mounted read-only.
        changes[path] = action
    effects = {}
    for path, action in sorted(changes.items()):
        if any(other.startswith(path + "/") for other in changes):
            continue
        digest = None
        if action != "D":
            digest = tar_fingerprint(docker(["cp", f"{container}:{path}", "-"]).stdout)
        effects[path] = {"action": action, "fingerprint": digest}
    return effects


def run_linux(binary, script, image_id, fixtures):
    shell = (
        "mount --bind /work/bench/stdlib-port/fixtures/modules-200.txt /proc/modules "
        "&& mount --bind /work/bench/stdlib-port/fixtures/meminfo.txt /proc/meminfo "
        "&& mount --bind /work/bench/stdlib-port/fixtures/uptime.txt /proc/uptime "
        '&& test "$(wc -l </proc/modules)" -eq 200 '
        '&& test "$(sha256sum /proc/modules | cut -d " " -f 1)" = "$1" '
        '&& test "$(sha256sum /proc/meminfo | cut -d " " -f 1)" = "$2" '
        '&& test "$(sha256sum /proc/uptime | cut -d " " -f 1)" = "$3" '
        '&& exec /bench/xsh "$4"'
    )
    command = [
        "create", "--platform", "linux/arm64", "--privileged",
        "-v", f"{ROOT}:/work:ro",
        "-v", f"{binary}:/bench/xsh:ro",
        "-w", "/work/bench/stdlib-port",
        "-e", "TARGET=aarch64-unknown-linux-musl",
        "-e", "XSH_LINUX_REAL=1",
        image_id, "sh", "-c", shell, "sh",
        fixtures["fixtures/modules-200.txt"],
        fixtures["fixtures/meminfo.txt"],
        fixtures["fixtures/uptime.txt"],
        f"/work/bench/stdlib-port/{os.path.basename(script)}",
    ]
    container = docker(command).stdout.decode().strip()
    if not re.fullmatch(r"[0-9a-f]{64}", container):
        raise RuntimeError(f"invalid Docker container ID: {container!r}")
    try:
        output = docker(["start", "--attach", container], check=False)
        state = docker(
            ["inspect", "--format", "{{.State.Status}} {{.State.ExitCode}}", container]
        ).stdout.decode().strip().split()
        if len(state) != 2 or state[0] != "exited":
            raise RuntimeError(f"Docker benchmark container did not exit: {state!r}")
        return int(state[1]), output.stdout, output.stderr, container_effects(container)
    finally:
        docker(["rm", "-f", container])


def encode_run(name, run):
    status, stdout, stderr, effects = run
    normalized = SELF_TIMED_FIELD.sub(b"<ms>", stdout) if name in SELF_TIMED else stdout
    return {
        "status": status,
        "stdout_base64": base64.b64encode(stdout).decode("ascii"),
        "normalized_stdout_base64": base64.b64encode(normalized).decode("ascii"),
        "stderr_base64": base64.b64encode(stderr).decode("ascii"),
        "side_effects": effects,
    }


def compare_workload(name, reference, candidate, expected_status=0, linux=None):
    """Compare one frozen script before timing, with explicit effect scope."""
    script = os.path.join(HERE, name + ".xsh")
    if not os.path.isfile(script):
        raise FileNotFoundError(script)
    if linux is None:
        reference_run = run_host(reference, script)
        candidate_run = run_host(candidate, script)
        scope = "benchmark directory and core tree"
    else:
        image_id, fixtures = linux
        reference_run = run_linux(reference, script, image_id, fixtures)
        candidate_run = run_linux(candidate, script, image_id, fixtures)
        scope = "container writable layer outside read-only worktree and executable mounts"
    left = encode_run(name, reference_run)
    right = encode_run(name, candidate_run)
    matches = {
        "status": left["status"] == right["status"] == expected_status,
        "stdout": left["normalized_stdout_base64"] == right["normalized_stdout_base64"],
        "stderr": left["stderr_base64"] == right["stderr_base64"],
        "side_effects": left["side_effects"] == right["side_effects"],
    }
    return {
        "expected_status": expected_status,
        "effect_scope": scope,
        "reference": left,
        "candidate": right,
        "matches": matches,
        "passed": all(matches.values()),
    }


def self_test():
    with tempfile.TemporaryDirectory(prefix="xsh-parity-") as directory:
        reference = os.path.join(directory, "reference")
        candidate = os.path.join(directory, "candidate")
        for path, word in [(reference, "same"), (candidate, "same")]:
            with open(path, "w", encoding="ascii") as handle:
                handle.write(f"#!/bin/sh\nprintf '{word}\\n'\nprintf 'warn\\n' >&2\nexit 7\n")
            os.chmod(path, 0o755)
        equal = compare_workload("cold_trivial", reference, candidate, 7)
        assert equal["passed"] and equal["reference"]["status"] == 7
        with open(candidate, "w", encoding="ascii") as handle:
            handle.write("#!/bin/sh\nprintf 'different\\n'\nprintf 'warn\\n' >&2\nexit 7\n")
        different = compare_workload("cold_trivial", reference, candidate, 7)
        assert not different["passed"] and not different["matches"]["stdout"]
        before = snapshot_paths([directory])
        with open(os.path.join(directory, "new-file"), "w", encoding="ascii") as handle:
            handle.write("side effect")
        assert changed_paths(before, snapshot_paths([directory]))
    first = encode_run("linux_meminfo", (0, b"meminfo 1 ms bytes=5\n", b"", {}))
    second = encode_run("linux_meminfo", (0, b"meminfo 2 ms bytes=5\n", b"", {}))
    assert first["stdout_base64"] != second["stdout_base64"]
    assert first["normalized_stdout_base64"] == second["normalized_stdout_base64"]
    print("parity self-test: expected nonzero and output mismatch detected")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    from run import WORKLOADS, ensure_fixtures, expected_status

    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--linux", action="store_true")
    parser.add_argument("--out")
    args = parser.parse_args()
    if not os.path.isabs(args.reference) or not os.path.isabs(args.candidate):
        parser.error("binary paths must be absolute")
    fixtures = ensure_fixtures()
    linux = None
    if args.linux:
        image = os.environ.get("XSH_TEST_IMAGE", "xsh-test")
        image_id = docker(["image", "inspect", "--format", "{{.Id}}", image]).stdout.decode().strip()
        linux = (image_id, fixtures)
    results = {}
    skipped = []
    for name, _, _, linux_only in WORKLOADS:
        if linux_only and not args.linux:
            skipped.append(name)
            continue
        result = compare_workload(name, args.reference, args.candidate, expected_status(name), linux)
        results[name] = result
        print(f"{name:22} {'identical' if result['passed'] else 'DIFFERS'}")
    if args.out:
        with open(args.out, "w") as handle:
            json.dump({"workloads": results, "skipped": skipped}, handle, indent=2, sort_keys=True)
    failed = [name for name, result in results.items() if not result["passed"]]
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
