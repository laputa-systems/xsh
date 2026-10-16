#!/usr/bin/env python3
"""Run the frozen W9 diagnostic and publication parity check."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
import pathlib
import re
import secrets
import shlex
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from functools import lru_cache
from typing import Iterable, Literal, overload

import w9_program_snapshot


ROOT = pathlib.Path(__file__).resolve().parents[2]
_COMMON_GIT_DIRECTORY = pathlib.Path(
    subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "--path-format=absolute", "--git-common-dir"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout.strip()
).resolve()
if _COMMON_GIT_DIRECTORY.name != ".git":
    raise RuntimeError(f"unsupported publication Git common directory: {_COMMON_GIT_DIRECTORY}")
CONTROLLER_ROOT = _COMMON_GIT_DIRECTORY.parent
REFERENCE_RECEIPT_SHA256 = "8054e5f05ae740f746eea1ed980077b5d04bb57c0cc108dd3ee3c402bf9d85c8"
REFERENCE_BINARY_SHA256 = "c58582007f4999eaa953ebdb54c62902557505099e928fc2f168afcc167fd8cc"
REFERENCE_VALIDATION_SHA256 = "7d2883e623ec6b72093afc714307b89f2f0110ef447b64738d7da2a500a035cd"
INPUT_MANIFEST_SHA256 = "1ec4e8eb67681591d25c69b395387ba4acbb4bfe1e4ab514a858e60ff1a56305"
CORPUS_BLAKE3 = "a7b1f70466c4e978222af3d329e9c71b0a15030480e24b8d7f77a5fa4f57f17d"
HELPER_SHA256 = "2ff5aaa74f8ae3d0597a3c14796d8643211a5b57b8b95147ba87205540ca95b0"
WRAPPER_SHA256 = "f6a76b9bcb385e1e328332c9815ce48cb0e864d3ffe951578202a774d476f64e"
PINNED_IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
# Candidate build configuration is checked against these controller-owned pins.
PINNED_BUILD_TARGET = "x86_64-unknown-linux-musl"
PINNED_BUILD_PROFILE = "verification"
PINNED_BUILD_RUNNER_SHA256 = "3477001c6cbe8e9604cb712c10d73447b7e5ee08f0ae4f5bf606f7d8a319e46c"
APPROVED_HELPER_SOURCE = pathlib.Path(
    "/home/josh/d/laputa-systems/xsh-consolidation-lanes/module-record-driver/"
    ".work/consolidation/w9-repository-parity.rs"
)
APPROVED_BUILD_RUNNER = pathlib.Path(
    "/home/josh/d/laputa-systems/xsh-consolidation/.work/consolidation/infra/run-native.sh"
)
HELPER_RELATIVE_PATH = pathlib.Path(".work/consolidation/w9-repository-parity.rs")
BUILD_ARTIFACT_KIND = "repository-parity-integration-test"
PARITY_TEST_NAME = "sema::w9_repository_parity_receipt"
PARITY_TEST_INCLUDE = b'include!("../.work/consolidation/w9-repository-parity.rs");'
RUNNER_LOCK_NAME = ".w9-publication-run.lock"
DOCKER_LOCAL_ENDPOINT = "unix:///var/run/docker.sock"
DOCKER_INSPECT_TIMEOUT_SECONDS = 10
DOCKER_OVERRIDE_VARIABLES = (
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
    "DOCKER_CONFIG",
    "DOCKER_TLS_VERIFY",
    "DOCKER_CERT_PATH",
)
SHELL_OVERRIDE_VARIABLES = ("BASH_ENV", "ENV", "SHELLOPTS", "BASHOPTS")
DOCKER_RESOURCE_FLAGS = (
    "--cpus",
    "--cpu-period",
    "--cpu-quota",
    "--cpu-rt-period",
    "--cpu-rt-runtime",
    "--cpu-shares",
    "--cpuset-cpus",
    "--cpuset-mems",
    "--memory",
    "--memory-reservation",
    "--memory-swap",
    "--memory-swappiness",
)
REFERENCE_HEAD = "e240efad273fe35d66d91f4037b6a8193527da2e"
SOURCE_COHORT_SCHEMA_3 = "3"
SOURCE_COHORT_SCHEMA = "4"
SOURCE_COHORT_SCHEMAS = frozenset({SOURCE_COHORT_SCHEMA_3, SOURCE_COHORT_SCHEMA})
PROGRAM_SNAPSHOT_ROOT = CONTROLLER_ROOT / ".work/wt/w9-program-e240"
PROGRAM_SNAPSHOT_HELPER = ROOT / "dev/consolidation/w9_program_snapshot.py"
PROGRAM_SNAPSHOT_HELPER_SHA256 = "f2022a43b12acd30553411a899c37a16ac0815737f52e5598f16058257d376a1"
SOURCE_BUILD_TARGET = "x86_64-unknown-linux-musl"
SOURCE_BUILD_PROFILE = "verification"
SOURCE_BUILD_IMAGE = PINNED_IMAGE
SOURCE_BUILDER_RELATIVE_PATH = pathlib.Path("dev/consolidation/w9-source-build.sh")
SOURCE_BUILDER_SHA256 = "1e8c0c44fe09214df8135a49eec961329cb1a64dc1c636a38d6fdfac4e535865"
SOURCE_BUILDER_PATH = ROOT / SOURCE_BUILDER_RELATIVE_PATH
SOURCE_LINKER_PATH = CONTROLLER_ROOT / "dev/musl-linker.sh"
SOURCE_LINKER_SHA256 = "583039d8ee08330c3eaab1970802181f1ed766f412bfe9d7493fb39135ebc727"
SOURCE_RUSTFLAGS = (
    "-C target-feature=+crt-static "
    "-C link-arg=--defsym=__isoc23_sscanf=sscanf "
    "-C link-arg=--defsym=__isoc23_strtol=strtol"
)
SOURCE_RECEIPT_FIELDS_3 = (
    "schema",
    "role",
    "candidate_root",
    "candidate_commit",
    "candidate_tree",
    "input_root",
    "input_manifest",
    "input_manifest_sha256",
    "corpus_blake3",
    "builder_path",
    "builder_sha256",
    "builder_image",
    "build_target",
    "build_profile",
    "rustflags",
    "linker_path",
    "linker_sha256",
    "runner",
    "runner_sha256",
)
SOURCE_RECEIPT_FIELDS = (
    *SOURCE_RECEIPT_FIELDS_3,
    "controller_root",
    "controller_commit",
    "controller_tree",
    "program_root",
    "program_commit",
    "program_tree",
    "program_config_path",
    "program_snapshot_helper",
    "program_snapshot_helper_sha256",
)
REFERENCE_INPUT_ROOT = pathlib.Path(
    "/home/josh/d/laputa-systems/xsh-consolidation-lanes/module-record-driver/"
    ".work/consolidation/w9-corrected-pair/reference/.work/consolidation/w9-root-e240"
)
PROFILES = ("default", "reveal", "execution", "check-cli")
OPTION_HEADER = (
    'options=[("default", CheckOptions { interactive_commands: None, reveal_types: false, embedded_bodies: false }), '
    '("reveal", CheckOptions { interactive_commands: None, reveal_types: true, embedded_bodies: false }), '
    '("execution", CheckOptions { interactive_commands: None, reveal_types: false, embedded_bodies: true }), '
    '("check-cli", CheckOptions { interactive_commands: None, reveal_types: true, embedded_bodies: true })]'
)
RECEIPT_FIELDS = (
    "schema",
    "candidate_root",
    "candidate_commit",
    "candidate_tree",
    "candidate_binary",
    "candidate_binary_sha256",
    "build_receipt",
    "build_receipt_sha256",
    "helper",
    "helper_sha256",
    "wrapper",
    "wrapper_sha256",
    "wrapper_root",
    "reference_receipt",
    "reference_receipt_sha256",
    "reference_binary",
    "reference_binary_sha256",
    "reference_validation",
    "reference_validation_sha256",
    "input_root",
    "input_manifest",
    "input_manifest_sha256",
    "corpus_blake3",
    "runner",
    "runner_sha256",
)
BUILD_RECEIPT_FIELDS = (
    "schema",
    "artifact_kind",
    "source_commit",
    "source_tree",
    "binary_path",
    "binary_sha256",
    "target",
    "profile",
    "builder_image",
    "builder_path",
    "builder_sha256",
    "helper_sha256",
    "overlay_sha256",
)
RECEIPT_NAME = re.compile(r"parallel-(0|[1-9][0-9]*)-(0|[1-9][0-9]*)-all\.receipt\Z")


class PublicationError(RuntimeError):
    pass


class CleanupFailure(PublicationError):
    pass


@dataclass(frozen=True)
class ManifestSource:
    relative_path: str
    sha256: str
    mode: int


@dataclass(frozen=True)
class HistoricalManifestSource:
    relative_path: str
    sha256: str


ManifestInput = ManifestSource | HistoricalManifestSource


@dataclass(frozen=True)
class ReceiptSource:
    relative_path: str
    blake3: str


@dataclass(frozen=True)
class ParsedReceipt:
    header: tuple[str, ...]
    inputs: dict[int, ReceiptSource]
    outcomes: dict[tuple[int, str, str], tuple[str, str]]
    terminals: tuple[str, ...]


@dataclass(frozen=True)
class LockClaim:
    path: pathlib.Path
    descriptor: int
    parent_descriptor: int


def open_absolute_nofollow(path: pathlib.Path, *, directory: bool = False) -> int:
    if not path.is_absolute() or not hasattr(os, "O_NOFOLLOW") or not hasattr(os, "O_DIRECTORY"):
        raise PublicationError("W9 publication runner requires absolute paths and O_NOFOLLOW directory opens")
    flags = os.O_RDONLY | os.O_DIRECTORY | getattr(os, "O_CLOEXEC", 0)
    descriptor = os.open("/", flags)
    try:
        components = path.parts[1:]
        for index, component in enumerate(components):
            final = index == len(components) - 1
            component_flags = os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
            if not final or directory:
                component_flags |= os.O_DIRECTORY
            next_descriptor = os.open(component, component_flags, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = next_descriptor
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    descriptor = open_absolute_nofollow(path)
    if not stat.S_ISREG(os.fstat(descriptor).st_mode):
        os.close(descriptor)
        raise PublicationError(f"expected a regular non-symlink file: {path}")
    with os.fdopen(descriptor, "rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def read_regular_text(path: pathlib.Path) -> str:
    descriptor = open_absolute_nofollow(path)
    if not stat.S_ISREG(os.fstat(descriptor).st_mode):
        os.close(descriptor)
        raise PublicationError(f"expected a regular non-symlink file: {path}")
    with os.fdopen(descriptor, "rb") as source:
        return source.read().decode("utf-8")


def read_regular_bytes(path: pathlib.Path) -> bytes:
    descriptor = open_absolute_nofollow(path)
    if not stat.S_ISREG(os.fstat(descriptor).st_mode):
        os.close(descriptor)
        raise PublicationError(f"expected a regular non-symlink file: {path}")
    with os.fdopen(descriptor, "rb") as source:
        return source.read()


@lru_cache(maxsize=1)
def w9_build_contract() -> dict[str, str]:
    path = ROOT / "dev/consolidation/w9-cache-build-config.sh"
    required = {
        "W9_CACHE_BUILD_SCHEMA",
        "W9_CACHE_BUILD_TARGET",
        "W9_CACHE_BUILD_PROFILE",
        "W9_CACHE_BUILD_IMAGE",
        "W9_CACHE_BUILD_RUNNER_SHA256",
    }
    result: dict[str, str] = {}
    for line in read_regular_text(path).splitlines():
        match = re.fullmatch(r"readonly (W9_CACHE_BUILD_[A-Z0-9_]+)=(\S+)", line)
        if match is None or match.group(1) not in required:
            continue
        key, value = match.groups()
        if key in result:
            raise PublicationError(f"W9 build config has duplicate {key}")
        result[key] = value
    if set(result) != required:
        missing = sorted(required - set(result))
        raise PublicationError(f"W9 build config is incomplete: missing={missing}")
    if result["W9_CACHE_BUILD_IMAGE"] != PINNED_IMAGE:
        raise PublicationError("W9 build image differs from the approved publication image")
    for key, approved in (
        ("W9_CACHE_BUILD_TARGET", PINNED_BUILD_TARGET),
        ("W9_CACHE_BUILD_PROFILE", PINNED_BUILD_PROFILE),
        ("W9_CACHE_BUILD_RUNNER_SHA256", PINNED_BUILD_RUNNER_SHA256),
    ):
        if result[key] != approved:
            raise PublicationError(f"W9 build config {key} differs from its approved pin")
    return result


def parse_build_receipt(path: pathlib.Path) -> dict[str, str]:
    record: dict[str, str] = {}
    for line_number, line in enumerate(read_regular_text(path).splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 2 or not fields[0] or not fields[1]:
            raise PublicationError(f"{path}: malformed W9 build receipt row {line_number}")
        key, value = fields
        if key not in BUILD_RECEIPT_FIELDS:
            raise PublicationError(f"{path}: unknown W9 build receipt field {key!r}")
        if key in record:
            raise PublicationError(f"{path}: duplicate W9 build receipt field {key}")
        if any(char in value for char in "\r\n\t"):
            raise PublicationError(f"{path}: invalid W9 build receipt field {key}")
        record[key] = value
    if set(record) != set(BUILD_RECEIPT_FIELDS):
        missing = sorted(set(BUILD_RECEIPT_FIELDS) - set(record))
        raise PublicationError(f"{path}: W9 build receipt fields differ: missing={missing}")
    contract = w9_build_contract()
    if record["schema"] != contract["W9_CACHE_BUILD_SCHEMA"]:
        raise PublicationError(f"{path}: unsupported W9 build receipt schema {record['schema']!r}")
    if record["artifact_kind"] != BUILD_ARTIFACT_KIND:
        raise PublicationError(f"{path}: unsupported W9 build receipt artifact {record['artifact_kind']!r}")
    for field in ("source_commit", "source_tree"):
        if not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", record[field]):
            raise PublicationError(f"{path}: malformed W9 build receipt {field}")
    for field in ("binary_sha256", "builder_sha256", "helper_sha256", "overlay_sha256"):
        if not re.fullmatch(r"[0-9a-f]{64}", record[field]):
            raise PublicationError(f"{path}: malformed W9 build receipt {field}")
    for field in ("binary_path", "builder_path"):
        if not pathlib.Path(record[field]).is_absolute():
            raise PublicationError(f"{path}: W9 build receipt {field} is not absolute")
    for field, key in (("target", "W9_CACHE_BUILD_TARGET"), ("profile", "W9_CACHE_BUILD_PROFILE"),
                       ("builder_image", "W9_CACHE_BUILD_IMAGE")):
        if record[field] != contract[key]:
            raise PublicationError(f"{path}: W9 build receipt {field} differs from the pinned build contract")
    return record


def cargo_target_directory_from_plan(arguments: bytes) -> pathlib.Path:
    if not arguments.endswith(b"\0"):
        raise PublicationError("source-qualified builder plan is not NUL-terminated")
    encoded_arguments = arguments[:-1].split(b"\0")
    if not encoded_arguments or any(not value for value in encoded_arguments):
        raise PublicationError("source-qualified builder plan has an empty argument")
    planned_arguments = [os.fsdecode(value) for value in encoded_arguments]
    target_values = [
        planned_arguments[index + 1][len("CARGO_TARGET_DIR="):]
        for index, argument in enumerate(planned_arguments[:-1])
        if argument == "-e" and planned_arguments[index + 1].startswith("CARGO_TARGET_DIR=")
    ]
    if len(target_values) != 1 or not target_values[0]:
        raise PublicationError("source-qualified builder plan must set exactly one CARGO_TARGET_DIR")
    target = pathlib.Path(target_values[0])
    if not target.is_absolute() or pathlib.Path(os.path.abspath(target)) != target:
        raise PublicationError("planned Cargo target directory must be a canonical absolute path")
    controller_scratch = real_directory(CONTROLLER_ROOT / ".work/tmp", "controller scratch directory")
    if target.parent != controller_scratch:
        raise PublicationError(f"planned Cargo target directory is outside controller .work/tmp: {target}")
    if os.path.lexists(target):
        target = real_directory(target, "qualified Cargo target directory")
    return target


def cargo_integration_executable(messages: str, candidate_root: pathlib.Path) -> pathlib.Path:
    return _cargo_integration_executable(messages, candidate_root, "source")


def cargo_target_integration_executable(messages: str, artifact_root: pathlib.Path) -> pathlib.Path:
    return _cargo_integration_executable(messages, artifact_root, "artifact")


def _cargo_integration_executable(
    messages: str,
    artifact_root: pathlib.Path,
    containment_kind: Literal["source", "artifact"],
) -> pathlib.Path:
    artifact_root = real_directory(artifact_root, f"selected Cargo {containment_kind} root")
    executables: list[pathlib.Path] = []
    for line_number, line in enumerate(messages.splitlines(), 1):
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError as error:
            raise PublicationError(f"cargo build output row {line_number} is not JSON") from error
        if message.get("reason") != "compiler-artifact":
            continue
        target = message.get("target")
        if not isinstance(target, dict) or target.get("name") != "integration" or "test" not in target.get("kind", []):
            continue
        executable = message.get("executable")
        if not isinstance(executable, str) or not executable:
            raise PublicationError("cargo did not report an executable for the integration test target")
        path = regular_file(executable, "candidate integration test harness", executable=True)
        if not under(path, artifact_root):
            raise PublicationError(
                f"candidate integration test harness is outside its {containment_kind} root: {path}"
            )
        executables.append(path)
    if len(executables) != 1:
        raise PublicationError(f"cargo reported {len(executables)} integration test harness executables; expected one")
    return executables[0]


def parity_overlay_bytes(source: bytes) -> bytes:
    if PARITY_TEST_INCLUDE in source:
        raise PublicationError("source tests/sema.rs already includes the parity helper")
    separator = b"" if source.endswith(b"\n") else b"\n"
    return source + separator + PARITY_TEST_INCLUDE + b"\n"


def parity_overlay_sha256(root: pathlib.Path) -> str:
    source = regular_file(root / "tests/sema.rs", "candidate semantic integration tests")
    return hashlib.sha256(parity_overlay_bytes(read_regular_bytes(source))).hexdigest()


def validate_overlay_binding(record: dict[str, str], root: pathlib.Path) -> None:
    expected = parity_overlay_sha256(root)
    if record["overlay_sha256"] != expected:
        raise PublicationError(
            f"candidate build receipt overlay hash mismatch: receipt={record['overlay_sha256']} actual={expected}"
        )


def validate_harness_test_listing(output: str) -> None:
    expected = f"{PARITY_TEST_NAME}: test"
    matches = output.splitlines().count(expected)
    if matches != 1:
        raise PublicationError(f"harness list expected exactly one {expected!r} entry, found {matches}")


def candidate_scratch_parent(root: pathlib.Path) -> pathlib.Path:
    root_descriptor = open_absolute_nofollow(root, directory=True)
    work_descriptor = -1
    worktree_descriptor = -1
    try:
        work_descriptor = open_or_create_child_directory(root_descriptor, ".work", "candidate .work")
        worktree_descriptor = open_or_create_child_directory(work_descriptor, "wt", "candidate .work/wt")
    finally:
        if worktree_descriptor >= 0:
            os.close(worktree_descriptor)
        if work_descriptor >= 0:
            os.close(work_descriptor)
        os.close(root_descriptor)
    return real_directory(root / ".work/wt", "candidate scratch directory")


def exclude_work_from_git(root: pathlib.Path) -> None:
    path = regular_file(root / ".git/info/exclude", "scratch git exclude rules")
    contents = read_regular_bytes(path)
    if b"/.work/" in contents.splitlines():
        return
    parent_descriptor = open_absolute_nofollow(path.parent, directory=True)
    descriptor = -1
    try:
        descriptor = os.open(
            path.name,
            os.O_WRONLY | os.O_APPEND | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0),
            dir_fd=parent_descriptor,
        )
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise PublicationError(f"scratch git exclude rules are not a regular file: {path}")
        suffix = b"" if not contents or contents.endswith(b"\n") else b"\n"
        payload = suffix + b"/.work/\n"
        view = memoryview(payload)
        while view:
            written = os.write(descriptor, view)
            if written == 0:
                raise OSError("short write while excluding scratch .work")
            view = view[written:]
        os.fsync(descriptor)
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        os.close(parent_descriptor)


@contextlib.contextmanager
def scratch_directory(parent: pathlib.Path):
    directory = pathlib.Path(tempfile.mkdtemp(prefix="w9-harness-overlay-", dir=parent))
    try:
        yield directory
    except BaseException as primary_error:
        try:
            shutil.rmtree(directory)
        except OSError as cleanup_error:
            raise CleanupFailure(
                f"{primary_error}; scratch cleanup failed for {directory}: {cleanup_error}"
            ) from primary_error
        raise
    else:
        try:
            shutil.rmtree(directory)
        except OSError as cleanup_error:
            raise CleanupFailure(f"scratch cleanup failed for {directory}: {cleanup_error}") from cleanup_error


def copy_executable(source: pathlib.Path, destination: pathlib.Path) -> pathlib.Path:
    source = regular_file(source, "built integration test harness", executable=True)
    source_descriptor = open_absolute_nofollow(source)
    destination_parent_descriptor = open_absolute_nofollow(destination.parent, directory=True)
    destination_descriptor = -1
    destination_created = False
    try:
        source_metadata = os.fstat(source_descriptor)
        if not stat.S_ISREG(source_metadata.st_mode) or not source_metadata.st_mode & 0o111:
            raise PublicationError(f"built integration test harness is not executable: {source}")
        mode = stat.S_IMODE(source_metadata.st_mode)
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        destination_descriptor = os.open(destination.name, flags, mode, dir_fd=destination_parent_descriptor)
        destination_created = True
        while True:
            block = os.read(source_descriptor, 1024 * 1024)
            if not block:
                break
            view = memoryview(block)
            while view:
                written = os.write(destination_descriptor, view)
                if written == 0:
                    raise OSError("short write while copying integration test harness")
                view = view[written:]
        os.fchmod(destination_descriptor, mode)
        os.fsync(destination_descriptor)
        os.close(destination_descriptor)
        destination_descriptor = -1
        os.fsync(destination_parent_descriptor)
    except BaseException:
        if destination_descriptor >= 0:
            os.close(destination_descriptor)
        if destination_created:
            try:
                os.unlink(destination.name, dir_fd=destination_parent_descriptor)
                os.fsync(destination_parent_descriptor)
            except FileNotFoundError:
                pass
        raise
    finally:
        os.close(source_descriptor)
        os.close(destination_parent_descriptor)
    return regular_file(destination, "candidate integration test harness", executable=True)


def run_harness_test_listing(
    builder: pathlib.Path,
    task_tree: pathlib.Path,
    binary: pathlib.Path,
    docker_environment: dict[str, str],
    program_root: pathlib.Path | None = None,
) -> None:
    try:
        command = [str(builder), str(task_tree), "run"]
        if program_root is not None:
            command.append(f"W9_PROGRAM_ROOT={program_root}")
        command.extend((str(binary), "--list"))
        result = subprocess.run(
            command,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=docker_environment,
        )
    except OSError as error:
        raise PublicationError(f"could not list pinned integration harness tests: {error}") from error
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise PublicationError(f"pinned integration harness list failed with status {result.returncode}: {detail}")
    validate_harness_test_listing(result.stdout)


def require_sha256(path: pathlib.Path, expected: str, label: str) -> str:
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise PublicationError(f"{label}: expected SHA-256 is malformed")
    actual = sha256_file(path)
    if actual != expected:
        raise PublicationError(f"{label}: SHA-256 mismatch expected={expected} actual={actual}")
    return actual


def canonical_path(value: str | pathlib.Path, kind: str) -> pathlib.Path:
    raw = pathlib.Path(value).expanduser()
    if not raw.is_absolute():
        raw = pathlib.Path.cwd() / raw
    absolute = pathlib.Path(os.path.abspath(raw))
    try:
        resolved = absolute.resolve(strict=True)
    except OSError as error:
        raise PublicationError(f"{kind}: cannot resolve {absolute}: {error}") from error
    if resolved != absolute:
        raise PublicationError(f"{kind}: symlink or non-canonical path is not allowed: {absolute}")
    return resolved


def regular_file(value: str | pathlib.Path, kind: str, executable: bool = False) -> pathlib.Path:
    path = canonical_path(value, kind)
    try:
        descriptor = open_absolute_nofollow(path)
    except OSError as error:
        raise PublicationError(f"{kind}: cannot safely open {path}: {error}") from error
    mode = os.fstat(descriptor).st_mode
    os.close(descriptor)
    if not stat.S_ISREG(mode):
        raise PublicationError(f"{kind}: expected a regular non-symlink file: {path}")
    if executable and not mode & 0o111:
        raise PublicationError(f"{kind}: file is not executable: {path}")
    return path


def real_directory(value: str | pathlib.Path, kind: str) -> pathlib.Path:
    path = canonical_path(value, kind)
    try:
        descriptor = open_absolute_nofollow(path, directory=True)
    except OSError as error:
        raise PublicationError(f"{kind}: cannot safely open directory {path}: {error}") from error
    mode = os.fstat(descriptor).st_mode
    os.close(descriptor)
    if not stat.S_ISDIR(mode):
        raise PublicationError(f"{kind}: expected a real directory: {path}")
    return path


def open_or_create_child_directory(parent_descriptor: int, name: str, kind: str) -> int:
    try:
        os.mkdir(name, 0o700, dir_fd=parent_descriptor)
    except FileExistsError:
        pass
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
    try:
        descriptor = os.open(name, flags, dir_fd=parent_descriptor)
    except OSError as error:
        raise PublicationError(f"{kind}: cannot safely open directory: {error}") from error
    if not stat.S_ISDIR(os.fstat(descriptor).st_mode):
        os.close(descriptor)
        raise PublicationError(f"{kind}: expected a real directory")
    return descriptor


def stage_helper(
    candidate_root: pathlib.Path,
    source_path: pathlib.Path = APPROVED_HELPER_SOURCE,
    expected_sha256: str = HELPER_SHA256,
) -> pathlib.Path:
    root = real_directory(candidate_root, "candidate source")
    source = regular_file(source_path, "approved parity helper")
    helper_bytes = read_regular_bytes(source)
    actual_sha256 = hashlib.sha256(helper_bytes).hexdigest()
    if actual_sha256 != expected_sha256:
        raise PublicationError(
            f"approved parity helper: SHA-256 mismatch expected={expected_sha256} actual={actual_sha256}"
        )

    root_descriptor = open_absolute_nofollow(root, directory=True)
    work_descriptor = -1
    consolidation_descriptor = -1
    descriptor = -1
    temporary_name = f".{HELPER_RELATIVE_PATH.name}.{os.getpid()}.{secrets.token_hex(8)}.tmp"
    temporary_created = False
    destination_created = False
    staged_complete = False
    destination = root / HELPER_RELATIVE_PATH
    try:
        work_descriptor = open_or_create_child_directory(root_descriptor, ".work", "candidate .work")
        consolidation_descriptor = open_or_create_child_directory(
            work_descriptor, "consolidation", "candidate .work/consolidation"
        )
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        try:
            descriptor = os.open(temporary_name, flags, 0o644, dir_fd=consolidation_descriptor)
        except FileExistsError as error:
            raise PublicationError(f"temporary parity helper path already exists: {temporary_name}") from error
        temporary_created = True
        output = os.fdopen(descriptor, "wb")
        descriptor = -1
        with output:
            view = memoryview(helper_bytes)
            while view:
                written = output.write(view)
                if written == 0:
                    raise OSError("short write while staging parity helper")
                view = view[written:]
            output.flush()
            os.fsync(output.fileno())
        verify_descriptor = os.open(
            temporary_name,
            os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0),
            dir_fd=consolidation_descriptor,
        )
        if not stat.S_ISREG(os.fstat(verify_descriptor).st_mode):
            os.close(verify_descriptor)
            raise PublicationError(f"staged parity helper is not a regular file: {destination}")
        verified_digest = hashlib.sha256()
        with os.fdopen(verify_descriptor, "rb") as staged:
            for block in iter(lambda: staged.read(1024 * 1024), b""):
                verified_digest.update(block)
        if verified_digest.hexdigest() != expected_sha256:
            raise PublicationError(f"staged parity helper verification failed: {destination}")
        try:
            os.link(
                temporary_name,
                HELPER_RELATIVE_PATH.name,
                src_dir_fd=consolidation_descriptor,
                dst_dir_fd=consolidation_descriptor,
                follow_symlinks=False,
            )
        except FileExistsError as error:
            raise PublicationError(f"refusing to overwrite staged parity helper: {destination}") from error
        destination_created = True
        os.fsync(consolidation_descriptor)
        os.unlink(temporary_name, dir_fd=consolidation_descriptor)
        temporary_created = False
        os.fsync(consolidation_descriptor)
        destination_created = False
        staged_complete = True
    finally:
        if consolidation_descriptor >= 0:
            if temporary_created:
                try:
                    os.unlink(temporary_name, dir_fd=consolidation_descriptor)
                    os.fsync(consolidation_descriptor)
                except FileNotFoundError:
                    pass
        if descriptor >= 0:
            os.close(descriptor)
            if destination_created:
                try:
                    os.unlink(HELPER_RELATIVE_PATH.name, dir_fd=consolidation_descriptor)
                    os.fsync(consolidation_descriptor)
                except FileNotFoundError:
                    pass
        if consolidation_descriptor >= 0:
            os.close(consolidation_descriptor)
        if work_descriptor >= 0:
            os.close(work_descriptor)
        os.close(root_descriptor)

    if not staged_complete:
        raise PublicationError(f"staged parity helper was not created: {destination}")
    return regular_file(destination, "staged parity helper")


def git_output(root: pathlib.Path, *args: str) -> str:
    try:
        result = subprocess.run(
            ["git", "-C", str(root), *args],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", "")
        raise PublicationError(f"git {' '.join(args)} failed for {root}: {detail or error}") from error
    return result.stdout.strip()


def git_common_controller_root(root: pathlib.Path) -> pathlib.Path:
    common_directory = real_directory(
        git_output(root, "rev-parse", "--path-format=absolute", "--git-common-dir"),
        "shared source Git directory",
    )
    if common_directory.name != ".git":
        raise PublicationError(f"unsupported source Git common directory: {common_directory}")
    return real_directory(common_directory.parent, "source controller root")


def require_only_parity_overlay(root: pathlib.Path, failure_message: str) -> None:
    status = git_output(root, "status", "--porcelain", "--untracked-files=all")
    if status != "M tests/sema.rs":
        raise PublicationError(failure_message)


def source_identity(root: pathlib.Path) -> tuple[str, str]:
    top = real_directory(git_output(root, "rev-parse", "--show-toplevel"), "candidate git root")
    if top != root:
        raise PublicationError(f"candidate root is not the git worktree root: {root} (git root {top})")
    status = git_output(root, "status", "--porcelain", "--untracked-files=all")
    if status:
        raise PublicationError(f"candidate source worktree is dirty:\n{status}")
    return git_output(root, "rev-parse", "HEAD"), git_output(root, "rev-parse", "HEAD^{tree}")


def under(path: pathlib.Path, parent: pathlib.Path) -> bool:
    return path == parent or parent in path.parents


def wrapper_root_for(candidate_root: pathlib.Path) -> pathlib.Path:
    checkout_root = pathlib.Path("/home/josh/d/laputa-systems/xsh")
    if under(candidate_root, checkout_root):
        return checkout_root
    return candidate_root


def validate_wrapper_root(wrapper_root: pathlib.Path, candidate_root: pathlib.Path) -> None:
    allowed = (
        pathlib.Path("/home/josh/d/laputa-systems/xsh"),
        pathlib.Path("/home/josh/d/laputa-systems/xsh-consolidation"),
        pathlib.Path("/home/josh/d/laputa-systems/xsh-consolidation/.work/consolidation/ratchets"),
        pathlib.Path("/home/josh/d/laputa-systems/xsh-consolidation-lanes"),
    )
    permitted = wrapper_root in allowed or any(
        wrapper_root != parent and under(wrapper_root, parent) for parent in allowed[-1:]
    )
    if not permitted:
        raise PublicationError(f"wrapper root is outside the pinned wrapper's allowed trees: {wrapper_root}")
    if not under(candidate_root, wrapper_root):
        raise PublicationError(f"wrapper root does not contain candidate source: {wrapper_root}")


@overload
def parse_input_manifest(
    path: pathlib.Path,
    *,
    strict_modes: Literal[True],
) -> tuple[dict[int, ManifestSource], dict[str, str]]: ...


@overload
def parse_input_manifest(
    path: pathlib.Path,
    *,
    strict_modes: Literal[False] = False,
) -> tuple[dict[int, HistoricalManifestSource], dict[str, str]]: ...


def parse_input_manifest(
    path: pathlib.Path,
    *,
    strict_modes: bool = False,
) -> tuple[dict[int, ManifestInput], dict[str, str]]:
    by_index: dict[int, ManifestInput] = {}
    by_path: dict[str, str] = {}
    for row_number, line in enumerate(read_regular_text(path).splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 4:
            raise PublicationError(f"input manifest row {row_number}: expected four tab-separated fields")
        raw_index, relative, digest, raw_mode = fields
        try:
            index = int(raw_index)
        except ValueError as error:
            raise PublicationError(f"input manifest row {row_number}: invalid index {raw_index!r}") from error
        pure = pathlib.PurePosixPath(relative)
        if index != row_number - 1 or pure.is_absolute() or ".." in pure.parts:
            raise PublicationError(f"input manifest row {row_number}: invalid index or path {relative!r}")
        if not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise PublicationError(f"input manifest row {row_number}: malformed source SHA-256")
        if strict_modes:
            if (
                not relative
                or pure.as_posix() != relative
                or "\\" in relative
                or re.fullmatch(r"[0-7]{3,4}", raw_mode) is None
            ):
                raise PublicationError(f"input manifest row {row_number}: invalid source path or mode")
            mode = int(raw_mode, 8)
            if mode not in {0o444, 0o555}:
                raise PublicationError(f"input manifest row {row_number}: unsupported source mode {raw_mode!r}")
            source = ManifestSource(relative, digest, mode)
        else:
            source = HistoricalManifestSource(relative, digest)
        if index in by_index or relative in by_path:
            raise PublicationError(f"input manifest row {row_number}: duplicate index or path")
        by_index[index] = source
        by_path[relative] = digest
    if len(by_index) != 1690 or set(by_index) != set(range(1690)):
        raise PublicationError(f"input manifest: expected 1690 sequential sources, found {len(by_index)}")
    return by_index, by_path


def verify_corpus_tree(
    root: pathlib.Path,
    expected_paths: dict[str, str],
    expected_modes: dict[str, int] | None = None,
) -> None:
    actual_paths: list[str] = []
    for directory, names, files in os.walk(root, followlinks=False):
        base = pathlib.Path(directory)
        relative_directory = base.relative_to(root).as_posix()
        if base.stat().st_mode & 0o222:
            raise PublicationError(f"input corpus directory is writable: {base}")
        if expected_modes is not None:
            linked_directories = [name for name in names if name not in {".git", ".work", "target"} and (base / name).is_symlink()]
            if linked_directories:
                raise PublicationError(f"input corpus contains a symlink directory: {(base / linked_directories[0]).relative_to(root).as_posix()}")
        names[:] = [
            name
            for name in names
            if name not in {".git", ".work", "target"}
            and not (relative_directory == "dev" and name == "compat")
            and not (base / name).is_symlink()
        ]
        for name in files:
            if not name.endswith(".xsh"):
                continue
            path = base / name
            if path.is_symlink() or not path.is_file():
                if expected_modes is not None:
                    raise PublicationError(f"input corpus source is not a regular file: {path.relative_to(root).as_posix()}")
                continue
            if path.stat().st_mode & 0o222:
                raise PublicationError(f"input corpus source is writable: {path.relative_to(root).as_posix()}")
            actual_paths.append(path.relative_to(root).as_posix())
    actual_paths.sort()
    expected_names = sorted(expected_paths)
    if actual_paths != expected_names:
        missing = sorted(set(expected_names) - set(actual_paths))[:10]
        extra = sorted(set(actual_paths) - set(expected_names))[:10]
        raise PublicationError(f"input corpus file list differs from approved manifest: missing={missing} extra={extra}")
    for relative in actual_paths:
        source = root / relative
        if expected_modes is not None:
            descriptor = open_absolute_nofollow(source)
            try:
                metadata = os.fstat(descriptor)
                if not stat.S_ISREG(metadata.st_mode):
                    raise PublicationError(f"input corpus source is not a regular file: {relative}")
                actual_mode = stat.S_IMODE(metadata.st_mode)
                expected_mode = expected_modes[relative]
                if actual_mode != expected_mode:
                    raise PublicationError(
                        f"input corpus mode changed at {relative}: expected={expected_mode:04o} actual={actual_mode:04o}"
                    )
            finally:
                os.close(descriptor)
        actual = sha256_file(source)
        expected = expected_paths[relative]
        if actual != expected:
            raise PublicationError(
                f"input corpus source changed at {relative}: expected sha256={expected} actual={actual}"
            )


def validate_source_qualified_inputs(
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
) -> tuple[dict[int, ManifestSource], dict[str, str]]:
    input_root = real_directory(input_root, "source-qualified input root")
    input_manifest = regular_file(input_manifest, "source-qualified input manifest")
    require_sha256(input_manifest, INPUT_MANIFEST_SHA256, "source-qualified input manifest")
    by_index, by_path = parse_input_manifest(input_manifest, strict_modes=True)
    expected_modes = {source.relative_path: source.mode for source in by_index.values()}
    verify_corpus_tree(input_root, by_path, expected_modes)
    return by_index, by_path


def validate_source_program_entries(
    snapshot: w9_program_snapshot.ProgramSnapshot,
    sources: Iterable[ManifestSource],
) -> None:
    try:
        for source in sources:
            program_entry = w9_program_snapshot.program_entry_path(
                snapshot, source.relative_path
            )
            w9_program_snapshot.program_config_path(snapshot, program_entry)
    except w9_program_snapshot.ProgramSnapshotError as error:
        raise PublicationError(f"source-qualified program entries are invalid: {error}") from error


def validate_work_ignored(root: pathlib.Path) -> None:
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "check-ignore", "--quiet", ".work/"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    except OSError as error:
        raise PublicationError(f"cannot verify source .work ignore rule: {error}") from error
    if result.returncode != 0:
        detail = result.stderr.strip()
        raise PublicationError(f"source .work is not ignored{': ' + detail if detail else ''}")


def parse_validation(path: pathlib.Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in read_regular_text(path).splitlines():
        if "=" not in line:
            raise PublicationError(f"reference validation: malformed line {line!r}")
        key, value = line.split("=", 1)
        if key in result:
            raise PublicationError(f"reference validation: duplicate field {key}")
        result[key] = value
    expected = {
        "status": "validated",
        "reference_head": REFERENCE_HEAD,
        "reference_binary_sha256": REFERENCE_BINARY_SHA256,
        "helper_sha256": HELPER_SHA256,
        "receipt_sha256": REFERENCE_RECEIPT_SHA256,
        "input_root": str(REFERENCE_INPUT_ROOT),
        "input_blake3_manifest_digest": CORPUS_BLAKE3,
        "input_file_manifest_sha256": INPUT_MANIFEST_SHA256,
        "input_sources": "1690 input_rows=1690 output_rows=6760 profiles=default,reveal,execution,check-cli",
        "options_header": OPTION_HEADER,
    }
    for key, value in expected.items():
        if result.get(key) != value:
            raise PublicationError(f"reference validation field {key} mismatch: expected={value!r} actual={result.get(key)!r}")
    return result


def parse_receipt(path: pathlib.Path, expected_paths: dict[int, ManifestInput]) -> ParsedReceipt:
    lines = read_regular_text(path).splitlines()
    if len(lines) < 7:
        raise PublicationError(f"{path}: receipt is incomplete")
    header = tuple(lines[:6])
    if not header[0].startswith("corpus=") or not header[1].startswith("shard="):
        raise PublicationError(f"{path}: malformed receipt header")
    if not header[2].startswith("range=") or not header[3].startswith("shard_size="):
        raise PublicationError(f"{path}: malformed receipt range header")
    if not header[4].startswith("input_digest=") or not header[5].startswith("options="):
        raise PublicationError(f"{path}: malformed receipt identity header")
    corpus_digest = header[4].removeprefix("input_digest=")
    if not re.fullmatch(r"[0-9a-f]{64}", corpus_digest):
        raise PublicationError(f"{path}: malformed corpus digest")
    inputs: dict[int, ReceiptSource] = {}
    outcomes: dict[tuple[int, str, str], tuple[str, str]] = {}
    terminals: list[str] = []
    for line in lines[6:]:
        fields = line.split("\t")
        if fields[0] == "input" and len(fields) == 4:
            try:
                index = int(fields[1])
            except ValueError as error:
                raise PublicationError(f"{path}: malformed input index {fields[1]!r}") from error
            relative, digest = fields[2], fields[3]
            if index not in expected_paths or expected_paths[index].relative_path != relative:
                raise PublicationError(f"{path}: wrong input path at index {index}: {relative}")
            if not re.fullmatch(r"[0-9a-f]{64}", digest):
                raise PublicationError(f"{path}: malformed source BLAKE3 for index {index}")
            if index in inputs:
                raise PublicationError(f"{path}: duplicate input index {index}")
            inputs[index] = ReceiptSource(relative, digest)
        elif fields[0] == "output" and len(fields) == 6:
            try:
                index = int(fields[1])
            except ValueError as error:
                raise PublicationError(f"{path}: malformed output index {fields[1]!r}") from error
            relative, profile = fields[2], fields[3]
            key = (index, relative, profile)
            if index not in expected_paths or expected_paths[index].relative_path != relative:
                raise PublicationError(f"{path}: unknown source outcome {key}")
            if profile not in PROFILES:
                raise PublicationError(f"{path}: unknown profile {profile!r}")
            if key in outcomes:
                raise PublicationError(f"{path}: duplicate outcome {key}")
            for digest in fields[4:]:
                if not re.fullmatch(r"[0-9a-f]{64}", digest):
                    raise PublicationError(f"{path}: malformed diagnostic/publication digest for {key}")
            outcomes[key] = (fields[4], fields[5])
        elif fields[0] == "terminal" and len(fields) == 3:
            terminals.append(line)
        else:
            raise PublicationError(f"{path}: malformed receipt row {line!r}")
    if len(terminals) != 1:
        raise PublicationError(f"{path}: expected exactly one completion record, found {len(terminals)}")
    return ParsedReceipt(header, inputs, outcomes, tuple(terminals))


def validate_reference_receipt(
    path: pathlib.Path,
    expected_paths: dict[int, HistoricalManifestSource],
    corpus_digest: str,
) -> ParsedReceipt:
    receipt = parse_receipt(path, expected_paths)
    count = len(expected_paths)
    if receipt.header != (
        f"corpus={count}",
        "shard=0/1",
        f"range=0..{count}",
        f"shard_size={count}",
        f"input_digest={corpus_digest}",
        OPTION_HEADER,
    ):
        raise PublicationError(f"{path}: approved full-corpus receipt header differs")
    if set(receipt.inputs) != set(range(count)):
        raise PublicationError(f"{path}: approved reference input coverage is incomplete")
    # The input manifest carries SHA-256; receipt source rows carry BLAKE3.
    # The caller authenticates each artifact with its own pin, and the corpus
    # itself is checked against the SHA-256 manifest separately.
    expected_outcomes = {
        (index, source.relative_path, profile)
        for index, source in expected_paths.items()
        for profile in PROFILES
    }
    if set(receipt.outcomes) != expected_outcomes:
        raise PublicationError(f"{path}: approved reference must contain all four outcomes per input")
    if receipt.terminals != (f"terminal\tshard=0\tcompleted={count}",):
        raise PublicationError(f"{path}: approved reference terminal record differs")
    return receipt


def validate_candidate_shard(
    path: pathlib.Path,
    receipt: ParsedReceipt,
    start: int,
    end: int,
    count: int,
    corpus_digest: str,
) -> None:
    shard = start // 100
    shard_count = (count + 99) // 100
    if receipt.header != (
        f"corpus={count}",
        f"shard={shard}/{shard_count}",
        f"range={start}..{end}",
        "shard_size=100",
        f"input_digest={corpus_digest}",
        OPTION_HEADER,
    ):
        raise PublicationError(f"{path}: candidate receipt header differs from range {start}..{end}")
    expected_inputs = set(range(start, end))
    if set(receipt.inputs) != expected_inputs:
        missing = sorted(expected_inputs - set(receipt.inputs))[:10]
        extra = sorted(set(receipt.inputs) - expected_inputs)[:10]
        raise PublicationError(f"{path}: incomplete input range; missing={missing} extra={extra}")
    expected_outcomes = {
        (index, receipt.inputs[index].relative_path, profile)
        for index in range(start, end)
        for profile in PROFILES
    }
    if set(receipt.outcomes) != expected_outcomes:
        missing = sorted(expected_outcomes - set(receipt.outcomes))[:10]
        extra = sorted(set(receipt.outcomes) - expected_outcomes)[:10]
        raise PublicationError(f"{path}: incomplete four-profile outcomes; missing={missing} extra={extra}")
    if receipt.terminals != (f"terminal\tshard={shard}\tcompleted={end - start}",):
        raise PublicationError(f"{path}: candidate completion record differs")


def compare_outcomes(
    reference: ParsedReceipt,
    candidate: ParsedReceipt,
    keys: set[tuple[int, str, str]],
) -> None:
    for key in sorted(keys):
        reference_diagnostic, reference_publication = reference.outcomes[key]
        candidate_diagnostic, candidate_publication = candidate.outcomes[key]
        if reference_diagnostic != candidate_diagnostic:
            raise PublicationError(
                f"diagnostic mismatch at {key}: reference={reference_diagnostic} candidate={candidate_diagnostic}"
            )
        if reference_publication != candidate_publication:
            raise PublicationError(
                f"publication mismatch at {key}: reference={reference_publication} candidate={candidate_publication}"
            )


def compare_shard(
    reference: ParsedReceipt,
    path: pathlib.Path,
    expected_paths: dict[int, HistoricalManifestSource],
    start: int,
    end: int,
    corpus_digest: str,
) -> None:
    candidate = parse_receipt(path, expected_paths)
    validate_candidate_shard(path, candidate, start, end, len(expected_paths), corpus_digest)
    for index in range(start, end):
        if candidate.inputs[index] != reference.inputs[index]:
            raise PublicationError(f"input path or source BLAKE3 mismatch at index {index}: {candidate.inputs[index]}")
    compare_outcomes(reference, candidate, set(candidate.outcomes))


def expected_ranges(count: int) -> list[tuple[int, int]]:
    return [(start, min(start + 25, count)) for start in range(0, count, 25)]


def candidate_receipt_path(candidate_dir: pathlib.Path, start: int, end: int) -> pathlib.Path:
    return candidate_dir / f"parallel-{start}-{end - 1}-all.receipt"


def discover_candidate_receipts(candidate_dir: pathlib.Path, count: int) -> dict[tuple[int, int], pathlib.Path]:
    candidate_dir = real_directory(candidate_dir, "candidate receipt directory")
    expected = {candidate_receipt_path(candidate_dir, start, end).name: (start, end)
                for start, end in expected_ranges(count)}
    found: dict[tuple[int, int], pathlib.Path] = {}
    names = sorted(path.name for path in candidate_dir.iterdir())
    if not names:
        raise PublicationError(f"{candidate_dir}: no candidate receipt files")
    for name in names:
        if not RECEIPT_NAME.fullmatch(name) or name not in expected:
            raise PublicationError(f"{candidate_dir}: unexpected candidate receipt filename {name!r}")
        path = candidate_dir / name
        regular_file(path, "candidate range receipt")
        found[expected[name]] = path
    missing = sorted(set(expected.values()) - set(found))
    if missing:
        raise PublicationError(f"{candidate_dir}: missing candidate ranges {missing[:10]}")
    return found


def verify_receipt_bundle(
    reference: ParsedReceipt,
    candidate_dir: pathlib.Path,
    expected_paths: dict[int, HistoricalManifestSource],
    corpus_digest: str,
) -> str:
    count = len(expected_paths)
    candidates = discover_candidate_receipts(candidate_dir, count)
    all_inputs: dict[int, ReceiptSource] = {}
    all_outcomes: dict[tuple[int, str, str], tuple[str, str]] = {}
    for (start, end), path in sorted(candidates.items()):
        receipt = parse_receipt(path, expected_paths)
        validate_candidate_shard(path, receipt, start, end, count, corpus_digest)
        for index, value in receipt.inputs.items():
            if index in all_inputs:
                raise PublicationError(f"duplicate candidate input index {index}")
            if value != reference.inputs[index]:
                raise PublicationError(f"input path or source BLAKE3 mismatch at index {index}: {value}")
            all_inputs[index] = value
        for key, value in receipt.outcomes.items():
            if key in all_outcomes:
                raise PublicationError(f"duplicate candidate outcome {key}")
            all_outcomes[key] = value
    if set(all_inputs) != set(range(count)):
        missing = sorted(set(range(count)) - set(all_inputs))[:10]
        raise PublicationError(f"candidate source coverage is incomplete: missing={missing}")
    expected_outcomes = set(reference.outcomes)
    if set(all_outcomes) != expected_outcomes:
        missing = sorted(expected_outcomes - set(all_outcomes))[:10]
        extra = sorted(set(all_outcomes) - expected_outcomes)[:10]
        raise PublicationError(f"candidate outcome coverage differs: missing={missing} extra={extra}")
    candidate = ParsedReceipt((), all_inputs, all_outcomes, ())
    compare_outcomes(reference, candidate, expected_outcomes)
    return f"corpus={count} outcomes={len(all_outcomes)} input_blake3_digest={corpus_digest}"


def validate_reference_inputs(
    reference_receipt: pathlib.Path,
    reference_binary: pathlib.Path,
    validation: pathlib.Path,
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
) -> tuple[dict[int, HistoricalManifestSource], dict[str, str], ParsedReceipt]:
    require_sha256(reference_receipt, REFERENCE_RECEIPT_SHA256, "approved reference receipt")
    require_sha256(reference_binary, REFERENCE_BINARY_SHA256, "approved reference binary")
    require_sha256(validation, REFERENCE_VALIDATION_SHA256, "approved reference validation")
    require_sha256(input_manifest, INPUT_MANIFEST_SHA256, "approved input manifest")
    validated = parse_validation(validation)
    if input_root != REFERENCE_INPUT_ROOT or validated["input_root"] != str(input_root):
        raise PublicationError(f"input root differs from the approved path: expected={REFERENCE_INPUT_ROOT} actual={input_root}")
    manifest_by_index, manifest_by_path = parse_input_manifest(input_manifest)
    reference = validate_reference_receipt(reference_receipt, manifest_by_index, CORPUS_BLAKE3)
    return manifest_by_index, manifest_by_path, reference


def validate_build_receipt_binding(
    receipt_path: pathlib.Path,
    candidate_root: pathlib.Path,
    candidate_commit: str,
    candidate_tree: str,
    binary: pathlib.Path,
    binary_sha256: str,
    expected_receipt_sha256: str | None = None,
) -> tuple[pathlib.Path, str]:
    receipt = regular_file(receipt_path, "candidate verification build receipt")
    if not under(receipt, candidate_root / ".work"):
        raise PublicationError(f"candidate build receipt must be under candidate source .work/: {receipt}")
    if expected_receipt_sha256 is not None:
        require_sha256(receipt, expected_receipt_sha256, "candidate verification build receipt")
    record = parse_build_receipt(receipt)
    if record["source_commit"] != candidate_commit:
        raise PublicationError(
            f"candidate build receipt source commit mismatch: receipt={record['source_commit']} actual={candidate_commit}"
        )
    if record["source_tree"] != candidate_tree:
        raise PublicationError(
            f"candidate build receipt source tree mismatch: receipt={record['source_tree']} actual={candidate_tree}"
        )
    if record["binary_path"] != str(binary):
        raise PublicationError(
            f"candidate build receipt binary path mismatch: receipt={record['binary_path']} actual={binary}"
        )
    if record["binary_sha256"] != binary_sha256:
        raise PublicationError("candidate build receipt binary hash mismatch")
    if record["helper_sha256"] != HELPER_SHA256:
        raise PublicationError("candidate build receipt helper hash is not pinned")
    validate_overlay_binding(record, candidate_root)
    contract = w9_build_contract()
    if record["builder_sha256"] != contract["W9_CACHE_BUILD_RUNNER_SHA256"]:
        raise PublicationError("candidate build receipt builder hash is not pinned")
    builder = regular_file(record["builder_path"], "pinned candidate build runner", executable=True)
    if record["builder_path"] != str(builder):
        raise PublicationError("candidate build receipt builder path is not canonical")
    require_sha256(builder, contract["W9_CACHE_BUILD_RUNNER_SHA256"], "pinned candidate build runner")
    if PINNED_IMAGE not in read_regular_text(builder):
        raise PublicationError("pinned candidate build runner does not name the approved image")
    return receipt, sha256_file(receipt)


def publication_work_root(
    candidate_root: pathlib.Path,
    source_record: dict[str, str] | None = None,
) -> pathlib.Path:
    candidate_root = real_directory(candidate_root, "candidate source")
    if source_record is None or source_record.get("schema") != SOURCE_COHORT_SCHEMA:
        return candidate_root / ".work"
    if source_record.get("candidate_root") != str(candidate_root):
        raise PublicationError("source-qualified output candidate root differs from its receipt")
    controller_root = real_directory(
        source_record.get("controller_root", ""), "source-qualified capture controller"
    )
    if source_record.get("controller_root") != str(controller_root):
        raise PublicationError("source-qualified output controller path differs from its receipt")
    if source_identity(controller_root) != (
        source_record.get("controller_commit"), source_record.get("controller_tree")
    ):
        raise PublicationError("source-qualified output controller identity differs from its receipt")
    return real_directory(controller_root / ".work", "source-qualified controller .work")


def reference_source_root(candidate_root: pathlib.Path) -> pathlib.Path:
    candidate_root = real_directory(candidate_root, "candidate source")
    if candidate_root.parent.name == "wt" and candidate_root.parent.parent.name == ".work":
        worktrees = candidate_root.parent
    else:
        worktrees = candidate_root / ".work/wt"
    return worktrees / "w9-adapter-ref-e240"


def _paths_overlap(left: pathlib.Path, right: pathlib.Path) -> bool:
    return under(left, right) or under(right, left)


def _canonical_protected_root(path: pathlib.Path, kind: str) -> pathlib.Path:
    path = pathlib.Path(os.path.abspath(path))
    missing: list[str] = []
    existing = path
    while not os.path.lexists(existing):
        if existing.parent == existing:
            raise PublicationError(f"{kind}: no existing path ancestor for {path}")
        missing.append(existing.name)
        existing = existing.parent
    canonical_existing = real_directory(existing, kind)
    return canonical_existing.joinpath(*reversed(missing))


def _source_qualified_protected_roots(
    candidate_root: pathlib.Path,
    source_record: dict[str, str],
) -> tuple[pathlib.Path, ...]:
    controller_root = real_directory(
        source_record["controller_root"], "source-qualified capture controller"
    )
    builder_path = regular_file(
        source_record["builder_path"], "source-qualified build runner", executable=True
    )
    if len(builder_path.parents) < 3:
        raise PublicationError("source-qualified builder path has no worktree root")
    builder_root = builder_path.parents[2]
    common_root = git_common_controller_root(builder_path.parent)
    roots = [
        pathlib.Path(source_record["candidate_root"]),
        pathlib.Path(source_record["input_root"]),
        pathlib.Path(source_record["program_root"]),
        common_root / ".work/wt",
        common_root / ".work/tmp/w9-container-scratch",
        reference_source_root(candidate_root),
    ]
    if builder_root not in {controller_root, common_root}:
        roots.append(builder_root)
    return tuple(
        dict.fromkeys(
            _canonical_protected_root(root, "source-qualified protected root")
            for root in roots
        )
    )


def _validate_publication_output_path(
    path: pathlib.Path,
    candidate_root: pathlib.Path,
    source_record: dict[str, str] | None,
    kind: str,
) -> None:
    work_root = publication_work_root(candidate_root, source_record)
    if not under(path, work_root):
        raise PublicationError(f"{kind} must be under publication work root: {path}")
    if source_record is not None and source_record.get("schema") == SOURCE_COHORT_SCHEMA:
        protected_roots = _source_qualified_protected_roots(candidate_root, source_record)
        if any(_paths_overlap(path, protected_root) for protected_root in protected_roots):
            raise PublicationError(f"{kind} overlaps a source-qualified protected root: {path}")


def new_work_file_path(
    value: str | pathlib.Path,
    candidate_root: pathlib.Path,
    kind: str,
    source_record: dict[str, str] | None = None,
) -> pathlib.Path:
    raw = pathlib.Path(value).expanduser()
    if not raw.is_absolute():
        raw = pathlib.Path.cwd() / raw
    absolute = pathlib.Path(os.path.abspath(raw))
    parent = real_directory(absolute.parent, f"{kind} parent")
    path = parent / absolute.name
    _validate_publication_output_path(path, candidate_root, source_record, kind)
    if path.name in {"", ".", ".."}:
        raise PublicationError(f"{kind} must name a new file: {path}")
    if os.path.lexists(path):
        raise PublicationError(f"{kind} already exists: {path}")
    return path


def write_new_receipt(path: pathlib.Path, record: dict[str, str], fields: tuple[str, ...], kind: str) -> None:
    if set(record) != set(fields):
        raise PublicationError(f"{kind} fields do not match its schema")
    data = "".join(f"{key}\t{record[key]}\n" for key in fields).encode("utf-8")
    parent_descriptor = open_absolute_nofollow(path.parent, directory=True)
    descriptor = -1
    created = False
    try:
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        descriptor = os.open(path.name, flags, 0o600, dir_fd=parent_descriptor)
        created = True
        view = memoryview(data)
        while view:
            written = os.write(descriptor, view)
            if written == 0:
                raise OSError(f"short write while recording {kind}")
            view = view[written:]
        os.fsync(descriptor)
        os.close(descriptor)
        descriptor = -1
        os.fsync(parent_descriptor)
    except FileExistsError as error:
        raise PublicationError(f"{kind} appeared during creation: {path}") from error
    except BaseException:
        if descriptor >= 0:
            os.close(descriptor)
        if created:
            try:
                os.unlink(path.name, dir_fd=parent_descriptor)
                os.fsync(parent_descriptor)
            except FileNotFoundError:
                pass
        raise
    finally:
        os.close(parent_descriptor)


def build_harness(args: argparse.Namespace) -> None:
    root = real_directory(args.candidate_root, "candidate source")
    commit, tree = source_identity(root)
    helper = regular_file(root / HELPER_RELATIVE_PATH, "staged parity helper")
    helper_sha256 = require_sha256(helper, HELPER_SHA256, "staged parity helper")
    source_tests = regular_file(root / "tests/sema.rs", "candidate semantic integration tests")
    source_tests_bytes = read_regular_bytes(source_tests)
    overlay_bytes = parity_overlay_bytes(source_tests_bytes)
    overlay_sha256 = hashlib.sha256(overlay_bytes).hexdigest()
    builder = regular_file(APPROVED_BUILD_RUNNER, "pinned candidate build runner", executable=True)
    contract = w9_build_contract()
    builder_sha256 = require_sha256(builder, contract["W9_CACHE_BUILD_RUNNER_SHA256"], "pinned candidate build runner")
    if PINNED_IMAGE not in read_regular_text(builder):
        raise PublicationError("pinned candidate build runner does not name the approved image")
    receipt_path = new_work_file_path(args.receipt, root, "candidate build receipt")
    docker_environment = local_docker_environment()
    scratch_parent = candidate_scratch_parent(root)
    binary = new_work_file_path(
        root / ".work/consolidation" / f"w9-parity-harness-{secrets.token_hex(8)}",
        root,
        "candidate integration test harness",
    )
    with scratch_directory(scratch_parent) as scratch_directory_path:
        scratch_root = scratch_directory_path / "source"
        clone = subprocess.run(
            ["git", "clone", "--quiet", "--no-local", "--no-checkout", "--depth=1", str(root), str(scratch_root)],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        if clone.returncode != 0:
            detail = clone.stderr.strip() or clone.stdout.strip()
            raise PublicationError(f"could not create source-matched harness checkout: {detail}")
        git_output(scratch_root, "checkout", "--quiet", "--detach", commit)
        scratch_commit = git_output(scratch_root, "rev-parse", "HEAD")
        scratch_tree = git_output(scratch_root, "rev-parse", "HEAD^{tree}")
        if (scratch_commit, scratch_tree) != (commit, tree):
            raise PublicationError("scratch harness checkout source identity differs from candidate")
        if git_output(scratch_root, "status", "--porcelain", "--untracked-files=all"):
            raise PublicationError("scratch harness checkout is dirty before applying the parity overlay")
        exclude_work_from_git(scratch_root)
        scratch_tests = regular_file(scratch_root / "tests/sema.rs", "scratch semantic integration tests")
        if read_regular_bytes(scratch_tests) != source_tests_bytes:
            raise PublicationError("scratch harness checkout semantic tests differ from candidate source")
        scratch_helper = stage_helper(scratch_root, helper, helper_sha256)
        scratch_tests.write_bytes(overlay_bytes)
        if sha256_file(scratch_tests) != overlay_sha256:
            raise PublicationError("scratch parity overlay hash differs from the recorded overlay")
        require_only_parity_overlay(
            scratch_root,
            "scratch harness checkout contains changes outside the parity overlay",
        )
        command = [
            str(builder),
            str(scratch_root),
            "build",
            "cargo",
            "test",
            "--locked",
            "--profile",
            contract["W9_CACHE_BUILD_PROFILE"],
            "--target",
            contract["W9_CACHE_BUILD_TARGET"],
            "-p",
            "xsh",
            "--test",
            "integration",
            "--no-run",
            "--message-format=json-render-diagnostics",
        ]
        try:
            result = subprocess.run(
                command,
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=docker_environment,
            )
        except OSError as error:
            raise PublicationError(f"could not start pinned integration harness build: {error}") from error
        if result.returncode != 0:
            detail = result.stderr.strip() or result.stdout.strip()
            raise PublicationError(f"pinned integration harness build failed with status {result.returncode}: {detail}")
        scratch_binary = cargo_integration_executable(result.stdout, scratch_root)
        if (git_output(scratch_root, "rev-parse", "HEAD"),
                git_output(scratch_root, "rev-parse", "HEAD^{tree}")) != (commit, tree):
            raise PublicationError("scratch harness source identity changed during build")
        require_only_parity_overlay(
            scratch_root,
            "scratch harness changes escaped the parity overlay during build",
        )
        if sha256_file(scratch_helper) != helper_sha256 or sha256_file(scratch_tests) != overlay_sha256:
            raise PublicationError("scratch parity overlay or helper changed during build")
        run_harness_test_listing(builder, scratch_root, scratch_binary, docker_environment)
        scratch_binary_sha256 = sha256_file(scratch_binary)
        binary = copy_executable(scratch_binary, binary)
        require_sha256(binary, scratch_binary_sha256, "copied candidate integration test harness")
        run_harness_test_listing(builder, root, binary, docker_environment)
    if source_identity(root) != (commit, tree):
        raise PublicationError("candidate source changed during integration harness build")
    require_sha256(builder, builder_sha256, "pinned candidate build runner")
    require_sha256(helper, helper_sha256, "staged parity helper")
    binary_sha256 = sha256_file(binary)
    record = {
        "schema": contract["W9_CACHE_BUILD_SCHEMA"],
        "artifact_kind": BUILD_ARTIFACT_KIND,
        "source_commit": commit,
        "source_tree": tree,
        "binary_path": str(binary),
        "binary_sha256": binary_sha256,
        "target": contract["W9_CACHE_BUILD_TARGET"],
        "profile": contract["W9_CACHE_BUILD_PROFILE"],
        "builder_image": contract["W9_CACHE_BUILD_IMAGE"],
        "builder_path": str(builder),
        "builder_sha256": builder_sha256,
        "helper_sha256": helper_sha256,
        "overlay_sha256": overlay_sha256,
    }
    write_new_receipt(receipt_path, record, BUILD_RECEIPT_FIELDS, "candidate build receipt")
    validate_build_receipt_binding(receipt_path, root, commit, tree, binary, binary_sha256)
    print(f"candidate harness: {binary}")
    print(f"candidate build receipt: {receipt_path}")


def validate_candidate_files(record: dict[str, str]) -> tuple[pathlib.Path, pathlib.Path, pathlib.Path, pathlib.Path]:
    root = real_directory(record["candidate_root"], "candidate source")
    commit, tree = source_identity(root)
    if commit != record["candidate_commit"]:
        raise PublicationError(f"candidate commit mismatch: receipt={record['candidate_commit']} actual={commit}")
    if tree != record["candidate_tree"]:
        raise PublicationError(f"candidate tree mismatch: receipt={record['candidate_tree']} actual={tree}")
    binary = regular_file(record["candidate_binary"], "candidate test binary", executable=True)
    helper = regular_file(record["helper"], "candidate parity helper")
    wrapper = regular_file(record["wrapper"], "native test wrapper", executable=True)
    wrapper_root = real_directory(record["wrapper_root"], "native test wrapper root")
    validate_wrapper_root(wrapper_root, root)
    if not under(binary, root):
        raise PublicationError(f"candidate binary must be inside candidate source root: {binary}")
    if not under(helper, root):
        raise PublicationError(f"candidate parity helper must be inside candidate source root: {helper}")
    if helper != root / HELPER_RELATIVE_PATH:
        raise PublicationError(f"candidate parity helper was not staged at the approved path: {helper}")
    require_sha256(binary, record["candidate_binary_sha256"], "candidate test binary")
    build_receipt, _build_receipt_sha256 = validate_build_receipt_binding(
        pathlib.Path(record["build_receipt"]),
        root,
        commit,
        tree,
        binary,
        record["candidate_binary_sha256"],
        record["build_receipt_sha256"],
    )
    if record["build_receipt"] != str(build_receipt):
        raise PublicationError("candidate build receipt path is not canonical")
    require_sha256(helper, record["helper_sha256"], "candidate parity helper")
    require_sha256(wrapper, WRAPPER_SHA256, "pinned native test wrapper")
    if PINNED_IMAGE not in read_regular_text(wrapper):
        raise PublicationError("pinned native test wrapper does not name the approved image")
    if record["helper_sha256"] != HELPER_SHA256:
        raise PublicationError(f"candidate parity helper is not the approved helper: {record['helper_sha256']}")
    if record["wrapper_sha256"] != WRAPPER_SHA256:
        raise PublicationError(f"candidate wrapper receipt hash differs from approved wrapper: {record['wrapper_sha256']}")
    runner = validate_runner_binding(record["runner"], record["runner_sha256"])
    for field, path in (
        ("candidate_root", root),
        ("candidate_binary", binary),
        ("helper", helper),
        ("wrapper", wrapper),
        ("wrapper_root", wrapper_root),
        ("runner", runner),
    ):
        if record[field] != str(path):
            raise PublicationError(f"candidate receipt {field} path is not canonical")
    return root, binary, helper, wrapper


def validate_source_receipt_record(record: dict[str, str]) -> dict[str, str]:
    schema = record.get("schema")
    fields = SOURCE_RECEIPT_FIELDS if schema == SOURCE_COHORT_SCHEMA else SOURCE_RECEIPT_FIELDS_3
    if schema not in SOURCE_COHORT_SCHEMAS:
        raise PublicationError(f"unsupported source-qualified receipt schema {schema!r}")
    if set(record) != set(fields):
        missing = sorted(set(fields) - set(record))
        extra = sorted(set(record) - set(fields))
        raise PublicationError(f"source-qualified receipt fields differ: missing={missing} extra={extra}")
    if record["role"] != "source-cohort":
        raise PublicationError("source-qualified receipt schema or role differs")
    for field in fields:
        value = record[field]
        if not value or any(char in value for char in "\r\n\t"):
            raise PublicationError(f"source-qualified receipt has invalid {field}")
    for field in ("candidate_commit", "candidate_tree"):
        if not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", record[field]):
            raise PublicationError(f"source-qualified receipt has malformed {field}")
    for field in ("input_manifest_sha256", "corpus_blake3", "builder_sha256", "linker_sha256", "runner_sha256"):
        if not re.fullmatch(r"[0-9a-f]{64}", record[field]):
            raise PublicationError(f"source-qualified receipt has malformed {field}")
    root = real_directory(record["candidate_root"], "source-qualified candidate source")
    if record["candidate_root"] != str(root):
        raise PublicationError("source-qualified candidate root path is not canonical")
    validate_work_ignored(root)
    if source_identity(root) != (record["candidate_commit"], record["candidate_tree"]):
        raise PublicationError("source-qualified candidate identity differs")
    if git_common_controller_root(root) != CONTROLLER_ROOT:
        raise PublicationError("source-qualified candidate does not share the publication controller Git root")
    input_root = real_directory(record["input_root"], "source-qualified input root")
    input_manifest = regular_file(record["input_manifest"], "source-qualified input manifest")
    if record["input_root"] != str(input_root) or record["input_manifest"] != str(input_manifest):
        raise PublicationError("source-qualified input paths are not canonical")
    if record["input_manifest_sha256"] != INPUT_MANIFEST_SHA256:
        raise PublicationError("source-qualified input manifest differs from its pin")
    require_sha256(input_manifest, INPUT_MANIFEST_SHA256, "source-qualified input manifest")
    if record["corpus_blake3"] != CORPUS_BLAKE3:
        raise PublicationError("source-qualified corpus digest differs from its pin")
    builder = regular_file(record["builder_path"], "source-qualified build runner", executable=True)
    if git_common_controller_root(builder.parent) != CONTROLLER_ROOT:
        raise PublicationError("source-qualified build runner uses a different controller Git root")
    if SOURCE_BUILDER_SHA256 == "" or require_sha256(builder, SOURCE_BUILDER_SHA256, "source-qualified build runner") != record["builder_sha256"]:
        raise PublicationError("source-qualified build runner hash differs")
    if (
        record["builder_image"] != SOURCE_BUILD_IMAGE
        or record["build_target"] != SOURCE_BUILD_TARGET
        or record["build_profile"] != SOURCE_BUILD_PROFILE
        or record["rustflags"] != SOURCE_RUSTFLAGS
    ):
        raise PublicationError("source-qualified build contract differs")
    linker = regular_file(record["linker_path"], "source-qualified musl linker")
    if linker != SOURCE_LINKER_PATH or require_sha256(linker, SOURCE_LINKER_SHA256, "source-qualified musl linker") != record["linker_sha256"]:
        raise PublicationError("source-qualified musl linker identity differs")
    if schema == SOURCE_COHORT_SCHEMA_3:
        candidate_runner = regular_file(root / "dev/consolidation/w9-publication-run.py", "source-qualified publication runner")
        if record["runner"] != str(candidate_runner):
            raise PublicationError("source-qualified publication runner path is not canonical")
        require_sha256(candidate_runner, record["runner_sha256"], "source-qualified publication runner")
        return record
    controller_root = real_directory(record["controller_root"], "source-qualified capture controller")
    if controller_root != ROOT or record["controller_root"] != str(controller_root):
        raise PublicationError("source-qualified capture controller root differs")
    if re.fullmatch(r"[0-9a-f]{40}", record["controller_commit"]) is None or re.fullmatch(
        r"[0-9a-f]{40}", record["controller_tree"]
    ) is None:
        raise PublicationError("source-qualified capture controller identity is malformed")
    if source_identity(controller_root) != (record["controller_commit"], record["controller_tree"]):
        raise PublicationError("source-qualified capture controller identity differs")
    if git_output(controller_root, "status", "--porcelain=v1", "--untracked-files=all"):
        raise PublicationError("source-qualified capture controller is not clean")
    if record["runner"] != str(controller_runner_path(controller_root)):
        raise PublicationError("source-qualified controller runner path differs")
    controller_runner = regular_file(record["runner"], "source-qualified controller runner")
    require_sha256(controller_runner, record["runner_sha256"], "source-qualified controller runner")
    if re.fullmatch(r"[0-9a-f]{40}", record["program_commit"]) is None or re.fullmatch(
        r"[0-9a-f]{40}", record["program_tree"]
    ) is None:
        raise PublicationError("source-qualified program snapshot identity is malformed")
    program_root = real_directory(record["program_root"], "source-qualified common program snapshot")
    if program_root != PROGRAM_SNAPSHOT_ROOT.resolve(strict=True) or record["program_root"] != str(program_root):
        raise PublicationError("source-qualified common program snapshot path differs")
    try:
        snapshot = w9_program_snapshot.validate_program_snapshot(program_root)
    except w9_program_snapshot.ProgramSnapshotError as error:
        raise PublicationError(f"source-qualified common program snapshot is invalid: {error}") from error
    if (snapshot.commit, snapshot.tree) != (record["program_commit"], record["program_tree"]):
        raise PublicationError("source-qualified common program snapshot identity differs")
    if record["program_config_path"] != str(snapshot.config_path):
        raise PublicationError("source-qualified common program config path differs")
    snapshot_helper = regular_file(record["program_snapshot_helper"], "program snapshot validator")
    if snapshot_helper != PROGRAM_SNAPSHOT_HELPER or record["program_snapshot_helper"] != str(snapshot_helper):
        raise PublicationError("program snapshot validator path differs")
    if require_sha256(snapshot_helper, PROGRAM_SNAPSHOT_HELPER_SHA256, "program snapshot validator") != record[
        "program_snapshot_helper_sha256"
    ]:
        raise PublicationError("program snapshot validator hash differs")
    return record


def controller_runner_path(controller_root: pathlib.Path) -> pathlib.Path:
    return controller_root / "dev/consolidation/w9-publication-run.py"


def is_source_cohort_schema(schema: str | None) -> bool:
    return schema in SOURCE_COHORT_SCHEMAS


def parse_source_receipt(path: pathlib.Path) -> dict[str, str]:
    record: dict[str, str] = {}
    for line_number, line in enumerate(read_regular_text(path).splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 2 or not fields[0] or not fields[1]:
            raise PublicationError(f"{path}: malformed source-qualified receipt row {line_number}")
        key, value = fields
        if key in record:
            raise PublicationError(f"{path}: duplicate source-qualified receipt field {key}")
        record[key] = value
    validate_source_receipt_record(record)
    return record


def validate_runner_binding(value: str | pathlib.Path, digest: str) -> pathlib.Path:
    current = pathlib.Path(__file__).resolve()
    runner = regular_file(value, "publication runner")
    if runner != current:
        raise PublicationError(f"candidate receipt publication runner differs from current runner: {runner}")
    require_sha256(current, digest, "publication runner")
    return current


def validate_receipt_shape(record: dict[str, str]) -> None:
    if set(record) != set(RECEIPT_FIELDS):
        missing = sorted(set(RECEIPT_FIELDS) - set(record))
        extra = sorted(set(record) - set(RECEIPT_FIELDS))
        raise PublicationError(f"candidate receipt fields differ: missing={missing} extra={extra}")
    if record["schema"] != "2":
        raise PublicationError(f"unsupported candidate receipt schema {record['schema']!r}")
    for field in RECEIPT_FIELDS:
        value = record[field]
        if not value or any(char in value for char in "\r\n\t"):
            raise PublicationError(f"candidate receipt has invalid {field}")
    for field in ("candidate_binary_sha256", "build_receipt_sha256", "helper_sha256", "wrapper_sha256",
                  "reference_receipt_sha256", "reference_binary_sha256",
                  "reference_validation_sha256", "input_manifest_sha256", "runner_sha256"):
        if not re.fullmatch(r"[0-9a-f]{64}", record[field]):
            raise PublicationError(f"candidate receipt has malformed {field}")
    for field in ("candidate_commit", "candidate_tree"):
        if not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", record[field]):
            raise PublicationError(f"candidate receipt has malformed {field}")


def parse_candidate_receipt(path: pathlib.Path) -> dict[str, str]:
    record: dict[str, str] = {}
    for line_number, line in enumerate(read_regular_text(path).splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 2 or not fields[0] or not fields[1]:
            raise PublicationError(f"{path}: malformed candidate receipt row {line_number}")
        key, value = fields
        if key in record:
            raise PublicationError(f"{path}: duplicate candidate receipt field {key}")
        record[key] = value
    if is_source_cohort_schema(record.get("schema")):
        validate_source_receipt_record(record)
        return record
    validate_receipt_shape(record)
    return record


def validate_recorded_pins(record: dict[str, str]) -> None:
    pins = {
        "reference_receipt_sha256": REFERENCE_RECEIPT_SHA256,
        "reference_binary_sha256": REFERENCE_BINARY_SHA256,
        "reference_validation_sha256": REFERENCE_VALIDATION_SHA256,
        "input_manifest_sha256": INPUT_MANIFEST_SHA256,
        "corpus_blake3": CORPUS_BLAKE3,
    }
    for field, expected in pins.items():
        if record[field] != expected:
            raise PublicationError(f"candidate receipt {field} differs from approved pin")


def seal_candidate(args: argparse.Namespace) -> None:
    root = real_directory(args.candidate_root, "candidate source")
    commit, tree = source_identity(root)
    binary = regular_file(args.candidate_binary, "candidate test binary", executable=True)
    helper = regular_file(args.helper, "candidate parity helper")
    if helper != root / HELPER_RELATIVE_PATH:
        raise PublicationError(f"candidate parity helper must be staged at {root / HELPER_RELATIVE_PATH}")
    wrapper = regular_file(args.wrapper, "native test wrapper", executable=True)
    build_receipt = regular_file(args.build_receipt, "candidate verification build receipt")
    reference_receipt = regular_file(args.reference_receipt, "approved reference receipt")
    reference_binary = regular_file(args.reference_binary, "approved reference binary", executable=True)
    validation = regular_file(args.reference_validation, "approved reference validation")
    input_root = real_directory(args.input_root, "frozen corpus root")
    input_manifest = regular_file(args.input_manifest, "approved input manifest")
    if not under(binary, root) or not under(helper, root):
        raise PublicationError("candidate binary and parity helper must be inside candidate source root")
    wrapper_root = wrapper_root_for(root)
    validate_wrapper_root(wrapper_root, root)
    require_sha256(reference_receipt, REFERENCE_RECEIPT_SHA256, "approved reference receipt")
    require_sha256(reference_binary, REFERENCE_BINARY_SHA256, "approved reference binary")
    require_sha256(validation, REFERENCE_VALIDATION_SHA256, "approved reference validation")
    require_sha256(input_manifest, INPUT_MANIFEST_SHA256, "approved input manifest")
    helper_sha = require_sha256(helper, HELPER_SHA256, "candidate parity helper")
    build_receipt, build_receipt_sha256 = validate_build_receipt_binding(
        build_receipt,
        root,
        commit,
        tree,
        binary,
        sha256_file(binary),
    )
    wrapper_sha = require_sha256(wrapper, WRAPPER_SHA256, "pinned native test wrapper")
    if PINNED_IMAGE not in read_regular_text(wrapper):
        raise PublicationError("pinned native test wrapper does not name the approved image")
    manifest_by_index, _manifest_by_path = parse_input_manifest(input_manifest)
    validated = parse_validation(validation)
    if input_root != REFERENCE_INPUT_ROOT or validated["input_root"] != str(input_root):
        raise PublicationError(f"input root differs from approved path: expected={REFERENCE_INPUT_ROOT} actual={input_root}")
    validate_reference_receipt(reference_receipt, manifest_by_index, CORPUS_BLAKE3)
    receipt_path = pathlib.Path(os.path.abspath(pathlib.Path(args.receipt).expanduser()))
    work_root = root / ".work"
    if not under(receipt_path, work_root) or receipt_path.parent.resolve(strict=True) != receipt_path.parent:
        raise PublicationError(f"candidate receipt must be in candidate source .work/: {receipt_path}")
    if not receipt_path.parent.is_dir():
        raise PublicationError(f"candidate receipt parent does not exist: {receipt_path.parent}")
    record = {
        "schema": "2",
        "candidate_root": str(root),
        "candidate_commit": commit,
        "candidate_tree": tree,
        "candidate_binary": str(binary),
        "candidate_binary_sha256": sha256_file(binary),
        "build_receipt": str(build_receipt),
        "build_receipt_sha256": build_receipt_sha256,
        "helper": str(helper),
        "helper_sha256": helper_sha,
        "wrapper": str(wrapper),
        "wrapper_sha256": wrapper_sha,
        "wrapper_root": str(wrapper_root),
        "reference_receipt": str(reference_receipt),
        "reference_receipt_sha256": REFERENCE_RECEIPT_SHA256,
        "reference_binary": str(reference_binary),
        "reference_binary_sha256": REFERENCE_BINARY_SHA256,
        "reference_validation": str(validation),
        "reference_validation_sha256": REFERENCE_VALIDATION_SHA256,
        "input_root": str(input_root),
        "input_manifest": str(input_manifest),
        "input_manifest_sha256": INPUT_MANIFEST_SHA256,
        "corpus_blake3": CORPUS_BLAKE3,
        "runner": str(pathlib.Path(__file__).resolve()),
        "runner_sha256": sha256_file(pathlib.Path(__file__).resolve()),
    }
    validate_receipt_shape(record)
    flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
    parent_descriptor = open_absolute_nofollow(receipt_path.parent, directory=True)
    try:
        descriptor = os.open(receipt_path.name, flags, 0o600, dir_fd=parent_descriptor)
    except FileExistsError as error:
        os.close(parent_descriptor)
        raise PublicationError(f"candidate receipt already exists: {receipt_path}") from error
    except BaseException:
        os.close(parent_descriptor)
        raise
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            output.write("".join(f"{key}\t{record[key]}\n" for key in RECEIPT_FIELDS))
            output.flush()
            os.fsync(output.fileno())
        os.fsync(parent_descriptor)
    except BaseException:
        try:
            os.unlink(receipt_path.name, dir_fd=parent_descriptor)
        except FileNotFoundError:
            pass
        raise
    finally:
        os.close(parent_descriptor)
    print(f"candidate receipt: {receipt_path}")


def validate_source_cohort_inputs(
    record: dict[str, str],
) -> tuple[dict[int, ManifestSource], dict[str, str]]:
    validate_source_receipt_record(record)
    input_root = pathlib.Path(record["input_root"])
    input_manifest = pathlib.Path(record["input_manifest"])
    by_index, by_path = validate_source_qualified_inputs(input_root, input_manifest)
    if record["schema"] == SOURCE_COHORT_SCHEMA:
        try:
            snapshot = w9_program_snapshot.validate_program_snapshot(
                pathlib.Path(record["program_root"])
            )
        except w9_program_snapshot.ProgramSnapshotError as error:
            raise PublicationError(f"source-qualified program snapshot is invalid: {error}") from error
        if (
            snapshot.commit != record["program_commit"]
            or snapshot.tree != record["program_tree"]
            or str(snapshot.config_path) != record["program_config_path"]
        ):
            raise PublicationError("source-qualified program snapshot identity differs")
        validate_source_program_entries(snapshot, by_index.values())
    return by_index, by_path


def validate_all_inputs(
    record: dict[str, str],
) -> tuple[dict[int, HistoricalManifestSource], dict[str, str], ParsedReceipt]:
    validate_receipt_shape(record)
    validate_recorded_pins(record)
    root, _binary, _helper, _wrapper = validate_candidate_files(record)
    if record["candidate_root"] != str(root):
        raise PublicationError("candidate source root path is not canonical")
    reference_receipt = regular_file(record["reference_receipt"], "approved reference receipt")
    reference_binary = regular_file(record["reference_binary"], "approved reference binary", executable=True)
    validation = regular_file(record["reference_validation"], "approved reference validation")
    input_root = real_directory(record["input_root"], "frozen corpus root")
    input_manifest = regular_file(record["input_manifest"], "approved input manifest")
    if record["reference_receipt"] != str(reference_receipt):
        raise PublicationError("reference receipt path is not canonical")
    if record["reference_binary"] != str(reference_binary):
        raise PublicationError("reference binary path is not canonical")
    if record["reference_validation"] != str(validation):
        raise PublicationError("reference validation path is not canonical")
    if record["input_root"] != str(input_root) or record["input_manifest"] != str(input_manifest):
        raise PublicationError("input corpus path is not canonical")
    paths_by_index, paths_by_path, reference = validate_reference_inputs(
        reference_receipt, reference_binary, validation, input_root, input_manifest
    )
    verify_corpus_tree(input_root, paths_by_path)
    return paths_by_index, paths_by_path, reference


def seal_source_cohort(args: argparse.Namespace) -> None:
    root = real_directory(args.candidate_root, "source-qualified candidate source")
    commit, tree = source_identity(root)
    validate_work_ignored(root)
    input_root = real_directory(args.input_root, "source-qualified input root")
    input_manifest = regular_file(args.input_manifest, "source-qualified input manifest")
    paths_by_index, _paths_by_path = validate_source_qualified_inputs(input_root, input_manifest)
    if len(paths_by_index) != 1690:
        raise PublicationError(f"source-qualified cohort count differs: {len(paths_by_index)}")
    if git_common_controller_root(root) != CONTROLLER_ROOT:
        raise PublicationError("source-qualified candidate does not share the publication controller Git root")
    builder = regular_file(SOURCE_BUILDER_PATH, "source-qualified build runner", executable=True)
    if SOURCE_BUILDER_SHA256 == "":
        raise PublicationError("source-qualified build runner is not pinned")
    builder_sha256 = require_sha256(builder, SOURCE_BUILDER_SHA256, "source-qualified build runner")
    linker = regular_file(SOURCE_LINKER_PATH, "source-qualified musl linker")
    linker_sha256 = require_sha256(linker, SOURCE_LINKER_SHA256, "source-qualified musl linker")
    controller_root = real_directory(ROOT, "source-qualified capture controller")
    controller_commit, controller_tree = source_identity(controller_root)
    if git_output(controller_root, "status", "--porcelain=v1", "--untracked-files=all"):
        raise PublicationError("source-qualified capture controller is not clean")
    program_snapshot = w9_program_snapshot.validate_program_snapshot(PROGRAM_SNAPSHOT_ROOT)
    program_root = real_directory(program_snapshot.root, "source-qualified common program snapshot")
    snapshot_helper = regular_file(PROGRAM_SNAPSHOT_HELPER, "program snapshot validator")
    snapshot_helper_sha256 = require_sha256(
        snapshot_helper, PROGRAM_SNAPSHOT_HELPER_SHA256, "program snapshot validator"
    )
    runner_path = regular_file(controller_runner_path(controller_root), "source-qualified controller runner")
    record = {
        "schema": SOURCE_COHORT_SCHEMA,
        "role": "source-cohort",
        "candidate_root": str(root),
        "candidate_commit": commit,
        "candidate_tree": tree,
        "input_root": str(input_root),
        "input_manifest": str(input_manifest),
        "input_manifest_sha256": INPUT_MANIFEST_SHA256,
        "corpus_blake3": CORPUS_BLAKE3,
        "builder_path": str(builder),
        "builder_sha256": builder_sha256,
        "builder_image": SOURCE_BUILD_IMAGE,
        "build_target": SOURCE_BUILD_TARGET,
        "build_profile": SOURCE_BUILD_PROFILE,
        "rustflags": SOURCE_RUSTFLAGS,
        "linker_path": str(linker),
        "linker_sha256": linker_sha256,
        "controller_root": str(controller_root),
        "controller_commit": controller_commit,
        "controller_tree": controller_tree,
        "program_root": str(program_root),
        "program_commit": program_snapshot.commit,
        "program_tree": program_snapshot.tree,
        "program_config_path": str(program_snapshot.config_path),
        "program_snapshot_helper": str(snapshot_helper),
        "program_snapshot_helper_sha256": snapshot_helper_sha256,
        "runner": str(runner_path),
        "runner_sha256": sha256_file(runner_path),
    }
    validate_source_program_entries(program_snapshot, paths_by_index.values())
    validate_source_receipt_record(record)
    receipt_path = new_work_file_path(args.receipt, root, "source-qualified cohort receipt")
    write_new_receipt(receipt_path, record, SOURCE_RECEIPT_FIELDS, "source-qualified cohort receipt")
    print(f"source-qualified receipt: {receipt_path}")


def output_lock_path(output: pathlib.Path) -> pathlib.Path:
    return output.parent / f".{output.name}.lock"


def global_runner_lock_path(candidate_root: pathlib.Path) -> pathlib.Path:
    common_git_directory = pathlib.Path(git_output(candidate_root, "rev-parse", "--git-common-dir"))
    if not common_git_directory.is_absolute():
        common_git_directory = candidate_root / common_git_directory
    common_git_directory = real_directory(common_git_directory, "shared candidate git directory")
    shared_work = real_directory(common_git_directory.parent / ".work", "shared publication .work")
    return shared_work / RUNNER_LOCK_NAME


def check_output_available(output: pathlib.Path) -> None:
    lock = output_lock_path(output)
    if output.exists() or output.is_symlink():
        raise PublicationError(f"refusing to reuse existing output path: {output}")
    if lock.exists() or lock.is_symlink():
        raise PublicationError(f"refusing to run while output lock exists: {lock}")


def claim_output(output: pathlib.Path) -> tuple[pathlib.Path, int, int]:
    check_output_available(output)
    lock = output_lock_path(output)
    parent_descriptor = open_absolute_nofollow(output.parent, directory=True)
    try:
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        descriptor = os.open(lock.name, flags, 0o600, dir_fd=parent_descriptor)
    except FileExistsError as error:
        os.close(parent_descriptor)
        raise PublicationError(f"output lock appeared during claim: {lock}") from error
    except BaseException:
        os.close(parent_descriptor)
        raise
    output_created = False
    try:
        os.write(descriptor, f"pid={os.getpid()}\noutput={output}\n".encode())
        os.fsync(descriptor)
        os.mkdir(output.name, dir_fd=parent_descriptor)
        output_created = True
        os.fsync(parent_descriptor)
    except BaseException:
        os.close(descriptor)
        if output_created:
            os.rmdir(output.name, dir_fd=parent_descriptor)
        try:
            os.unlink(lock.name, dir_fd=parent_descriptor)
        finally:
            os.close(parent_descriptor)
        raise
    return lock, descriptor, parent_descriptor


def claim_global_runner_lock(candidate_root: pathlib.Path, output: pathlib.Path) -> LockClaim:
    lock = global_runner_lock_path(candidate_root)
    work_root = lock.parent
    parent_descriptor = open_absolute_nofollow(work_root, directory=True)
    descriptor = -1
    created = False
    try:
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        descriptor = os.open(lock.name, flags, 0o600, dir_fd=parent_descriptor)
        created = True
        content = f"pid={os.getpid()}\noutput={output}\n".encode()
        view = memoryview(content)
        while view:
            written = os.write(descriptor, view)
            if written == 0:
                raise OSError("short write while claiming W9 publication runner lock")
            view = view[written:]
        os.fsync(descriptor)
        os.fsync(parent_descriptor)
        return LockClaim(lock, descriptor, parent_descriptor)
    except FileExistsError as error:
        os.close(parent_descriptor)
        raise PublicationError(f"another W9 publication run holds the shared lock: {lock}") from error
    except BaseException:
        if descriptor >= 0:
            os.close(descriptor)
        if created:
            try:
                os.unlink(lock.name, dir_fd=parent_descriptor)
                os.fsync(parent_descriptor)
            except FileNotFoundError:
                pass
        os.close(parent_descriptor)
        raise


def close_lock_claim(claim: LockClaim) -> list[BaseException]:
    errors: list[BaseException] = []
    for descriptor in (claim.descriptor, claim.parent_descriptor):
        try:
            os.close(descriptor)
        except BaseException as error:
            errors.append(error)
    return errors


def release_lock_claim(claim: LockClaim) -> None:
    errors: list[BaseException] = []
    try:
        os.unlink(claim.path.name, dir_fd=claim.parent_descriptor)
    except FileNotFoundError:
        pass
    except BaseException as error:
        errors.append(error)
    try:
        os.fsync(claim.parent_descriptor)
    except BaseException as error:
        errors.append(error)
    errors.extend(close_lock_claim(claim))
    if errors:
        raise PublicationError(f"could not fully release lock {claim.path}: {errors[0]}") from errors[0]


def retain_lock_claim(claim: LockClaim) -> None:
    errors = close_lock_claim(claim)
    if errors:
        raise PublicationError(f"could not close retained lock {claim.path}: {errors[0]}") from errors[0]


@contextlib.contextmanager
def claim_run_resources(
    candidate_root: pathlib.Path,
    output: pathlib.Path,
    completed: list[tuple[int, int]],
):
    previous_sigterm = signal.signal(signal.SIGTERM, handle_termination)
    previous_sigint = signal.signal(signal.SIGINT, handle_termination)
    blocked_signals = {signal.SIGTERM, signal.SIGINT}
    original_mask: set[signal.Signals] | None = None
    global_claim: LockClaim | None = None
    output_claim: LockClaim | None = None
    primary_error: BaseException | None = None
    try:
        original_mask = signal.pthread_sigmask(signal.SIG_BLOCK, blocked_signals)
        try:
            global_claim = claim_global_runner_lock(candidate_root, output)
            lock_path, descriptor, parent_descriptor = claim_output(output)
            output_claim = LockClaim(lock_path, descriptor, parent_descriptor)
        finally:
            signal.pthread_sigmask(signal.SIG_SETMASK, original_mask)
        yield
    except BaseException as error:
        primary_error = error
        signal.pthread_sigmask(signal.SIG_BLOCK, blocked_signals)
        if output_claim is not None:
            try:
                write_status(
                    output,
                    "incomplete",
                    completed,
                    f"{type(error).__name__}: {error}",
                )
            except BaseException as status_error:
                print(f"could not record incomplete publication status: {status_error}", file=sys.stderr)
        raise
    finally:
        cleanup_interrupt: BaseException | None = None
        try:
            signal.pthread_sigmask(signal.SIG_BLOCK, blocked_signals)
        except BaseException as error:
            cleanup_interrupt = error
        cleanup_errors: list[BaseException] = []
        retain_claims = isinstance(primary_error, CleanupFailure)
        for claim in (output_claim, global_claim):
            if claim is None:
                continue
            try:
                if retain_claims:
                    retain_lock_claim(claim)
                else:
                    release_lock_claim(claim)
            except BaseException as error:
                cleanup_errors.append(error)
        if original_mask is None:
            original_mask = set()
        try:
            signal.pthread_sigmask(signal.SIG_SETMASK, original_mask)
        except BaseException as error:
            if cleanup_interrupt is None:
                cleanup_interrupt = error
            try:
                signal.pthread_sigmask(signal.SIG_SETMASK, original_mask)
            except BaseException as restore_error:
                cleanup_interrupt = PublicationError(
                    f"could not restore termination signal mask after {error}: {restore_error}"
                )
        finally:
            signal.signal(signal.SIGTERM, previous_sigterm)
            signal.signal(signal.SIGINT, previous_sigint)
        if cleanup_errors:
            detail = "; ".join(str(error) for error in cleanup_errors)
            if primary_error is None and cleanup_interrupt is None:
                raise PublicationError(f"publication lock cleanup failed: {detail}") from cleanup_errors[0]
            print(f"publication lock cleanup failed: {detail}", file=sys.stderr)
        if cleanup_interrupt is not None:
            if primary_error is None:
                raise cleanup_interrupt
            print(f"termination arrived during publication lock cleanup: {cleanup_interrupt}", file=sys.stderr)


def require_output_path(
    value: str,
    candidate_root: pathlib.Path,
    source_record: dict[str, str] | None = None,
) -> pathlib.Path:
    raw = pathlib.Path(value).expanduser()
    if not raw.is_absolute():
        raw = pathlib.Path.cwd() / raw
    output = pathlib.Path(os.path.abspath(raw))
    parent = real_directory(output.parent, "output parent")
    output = parent / output.name
    _validate_publication_output_path(output, candidate_root, source_record, "output")
    if ":" in str(output):
        raise PublicationError(f"output path contains the PATH separator and cannot safely host the Docker shim: {output}")
    if output.name in {"", ".", ".."}:
        raise PublicationError("output path must name a new directory")
    check_output_available(output)
    return output


def write_status(output: pathlib.Path, status: str, completed: list[tuple[int, int]], detail: str = "") -> None:
    content = [f"status={status}", f"completed_ranges={len(completed)}"]
    content.extend(f"completed={start}..{end}" for start, end in completed)
    if detail:
        content.append(f"detail={detail.replace(chr(10), ' ')}")
    directory_descriptor = open_absolute_nofollow(output, directory=True)
    temporary = ".attempt-status.tmp"
    destination = "attempt-status.txt"
    temporary_descriptor = -1
    created = False
    try:
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        temporary_descriptor = os.open(temporary, flags, 0o600, dir_fd=directory_descriptor)
        created = True
        data = ("\n".join(content) + "\n").encode("utf-8")
        view = memoryview(data)
        while view:
            written = os.write(temporary_descriptor, view)
            if written == 0:
                raise OSError("short write while recording publication status")
            view = view[written:]
        os.fsync(temporary_descriptor)
        os.close(temporary_descriptor)
        temporary_descriptor = -1
        os.replace(temporary, destination, src_dir_fd=directory_descriptor, dst_dir_fd=directory_descriptor)
        created = False
        os.fsync(directory_descriptor)
    except BaseException:
        if temporary_descriptor >= 0:
            os.close(temporary_descriptor)
        if created:
            try:
                os.unlink(temporary, dir_fd=directory_descriptor)
            except FileNotFoundError:
                pass
        raise
    finally:
        os.close(directory_descriptor)


def validate_docker_target(
    environment: dict[str, str],
    context_name: str,
    endpoint: str,
) -> dict[str, str]:
    overrides = sorted(name for name in DOCKER_OVERRIDE_VARIABLES if name in environment)
    if overrides:
        raise PublicationError(f"Docker endpoint overrides are not allowed: {overrides}")
    if context_name != "default":
        raise PublicationError(f"Docker context must be local 'default', found {context_name!r}")
    if endpoint != DOCKER_LOCAL_ENDPOINT:
        raise PublicationError(f"Docker endpoint must be local {DOCKER_LOCAL_ENDPOINT!r}, found {endpoint!r}")
    pinned_environment = dict(environment)
    pinned_environment["DOCKER_HOST"] = DOCKER_LOCAL_ENDPOINT
    return pinned_environment


def sanitized_shell_environment(environment: dict[str, str]) -> dict[str, str]:
    return {
        name: value
        for name, value in environment.items()
        if name not in SHELL_OVERRIDE_VARIABLES and not name.startswith("BASH_FUNC_")
    }


def verify_unix_socket(socket_path: pathlib.Path) -> None:
    try:
        mode = os.stat(socket_path, follow_symlinks=False).st_mode
    except OSError as error:
        raise PublicationError(f"Unix socket is unavailable at {socket_path}: {error}") from error
    if not stat.S_ISSOCK(mode):
        raise PublicationError(f"endpoint is not a Unix socket: {socket_path}")


def verify_local_docker_socket(endpoint: str) -> None:
    if endpoint != DOCKER_LOCAL_ENDPOINT:
        raise PublicationError(f"Docker endpoint must be local {DOCKER_LOCAL_ENDPOINT!r}, found {endpoint!r}")
    verify_unix_socket(pathlib.Path(DOCKER_LOCAL_ENDPOINT.removeprefix("unix://")))


def local_docker_environment() -> dict[str, str]:
    environment = sanitized_shell_environment(dict(os.environ))
    environment["PATH"] = "/usr/bin:/bin"
    overrides = sorted(name for name in DOCKER_OVERRIDE_VARIABLES if name in environment)
    if overrides:
        raise PublicationError(f"Docker endpoint overrides are not allowed: {overrides}")
    try:
        context = subprocess.run(
            ["/usr/bin/docker", "context", "show"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=environment,
        ).stdout.strip()
        endpoint = subprocess.run(
            ["/usr/bin/docker", "context", "inspect", "default", "--format", "{{.Endpoints.docker.Host}}"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=environment,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise PublicationError(f"cannot verify local Docker context and endpoint: {error}") from error
    pinned_environment = validate_docker_target(environment, context, endpoint)
    verify_local_docker_socket(endpoint)
    return pinned_environment


def docker_shim(job_dir: pathlib.Path, container: str, cpu: int, input_root: pathlib.Path) -> pathlib.Path:
    shim_dir = job_dir / "docker-shim"
    shim_dir.mkdir()
    shim = shim_dir / "docker"
    input_mount = shlex.quote(f"{input_root}:{input_root}:ro")
    resource_flags = "|".join(f"{flag}|{flag}=*" for flag in DOCKER_RESOURCE_FLAGS)
    contents = (
        "#!/usr/bin/env bash\nset -euo pipefail\n"
        "if [[ ${1-} == run ]]; then\n  shift\n  args=()\n"
        "  while (($#)); do\n    case $1 in\n"
        f"      {resource_flags})\n"
        "        if [[ $1 == *=* ]]; then shift; else\n"
        "          if (($# < 2)); then echo \"Docker resource option $1 needs a value\" >&2; exit 2; fi\n"
        "          shift 2\n"
        "        fi ;;\n"
        "      *) args+=(\"$1\"); shift ;;\n"
        "    esac\n  done\n"
        f"  exec /usr/bin/docker run --name={container} --cpuset-cpus={cpu} --cpus=1 --memory=1g -v {input_mount} \"${{args[@]}}\"\n"
        "fi\nexec /usr/bin/docker \"$@\"\n"
    )
    flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
    descriptor = os.open(shim, flags, 0o755)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        output.write(contents)
        output.flush()
        os.fchmod(output.fileno(), 0o755)
        os.fsync(output.fileno())
    return shim_dir


def docker_container_names(filter_pattern: str, environment: dict[str, str]) -> tuple[str, ...]:
    try:
        result = subprocess.run(
            ["/usr/bin/docker", "ps", "-a", "--filter", f"name={filter_pattern}", "--format", "{{.Names}}"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=environment,
            timeout=DOCKER_INSPECT_TIMEOUT_SECONDS,
        )
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        raise PublicationError(f"cannot inspect Docker containers matching {filter_pattern}: {error}") from error
    return tuple(name for name in result.stdout.splitlines() if name)


def existing_container(name: str, environment: dict[str, str]) -> bool:
    return name in docker_container_names(f"^{name}$", environment)


def inspect_owned_containers(filter_pattern: str, environment: dict[str, str]) -> tuple[str, ...]:
    try:
        return docker_container_names(filter_pattern, environment)
    except PublicationError as error:
        raise CleanupFailure(
            f"could not confirm owned Docker containers are stopped: {error}; shared and output locks will be retained"
        ) from error


def stop_owned_job(job: dict[str, object], environment: dict[str, str]) -> list[str]:
    errors: list[str] = []
    docker_errors: list[str] = []
    container = str(job["container"])

    def stop_if_present(stage: str, grace: int, timeout: int) -> None:
        try:
            present = existing_container(container, environment)
        except PublicationError as error:
            docker_errors.append(f"could not verify Docker container {container} before {stage}: {error}")
            present = True
        if not present:
            return
        try:
            result = subprocess.run(
                ["/usr/bin/docker", "stop", "--time", str(grace), container],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=environment,
                timeout=timeout,
            )
            if result.returncode != 0:
                docker_errors.append(
                    f"Docker stop for {container} failed during {stage} with status {result.returncode}"
                )
        except subprocess.TimeoutExpired:
            docker_errors.append(f"timed out stopping Docker container {container} during {stage}")
        except OSError as error:
            docker_errors.append(f"could not stop Docker container {container} during {stage}: {error}")

    stop_if_present("initial cleanup", 5, 10)
    process = job.get("process")
    if process is not None:
        try:
            os.killpg(process.pid, signal.SIGINT)
        except ProcessLookupError:
            pass
        except OSError as error:
            errors.append(f"could not interrupt worker process {process.pid}: {error}")
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            except OSError as error:
                errors.append(f"could not terminate worker process {process.pid}: {error}")
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                except OSError as error:
                    errors.append(f"could not kill worker process {process.pid}: {error}")
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    errors.append(f"worker process {process.pid} did not exit after SIGKILL")
                except OSError as error:
                    errors.append(f"could not reap worker process {process.pid}: {error}")
            except OSError as error:
                errors.append(f"could not wait for worker process {process.pid}: {error}")
        except OSError as error:
            errors.append(f"could not wait for worker process {process.pid}: {error}")
    output_handle = job.get("output")
    if output_handle is not None and not output_handle.closed:
        try:
            output_handle.close()
        except OSError as error:
            errors.append(f"could not close worker log: {error}")
    stop_if_present("final cleanup", 1, 5)
    try:
        remains = existing_container(container, environment)
    except PublicationError as error:
        errors.extend(docker_errors)
        errors.append(f"could not verify Docker container {container} after cleanup: {error}")
    else:
        if remains:
            errors.extend(docker_errors)
            errors.append(f"owned Docker container {container} remains after cleanup")
    return errors


def start_and_register_job(
    job: dict[str, object],
    active: list[dict[str, object]],
    command: list[str],
    cwd: str,
    environment: dict[str, str],
) -> subprocess.Popen[bytes]:
    blocked_signals = {signal.SIGTERM, signal.SIGINT}
    original_mask = signal.pthread_sigmask(signal.SIG_BLOCK, blocked_signals)
    process: subprocess.Popen[bytes] | None = None
    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=environment,
            stdout=job["output"],
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        job["process"] = process
        active.append(job)
        signal.pthread_sigmask(signal.SIG_SETMASK, original_mask)
        return process
    except BaseException as start_error:
        signal.pthread_sigmask(signal.SIG_BLOCK, blocked_signals)
        if process is not None:
            errors = stop_owned_job(job, environment)
            if errors:
                detail = "; ".join(errors)
                raise CleanupFailure(
                    f"worker start failed and cleanup failed: {detail}; shared and output locks will be retained"
                ) from start_error
        if job in active:
            active.remove(job)
        raise
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, original_mask)


def launch_job(
    record: dict[str, str],
    output: pathlib.Path,
    worker: str,
    start: int,
    end: int,
    active: list[dict[str, object]],
    docker_environment: dict[str, str],
) -> dict[str, object]:
    validate_candidate_files(record)
    cpu = {"A": 0, "B": 1, "C": 2, "D": 3}[worker]
    job_dir = output / "jobs" / worker / f"{start}-{end}"
    job_dir.mkdir(parents=True)
    receipt = candidate_receipt_path(output / "candidate", start, end)
    timing = job_dir / "timing.txt"
    log = job_dir / "worker.log"
    if any(path.exists() or path.is_symlink() for path in (receipt, timing, log)):
        raise PublicationError(f"refusing to overwrite shard artifact {worker}/{start}..{end}")
    container = f"w9pub-{os.getpid()}-{worker}-{start}"
    if existing_container(container, docker_environment):
        raise PublicationError(f"Docker container name already exists: {container}")
    shim_dir = docker_shim(job_dir, container, cpu, pathlib.Path(record["input_root"]))
    output_handle = log.open("xb")
    try:
        output_handle.write(
            (
                f"worker={worker} cpu={cpu} memory_limit=1GiB range={start}..{end} profiles=all "
                f"source_commit={record['candidate_commit']} source_tree={record['candidate_tree']} "
                f"candidate_binary_sha256={record['candidate_binary_sha256']} helper_sha256={record['helper_sha256']} "
                f"wrapper_sha256={record['wrapper_sha256']} corpus_blake3={record['corpus_blake3']}\n"
            ).encode()
        )
        output_handle.flush()
    except BaseException:
        output_handle.close()
        raise
    environment = dict(docker_environment)
    environment["PATH"] = f"{shim_dir}:/usr/bin:/bin"
    command = [
        "timeout", "900s", record["wrapper"], record["wrapper_root"], "run", "env",
        f"W9_INPUT_ROOT={record['input_root']}",
        f"W9_RECEIPT_OUTPUT={receipt}",
        f"W9_TIMING_OUTPUT={timing}",
        f"W9_SHARD_INDEX={start // 100}",
        "W9_SHARD_SIZE=100",
        f"W9_START={start}",
        f"W9_END={end}",
        "taskset", "-c", str(cpu), record["candidate_binary"],
        "--exact", "sema::w9_repository_parity_receipt", "--nocapture",
    ]
    job = {
        "worker": worker,
        "cpu": cpu,
        "start": start,
        "end": end,
        "container": container,
        "receipt": receipt,
        "timing": timing,
        "log": log,
        "process": None,
        "output": output_handle,
    }
    try:
        start_and_register_job(job, active, command, record["wrapper_root"], environment)
    except BaseException:
        if not output_handle.closed:
            output_handle.close()
        raise
    return job


def stop_jobs(active: list[dict[str, object]], environment: dict[str, str]) -> None:
    blocked_signals = {signal.SIGTERM, signal.SIGINT}
    interrupted: KeyboardInterrupt | None = None
    try:
        original_mask = signal.pthread_sigmask(signal.SIG_BLOCK, blocked_signals)
    except KeyboardInterrupt as error:
        interrupted = error
        original_mask = signal.pthread_sigmask(signal.SIG_BLOCK, blocked_signals)
    errors: list[str] = []
    for job in list(active):
        try:
            errors.extend(stop_owned_job(job, environment))
        except KeyboardInterrupt as error:
            interrupted = interrupted or error
            try:
                errors.extend(stop_owned_job(job, environment))
            except BaseException as retry_error:
                errors.append(f"could not complete cleanup for {job['container']}: {retry_error}")
        except BaseException as error:
            errors.append(f"could not complete cleanup for {job['container']}: {error}")
        try:
            active.remove(job)
        except ValueError as error:
            errors.append(f"could not release worker ownership for {job['container']}: {error}")
    if errors:
        failure = CleanupFailure(
            "worker cleanup failed: " + "; ".join(errors) + "; shared and output locks will be retained"
        )
        if interrupted is not None:
            raise failure from interrupted
        raise failure
    if interrupted is not None:
        raise interrupted
    signal.pthread_sigmask(signal.SIG_SETMASK, original_mask)


def handle_termination(signum: int, _frame: object) -> None:
    signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM, signal.SIGINT})
    raise KeyboardInterrupt(f"received signal {signum}")


def execute_run(args: argparse.Namespace) -> None:
    receipt_path = regular_file(args.candidate_receipt, "candidate source/binary receipt")
    record = parse_candidate_receipt(receipt_path)
    paths_by_index, _paths_by_name, reference = validate_all_inputs(record)
    candidate_root = pathlib.Path(record["candidate_root"])
    output = require_output_path(args.output, candidate_root)
    docker_environment = local_docker_environment()
    if args.dry_run:
        print(
            f"preflight ok: source={record['candidate_commit']} tree={record['candidate_tree']} "
            f"corpus=1690 profiles={','.join(PROFILES)} workers=4 cpus=0,1,2,3 "
            f"memory_per_worker=1GiB output={output}"
        )
        return
    completed: list[tuple[int, int]] = []
    active: list[dict[str, object]] = []
    with claim_run_resources(candidate_root, output, completed):
        try:
            (output / "candidate").mkdir()
            (output / "jobs").mkdir()
            write_status(output, "incomplete", completed)
            ranges = expected_ranges(len(paths_by_index))
            queues = {worker: list(ranges[index::4]) for index, worker in enumerate(("A", "B", "C", "D"))}
            for worker in queues:
                start, end = queues[worker].pop(0)
                launch_job(record, output, worker, start, end, active, docker_environment)
            while active:
                changed = False
                for job in list(active):
                    process = job["process"]
                    status = process.poll()
                    if status is None:
                        continue
                    changed = True
                    output_handle = job["output"]
                    if not output_handle.closed:
                        output_handle.close()
                    receipt_path = job["receipt"]
                    timing_path = job["timing"]
                    if status != 0:
                        raise PublicationError(
                            f"worker {job['worker']} range {job['start']}..{job['end']} failed "
                            f"status={status}; log={job['log']}"
                        )
                    try:
                        regular_file(receipt_path, "candidate range receipt")
                        regular_file(timing_path, "candidate range timing")
                    except PublicationError as error:
                        raise PublicationError(
                            f"worker {job['worker']} range {job['start']}..{job['end']} failed: {error}; log={job['log']}"
                        ) from error
                    validate_candidate_files(record)
                    compare_shard(
                        reference,
                        receipt_path,
                        paths_by_index,
                        int(job["start"]),
                        int(job["end"]),
                        record["corpus_blake3"],
                    )
                    completed.append((int(job["start"]), int(job["end"])))
                    completed.sort()
                    write_status(output, "incomplete", completed)
                    active.remove(job)
                    worker = str(job["worker"])
                    if queues[worker]:
                        start, end = queues[worker].pop(0)
                        launch_job(record, output, worker, start, end, active, docker_environment)
                if not changed:
                    time.sleep(1)
            validate_candidate_files(record)
            remaining = inspect_owned_containers(f"^w9pub-{os.getpid()}-", docker_environment)
            if remaining:
                stranded_jobs = [
                    {"container": name, "process": None, "output": None}
                    for name in remaining
                ]
                stop_jobs(stranded_jobs, docker_environment)
                raise PublicationError(
                    "owned Docker containers remained after the parity workers exited: " + ", ".join(remaining)
                )
            summary = verify_receipt_bundle(reference, output / "candidate", paths_by_index, record["corpus_blake3"])
            validate_all_inputs(record)
            write_status(output, "complete", completed, summary)
            print(summary)
        except BaseException as primary_error:
            try:
                stop_jobs(active, docker_environment)
            except CleanupFailure as cleanup_error:
                raise CleanupFailure(f"run failed: {primary_error}; {cleanup_error}") from primary_error
            raise


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    stage = commands.add_parser("stage-helper", help="stage the pinned parity helper into candidate .work")
    stage.add_argument("--candidate-root", required=True)
    build = commands.add_parser("build-harness", help="build and record the pinned integration test harness")
    build.add_argument("--candidate-root", required=True)
    build.add_argument("--receipt", required=True)
    seal = commands.add_parser("seal", help="write the candidate source and binary receipt")
    seal.add_argument("--candidate-root", required=True)
    seal.add_argument("--candidate-binary", required=True)
    seal.add_argument("--helper", required=True)
    seal.add_argument("--build-receipt", required=True)
    seal.add_argument("--wrapper", required=True)
    seal.add_argument("--reference-receipt", required=True)
    seal.add_argument("--reference-binary", required=True)
    seal.add_argument("--reference-validation", required=True)
    seal.add_argument("--input-root", required=True)
    seal.add_argument("--input-manifest", required=True)
    seal.add_argument("--receipt", required=True)
    source = commands.add_parser("seal-source", help="write a source-qualified W9 cohort receipt")
    source.add_argument("--candidate-root", required=True)
    source.add_argument("--input-root", required=True)
    source.add_argument("--input-manifest", required=True)
    source.add_argument("--receipt", required=True)
    run = commands.add_parser("run", help="run all 1690 sources in all four profiles")
    run.add_argument("--candidate-receipt", required=True)
    run.add_argument("--output", required=True)
    run.add_argument("--dry-run", action="store_true", help="validate every input and print the bounded run plan")
    return root


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        if args.command == "stage-helper":
            print(f"staged parity helper: {stage_helper(pathlib.Path(args.candidate_root))}")
        elif args.command == "build-harness":
            build_harness(args)
        elif args.command == "seal":
            seal_candidate(args)
        elif args.command == "seal-source":
            seal_source_cohort(args)
        elif args.command == "run":
            execute_run(args)
        else:
            raise PublicationError(f"unknown command {args.command}")
    except (PublicationError, OSError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        print(f"W9 publication parity: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
