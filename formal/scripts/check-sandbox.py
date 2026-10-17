#!/usr/bin/env python3
"""Keep Lake's bubblewrap sandbox compatible with the bounded proof check."""

from __future__ import annotations

import os
import sys


def sandbox_args(args: list[str], *, threads: str, stack_size_kb: str) -> list[str]:
    boundary = args.index("--")
    mounts = args[:boundary]
    root = mounts.index("--ro-bind")
    if mounts[root + 1 : root + 3] != ["/", "/"]:
        raise ValueError("expected Lake's read-only root mount")
    mounts[root + 3 : root + 3] = ["--tmpfs", "/run"]
    return mounts + [
        "--setenv", "LEAN_NUM_THREADS", threads,
        "--setenv", "LEAN_STACK_SIZE_KB", stack_size_kb,
    ] + args[boundary:]


def main() -> None:
    bwrap = sys.argv[1]
    args = sandbox_args(sys.argv[4:], threads=sys.argv[2], stack_size_kb=sys.argv[3])
    os.execv(bwrap, [bwrap, *args])


if __name__ == "__main__":
    main()
