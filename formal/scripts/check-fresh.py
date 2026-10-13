#!/usr/bin/env python3
"""Run the pinned fresh kernel check with a process-group and memory guard."""

from __future__ import annotations

import os
import resource
import signal
import subprocess
import sys
from pathlib import Path


WALL_LIMIT_SECONDS = 180
TERM_GRACE_SECONDS = 5
ADDRESS_SPACE_LIMIT = 8 * 1024**3


def set_address_space_limit() -> None:
    if not hasattr(resource, "RLIMIT_AS"):
        raise RuntimeError("this platform does not expose an address-space limit")
    soft, hard = resource.getrlimit(resource.RLIMIT_AS)
    limit = ADDRESS_SPACE_LIMIT if hard == resource.RLIM_INFINITY else min(ADDRESS_SPACE_LIMIT, hard)
    resource.setrlimit(resource.RLIMIT_AS, (limit, limit))


def terminate_process_group(process: subprocess.Popen[str]) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=TERM_GRACE_SECONDS)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.communicate()


def main() -> int:
    formal_dir = Path(__file__).resolve().parents[1]
    try:
        repo_root = Path(
            subprocess.check_output(
                ["git", "-C", str(formal_dir), "rev-parse", "--show-toplevel"],
                text=True,
            ).strip()
        )
        exclude_path = Path(
            subprocess.check_output(
                ["git", "-C", str(repo_root), "rev-parse", "--git-path", "info/exclude"],
                text=True,
            ).strip()
        )
    except subprocess.CalledProcessError as error:
        print(f"cannot locate the repository's git exclude file: {error}", file=sys.stderr)
        return 2

    exclude_lines = exclude_path.read_text(encoding="utf-8").splitlines()
    if ".work/" not in exclude_lines:
        with exclude_path.open("a", encoding="utf-8") as exclude_file:
            exclude_file.write(".work/\n")

    work_root = repo_root / ".work"
    tmp_root = work_root / "tmp"
    cache_root = work_root / "lake-cache"
    tmp_root.mkdir(parents=True, exist_ok=True)
    cache_root.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env["TMPDIR"] = str(tmp_root)
    env["LAKE_CACHE_DIR"] = str(cache_root)
    command = [
        "lake",
        f"--dir={work_root}",
        f"--file={formal_dir / 'lakefile.toml'}",
        "env",
        "leanchecker",
        "--fresh",
        "Xsh",
    ]

    try:
        process = subprocess.Popen(
            command,
            cwd=formal_dir,
            env=env,
            start_new_session=True,
            preexec_fn=set_address_space_limit,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
    except (OSError, subprocess.SubprocessError) as error:
        print(f"could not start the fresh kernel check: {error}", file=sys.stderr)
        return 2

    try:
        output, _ = process.communicate(timeout=WALL_LIMIT_SECONDS)
    except subprocess.TimeoutExpired:
        terminate_process_group(process)
        print(f"fresh kernel check exceeded {WALL_LIMIT_SECONDS}s and was terminated", file=sys.stderr)
        return 124

    sys.stdout.write(output)
    if process.returncode == 0:
        print("fresh kernel check passed")
    return process.returncode


if __name__ == "__main__":
    raise SystemExit(main())
