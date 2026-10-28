from __future__ import annotations

import contextlib
import contextvars
import fcntl
import hashlib
import os
from pathlib import Path
import signal
import stat
import sys
import threading
import types


class LeaseBusy(Exception):
    pass


_PARALLEL_CARGO_CAPACITY = 3


_native_descriptor = contextvars.ContextVar("util_linux_native_descriptor", default=None)
_native_quarantine = contextvars.ContextVar("util_linux_native_quarantine", default=None)
_cargo_held = contextvars.ContextVar("util_linux_cargo_held", default=False)
_cargo_guard_module = sys.modules.get("_xsh_util_linux_cargo_lease_guard")
if _cargo_guard_module is None:
    cargo_guard_module = types.ModuleType("_xsh_util_linux_cargo_lease_guard")
    cargo_guard_module.lock = threading.Lock()
    _cargo_guard_module = sys.modules.setdefault("_xsh_util_linux_cargo_lease_guard", cargo_guard_module)
_cargo_process_guard = _cargo_guard_module.lock


def _native_lease():
    held = _native_descriptor.get()
    if held is None:
        raise RuntimeError("cannot retain unresolved cleanup without the native lease")
    return held


def _write_lease_quarantine(descriptor: int, message: bytes) -> None:
    written = os.pwrite(descriptor, message, 0)
    if written != len(message):
        raise OSError("short write while retaining unresolved cleanup")
    os.ftruncate(descriptor, len(message))
    os.fsync(descriptor)


def _write_marker(path: Path, message: bytes) -> tuple[int, int]:
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
        info = os.fstat(marker_fd)
    finally:
        os.close(marker_fd)
    directory_fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(directory_fd)
    finally:
        os.close(directory_fd)
    return info.st_dev, info.st_ino


def _establish_cleanup_quarantine(descriptor: int, path: Path, detail: str):
    message = ("cleanup-unresolved: util-linux " + detail + "\n").encode("utf-8")
    marker_identity = _write_marker(path, message)
    _write_lease_quarantine(descriptor, message)
    return descriptor, path, message, marker_identity


def prepare_cleanup_unresolved(detail: str) -> None:
    descriptor, path = _native_lease()
    if _native_quarantine.get() is not None:
        raise RuntimeError("native lease already has an unresolved cleanup quarantine")
    _native_quarantine.set(_establish_cleanup_quarantine(descriptor, path, detail))


def _resolve_cleanup_unresolved() -> None:
    descriptor, path = _native_lease()
    quarantine = _native_quarantine.get()
    if quarantine is None or quarantine[:2] != (descriptor, path):
        raise RuntimeError("native lease has no matching cleanup quarantine")
    _, _, message, marker_identity = quarantine
    marker = path.with_suffix(".cleanup-unresolved")
    info = marker.lstat()
    if not stat.S_ISREG(info.st_mode) or (info.st_dev, info.st_ino) != marker_identity:
        raise ValueError("cleanup marker identity changed before resolution")
    directory_fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        _write_lease_quarantine(descriptor, b"")
        try:
            os.unlink(marker, dir_fd=directory_fd)
            os.fsync(directory_fd)
        except BaseException as error:
            restored_identity = marker_identity
            try:
                current = path.with_suffix(".cleanup-unresolved")
                if not current.exists() and not current.is_symlink():
                    restored_identity = _write_marker(path, message)
            except BaseException as restore_error:
                error.add_note(f"failed to restore cleanup quarantine: {restore_error!r}")
            try:
                _write_lease_quarantine(descriptor, message)
            except BaseException as restore_error:
                error.add_note(f"failed to restore lease sentinel: {restore_error!r}")
            _native_quarantine.set((descriptor, path, message, restored_identity))
            raise
    finally:
        os.close(directory_fd)
    _native_quarantine.set(None)


def resolve_cleanup_unresolved() -> None:
    previous_mask = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGINT, signal.SIGTERM})
    error = None
    try:
        _resolve_cleanup_unresolved()
    except BaseException as failure:
        error = failure
    mask_restore_failed = False
    try:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)
    except BaseException as failure:
        mask_restore_failed = True
        if error is None:
            error = failure
        else:
            error.add_note(f"signal mask restoration failed: {failure!r}")
    if error is not None:
        if mask_restore_failed:
            error.add_note("signal mask restoration failed after cleanup quarantine transition")
        raise error


def mark_cleanup_unresolved(detail: str) -> None:
    descriptor, path = _native_lease()
    _establish_cleanup_quarantine(descriptor, path, detail)


def require_native_lease() -> None:
    if _native_descriptor.get() is None:
        raise RuntimeError("container execution requires the original native lease")


def require_cargo_lease() -> None:
    if not _cargo_held.get():
        raise RuntimeError("build execution requires the original cargo lease")


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


LEASE_DIRECTORY = Path(".work/consolidation/infra")
LEASE_NAMES = ("final-performance.lock", "native-suite.lock", "cargo.lock")


def ensure_lease_files(base: Path) -> Path:
    directory = base / LEASE_DIRECTORY
    descriptor = os.open(base, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in LEASE_DIRECTORY.parts:
            try:
                os.mkdir(part, 0o755, dir_fd=descriptor)
            except FileExistsError:
                pass
            except OSError as error:
                raise ValueError(f"shared lease directory is unreachable: {directory}: {error}") from error
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        for name in LEASE_NAMES:
            try:
                lease = os.open(name, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600, dir_fd=descriptor)
            except OSError as error:
                raise ValueError(f"shared lease file is unreachable: {directory / name}: {error}") from error
            os.close(lease)
    finally:
        os.close(descriptor)
    return directory


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
def _cargo_lease(
    root: Path,
    path: Path,
    expected: tuple[int, int] | None,
    target: Path | None,
):
    if not _cargo_process_guard.acquire(blocking=False):
        raise LeaseBusy("Cargo lease already held in this process")
    try:
        target_identity = _cargo_target_identity(root, target) if target is not None else None
        mode = fcntl.LOCK_SH if target is not None else fcntl.LOCK_EX
        with acquire([(path, mode, expected)]) as descriptors:
            descriptor = descriptors[0]
            if target is not None:
                _acquire_parallel_cargo_ranges(descriptor, target_identity[1])
            token = _cargo_held.set(True)
            try:
                try:
                    yield descriptor
                except BaseException as error:
                    if target is not None:
                        try:
                            current_identity = _cargo_target_identity(root, target)
                        except BaseException as identity_error:
                            raise error from identity_error
                        if current_identity != target_identity:
                            raise error from ValueError("Cargo output directory changed during lease")
                    raise
                else:
                    if target is not None:
                        current_identity = _cargo_target_identity(root, target)
                        if current_identity != target_identity:
                            raise ValueError("Cargo output directory changed during lease")
            finally:
                _cargo_held.reset(token)
    finally:
        _cargo_process_guard.release()


def _cargo_target_identity(root: Path, target: Path) -> tuple[Path, tuple[int, int]]:
    source = root.resolve(strict=True)
    work = source / ".work"
    expected = work / "target"
    if target != expected or work.is_symlink() or target.is_symlink():
        raise ValueError("parallel Cargo output must be the source-local .work/target directory")
    work_info = work.stat(follow_symlinks=False)
    target_info = target.stat(follow_symlinks=False)
    if not stat.S_ISDIR(work_info.st_mode) or not stat.S_ISDIR(target_info.st_mode):
        raise ValueError("parallel Cargo output path is not a directory")
    return expected, (target_info.st_dev, target_info.st_ino)


def _cargo_target_offset(identity: tuple[int, int]) -> int:
    digest = hashlib.blake2b(os.fsencode(f"{identity[0]}:{identity[1]}"), digest_size=8).digest()
    return _PARALLEL_CARGO_CAPACITY + (int.from_bytes(digest, "big") & ((1 << 62) - 1))


def _acquire_parallel_cargo_ranges(descriptor: int, identity: tuple[int, int]) -> int:
    target_offset = _cargo_target_offset(identity)
    for slot in range(_PARALLEL_CARGO_CAPACITY):
        try:
            fcntl.lockf(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB, 1, slot, os.SEEK_SET)
        except BlockingIOError:
            continue
        try:
            fcntl.lockf(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB, 1, target_offset, os.SEEK_SET)
        except BlockingIOError as error:
            fcntl.lockf(descriptor, fcntl.LOCK_UN, 1, slot, os.SEEK_SET)
            raise LeaseBusy("Cargo output already has a build lease") from error
        return slot
    raise LeaseBusy("parallel Cargo build capacity is full")


@contextlib.contextmanager
def campaign_leases(
    root: Path,
    *,
    native: bool = False,
    cargo: bool = False,
    cargo_target: Path | None = None,
):
    if cargo_target is not None and not cargo:
        raise ValueError("a parallel Cargo target requires the Cargo lease")
    infra = ensure_lease_files(original_root(root))
    paths = [(infra / "final-performance.lock", fcntl.LOCK_SH, None)]
    if native:
        paths.append((infra / "native-suite.lock", fcntl.LOCK_EX, None))
    with acquire(paths) as descriptors:
        token = _native_descriptor.set((descriptors[1], paths[1][0]) if native else None)
        quarantine_token = _native_quarantine.set(None)
        try:
            if cargo:
                with _cargo_lease(root, infra / "cargo.lock", None, cargo_target) as cargo_descriptor:
                    yield (*descriptors, cargo_descriptor)
            else:
                yield descriptors
        finally:
            _native_quarantine.reset(quarantine_token)
            _native_descriptor.reset(token)
