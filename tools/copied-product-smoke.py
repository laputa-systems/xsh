#!/usr/bin/env python3
"""Exercise copied products and the packaged core archive without source files."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parent.parent


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(argv, bundle, linux):
    if linux:
        command = [
            "docker", "run", "--rm", "--platform", "linux/arm64",
            "-v", f"{bundle}:/bundle:rw", "-w", "/bundle",
            "-e", "HOME=/bundle", "-e", "XSHI_ALLOW_NON_TTY_FOR_TESTS=1",
            "xsh-test", *argv,
        ]
        result = subprocess.run(command, capture_output=True)
    else:
        env = {
            "PATH": "/usr/bin:/bin", "HOME": str(bundle), "TMPDIR": str(bundle),
            "XSHI_ALLOW_NON_TTY_FOR_TESTS": "1",
        }
        result = subprocess.run(argv, cwd=bundle, env=env, capture_output=True)
    if result.returncode:
        raise RuntimeError(f"{argv[0]} exited {result.returncode}: {result.stderr!r}")
    return result.stdout, result.stderr


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--core-archive", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--linux", action="store_true")
    args = parser.parse_args()
    products = {name: (args.bin_dir / name).resolve() for name in ("xsh", "xshi", "xsht")}
    archive = args.core_archive.resolve()
    archive_hash = sha256(archive)
    sidecar = archive.with_name(archive.name.removesuffix(".tar.xz") + ".sha256")
    recorded_hash, recorded_path = sidecar.read_text().strip().split("  ", 1)
    if recorded_hash != archive_hash or recorded_path != f"dist/{archive.name}":
        raise RuntimeError("core archive checksum sidecar disagrees with the artifact")

    expected = set()
    for path in (ROOT / "core").rglob("*.xsh"):
        relative = path.relative_to(ROOT / "core")
        if "tests" not in relative.parts:
            expected.add(f"core/{relative if relative.parts[0] == 'lib' else relative.with_suffix('')}")
    with tempfile.TemporaryDirectory(prefix="xsh-copied-products-") as directory:
        bundle = Path(directory)
        for name, source in products.items():
            shutil.copy2(source, bundle / name)
        with tarfile.open(archive, "r:xz") as packaged:
            members = [member for member in packaged.getmembers() if member.isfile()]
            actual = {member.name for member in members}
            if actual != expected:
                raise RuntimeError(f"core archive source mismatch: {sorted(expected ^ actual)}")
            for member in members:
                library = member.name.startswith("core/lib/")
                source = ROOT / (member.name if library else member.name + ".xsh")
                expected_mode = 0o644 if library else 0o755
                if member.mode & 0o777 != expected_mode or packaged.extractfile(member).read() != source.read_bytes():
                    raise RuntimeError(f"packaged core content or executable mode differs: {member.name}")
            packaged.extractall(bundle, filter="data")
        (bundle / "payload.txt").write_text("portable\n")
        (bundle / "smoke.xsh").write_text(
            'proc main() [io] { print shlex.quote("a b") }\n'
        )
        binary = {name: f"/bundle/{name}" if args.linux else str(bundle / name)
                  for name in products}
        commands = {
            "xsh": [binary["xsh"], "smoke.xsh"],
            "xshi": [binary["xshi"], "--no-config", "-c", 'print "ok"'],
            "xsht_check": [binary["xsht"], "check", "smoke.xsh"],
            "xsht_api": [binary["xsht"], "api", "summary", "--format", "jsonl"],
            "core_module_check": [binary["xsht"], "check", "core/getty"],
            "core_ls": [binary["xsh"], "core/ls", "payload.txt"],
            "core_cat": [binary["xsh"], "core/cat", "payload.txt"],
        }
        expected_output = {
            "xsh": b"'a b'\n", "xshi": b"ok\n", "xsht_check": b"",
            "core_module_check": b"",
            "core_ls": b"payload.txt\n", "core_cat": b"portable\n",
        }
        checks = {}
        for name, command in commands.items():
            stdout, stderr = run(command, bundle, args.linux)
            if name in expected_output and stdout != expected_output[name]:
                raise RuntimeError(f"{name} stdout differs: {stdout!r}")
            if stderr:
                raise RuntimeError(f"{name} wrote stderr: {stderr!r}")
            if name == "xsht_api" and json.loads(stdout.splitlines()[0])["standard_modules"] < 35:
                raise RuntimeError("xsht api omitted standard modules")
            checks[name] = {
                "status": 0,
                "stdout_sha256": hashlib.sha256(stdout).hexdigest(),
                "stderr_sha256": hashlib.sha256(stderr).hexdigest(),
            }

    report = {
        "host": platform.platform(),
        "linux": args.linux,
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
