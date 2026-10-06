#!/usr/bin/env python3
"""Measure streaming codec throughput and peak RSS, checking every round trip."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import shutil
import subprocess
import tempfile


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(65536), b""):
            checksum.update(chunk)
    return checksum.hexdigest()


def measured(command, source, destination, stats):
    with source.open("rb") as stdin, destination.open("wb") as stdout:
        run = subprocess.run(
            ["/usr/bin/time", "-f", "%e %M", "-o", str(stats), *command],
            stdin=stdin, stdout=stdout, stderr=subprocess.PIPE, timeout=180,
        )
    if run.returncode != 0:
        raise RuntimeError(f"{command}: {run.stderr.decode(errors='replace')}")
    elapsed, rss = stats.read_text().split()
    return {"seconds": float(elapsed), "peak_rss_kib": int(rss)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--xsh", type=Path, required=True)
    parser.add_argument("--mib", type=int, default=32)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--zstd-reference", type=Path)
    args = parser.parse_args()
    if args.mib <= 0:
        parser.error("--mib must be positive")
    root = Path(__file__).resolve().parents[1]
    xsh = args.xsh.resolve()
    rng = random.Random(20261006)
    prefix = b"streaming binary payload\x00\xff\n" * 4096
    results = {"corpus": "seeded mixed repetitive and random binary", "input_bytes": args.mib * 1048576, "runs": []}
    with tempfile.TemporaryDirectory(prefix="xsh-compression-bench-") as directory:
        directory = Path(directory)
        source = directory / "input"
        with source.open("wb") as output:
            remaining = results["input_bytes"]
            while remaining:
                chunk = (prefix + rng.randbytes(16384))[:remaining]
                output.write(chunk)
                remaining -= len(chunk)
        expected = digest(source)
        tools = ["gzip", "bzip2", "xz", "lzma"] + (["zstd"] if args.zstd_reference else [])
        for tool in tools:
            reference = str(args.zstd_reference.resolve()) if tool == "zstd" else shutil.which(tool)
            if reference is None:
                raise RuntimeError(f"missing reference codec: {tool}")
            version = subprocess.run([reference, "--version"], stdin=subprocess.DEVNULL, capture_output=True, timeout=5)
            text = (version.stderr + version.stdout).decode(errors="replace")
            identity = "BusyBox" if "BusyBox" in text else text.splitlines()[0] if text else reference
            commands = {}
            encoded = {}
            for implementation, command in [
                (identity, [reference]),
                ("XSH", [str(xsh), str(root / "core" / f"{tool}.xsh"), "--"]),
            ]:
                packed, plain, stats = directory / ("packed-xsh" if implementation == "XSH" else "packed-reference"), directory / "plain", directory / "time"
                encoder = [*command, "encode", "/dev/stdin", "/dev/stdout"] if tool == "zstd" and implementation != "XSH" else [*command, "-c"]
                decoder = [*command, "decode", "/dev/stdin", "/dev/stdout"] if tool == "zstd" and implementation != "XSH" else [*command, "-dc"]
                encode = measured(encoder, source, packed, stats)
                decode = measured(decoder, packed, plain, stats)
                if digest(plain) != expected:
                    raise RuntimeError(f"round trip mismatch: {tool} {implementation}")
                results["runs"].append({"codec": tool, "implementation": implementation, "compressed_bytes": packed.stat().st_size, "encode": encode, "decode": decode})
                commands[implementation] = decoder
                encoded[implementation] = packed
                print(f"{tool} {implementation}: encode {encode}, decode {decode}", flush=True)
            for decoder, encoder in [("XSH", identity), (identity, "XSH")]:
                with encoded[encoder].open("rb") as stdin, plain.open("wb") as stdout:
                    run = subprocess.run(commands[decoder], stdin=stdin, stdout=stdout, stderr=subprocess.PIPE, timeout=180)
                if run.returncode != 0 or digest(plain) != expected:
                    raise RuntimeError(f"interoperability mismatch: {tool} {encoder} -> {decoder}: {run.stderr!r}")
        results["interoperability"] = "reference and XSH streams decoded by both implementations"
    args.output.write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
