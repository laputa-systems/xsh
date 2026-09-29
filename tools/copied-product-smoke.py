#!/usr/bin/env python3
"""Exercise copied products and the packaged core archive without source files."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import stat
import subprocess
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parent.parent


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_core_sidecar(archive):
    try:
        relative = archive.relative_to(ROOT)
    except ValueError as error:
        raise RuntimeError("core archive is outside the repository") from error
    if not relative.parts or relative.parts[0] != "dist":
        raise RuntimeError("core archive is outside dist")
    sidecar = archive.with_name(archive.name.removesuffix(".tar.xz") + ".sha256")
    recorded_hash, recorded_path = sidecar.read_text().strip().split("  ", 1)
    if recorded_hash != sha256(archive) or recorded_path != relative.as_posix():
        raise RuntimeError("core archive checksum sidecar disagrees with the artifact")


def validate_core_archive(packaged, expected):
    members = packaged.getmembers()
    if any(not member.isfile() for member in members):
        raise RuntimeError("core archive contains a non-file member")
    actual = {member.name for member in members}
    if len(actual) != len(members):
        raise RuntimeError("core archive contains a duplicate member")
    if actual != set(expected):
        raise RuntimeError(f"core archive source mismatch: {sorted(set(expected) ^ actual)}")
    for member in members:
        source, expected_mode = expected[member.name]
        if (member.mode not in (expected_mode, stat.S_IFREG | expected_mode)
                or packaged.extractfile(member).read() != source.read_bytes()):
            raise RuntimeError(f"packaged core content or executable mode differs: {member.name}")


def expected_core_entries(core):
    expected = {}
    for path in core.rglob("*.xsh"):
        relative = path.relative_to(core)
        if relative.parts[0] == "tests":
            continue
        library = relative.parts[0] == "lib"
        name = f"core/{relative if library else relative.with_suffix('')}"
        expected[name] = (path, 0o644 if library else 0o755)
    return expected


def run(argv, bundle, linux, workdir=None, linux_platform="linux/arm64"):
    workdir = bundle if workdir is None else workdir
    if linux:
        container_workdir = "/bundle" if workdir == bundle else f"/bundle/{workdir.relative_to(bundle)}"
        command = [
            "docker", "run", "--rm", "--platform", linux_platform,
            "-v", f"{bundle}:/bundle:rw", "-w", container_workdir,
            "-e", "HOME=/bundle", "-e", "XSHI_ALLOW_NON_TTY_FOR_TESTS=1",
            "xsh-test", *argv,
        ]
        result = subprocess.run(command, capture_output=True)
    else:
        env = {
            "PATH": "/usr/bin:/bin", "HOME": str(bundle), "TMPDIR": str(bundle),
            "XSHI_ALLOW_NON_TTY_FOR_TESTS": "1",
        }
        result = subprocess.run(argv, cwd=workdir, env=env, capture_output=True)
    if result.returncode:
        raise RuntimeError(f"{argv[0]} exited {result.returncode}: {result.stderr!r}")
    return result.stdout, result.stderr


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--core-archive", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--linux", action="store_true")
    parser.add_argument("--linux-platform", default="linux/arm64")
    args = parser.parse_args()
    products = {name: (args.bin_dir / name).resolve() for name in ("xsh", "xshi", "xsht")}
    archive = args.core_archive.resolve()
    archive_hash = sha256(archive)
    validate_core_sidecar(archive)

    expected = expected_core_entries(ROOT / "core")
    with tempfile.TemporaryDirectory(prefix="xsh-copied-products-") as directory:
        bundle = Path(directory)
        for name, source in products.items():
            shutil.copy2(source, bundle / name)
        with tarfile.open(archive, "r:xz") as packaged:
            validate_core_archive(packaged, expected)
            packaged.extractall(bundle, filter="data")
        (bundle / "payload.txt").write_text("portable\n")
        (bundle / "smoke.xsh").write_text(
            'proc main() [io] { print shlex.quote("a b") }\n'
        )
        unrelated_workdir = bundle / "unrelated-workdir"
        unrelated_workdir.mkdir()
        binary = {name: f"/bundle/{name}" if args.linux else str(bundle / name)
                  for name in products}
        packaged_report = "/bundle/core/system-report" if args.linux else str(bundle / "core/system-report")
        commands = {
            "xsh": [binary["xsh"], "smoke.xsh"],
            "xshi": [binary["xshi"], "--no-config", "-c", 'print "ok"'],
            "xsht_check": [binary["xsht"], "check", "smoke.xsh"],
            "xsht_api": [binary["xsht"], "api", "summary", "--format", "jsonl"],
            "core_module_check": [binary["xsht"], "check", "core/getty"],
            "core_ls": [binary["xsh"], "core/ls", "payload.txt"],
            "core_cat": [binary["xsh"], "core/cat", "payload.txt"],
            "system_report_version": [binary["xsh"], packaged_report, "--", "--version"],
        }
        if args.linux:
            commands["system_report_identity"] = [
                binary["xsh"], packaged_report, "--", "--section", "identity", "--json",
            ]
        expected_output = {
            "xsh": b"'a b'\n", "xshi": b"ok\n", "xsht_check": b"",
            "core_module_check": b"",
            "core_ls": b"payload.txt\n", "core_cat": b"portable\n",
            "system_report_version": b"system-report schema v1\n",
        }
        checks = {}
        live_identity = None
        for name, command in commands.items():
            workdir = unrelated_workdir if name.startswith("system_report_") else bundle
            stdout, stderr = run(command, bundle, args.linux, workdir=workdir,
                                 linux_platform=args.linux_platform)
            if name in expected_output and stdout != expected_output[name]:
                raise RuntimeError(f"{name} stdout differs: {stdout!r}")
            if stderr:
                raise RuntimeError(f"{name} wrote stderr: {stderr!r}")
            if name == "xsht_api" and json.loads(stdout.splitlines()[0])["standard_modules"] < 35:
                raise RuntimeError("xsht api omitted standard modules")
            if name == "system_report_identity":
                report = json.loads(stdout)
                if (report.get("schema_version") != 1
                        or report.get("source_mode") != "live_linux"
                        or report.get("identity", {}).get("status", {}).get("state") not in ("complete", "partial")
                        or report.get("cpu", {}).get("status", {}).get("state") != "not_requested"):
                    raise RuntimeError("packaged system-report did not collect live identity")
                live_identity = report
                (bundle / "saved-identity.json").write_bytes(stdout)
            checks[name] = {
                "status": 0,
                "stdout_sha256": hashlib.sha256(stdout).hexdigest(),
                "stderr_sha256": hashlib.sha256(stderr).hexdigest(),
            }
        if args.linux:
            replay_path = "/bundle/saved-identity.json"
            replay_stdout, replay_stderr = run(
                [binary["xsh"], packaged_report, "--", "--from", replay_path,
                 "--section", "identity", "--json"],
                bundle, args.linux, workdir=unrelated_workdir,
                linux_platform=args.linux_platform,
            )
            replay = json.loads(replay_stdout)
            if (replay.get("schema_version") != 1
                    or replay.get("identity") != live_identity["identity"]
                    or replay.get("cpu", {}).get("status", {}).get("state") != "not_requested"
                    or replay_stderr):
                raise RuntimeError("packaged system-report replay differs from saved identity")
            checks["system_report_replay"] = {
                "status": 0,
                "stdout_sha256": hashlib.sha256(replay_stdout).hexdigest(),
                "stderr_sha256": hashlib.sha256(replay_stderr).hexdigest(),
            }

    report = {
        "host": platform.platform(),
        "linux": args.linux,
        "linux_platform": args.linux_platform if args.linux else None,
        "docker_image_id": subprocess.check_output(
            ["docker", "image", "inspect", "--format", "{{.Id}}", "xsh-test"], text=True
        ).strip() if args.linux else None,
        "product_sha256": {name: sha256(path) for name, path in products.items()},
        "core_archive_sha256": archive_hash,
        "core_entries": len(expected),
        "checks": checks,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n")
    print(f"passed {len(checks)} isolated checks; {len(expected)} packaged core scripts")


if __name__ == "__main__":
    main()
