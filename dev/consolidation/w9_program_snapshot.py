from __future__ import annotations

import argparse
import os
import pathlib
import re
import stat
import subprocess
from dataclasses import dataclass
from types import MappingProxyType
from typing import Iterable, Mapping, Sequence


PROGRAM_COMMIT = "e240efad273fe35d66d91f4037b6a8193527da2e"
PROGRAM_TREE = "a1ef04ac7b6f759c272e8f841acee3a3b3a6ff08"
PROGRAM_CONFIG = "xsht-config.ini"
_BLAKE3 = re.compile(r"[0-9a-f]{64}\Z")
_REGULAR_FILE_MODES = {0o100644, 0o100755}
_DIRECTORY_FLAGS = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0)
_FILE_FLAGS = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)


class ProgramSnapshotError(RuntimeError):
    pass


@dataclass(frozen=True)
class ProgramSnapshot:
    root: pathlib.Path
    commit: str
    tree: str
    config_path: pathlib.Path
    _tracked_blobs: Mapping[str, str]
    _tracked_modes: Mapping[str, int]


@dataclass(frozen=True)
class LoadedProgramSource:
    path: pathlib.Path | str
    byte_length: int
    blake3: str


def _git(root: pathlib.Path, *arguments: str, input_data: bytes | None = None) -> bytes:
    result = subprocess.run(
        ["git", "-C", root, *arguments],
        check=False,
        input=input_data,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode:
        message = os.fsdecode(result.stderr).strip()
        raise ProgramSnapshotError(f"git {' '.join(arguments)} failed: {message}")
    return result.stdout


def _git_blob(root: pathlib.Path, oid: str) -> bytes:
    return _git(root, "cat-file", "blob", oid)


def _git_blobs(root: pathlib.Path, oids: Iterable[str]) -> Mapping[str, bytes]:
    unique_oids = sorted(set(oids))
    if not unique_oids:
        return MappingProxyType({})
    output = _git(root, "cat-file", "--batch", input_data=("\n".join(unique_oids) + "\n").encode("ascii"))
    position = 0
    blobs: dict[str, bytes] = {}
    for expected_oid in unique_oids:
        header_end = output.find(b"\n", position)
        if header_end < 0:
            raise ProgramSnapshotError("pinned Git object batch is truncated")
        header = output[position:header_end].split()
        if len(header) != 3 or header[0].decode("ascii", "strict") != expected_oid or header[1] != b"blob":
            raise ProgramSnapshotError("pinned Git object batch contains a non-blob entry")
        try:
            length = int(header[2])
        except ValueError as error:
            raise ProgramSnapshotError("pinned Git object batch contains an invalid size") from error
        position = header_end + 1
        end = position + length
        if end >= len(output) or output[end : end + 1] != b"\n":
            raise ProgramSnapshotError("pinned Git object batch has an invalid object boundary")
        blobs[expected_oid] = output[position:end]
        position = end + 1
    if position != len(output):
        raise ProgramSnapshotError("pinned Git object batch has trailing bytes")
    return MappingProxyType(blobs)


def _parse_tree(root: pathlib.Path, commit: str) -> tuple[dict[str, str], dict[str, int]]:
    blobs: dict[str, str] = {}
    modes: dict[str, int] = {}
    for record in _git(root, "ls-tree", "-r", "-z", "--full-tree", commit).split(b"\0"):
        if not record:
            continue
        try:
            metadata, raw_path = record.split(b"\t", 1)
            raw_mode, object_type, raw_oid = metadata.split(b" ", 2)
            relative = os.fsdecode(raw_path)
            mode = int(raw_mode, 8)
            oid = raw_oid.decode("ascii")
        except (ValueError, UnicodeDecodeError) as error:
            raise ProgramSnapshotError("pinned program tree contains a malformed entry") from error
        if (
            not relative
            or pathlib.PurePosixPath(relative).is_absolute()
            or ".." in pathlib.PurePosixPath(relative).parts
            or object_type != b"blob"
            or relative in modes
        ):
            raise ProgramSnapshotError("pinned program tree contains an unsupported entry")
        modes[relative] = mode
        blobs[relative] = oid
    return blobs, modes


def _canonical_root(root: pathlib.Path | str) -> pathlib.Path:
    try:
        canonical = pathlib.Path(root).resolve(strict=True)
    except (OSError, RuntimeError) as error:
        raise ProgramSnapshotError("program root is not a resolvable directory") from error
    if not canonical.is_dir():
        raise ProgramSnapshotError("program root is not a directory")
    top_level = pathlib.Path(
        os.fsdecode(_git(canonical, "rev-parse", "--path-format=absolute", "--show-toplevel").rstrip(b"\n"))
    )
    if top_level.resolve(strict=True) != canonical:
        raise ProgramSnapshotError("program root is not the root of a Git worktree")
    return canonical


def _open_beneath(root: pathlib.Path, relative: pathlib.PurePath, *, directory: bool) -> int:
    if not hasattr(os, "O_DIRECTORY") or not hasattr(os, "O_NOFOLLOW"):
        raise ProgramSnapshotError("program path validation requires directory and no-follow open flags")
    parts = relative.parts
    if relative.is_absolute() or (not parts and not directory) or any(part in {"", ".", ".."} for part in parts):
        raise ProgramSnapshotError("program source path is not a normalized relative path")
    try:
        current = os.open(root, _DIRECTORY_FLAGS)
    except OSError as error:
        raise ProgramSnapshotError("program root cannot be opened without following links") from error
    try:
        for component in parts[:-1] if not directory else parts:
            next_directory = os.open(component, _DIRECTORY_FLAGS, dir_fd=current)
            os.close(current)
            current = next_directory
        if directory:
            return current
        descriptor = os.open(parts[-1], _FILE_FLAGS, dir_fd=current)
        os.close(current)
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            os.close(descriptor)
            raise ProgramSnapshotError("program source is not a regular file")
        return descriptor
    except OSError as error:
        os.close(current)
        raise ProgramSnapshotError("program source path escapes or is not present in the snapshot") from error


def _read_beneath(root: pathlib.Path, relative: pathlib.PurePath) -> bytes:
    descriptor = _open_beneath(root, relative, directory=False)
    try:
        chunks: list[bytes] = []
        while True:
            chunk = os.read(descriptor, 1024 * 1024)
            if not chunk:
                return b"".join(chunks)
            chunks.append(chunk)
    except OSError as error:
        raise ProgramSnapshotError("program source could not be read") from error
    finally:
        os.close(descriptor)


def _stat_beneath(
    root: pathlib.Path,
    relative: pathlib.PurePath,
    *,
    follow_symlinks: bool,
) -> os.stat_result | None:
    parent = pathlib.PurePosixPath(*relative.parts[:-1])
    if not parent.parts:
        parent = pathlib.PurePosixPath(".")
    descriptor = _open_beneath(root, parent, directory=True)
    try:
        try:
            return os.stat(relative.parts[-1], dir_fd=descriptor, follow_symlinks=follow_symlinks)
        except FileNotFoundError:
            return None
        except OSError as error:
            raise ProgramSnapshotError("program config path could not be inspected") from error
    finally:
        os.close(descriptor)


def _validate_program_snapshot(
    root: pathlib.Path | str,
    expected_commit: str,
    expected_tree: str,
) -> ProgramSnapshot:
    canonical = _canonical_root(root)
    commit = os.fsdecode(_git(canonical, "rev-parse", "HEAD").strip())
    tree = os.fsdecode(_git(canonical, "rev-parse", "HEAD^{tree}").strip())
    if commit != expected_commit or tree != expected_tree:
        raise ProgramSnapshotError("program root does not match the pinned e240 source identity")
    if _git(canonical, "status", "--porcelain=v1", "--untracked-files=all"):
        raise ProgramSnapshotError("program root is not clean")
    tracked_blobs, tracked_modes = _parse_tree(canonical, commit)
    if tracked_modes.get(PROGRAM_CONFIG) not in _REGULAR_FILE_MODES:
        raise ProgramSnapshotError("pinned program config is missing or not a regular file")
    config = _read_beneath(canonical, pathlib.PurePosixPath(PROGRAM_CONFIG))
    if config != _git_blob(canonical, tracked_blobs[PROGRAM_CONFIG]):
        raise ProgramSnapshotError("program config bytes differ from the pinned tree")
    return ProgramSnapshot(
        root=canonical,
        commit=commit,
        tree=tree,
        config_path=canonical / PROGRAM_CONFIG,
        _tracked_blobs=MappingProxyType(tracked_blobs),
        _tracked_modes=MappingProxyType(tracked_modes),
    )


def validate_program_snapshot(root: pathlib.Path | str) -> ProgramSnapshot:
    return _validate_program_snapshot(root, PROGRAM_COMMIT, PROGRAM_TREE)


def _relative_program_path(root: pathlib.Path, path: pathlib.Path | str) -> pathlib.PurePosixPath:
    candidate = pathlib.Path(path)
    if not candidate.is_absolute():
        raise ProgramSnapshotError("program source path is not absolute")
    try:
        relative = candidate.relative_to(root)
    except ValueError as error:
        raise ProgramSnapshotError("program source path is outside the pinned root") from error
    if ".." in relative.parts or relative.as_posix() in {"", "."}:
        raise ProgramSnapshotError("program source path is not normalized")
    return pathlib.PurePosixPath(relative.as_posix())


def program_entry_path(snapshot: ProgramSnapshot, relative_path: str) -> pathlib.Path:
    relative = pathlib.PurePosixPath(relative_path)
    if (
        relative.is_absolute()
        or relative.as_posix() != relative_path
        or ".." in relative.parts
        or not relative_path.endswith(".xsh")
        or snapshot._tracked_modes.get(relative_path) not in _REGULAR_FILE_MODES
    ):
        raise ProgramSnapshotError("program entry is not a pinned XSH source path")
    data = _read_beneath(snapshot.root, relative)
    if data != _git_blob(snapshot.root, snapshot._tracked_blobs[relative_path]):
        raise ProgramSnapshotError("program entry bytes differ from the pinned tree")
    return snapshot.root / pathlib.Path(*relative.parts)


def program_config_path(
    snapshot: ProgramSnapshot,
    entry_path: pathlib.Path | str,
) -> pathlib.Path:
    relative_entry = _relative_program_path(snapshot.root, entry_path)
    relative_text = relative_entry.as_posix()
    if (
        not relative_text.endswith(".xsh")
        or snapshot._tracked_modes.get(relative_text) not in _REGULAR_FILE_MODES
    ):
        raise ProgramSnapshotError("program entry is not a pinned XSH source path")
    entry_descriptor = _open_beneath(snapshot.root, relative_entry, directory=False)
    os.close(entry_descriptor)
    entry_directories = relative_entry.parts[:-1]
    for depth in range(len(entry_directories), -1, -1):
        config_parts = (*entry_directories[:depth], PROGRAM_CONFIG)
        relative_config = pathlib.PurePosixPath(*config_parts)
        config_text = relative_config.as_posix()
        tracked_mode = snapshot._tracked_modes.get(config_text)
        if tracked_mode is not None and tracked_mode not in _REGULAR_FILE_MODES:
            raise ProgramSnapshotError("nearest program config is not a tracked regular file")
        metadata = _stat_beneath(snapshot.root, relative_config, follow_symlinks=False)
        if metadata is None:
            if tracked_mode in _REGULAR_FILE_MODES:
                raise ProgramSnapshotError("pinned program config is missing")
            continue
        if stat.S_ISLNK(metadata.st_mode):
            if tracked_mode in _REGULAR_FILE_MODES:
                raise ProgramSnapshotError("pinned program config is not a regular file")
            target = _stat_beneath(snapshot.root, relative_config, follow_symlinks=True)
            if target is not None and stat.S_ISREG(target.st_mode):
                raise ProgramSnapshotError("nearest program config is a symbolic link")
            continue
        if tracked_mode in _REGULAR_FILE_MODES and not stat.S_ISREG(metadata.st_mode):
            raise ProgramSnapshotError("pinned program config is not a regular file")
        if not stat.S_ISREG(metadata.st_mode):
            continue
        if tracked_mode not in _REGULAR_FILE_MODES:
            raise ProgramSnapshotError("nearest program config is not tracked")
        config_bytes = _read_beneath(snapshot.root, relative_config)
        if config_bytes != _git_blob(snapshot.root, snapshot._tracked_blobs[config_text]):
            raise ProgramSnapshotError("program config bytes differ from the pinned tree")
        return snapshot.root / pathlib.Path(*relative_config.parts)
    raise ProgramSnapshotError("program entry has no pinned project config")


def validate_module_roots(
    snapshot: ProgramSnapshot,
    roots: Sequence[pathlib.Path | str],
) -> tuple[pathlib.Path, ...]:
    if not roots:
        raise ProgramSnapshotError("program entry has no pinned project module roots")
    validated: list[pathlib.Path] = []
    for root in roots:
        candidate = pathlib.Path(root)
        if not candidate.is_absolute():
            raise ProgramSnapshotError("project module root is not absolute")
        try:
            relative = candidate.relative_to(snapshot.root)
        except ValueError as error:
            raise ProgramSnapshotError("project module root is outside the pinned root") from error
        if ".." in relative.parts:
            raise ProgramSnapshotError("project module root is not normalized")
        relative_text = relative.as_posix()
        if relative.parts and not any(
            tracked_path.startswith(relative_text + "/") for tracked_path in snapshot._tracked_modes
        ):
            raise ProgramSnapshotError("project module root is not present in the pinned tree")
        descriptor = _open_beneath(snapshot.root, relative, directory=True)
        os.close(descriptor)
        for index in range(1, len(relative.parts) + 1):
            prefix = pathlib.PurePosixPath(*relative.parts[:index]).as_posix()
            if prefix in snapshot._tracked_modes and snapshot._tracked_modes[prefix] not in _REGULAR_FILE_MODES:
                raise ProgramSnapshotError("project module root traverses a tracked non-regular path")
        validated.append(snapshot.root / relative)
    return tuple(validated)


def validate_loaded_program_sources(
    snapshot: ProgramSnapshot,
    sources: Iterable[LoadedProgramSource],
) -> None:
    observed: dict[str, tuple[bytes, int]] = {}
    for source in sources:
        if (
            type(source.byte_length) is not int
            or source.byte_length < 0
            or not isinstance(source.blake3, str)
            or not _BLAKE3.fullmatch(source.blake3)
        ):
            raise ProgramSnapshotError("loaded program source audit record is malformed")
        relative = _relative_program_path(snapshot.root, source.path)
        relative_text = relative.as_posix()
        if snapshot._tracked_modes.get(relative_text) not in _REGULAR_FILE_MODES:
            raise ProgramSnapshotError("loaded program source is not tracked as a regular file")
        data, previous_length = observed.get(relative_text, (b"", source.byte_length))
        if relative_text in observed:
            if previous_length != source.byte_length:
                raise ProgramSnapshotError("loaded program source has inconsistent byte lengths")
        else:
            data = _read_beneath(snapshot.root, relative)
            if len(data) != source.byte_length:
                raise ProgramSnapshotError("loaded program source length differs from the pinned tree")
            observed[relative_text] = (data, source.byte_length)
    pinned_blobs = _git_blobs(snapshot.root, (snapshot._tracked_blobs[relative] for relative in observed))
    for relative, (data, _) in observed.items():
        if data != pinned_blobs[snapshot._tracked_blobs[relative]]:
            raise ProgramSnapshotError("loaded program source bytes differ from the pinned tree")


def _main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate = subparsers.add_parser("validate")
    validate.add_argument("root", type=pathlib.Path)
    arguments = parser.parse_args()
    if arguments.command == "validate":
        snapshot = validate_program_snapshot(arguments.root)
        print(f"{snapshot.root}\t{snapshot.commit}\t{snapshot.tree}")
        return 0
    raise ProgramSnapshotError("unknown program snapshot command")


if __name__ == "__main__":
    try:
        raise SystemExit(_main())
    except ProgramSnapshotError as error:
        raise SystemExit(str(error)) from error
