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


def terminate_process_group(
    process: subprocess.Popen[str], initial_signal: int = signal.SIGTERM
) -> None:
    try:
        os.killpg(process.pid, initial_signal)
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
    try:
        process.wait(timeout=TERM_GRACE_SECONDS)
    finally:
        if process.stdout is not None:
            process.stdout.close()


class FreshCheckCancelled(Exception):
    def __init__(self, signum: int) -> None:
        self.signum = signum


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
        if not exclude_path.is_absolute():
            exclude_path = repo_root / exclude_path
        exclude_path = exclude_path.resolve()
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

    interrupted_by: int | None = None
    cleaning_up = False
    signal_delivery_error: OSError | None = None
    process: subprocess.Popen[str] | None = None
    completed = False

    def handle_signal(signum: int, _frame: object) -> None:
        nonlocal interrupted_by, signal_delivery_error
        if interrupted_by is not None:
            return
        interrupted_by = signum
        if process is not None:
            if not cleaning_up:
                raise FreshCheckCancelled(signum)
            try:
                os.killpg(process.pid, signum)
            except ProcessLookupError:
                pass
            except OSError as error:
                signal_delivery_error = error

    previous_handlers: dict[int, object] = {}
    result = 2
    try:
        for signum in (signal.SIGINT, signal.SIGTERM):
            previous_handlers[signum] = signal.signal(signum, handle_signal)

        try:
            try:
                if interrupted_by is None:
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
                        if interrupted_by is None:
                            print(f"could not start the fresh kernel check: {error}", file=sys.stderr)
                        else:
                            result = 128 + interrupted_by

                if process is not None:
                    if interrupted_by is not None:
                        raise FreshCheckCancelled(interrupted_by)
                    output, _ = process.communicate(timeout=WALL_LIMIT_SECONDS)
                    sys.stdout.write(output)
                    if process.returncode == 0:
                        print("fresh kernel check passed")
                    result = process.returncode
                    completed = True
                    cleaning_up = True
            except subprocess.TimeoutExpired:
                cleaning_up = True
                print(f"fresh kernel check exceeded {WALL_LIMIT_SECONDS}s and was terminated", file=sys.stderr)
                result = 124
            except BaseException:
                cleaning_up = True
                raise
        except FreshCheckCancelled as cancelled:
            interrupted_by = cancelled.signum
            cleaning_up = True
            print("fresh kernel check was interrupted", file=sys.stderr)
            result = 128 + cancelled.signum
    finally:
        cleaning_up = True
        if process is not None and (not completed or interrupted_by is not None):
            try:
                terminate_process_group(
                    process,
                    interrupted_by if interrupted_by is not None else signal.SIGTERM,
                )
            except (OSError, subprocess.SubprocessError) as error:
                print(f"could not fully stop the fresh kernel check: {error}", file=sys.stderr)
        for signum, previous_handler in previous_handlers.items():
            signal.signal(signum, previous_handler)

    if interrupted_by is not None:
        if signal_delivery_error is not None:
            print(f"could not forward signal to the fresh kernel check: {signal_delivery_error}", file=sys.stderr)
        result = 128 + interrupted_by
    return result


if __name__ == "__main__":
    raise SystemExit(main())
