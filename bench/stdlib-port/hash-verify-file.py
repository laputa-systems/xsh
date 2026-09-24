#!/usr/bin/env python3
"""Paired G02 diagnostic for file-verification workload shapes."""

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import tempfile
import time


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_digest(path):
    with open(path, "rb") as handle:
        return digest(handle.read())


def workload_source(root, embedded_root):
    tiny = os.path.join(root, "tiny")
    large = os.path.join(root, "large")
    tiny_data = b""
    large_data = (b"0123456789abcdef" * 47360)[: 740 * 1024]
    for path, data in [(tiny, tiny_data), (large, large_data)]:
        with open(path, "wb") as handle:
            handle.write(data)

    small = []
    for index in range(64):
        path = os.path.join(root, f"small-{index:02d}")
        data = f"small fixture {index:02d}\n".encode()
        with open(path, "wb") as handle:
            handle.write(data)
        small.append((os.path.join(embedded_root, os.path.basename(path)), digest(data)))

    def repeated(path, checksum, count):
        return (
            "proc main() [io, fs, error] {\n"
            "  var i = 0\n"
            f"  while i < {count} {{\n"
            f'    hash.verify_file(p"{path}", sha256: "{checksum}")?\n'
            "    i = i + 1\n"
            "  }\n"
            "  print \"ok\"\n"
            "}\n"
        )

    many_calls = "\n".join(
        f'  hash.verify_file(p"{path}", sha256: "{checksum}")?'
        for path, checksum in small
    )
    many = f'proc main() [io, fs, error] {{\n{many_calls}\n  print "ok"\n}}\n'
    errors = (
        "proc main() [io, fs, error] {\n"
        "  var rejected = 0\n"
        "  var i = 0\n"
        "  while i < 50 {\n"
        f'    match hash.verify_file(p"{embedded_root}/tiny", sha256: "00") {{\n'
        "      Ok(_) => {}\n"
        "      Err(_) => { rejected = rejected + 1 }\n"
        "    }\n"
        f'    match hash.verify_file(p"{embedded_root}/tiny", sha256: "{"0" * 64}") {{\n'
        "      Ok(_) => {}\n"
        "      Err(_) => { rejected = rejected + 1 }\n"
        "    }\n"
        f'    match hash.verify_file(p"{embedded_root}/missing", sha256: "00") {{\n'
        "      Ok(_) => {}\n"
        "      Err(_) => { rejected = rejected + 1 }\n"
        "    }\n"
        "    i = i + 1\n"
        "  }\n"
        '  print "${rejected}"\n'
        "}\n"
    )
    return {
        "tiny": repeated(os.path.join(embedded_root, "tiny"), digest(tiny_data), 50),
        "large": repeated(os.path.join(embedded_root, "large"), digest(large_data), 50),
        "many_small": many,
        "errors": errors,
    }


def run(binary, script):
    start = time.perf_counter()
    result = subprocess.run([binary, script], capture_output=True)
    elapsed = (time.perf_counter() - start) * 1000
    if result.returncode:
        raise RuntimeError(f"{binary} {script}: {result.stderr.decode(errors='replace')}")
    return elapsed, result.stdout, result.stderr


def measure(reference, candidate, script, rounds, samples):
    parity = None
    round_data = []
    for round_index in range(rounds):
        sides = {"reference": reference, "candidate": candidate}
        order = list(sides)
        if round_index % 2:
            order.reverse()
        for side in order:
            _, stdout, stderr = run(sides[side], script)
            observed = (stdout, stderr)
            if parity is None:
                parity = observed
            elif observed != parity:
                raise RuntimeError(f"output mismatch for {script}: {side}")
        raw = {"reference": [], "candidate": []}
        for index in range(samples):
            for side in order if index % 2 == 0 else reversed(order):
                elapsed, stdout, stderr = run(sides[side], script)
                if (stdout, stderr) != parity:
                    raise RuntimeError(f"output mismatch for {script}: {side}")
                raw[side].append(elapsed)
        round_data.append(raw)
    ref = statistics.median(statistics.median(row["reference"]) for row in round_data)
    cand = statistics.median(statistics.median(row["candidate"]) for row in round_data)
    budget = max(0.10 * ref, 2.0)
    return {
        "reference_ms": ref,
        "candidate_ms": cand,
        "allowed_increase_ms": budget,
        "passed": cand - ref <= budget,
        "stdout": parity[0].decode(),
        "round_data": round_data,
    }


def resident_size(binary, script):
    if platform.system() != "Darwin":
        return None
    observed = []
    for _ in range(3):
        result = subprocess.run(
            ["/usr/bin/time", "-l", binary, script], capture_output=True
        )
        if result.returncode:
            raise RuntimeError(result.stderr.decode(errors="replace"))
        match = re.search(rb"^\s*(\d+)\s+maximum resident set size$", result.stderr, re.M)
        if match is None:
            raise RuntimeError("/usr/bin/time -l did not report maximum resident set size")
        observed.append(int(match.group(1)))
    return {"samples_bytes": observed, "median_bytes": statistics.median(observed)}


def docker_run(root, reference, candidate, command):
    result = subprocess.run(
        [
            "docker", "run", "--rm", "--platform", "linux/arm64",
            "-v", f"{root}:/work/bench/stdlib-port:ro",
            "-v", f"{reference}:/bench/reference:ro",
            "-v", f"{candidate}:/bench/candidate:ro",
            "-w", "/work/bench/stdlib-port", "xsh-test", *command,
        ],
        capture_output=True,
    )
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace"))
    return result.stdout, result.stderr


def measure_linux(root, reference, candidate, names, rounds, samples):
    results = {name: [] for name in names}
    for name in names:
        script = f"/work/bench/stdlib-port/{name}.xsh"
        outputs = []
        for binary in ["/bench/reference", "/bench/candidate"]:
            outputs.append(docker_run(root, reference, candidate, [binary, script]))
        if outputs[0] != outputs[1]:
            raise RuntimeError(f"Linux output mismatch: {name}")
        results[name] = {"stdout": outputs[0][0].decode(), "round_data": []}

    for round_index in range(rounds):
        argv = ["/bench/candidate", "linux-runner.xsh", "/bench/reference", "/bench/candidate", str(round_index)]
        for name in names:
            argv.extend([name, str(samples), "0"])
        stdout, _ = docker_run(root, reference, candidate, argv)
        current = {name: {"reference": [], "candidate": []} for name in names}
        for line in stdout.decode().splitlines():
            name, phase, _, side, nanoseconds = line.split("\t")
            if phase == "sample":
                current[name][side].append(int(nanoseconds) / 1_000_000)
        for name in names:
            if any(len(current[name][side]) != samples for side in ["reference", "candidate"]):
                raise RuntimeError(f"incomplete Linux samples: {name}")
            results[name]["round_data"].append(current[name])

    for result in results.values():
        rows = result["round_data"]
        ref = statistics.median(statistics.median(row["reference"]) for row in rows)
        cand = statistics.median(statistics.median(row["candidate"]) for row in rows)
        budget = max(0.10 * ref, 2.0)
        result.update(reference_ms=ref, candidate_ms=cand, allowed_increase_ms=budget,
                      passed=cand - ref <= budget, rss=None)
    return results


def resident_size_linux(root, reference, candidate, script, binary):
    observed = []
    for _ in range(3):
        _, stderr = docker_run(root, reference, candidate,
                               ["/usr/bin/time", "-v", binary, script])
        match = re.search(rb"Maximum resident set size \(kbytes\):\s*(\d+)", stderr)
        if match is None:
            raise RuntimeError("Linux /usr/bin/time -v did not report peak RSS")
        observed.append(int(match.group(1)) * 1024)
    return {"samples_bytes": observed, "median_bytes": statistics.median(observed)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--samples", type=int, default=30)
    parser.add_argument("--linux", action="store_true")
    args = parser.parse_args()
    reference = os.path.abspath(args.reference)
    candidate = os.path.abspath(args.candidate)
    with tempfile.TemporaryDirectory(prefix="xsh-g02-") as root:
        embedded_root = "/work/bench/stdlib-port" if args.linux else root
        sources = workload_source(root, embedded_root)
        fixture_sha256 = {
            "tiny": file_digest(os.path.join(root, "tiny")),
            "large": file_digest(os.path.join(root, "large")),
            "many_small": digest(
                "".join(
                    file_digest(os.path.join(root, f"small-{index:02d}"))
                    for index in range(64)
                ).encode()
            ),
        }
        results = {}
        for name, source in sources.items():
            script = os.path.join(root, name + ".xsh")
            with open(script, "w", encoding="utf-8") as handle:
                handle.write(source)
        if args.linux:
            shutil.copyfile(os.path.join(os.path.dirname(__file__), "linux-runner.xsh"),
                            os.path.join(root, "linux-runner.xsh"))
            results = measure_linux(root, reference, candidate, sources, args.rounds, args.samples)
            for name in sources:
                script = f"/work/bench/stdlib-port/{name}.xsh"
                results[name]["rss"] = {
                    "reference": resident_size_linux(root, reference, candidate, script, "/bench/reference"),
                    "candidate": resident_size_linux(root, reference, candidate, script, "/bench/candidate"),
                }
        else:
            for name in sources:
                script = os.path.join(root, name + ".xsh")
                results[name] = measure(reference, candidate, script, args.rounds, args.samples)
                results[name]["rss"] = {
                    "reference": resident_size(reference, script),
                    "candidate": resident_size(candidate, script),
                }
        for name, source in sources.items():
            results[name]["source_sha256"] = digest(source.replace(embedded_root, "$ROOT").encode())
            print(f"{name}: {'pass' if results[name]['passed'] else 'FAIL'}")
    report = {
        "reference": reference,
        "candidate": candidate,
        "reference_sha256": file_digest(reference),
        "candidate_sha256": file_digest(candidate),
        "host": platform.platform(),
        "linux": args.linux,
        "docker_image_id": subprocess.check_output(
            ["docker", "image", "inspect", "--format", "{{.Id}}", "xsh-test"], text=True
        ).strip() if args.linux else None,
        "fixture_sha256": fixture_sha256,
        "rounds": args.rounds,
        "samples_per_round": args.samples,
        "results": results,
    }
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2)
        handle.write("\n")
    return int(any(not row["passed"] for row in results.values()))


if __name__ == "__main__":
    raise SystemExit(main())
