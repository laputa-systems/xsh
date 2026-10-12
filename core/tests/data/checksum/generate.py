#!/usr/bin/env python3
"""Generate and audit the expected stdin digests used by checksum test shards."""

import hashlib
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
DATA = ROOT / "core/tests/data/checksum"
TESTS = ROOT / "core/tests"
CHUNK_SIZE = 100
INPUT_COUNT = 1000
BLOCK = b"The quick brown fox jumps over the lazy dog\n" * 24
ALGORITHMS = {
    "md5sum": ("md5", "efe30c482e0b687e0cca0612f42ca29b"),
    "sha1sum": ("sha1", "d41337e834377140ae7f98460d71d908598ef04f"),
    "sha256sum": ("sha256", "8e1d3ed57ebc130f0f72508446559eeae06451ae6d61b1e8ce46370cfb8963c3"),
    "sha512sum": ("sha512", "fe413e0f177324d1353893ca0772ceba83fd41512ba63895a0eebb703ef9feac2fb4e92b2cb430b3bda41b46b0cb4ea8307190a5cc795157cfb680a9cd635d0f"),
}


def expected_output(tool):
    algorithm, _ = ALGORITHMS[tool]
    return b"".join(
        hashlib.new(algorithm, BLOCK[:length]).hexdigest().encode("ascii") + b"  -\n"
        for length in range(INPUT_COUNT)
    )


def verify_test_declarations():
    for tool, (_, aggregate) in ALGORITHMS.items():
        source = (TESTS / f"test-bb-{tool}.xsh").read_text()
        declarations = re.findall(
            rf'^test test_bb_{tool}_{tool}_chunk_(\d{{3}}) '
            rf'\{{ \|ctx\| checksum\.check_chunk\(ctx, "{tool}", (\d+)\) \}}$',
            source,
            re.MULTILINE,
        )
        expected_starts = list(range(0, INPUT_COUNT, CHUNK_SIZE))
        actual = [(int(name), int(start)) for name, start in declarations]
        wanted = [(index, start) for index, start in enumerate(expected_starts)]
        if actual != wanted:
            raise SystemExit(f"{tool}: checksum chunks differ from exact input coverage: {actual!r}")
        aggregate_test = re.findall(
            rf'^test test_bb_{tool}_{tool}_original_aggregate '
            rf'\{{ \|ctx\| checksum\.check_original_aggregate\(ctx, "{tool}", "{aggregate}"\) \}}$',
            source,
            re.MULTILINE,
        )
        if len(aggregate_test) != 1:
            raise SystemExit(f"{tool}: missing or duplicate original aggregate assertion")


def write():
    for tool in ALGORITHMS:
        (DATA / f"{tool}.txt").write_bytes(expected_output(tool))
    verify_test_declarations()
    check()


def check():
    for tool, (algorithm, aggregate) in ALGORITHMS.items():
        expected = expected_output(tool)
        path = DATA / f"{tool}.txt"
        if not path.exists() or path.read_bytes() != expected:
            raise SystemExit(f"{tool}: expected-output fixture is stale")
        actual_aggregate = hashlib.new(algorithm, expected).hexdigest()
        if actual_aggregate != aggregate:
            raise SystemExit(f"{tool}: concatenated 1,000 fixture outputs do not match the original aggregate")
    verify_test_declarations()
    print(f"verified {len(ALGORITHMS) * INPUT_COUNT} checksum inputs and all original aggregate goldens")


if __name__ == "__main__":
    {"write": write, "check": check}[sys.argv[1]]()
