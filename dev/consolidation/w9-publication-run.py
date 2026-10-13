#!/usr/bin/env python3
"""Run the frozen W9 diagnostic and publication parity check."""

from __future__ import annotations

import argparse
import hashlib
import os
import pathlib
import re
import shlex
import signal
import stat
import subprocess
import sys
import time
from dataclasses import dataclass


ROOT = pathlib.Path(__file__).resolve().parents[2]
REFERENCE_RECEIPT_SHA256 = "8054e5f05ae740f746eea1ed980077b5d04bb57c0cc108dd3ee3c402bf9d85c8"
REFERENCE_BINARY_SHA256 = "c58582007f4999eaa953ebdb54c62902557505099e928fc2f168afcc167fd8cc"
REFERENCE_VALIDATION_SHA256 = "7d2883e623ec6b72093afc714307b89f2f0110ef447b64738d7da2a500a035cd"
INPUT_MANIFEST_SHA256 = "1ec4e8eb67681591d25c69b395387ba4acbb4bfe1e4ab514a858e60ff1a56305"
CORPUS_BLAKE3 = "a7b1f70466c4e978222af3d329e9c71b0a15030480e24b8d7f77a5fa4f57f17d"
HELPER_SHA256 = "2ff5aaa74f8ae3d0597a3c14796d8643211a5b57b8b95147ba87205540ca95b0"
WRAPPER_SHA256 = "f6a76b9bcb385e1e328332c9815ce48cb0e864d3ffe951578202a774d476f64e"
PINNED_IMAGE = "sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
REFERENCE_HEAD = "e240efad273fe35d66d91f4037b6a8193527da2e"
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
RECEIPT_NAME = re.compile(r"parallel-(0|[1-9][0-9]*)-(0|[1-9][0-9]*)-all\.receipt\Z")


class PublicationError(RuntimeError):
    pass


@dataclass(frozen=True)
class ParsedReceipt:
    header: tuple[str, ...]
    inputs: dict[int, tuple[str, str]]
    outcomes: dict[tuple[int, str, str], tuple[str, str]]
    terminals: tuple[str, ...]


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


def parse_input_manifest(path: pathlib.Path) -> tuple[dict[int, tuple[str, str]], dict[str, str]]:
    by_index: dict[int, tuple[str, str]] = {}
    by_path: dict[str, str] = {}
    for row_number, line in enumerate(read_regular_text(path).splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 4:
            raise PublicationError(f"input manifest row {row_number}: expected four tab-separated fields")
        raw_index, relative, digest, _mode = fields
        try:
            index = int(raw_index)
        except ValueError as error:
            raise PublicationError(f"input manifest row {row_number}: invalid index {raw_index!r}") from error
        pure = pathlib.PurePosixPath(relative)
        if index != row_number - 1 or pure.is_absolute() or ".." in pure.parts:
            raise PublicationError(f"input manifest row {row_number}: invalid index or path {relative!r}")
        if not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise PublicationError(f"input manifest row {row_number}: malformed source SHA-256")
        if index in by_index or relative in by_path:
            raise PublicationError(f"input manifest row {row_number}: duplicate index or path")
        by_index[index] = (relative, digest)
        by_path[relative] = digest
    if len(by_index) != 1690 or set(by_index) != set(range(1690)):
        raise PublicationError(f"input manifest: expected 1690 sequential sources, found {len(by_index)}")
    return by_index, by_path


def verify_corpus_tree(root: pathlib.Path, expected_paths: dict[str, str]) -> None:
    actual_paths: list[str] = []
    for directory, names, files in os.walk(root, followlinks=False):
        base = pathlib.Path(directory)
        relative_directory = base.relative_to(root).as_posix()
        if base.stat().st_mode & 0o222:
            raise PublicationError(f"input corpus directory is writable: {base}")
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
        actual = sha256_file(root / relative)
        expected = expected_paths[relative]
        if actual != expected:
            raise PublicationError(
                f"input corpus source changed at {relative}: expected sha256={expected} actual={actual}"
            )


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


def parse_receipt(path: pathlib.Path, expected_paths: dict[int, tuple[str, str]]) -> ParsedReceipt:
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
    inputs: dict[int, tuple[str, str]] = {}
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
            if index not in expected_paths or expected_paths[index][0] != relative:
                raise PublicationError(f"{path}: wrong input path at index {index}: {relative}")
            if not re.fullmatch(r"[0-9a-f]{64}", digest):
                raise PublicationError(f"{path}: malformed source digest for index {index}")
            if index in inputs:
                raise PublicationError(f"{path}: duplicate input index {index}")
            inputs[index] = (relative, digest)
        elif fields[0] == "output" and len(fields) == 6:
            try:
                index = int(fields[1])
            except ValueError as error:
                raise PublicationError(f"{path}: malformed output index {fields[1]!r}") from error
            relative, profile = fields[2], fields[3]
            key = (index, relative, profile)
            if index not in expected_paths or expected_paths[index][0] != relative:
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
    expected_paths: dict[int, tuple[str, str]],
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
    expected_outcomes = {
        (index, relative, profile)
        for index, (relative, _digest) in expected_paths.items()
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
        (index, receipt.inputs[index][0], profile)
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
    expected_paths: dict[int, tuple[str, str]],
    start: int,
    end: int,
    corpus_digest: str,
) -> None:
    candidate = parse_receipt(path, expected_paths)
    validate_candidate_shard(path, candidate, start, end, len(expected_paths), corpus_digest)
    for index in range(start, end):
        if candidate.inputs[index] != reference.inputs[index]:
            raise PublicationError(f"input path or source digest mismatch at index {index}: {candidate.inputs[index]}")
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
    expected_paths: dict[int, tuple[str, str]],
    corpus_digest: str,
) -> str:
    count = len(expected_paths)
    candidates = discover_candidate_receipts(candidate_dir, count)
    all_inputs: dict[int, tuple[str, str]] = {}
    all_outcomes: dict[tuple[int, str, str], tuple[str, str]] = {}
    for (start, end), path in sorted(candidates.items()):
        receipt = parse_receipt(path, expected_paths)
        validate_candidate_shard(path, receipt, start, end, count, corpus_digest)
        for index, value in receipt.inputs.items():
            if index in all_inputs:
                raise PublicationError(f"duplicate candidate input index {index}")
            if value != reference.inputs[index]:
                raise PublicationError(f"input path or source digest mismatch at index {index}: {value}")
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
) -> tuple[dict[int, tuple[str, str]], dict[str, str], ParsedReceipt]:
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
    require_sha256(binary, record["candidate_binary_sha256"], "candidate test binary")
    require_sha256(helper, record["helper_sha256"], "candidate parity helper")
    require_sha256(wrapper, WRAPPER_SHA256, "pinned native test wrapper")
    if PINNED_IMAGE not in read_regular_text(wrapper):
        raise PublicationError("pinned native test wrapper does not name the approved image")
    if record["helper_sha256"] != HELPER_SHA256:
        raise PublicationError(f"candidate parity helper is not the approved helper: {record['helper_sha256']}")
    if record["wrapper_sha256"] != WRAPPER_SHA256:
        raise PublicationError(f"candidate wrapper receipt hash differs from approved wrapper: {record['wrapper_sha256']}")
    runner = regular_file(record["runner"], "publication runner")
    require_sha256(runner, record["runner_sha256"], "publication runner")
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


def validate_receipt_shape(record: dict[str, str]) -> None:
    if set(record) != set(RECEIPT_FIELDS):
        missing = sorted(set(RECEIPT_FIELDS) - set(record))
        extra = sorted(set(record) - set(RECEIPT_FIELDS))
        raise PublicationError(f"candidate receipt fields differ: missing={missing} extra={extra}")
    if record["schema"] != "1":
        raise PublicationError(f"unsupported candidate receipt schema {record['schema']!r}")
    for field in RECEIPT_FIELDS:
        value = record[field]
        if not value or any(char in value for char in "\r\n\t"):
            raise PublicationError(f"candidate receipt has invalid {field}")
    for field in ("candidate_binary_sha256", "helper_sha256", "wrapper_sha256",
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
    wrapper = regular_file(args.wrapper, "native test wrapper", executable=True)
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
        "schema": "1",
        "candidate_root": str(root),
        "candidate_commit": commit,
        "candidate_tree": tree,
        "candidate_binary": str(binary),
        "candidate_binary_sha256": sha256_file(binary),
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


def validate_all_inputs(record: dict[str, str]) -> tuple[dict[int, tuple[str, str]], dict[str, str], ParsedReceipt]:
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


def output_lock_path(output: pathlib.Path) -> pathlib.Path:
    return output.parent / f".{output.name}.lock"


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
    try:
        os.write(descriptor, f"pid={os.getpid()}\noutput={output}\n".encode())
        os.fsync(descriptor)
        os.mkdir(output.name, dir_fd=parent_descriptor)
        os.fsync(parent_descriptor)
    except BaseException:
        os.close(descriptor)
        os.unlink(lock.name, dir_fd=parent_descriptor)
        os.close(parent_descriptor)
        raise
    return lock, descriptor, parent_descriptor


def require_output_path(value: str, candidate_root: pathlib.Path) -> pathlib.Path:
    raw = pathlib.Path(value).expanduser()
    if not raw.is_absolute():
        raw = pathlib.Path.cwd() / raw
    output = pathlib.Path(os.path.abspath(raw))
    parent = real_directory(output.parent, "output parent")
    output = parent / output.name
    work_root = candidate_root / ".work"
    if not under(output, work_root):
        raise PublicationError(f"output must be under candidate source .work/: {output}")
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


def docker_shim(job_dir: pathlib.Path, container: str, cpu: int, input_root: pathlib.Path) -> pathlib.Path:
    shim_dir = job_dir / "docker-shim"
    shim_dir.mkdir()
    shim = shim_dir / "docker"
    input_mount = shlex.quote(f"{input_root}:{input_root}:ro")
    contents = (
        "#!/usr/bin/env bash\nset -euo pipefail\n"
        "if [[ ${1-} == run ]]; then\n  shift\n  args=()\n"
        "  while (($#)); do\n    case $1 in\n"
        "      --cpus=8|--memory=16g) shift ;;\n"
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


def existing_container(name: str) -> bool:
    try:
        result = subprocess.run(
            ["/usr/bin/docker", "ps", "-a", "--filter", f"name=^{name}$", "--format", "{{.Names}}"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise PublicationError(f"cannot inspect Docker container {name}: {error}") from error
    return bool(result.stdout.strip())


def launch_job(
    record: dict[str, str],
    output: pathlib.Path,
    worker: str,
    start: int,
    end: int,
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
    if existing_container(container):
        raise PublicationError(f"Docker container name already exists: {container}")
    shim_dir = docker_shim(job_dir, container, cpu, pathlib.Path(record["input_root"]))
    output_handle = log.open("xb")
    output_handle.write(
        (
            f"worker={worker} cpu={cpu} memory_limit=1GiB range={start}..{end} profiles=all "
            f"source_commit={record['candidate_commit']} source_tree={record['candidate_tree']} "
            f"candidate_binary_sha256={record['candidate_binary_sha256']} helper_sha256={record['helper_sha256']} "
            f"wrapper_sha256={record['wrapper_sha256']} corpus_blake3={record['corpus_blake3']}\n"
        ).encode()
    )
    output_handle.flush()
    environment = os.environ.copy()
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
    try:
        process = subprocess.Popen(
            command,
            cwd=record["wrapper_root"],
            env=environment,
            stdout=output_handle,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
    except BaseException:
        output_handle.close()
        raise
    return {
        "worker": worker,
        "cpu": cpu,
        "start": start,
        "end": end,
        "container": container,
        "receipt": receipt,
        "timing": timing,
        "log": log,
        "process": process,
        "output": output_handle,
    }


def stop_jobs(active: list[dict[str, object]]) -> None:
    for job in active:
        subprocess.run(
            ["/usr/bin/docker", "stop", "--time", "5", str(job["container"])],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        process = job["process"]
        try:
            os.killpg(process.pid, signal.SIGINT)
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + 15
    for job in active:
        process = job["process"]
        try:
            process.wait(timeout=max(0.0, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
    for job in active:
        process = job["process"]
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        process.wait()
        output_handle = job["output"]
        if not output_handle.closed:
            output_handle.close()
        subprocess.run(
            ["/usr/bin/docker", "stop", "--time", "1", str(job["container"])],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


def handle_termination(signum: int, _frame: object) -> None:
    raise KeyboardInterrupt(f"received signal {signum}")


def execute_run(args: argparse.Namespace) -> None:
    receipt_path = regular_file(args.candidate_receipt, "candidate source/binary receipt")
    record = parse_candidate_receipt(receipt_path)
    paths_by_index, _paths_by_name, reference = validate_all_inputs(record)
    candidate_root = pathlib.Path(record["candidate_root"])
    output = require_output_path(args.output, candidate_root)
    if args.dry_run:
        print(
            f"preflight ok: source={record['candidate_commit']} tree={record['candidate_tree']} "
            f"corpus=1690 profiles={','.join(PROFILES)} workers=4 cpus=0,1,2,3 "
            f"memory_per_worker=1GiB output={output}"
        )
        return
    lock_path, lock_descriptor, parent_descriptor = claim_output(output)
    completed: list[tuple[int, int]] = []
    active: list[dict[str, object]] = []
    output_created = True
    previous_sigterm = signal.signal(signal.SIGTERM, handle_termination)
    try:
        (output / "candidate").mkdir()
        (output / "jobs").mkdir()
        write_status(output, "incomplete", completed)
        ranges = expected_ranges(len(paths_by_index))
        queues = {worker: list(ranges[index::4]) for index, worker in enumerate(("A", "B", "C", "D"))}
        for worker in queues:
            start, end = queues[worker].pop(0)
            active.append(launch_job(record, output, worker, start, end))
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
                    active.append(launch_job(record, output, worker, start, end))
            if not changed:
                time.sleep(1)
        validate_candidate_files(record)
        remaining = subprocess.run(
            ["/usr/bin/docker", "ps", "-a", "--filter", f"name=^w9pub-{os.getpid()}-", "--format", "{{.Names}}"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout.strip()
        if remaining:
            raise PublicationError(f"owned Docker containers remain after parity run: {remaining}")
        summary = verify_receipt_bundle(reference, output / "candidate", paths_by_index, record["corpus_blake3"])
        write_status(output, "complete", completed, summary)
        print(summary)
    except BaseException as error:
        stop_jobs(active)
        if output_created:
            write_status(output, "incomplete", completed, f"{type(error).__name__}: {error}")
        raise
    finally:
        signal.signal(signal.SIGTERM, previous_sigterm)
        os.close(lock_descriptor)
        try:
            os.unlink(lock_path.name, dir_fd=parent_descriptor)
        except FileNotFoundError:
            pass
        os.fsync(parent_descriptor)
        os.close(parent_descriptor)


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    seal = commands.add_parser("seal", help="write a current candidate source/binary receipt")
    seal.add_argument("--candidate-root", required=True)
    seal.add_argument("--candidate-binary", required=True)
    seal.add_argument("--helper", required=True)
    seal.add_argument("--wrapper", required=True)
    seal.add_argument("--reference-receipt", required=True)
    seal.add_argument("--reference-binary", required=True)
    seal.add_argument("--reference-validation", required=True)
    seal.add_argument("--input-root", required=True)
    seal.add_argument("--input-manifest", required=True)
    seal.add_argument("--receipt", required=True)
    run = commands.add_parser("run", help="run all 1690 sources in all four profiles")
    run.add_argument("--candidate-receipt", required=True)
    run.add_argument("--output", required=True)
    run.add_argument("--dry-run", action="store_true", help="validate every input and print the bounded run plan")
    return root


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        if args.command == "seal":
            seal_candidate(args)
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
