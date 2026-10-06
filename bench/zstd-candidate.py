#!/usr/bin/env python3
"""Benchmark the installed libzstd streaming candidate without repository dependencies."""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import random
import subprocess
import sys
import tempfile


class Buffer(c.Structure):
    _fields_ = [("data", c.c_void_p), ("size", c.c_size_t), ("pos", c.c_size_t)]


def library():
    lib = c.CDLL("libzstd.so.1")
    definitions = {
        "ZSTD_createCStream": (c.c_void_p, []),
        "ZSTD_createDStream": (c.c_void_p, []),
        "ZSTD_freeCStream": (c.c_size_t, [c.c_void_p]),
        "ZSTD_freeDStream": (c.c_size_t, [c.c_void_p]),
        "ZSTD_initCStream": (c.c_size_t, [c.c_void_p, c.c_int]),
        "ZSTD_initDStream": (c.c_size_t, [c.c_void_p]),
        "ZSTD_CCtx_setParameter": (c.c_size_t, [c.c_void_p, c.c_int, c.c_int]),
        "ZSTD_compressStream2": (c.c_size_t, [c.c_void_p, c.POINTER(Buffer), c.POINTER(Buffer), c.c_int]),
        "ZSTD_decompressStream": (c.c_size_t, [c.c_void_p, c.POINTER(Buffer), c.POINTER(Buffer)]),
        "ZSTD_isError": (c.c_uint, [c.c_size_t]),
        "ZSTD_getErrorName": (c.c_char_p, [c.c_size_t]),
        "ZSTD_versionString": (c.c_char_p, []),
    }
    for name, (result, arguments) in definitions.items():
        function = getattr(lib, name)
        function.restype, function.argtypes = result, arguments
    return lib


def checked(lib, result):
    if lib.ZSTD_isError(result):
        raise ValueError(lib.ZSTD_getErrorName(result).decode())
    return result


def transform(source, destination, decode):
    lib = library()
    context = lib.ZSTD_createDStream() if decode else lib.ZSTD_createCStream()
    if not context:
        raise MemoryError("cannot allocate zstd stream")
    try:
        checked(lib, lib.ZSTD_initDStream(context) if decode else lib.ZSTD_initCStream(context, 3))
        if not decode:
            checked(lib, lib.ZSTD_CCtx_setParameter(context, 201, 1))
        output_data = c.create_string_buffer(131072)
        remaining = 1
        with source.open("rb") as stdin, destination.open("wb") as stdout:
            while True:
                chunk = stdin.read(131072)
                eof = not chunk
                input_data = c.create_string_buffer(chunk)
                input_buffer = Buffer(c.addressof(input_data), len(chunk), 0)
                if eof and decode:
                    if remaining:
                        raise ValueError("truncated zstd frame")
                    break
                while True:
                    output_buffer = Buffer(c.addressof(output_data), 131072, 0)
                    remaining = checked(lib,
                        lib.ZSTD_decompressStream(context, c.byref(output_buffer), c.byref(input_buffer)) if decode
                        else lib.ZSTD_compressStream2(context, c.byref(output_buffer), c.byref(input_buffer), 2 if eof else 0))
                    stdout.write(output_data.raw[:output_buffer.pos])
                    if input_buffer.pos == input_buffer.size and (not eof or remaining == 0):
                        break
                if eof:
                    break
    finally:
        checked(lib, lib.ZSTD_freeDStream(context) if decode else lib.ZSTD_freeCStream(context))


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(65536), b""):
            result.update(chunk)
    return result.hexdigest()


def benchmark(output, binary=None):
    candidate = subprocess.check_output([str(binary), "--version"], text=True).strip() if binary else "libzstd " + library().ZSTD_versionString().decode()
    report = {"candidate": candidate, "adapter": "Rust streaming wrapper, 64 KiB input/output buffers" if binary else "Python ctypes, 128 KiB input/output buffers", "corpus": "repetitive binary text with fresh seeded random data in each block", "measurement": "one child process per operation, elapsed time at 10 ms resolution and child peak RSS", "level": 3, "checksum": True, "runs": []}
    def run_transform(source, destination, decode):
        if binary:
            subprocess.run([str(binary), "decode" if decode else "encode", str(source), str(destination)], check=True, capture_output=True, timeout=120)
        else:
            transform(source, destination, decode)
    rng = random.Random(20261006)
    prefix = b"streaming binary payload\0\xff\n" * 4096
    with tempfile.TemporaryDirectory(prefix="zstd-candidate-") as directory:
        root = Path(directory)
        source, packed, plain, stats = (root / name for name in ["source", "packed", "plain", "stats"])
        for mib in [32, 64, 256]:
            with source.open("wb") as stream:
                remaining = mib * 1048576
                while remaining:
                    chunk = (prefix + rng.randbytes(16384))[:remaining]
                    stream.write(chunk)
                    remaining -= len(chunk)
            record = {"input_bytes": source.stat().st_size}
            for mode, input_file, output_file in [("encode", source, packed), ("decode", packed, plain)]:
                adapter = [str(binary), mode] if binary else [sys.executable, __file__, "--" + mode]
                command = ["/usr/bin/time", "-f", "%e %M", "-o", str(stats), *adapter, str(input_file), str(output_file)]
                run = subprocess.run(command, capture_output=True, timeout=120)
                if run.returncode:
                    raise RuntimeError(run.stderr.decode())
                seconds, rss = stats.read_text().split()
                record[mode] = {"seconds": float(seconds), "peak_rss_kib": int(rss)}
            if digest(source) != digest(plain):
                raise ValueError("zstd round trip mismatch")
            record["compressed_bytes"] = packed.stat().st_size
            report["runs"].append(record)
            print(record, flush=True)
        concatenated = root / "concatenated"
        with concatenated.open("wb") as stream:
            for _ in range(2):
                with packed.open("rb") as frame:
                    for chunk in iter(lambda: frame.read(65536), b""):
                        stream.write(chunk)
        run_transform(concatenated, plain, True)
        expected = hashlib.sha256()
        for _ in range(2):
            with source.open("rb") as stream:
                for chunk in iter(lambda: stream.read(65536), b""):
                    expected.update(chunk)
        if digest(plain) != expected.hexdigest():
            raise ValueError("concatenated frame mismatch")
        truncated = root / "truncated"
        with packed.open("rb") as stdin, truncated.open("wb") as stdout:
            remaining = packed.stat().st_size - 4
            while remaining:
                chunk = stdin.read(min(65536, remaining))
                stdout.write(chunk)
                remaining -= len(chunk)
        try:
            run_transform(truncated, plain, True)
        except (ValueError, subprocess.CalledProcessError):
            pass
        else:
            raise ValueError("truncated frame accepted")
        with packed.open("r+b") as stream:
            stream.seek(-1, 2)
            last = stream.read(1)
            stream.seek(-1, 2)
            stream.write(bytes([last[0] ^ 1]))
        try:
            run_transform(packed, plain, True)
        except (ValueError, subprocess.CalledProcessError):
            pass
        else:
            raise ValueError("corrupted checksum accepted")
    report["correctness"] = ["binary round trip", "concatenated frames", "truncated frame rejected", "checksum corruption rejected"]
    output.write_text(json.dumps(report, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--encode", nargs=2, type=Path)
    parser.add_argument("--decode", nargs=2, type=Path)
    parser.add_argument("--benchmark", type=Path)
    parser.add_argument("--binary", type=Path)
    args = parser.parse_args()
    if args.benchmark:
        benchmark(args.benchmark, args.binary)
    elif args.encode or args.decode:
        transform(*(args.decode or args.encode), decode=bool(args.decode))
    else:
        parser.error("choose --encode, --decode, or --benchmark")


if __name__ == "__main__":
    main()
