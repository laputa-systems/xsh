"""Check observable parity, then measure fresh processes in one pinned image."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import statistics
import subprocess
import sys
import time

from fixtures import digest, snapshot

HERE = Path(__file__).resolve().parent
WORKLOADS = ("startup", "spawn", "pipeline", "directory", "json")
TOOLS = ("xsh", "dash", "bash", "nushell", "ysh", "elvish", "python")
BINARIES = {"dash": "/opt/comparators/bin/dash", "bash": "/bin/bash", "nushell": "/opt/comparators/bin/nu", "ysh": "/opt/comparators/bin/ysh", "elvish": "/opt/comparators/bin/elvish", "python": "/usr/bin/python3"}


def argv_for(tool, workload, fixtures, xsh):
    folder, extension = {"xsh": ("xsh", "xsh"), "dash": ("posix", "sh"), "bash": ("posix", "sh"), "nushell": ("nushell", "nu"), "ysh": ("ysh", "ysh"), "elvish": ("elvish", "elv"), "python": ("python", "py")}[tool]
    options = {"bash": ["--noprofile", "--norc"], "nushell": ["--no-config-file"], "elvish": ["-norc"], "python": ["-I"]}.get(tool, [])
    argv = [str(xsh) if tool == "xsh" else BINARIES[tool], *options, str(HERE / folder / f"{workload}.{extension}")]
    if workload != "startup":
        if tool == "xsh":
            argv.append("--")
        argv.append(str(fixtures))
    if tool in ("dash", "bash") and workload == "json":
        argv.append(str(HERE / "python" / "json.py"))
    return argv


def order_for(round_index, sample_index, tools=TOOLS):
    # Cyclic rotations distribute position; reversals distribute adjacent pairs.
    offset = (round_index + sample_index) % len(tools)
    order = list(tools[offset:] + tools[:offset])
    return order[::-1] if (round_index + sample_index) % 2 else order


def changes(before, after):
    return {path: {"before": before.get(path), "after": after.get(path)} for path in sorted(before.keys() | after.keys()) if before.get(path) != after.get(path)}


def invoke(argv, cwd, environment, timeout, measured=False):
    before = snapshot(cwd)
    started = time.perf_counter_ns() if measured else None
    process = subprocess.Popen(argv, cwd=cwd, env=environment, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
    elapsed = time.perf_counter_ns() - started if measured else None
    effects = changes(before, snapshot(cwd))
    return {"wall_ns": elapsed, "status": process.returncode, "timed_out": timed_out, "stdout_base64": base64.b64encode(stdout).decode(), "stderr_base64": base64.b64encode(stderr).decode(), "effects": effects}


def equivalent(observation, expected):
    return observation["status"] == 0 and not observation["timed_out"] and base64.b64decode(observation["stdout_base64"]) == expected and observation["stderr_base64"] == "" and not observation["effects"]


def validate_fixtures(root):
    pinned = json.loads((HERE / "versions.lock.json").read_text())["fixtures_sha256"]
    if digest(root / "manifest.json") != pinned:
        raise ValueError("fixture manifest differs from the pinned workload")
    manifest = json.loads((root / "manifest.json").read_text())
    actual = snapshot(root)
    del actual["manifest.json"]
    if actual != manifest:
        raise ValueError("fixture bytes, modes, entries, or symlink targets differ from manifest")
    return digest(root / "manifest.json")


def validate_close(path, commit):
    if not path:
        raise ValueError("measurement requires --functional-close with the integrator's completion record")
    close = json.loads(path.read_text())
    if close.get("functional_complete") is not True or close.get("commit") != commit or not re.fullmatch("[0-9a-f]{40}", commit):
        raise ValueError("functional completion record must name the measured source commit")
    return close


def identities(xsh, lock, source_commit):
    queries = {"bash": [BINARIES["bash"], "--version"], "python": [BINARIES["python"], "--version"], "nushell": [BINARIES["nushell"], "--version"], "ysh": [BINARIES["ysh"], "--version"], "elvish": [BINARIES["elvish"], "-version"]}
    result = {}
    for tool in TOOLS:
        binary = xsh if tool == "xsh" else Path(BINARIES[tool])
        version = subprocess.check_output(queries[tool], text=True, timeout=15).strip() if tool in queries else None
        if version and not re.search(r"(?<![\d.])" + re.escape(lock["versions"][tool]) + r"(?![\d.])", version):
            raise ValueError(f"{tool} version differs from lock: {version}")
        result[tool] = {"executable": str(binary), "resolved_executable": str(binary.resolve()), "sha256": digest(binary), "version_output": version, "source_version": source_commit if tool == "xsh" else lock["versions"][tool]}
    return result


def inside(args):
    if not Path("/.dockerenv").exists() or not args.image_id or not args.base_image:
        raise ValueError("workloads must run inside the explicitly identified test image overlay")
    lock = json.loads((HERE / "versions.lock.json").read_text())
    architecture = {"x86_64": "amd64", "aarch64": "arm64"}[platform.machine()]
    if architecture in lock["base_images"] and args.base_image != lock["base_images"][architecture]:
        raise ValueError("comparator image parent differs from the committed architecture pin")
    if digest(HERE / "versions.lock.json") != digest(Path("/opt/comparators/source/versions.lock.json")):
        raise ValueError("comparator image was prepared with a different version lock")
    fixture_hash = validate_fixtures(args.fixtures)
    args.scratch.mkdir(parents=True, exist_ok=True)
    identities_record = identities(args.xsh, lock, args.source_commit)
    report = {
        "schema": 1,
        "mode": "measure" if args.measure else "parity",
        "source_commit": args.source_commit,
        "image_id": args.image_id,
        "base_image": args.base_image,
        "platform": platform.platform(),
        "machine": platform.machine(),
        "target": {"x86_64": "x86_64-unknown-linux-musl", "aarch64": "aarch64-unknown-linux-musl"}[platform.machine()],
        "cpuinfo": Path("/proc/cpuinfo").read_text(),
        "clock": vars(time.get_clock_info("perf_counter")),
        "fixtures_sha256": fixture_hash,
        "lock_sha256": digest(HERE / "versions.lock.json"),
        "harness_sha256": digest(Path(__file__)),
        "fixture_generator_sha256": digest(HERE / "fixtures.py"),
        "tools": identities_record,
        "scripts": {
            str(path.relative_to(HERE)): digest(path)
            for folder in ("xsh", "posix", "nushell", "ysh", "elvish", "python")
            for path in sorted((HERE / folder).glob("*")) if path.is_file()
        },
        "external_helpers": {
            path: digest(Path(path))
            for path in ("/usr/bin/printf", "/usr/bin/awk", "/usr/bin/find", "/usr/bin/sort", "/usr/bin/python3", "/bin/rm")
        },
        "parity": [],
        "samples": [],
        "summary": {},
    }
    if args.measure:
        report["functional_close"] = validate_close(args.functional_close, args.source_commit)
    environments = {}
    directories = {}
    for tool in TOOLS:
        for workload in WORKLOADS:
            key = (tool, workload)
            directory = args.scratch / f"{tool}-{workload}"
            directory.mkdir(exist_ok=False)
            for name in ("home", "tmp", "config", "cache", "data"):
                (directory / name).mkdir()
            directories[key] = directory
            environments[key] = {"PATH": "/opt/comparators/bin:/usr/local/bin:/usr/bin:/bin", "LC_ALL": "C", "LANG": "C", "TZ": "UTC", "HOME": str(directory / "home"), "TMPDIR": str(directory / "tmp"), "XDG_CONFIG_HOME": str(directory / "config"), "XDG_CACHE_HOME": str(directory / "cache"), "XDG_DATA_HOME": str(directory / "data"), "PYTHONDONTWRITEBYTECODE": "1", "NO_COLOR": "1"}
    report["environment"] = {f"{tool}/{workload}": value for (tool, workload), value in environments.items()}
    expected = {workload: (args.fixtures / "expected" / workload).read_bytes() for workload in WORKLOADS}

    def record(tool, workload, measured=False):
        key = (tool, workload)
        argv = argv_for(tool, workload, args.fixtures, args.xsh)
        observation = invoke(argv, directories[key], environments[key], args.timeout, measured=measured)
        observation.update({"tool": tool, "workload": workload, "argv": argv, "equal": equivalent(observation, expected[workload])})
        return observation

    def write_report():
        args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    try:
        for workload in WORKLOADS:
            for tool in TOOLS:
                observation = record(tool, workload)
                report["parity"].append(observation)
                write_report()
                if not observation["equal"]:
                    raise ValueError(f"observable parity failed: {tool}/{workload}; see {args.output}")
        report["method"] = {"rounds": 3, "samples_per_tool_per_round": 30, "startup_samples_per_tool_per_round": 60, "filesystem": "warm", "process": "fresh", "timing": "serialized wall clock including process creation, script loading/checking, helpers, stdout/stderr capture", "order": "cyclic rotation with alternating reversal; offsets include round index", "warmups_per_tool_workload": 1}
        if args.measure:
            for workload in WORKLOADS:
                for tool in TOOLS:
                    if not record(tool, workload)["equal"]:
                        raise ValueError(f"warmup parity failed: {tool}/{workload}")
                for round_index in range(3):
                    for sample_index in range(60 if workload == "startup" else 30):
                        order = order_for(round_index, sample_index)
                        for position, tool in enumerate(order):
                            observation = record(tool, workload, measured=True)
                            observation.update({"round": round_index, "sample": sample_index, "position": position, "order": order})
                            report["samples"].append(observation)
                            # Successful byte output is retained once in parity; sample hashes bind it.
                            observation["stdout_sha256"] = hashlib.sha256(base64.b64decode(observation["stdout_base64"])).hexdigest()
                            if observation["equal"]:
                                del observation["stdout_base64"]
                            else:
                                write_report()
                                raise ValueError(f"sample parity failed: {tool}/{workload}")
                        write_report()
                for tool in TOOLS:
                    values = [row["wall_ns"] for row in report["samples"] if row["tool"] == tool and row["workload"] == workload]
                    median = statistics.median(values)
                    rounds = [[row["wall_ns"] for row in report["samples"] if row["tool"] == tool and row["workload"] == workload and row["round"] == index] for index in range(3)]
                    report["summary"][f"{tool}/{workload}"] = {"samples": len(values), "median_ns": median, "mad_ns": statistics.median(abs(value - median) for value in values), "round_medians_ns": [statistics.median(values) for values in rounds]}
        if validate_fixtures(args.fixtures) != fixture_hash:
            raise ValueError("fixtures changed during the run")
        report["complete"] = True
        write_report()
    finally:
        # The host also removes scratch on launch failure; no workload children survive timeout.
        shutil.rmtree(args.scratch)
    print(f"{len(report['parity'])} parity checks passed; {len(report['samples'])} timing samples")


def host(args):
    if not args.image or not args.xsh or not args.fixtures or not args.output or not args.scratch:
        raise ValueError("supply --image, --xsh, --fixtures, --output, and --scratch")
    for path in (args.output, args.scratch):
        if Path("/tmp") == path.resolve() or Path("/tmp") in path.resolve().parents:
            raise ValueError("results and scratch must live on disk outside /tmp")
    image_info = json.loads(subprocess.check_output(["docker", "image", "inspect", args.image]))[0]
    image_id = image_info["Id"]
    base_image = image_info.get("Config", {}).get("Labels", {}).get("org.xsh.comparative.base")
    if not base_image or not base_image.startswith("sha256:"):
        raise ValueError("image must be a prepared overlay of the immutable Dockerfile.test image")
    if args.measure:
        validate_close(args.functional_close, args.source_commit)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.scratch.mkdir(parents=True, exist_ok=False)
    command = ["docker", "run", "--rm", "--init", "--network", "none", "--read-only", "--platform", "linux/" + image_info["Architecture"], "--user", f"{os.getuid()}:{os.getgid()}", "-v", f"{HERE}:/suite:ro", "-v", f"{args.xsh.resolve()}:/bench/xsh:ro", "-v", f"{args.fixtures.resolve()}:/fixtures:ro", "-v", f"{args.output.parent.resolve()}:/results", "-v", f"{args.scratch.resolve()}:/scratch", "-w", "/suite"]
    if args.functional_close:
        command += ["-v", f"{args.functional_close.resolve()}:/functional-close.json:ro"]
    command += [image_id, "/usr/bin/python3", "-B", "/suite/run.py", "--inside", "--xsh", "/bench/xsh", "--fixtures", "/fixtures", "--output", "/results/" + args.output.name, "--scratch", "/scratch/run", "--image-id", image_id, "--base-image", base_image, "--source-commit", args.source_commit, "--timeout", str(args.timeout)]
    if args.measure:
        command += ["--measure", "--functional-close", "/functional-close.json"]
    try:
        subprocess.run(command, check=True)
    finally:
        shutil.rmtree(args.scratch)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image")
    parser.add_argument("--xsh", type=Path)
    parser.add_argument("--fixtures", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--scratch", type=Path)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--measure", action="store_true")
    parser.add_argument("--functional-close", type=Path)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--inside", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--image-id", help=argparse.SUPPRESS)
    parser.add_argument("--base-image", help=argparse.SUPPRESS)
    args = parser.parse_args()
    inside(args) if args.inside else host(args)
