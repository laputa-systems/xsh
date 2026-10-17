from __future__ import annotations

import contextlib
import contextvars
import fcntl
import os
from pathlib import Path
import stat


class LeaseBusy(Exception):
    pass


_native_descriptor = contextvars.ContextVar("util_linux_native_descriptor", default=None)


def mark_cleanup_unresolved(detail: str) -> None:
    held = _native_descriptor.get()
    if held is None:
        raise RuntimeError("cannot retain unresolved cleanup without the native lease")
    descriptor, path = held
    message = ("cleanup-unresolved: util-linux " + detail + "\n").encode("utf-8")
    written = os.pwrite(descriptor, message, 0)
    if written != len(message):
        raise OSError("short write while retaining unresolved cleanup")
    os.fsync(descriptor)
    marker = path.with_suffix(".cleanup-unresolved")
    marker_fd = os.open(marker, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        offset = 0
        while offset < len(message):
            count = os.write(marker_fd, message[offset:])
            if count == 0:
                raise OSError("short write while retaining cleanup marker")
            offset += count
        os.fsync(marker_fd)
    finally:
        os.close(marker_fd)


def require_native_lease() -> None:
    if _native_descriptor.get() is None:
        raise RuntimeError("container execution requires the original native lease")


def original_root(root: Path) -> Path:
    git = root / ".git"
    if git.is_dir():
        return root.resolve(strict=True)
    marker = git.read_text().strip()
    if not marker.startswith("gitdir: "):
        raise ValueError(f"invalid worktree marker: {git}")
    directory = (root / marker.removeprefix("gitdir: ")).resolve(strict=True)
    common = (directory / (directory / "commondir").read_text().strip()).resolve(strict=True)
    return common.parent


@contextlib.contextmanager
def acquire(paths: list[tuple[Path, int, tuple[int, int] | None]]):
    descriptors = []
    try:
        for path, mode, expected in paths:
            descriptor = os.open(path, os.O_RDWR | os.O_NOFOLLOW)
            descriptors.append(descriptor)
            opened = os.fstat(descriptor)
            named = path.lstat()
            identity = (opened.st_dev, opened.st_ino)
            if not stat.S_ISREG(opened.st_mode) or identity != (named.st_dev, named.st_ino):
                raise ValueError(f"unsafe lease path: {path}")
            if expected is not None and identity != expected:
                raise ValueError(f"lease identity changed: {path}")
            try:
                fcntl.flock(descriptor, mode | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise LeaseBusy(f"lease busy: {path.name}") from error
            if os.pread(descriptor, 19, 0) == b"cleanup-unresolved:":
                raise ValueError(f"owned cleanup remains unresolved: {path}")
            marker = path.with_suffix(".cleanup-unresolved")
            if marker.exists() or marker.is_symlink():
                raise ValueError(f"owned cleanup marker exists: {path}")
        yield tuple(descriptors)
    finally:
        for descriptor in reversed(descriptors):
            os.close(descriptor)


@contextlib.contextmanager
def campaign_leases(root: Path, *, native: bool = False, cargo: bool = False):
    infra = original_root(root) / ".work/consolidation/infra"
    paths = [(infra / "final-performance.lock", fcntl.LOCK_SH, (66307, 62037547))]
    if native:
        paths.append((infra / "native-suite.lock", fcntl.LOCK_EX, (66307, 62012470)))
    if cargo:
        paths.append((infra / "cargo.lock", fcntl.LOCK_EX, (66307, 62044429)))
    with acquire(paths) as descriptors:
        token = _native_descriptor.set((descriptors[1], paths[1][0]) if native else None)
        try:
            yield descriptors
        finally:
            _native_descriptor.reset(token)
