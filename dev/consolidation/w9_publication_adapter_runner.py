from __future__ import annotations

import argparse
import hashlib
import importlib.util
import multiprocessing
import multiprocessing.connection
import os
import pathlib
import re
import signal
import stat
import subprocess
import sys
import tempfile
import threading
import time
from typing import NamedTuple

import w9_publication_adapter as adapter
import w9_publication_sources as source_alignment
import w9_publication_reference_correction as reference_correction


ROOT = pathlib.Path(__file__).resolve().parents[2]
APPROVED_HELPER = pathlib.Path(
    "/home/josh/d/laputa-systems/xsh-consolidation-lanes/module-record-driver/"
    ".work/consolidation/w9-repository-parity.rs"
)
SOURCE_APPROVED_HELPER = (
    pathlib.Path(__file__).resolve().parents[2]
    / "dev/consolidation/winddown/gate-tools/w9-repository-parity.rs"
)
OWNER_HELPER = ROOT / "dev/consolidation/w9-publication-owner-helper.rs"
APPROVED_HELPER_SHA256 = "2ff5aaa74f8ae3d0597a3c14796d8643211a5b57b8b95147ba87205540ca95b0"
OWNER_HELPER_SHA256 = "23fafe5b4140396242543bb4c59b0bcc1e0d4b59280f1f0e8dabf0cbd8431562"
REFERENCE_TREE = "a1ef04ac7b6f759c272e8f841acee3a3b3a6ff08"
RECORD_NAME = re.compile(r"record-(0|[1-9][0-9]*)-(default|reveal|execution|check-cli)\Z")
PROFILES = ("default", "reveal", "execution", "check-cli")
CAPTURE_SLOTS = (("A", 0), ("B", 1), ("C", 2), ("D", 3))
RAW_SCHEMA = "w9-publication-adapter\t1\n"
RAW_HASH_LIBTEST_PREFIX = "test sema::w9_repository_parity_receipt ... "
ADAPTER_BUILD_FIELDS = (
    "schema",
    "role",
    "source_root",
    "source_commit",
    "source_tree",
    "binary",
    "binary_sha256",
    "builder_path",
    "builder_sha256",
    "builder_image",
    "build_target",
    "build_profile",
    "approved_helper_path",
    "approved_helper_sha256",
    "helper_path",
    "helper_sha256",
    "owner_helper_path",
    "owner_helper_sha256",
    "owner_helper_source_path",
    "owner_helper_source_sha256",
    "overlay_sha256",
)
SOURCE_ADAPTER_BUILD_FIELDS = (
    "schema",
    "role",
    "source_root",
    "source_commit",
    "source_tree",
    "binary",
    "binary_sha256",
    "source_receipt",
    "source_receipt_sha256",
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
    "runner_path",
    "runner_sha256",
    "approved_helper_path",
    "approved_helper_sha256",
    "helper_path",
    "helper_sha256",
    "owner_helper_path",
    "owner_helper_sha256",
    "owner_helper_source_path",
    "owner_helper_source_sha256",
    "overlay_sha256",
)
CAPTURE_FIELDS = (
    "schema",
    "role",
    "source_root",
    "source_commit",
    "source_tree",
    "binary",
    "binary_sha256",
    "build_receipt",
    "build_receipt_sha256",
    "helper_sha256",
    "owner_helper_sha256",
    "input_root",
    "input_manifest",
    "input_manifest_sha256",
    "corpus_blake3",
    "capture_root",
    "raw_root",
    "receipt_directory",
    "range_count",
    "outcome_count",
)
SOURCE_CAPTURE_FIELDS = (
    "schema",
    "role",
    "source_receipt",
    "source_receipt_sha256",
    "source_root",
    "source_commit",
    "source_tree",
    "binary",
    "binary_sha256",
    "build_receipt",
    "build_receipt_sha256",
    "builder_path",
    "builder_sha256",
    "runner_path",
    "runner_sha256",
    "linker_path",
    "linker_sha256",
    "helper_sha256",
    "owner_helper_sha256",
    "input_root",
    "input_manifest",
    "input_manifest_sha256",
    "corpus_blake3",
    "capture_root",
    "raw_root",
    "receipt_directory",
    "range_count",
    "outcome_count",
)


class PublicationError(ValueError):
    pass


class _PublicationComparisonTermination(BaseException):
    def __init__(self, signum: int) -> None:
        self.signum = signum


class _PublicationComparisonContext(NamedTuple):
    reference_root: int
    candidate_root: int
    expected_inputs: dict[int, object]
    expected_reference_outcomes: object
    candidate_outcomes: dict[tuple[int, str, str], tuple[str, str]]
    reference_raw_hashes: dict[tuple[int, str, str], str]
    candidate_raw_hashes: dict[tuple[int, str, str], str]
    reference_exact_owners: dict[int, tuple[object, dict[str, object]]]
    candidate_exact_owners: dict[int, tuple[object, dict[str, object]]]
    reference_tui_source: bytes
    candidate_tui_source: bytes
    candidate_assignment_proofs: dict[int, object] | None
    assignment_facts_module: object | None


def _replace_once(source: str, before: str, after: str) -> str:
    count = source.count(before)
    if count != 1:
        raise PublicationError(f"adapter helper marker matched {count} times: {before[:80]!r}")
    return source.replace(before, after, 1)


def _adapter_overlay(source_tests: bytes, runner) -> bytes:
    overlay = runner.parity_overlay_bytes(source_tests)
    if overlay.count(runner.PARITY_TEST_INCLUDE) != 1:
        raise PublicationError("source-derived parity overlay does not contain one helper include")
    return overlay.replace(
        runner.PARITY_TEST_INCLUDE,
        b'include!("../.work/consolidation/w9-publication-adapter.rs");',
        1,
    )


def _prepare_reference_source(
    candidate_root: pathlib.Path,
    reference_source_root: pathlib.Path,
    runner,
) -> pathlib.Path:
    candidate_root = runner.real_directory(candidate_root, "candidate source root")
    reference_source_root = pathlib.Path(os.path.abspath(reference_source_root))
    if candidate_root.parent.name == "wt" and candidate_root.parent.parent.name == ".work":
        worktrees = candidate_root.parent
    else:
        worktrees = candidate_root / ".work/wt"
    expected_root = worktrees / "w9-adapter-ref-e240"
    if reference_source_root != expected_root:
        raise PublicationError(
            f"reference source clone must use the approved shared worktree path: {expected_root}"
        )
    clean_source = runner.real_directory(reference_source_root, "clean reference source clone")
    if runner.source_identity(clean_source) != (runner.REFERENCE_HEAD, REFERENCE_TREE):
        raise PublicationError("clean reference source clone identity differs from its pin")
    return clean_source


def _require_scratch_overlay_identity(
    scratch_root: pathlib.Path,
    commit: str,
    tree: str,
    role: str,
    runner,
) -> None:
    scratch_root = runner.real_directory(scratch_root, f"{role} adapter scratch source")
    git_root = runner.real_directory(
        runner.git_output(scratch_root, "rev-parse", "--show-toplevel"),
        f"{role} adapter scratch git root",
    )
    if git_root != scratch_root:
        raise PublicationError(f"{role} adapter scratch source root changed")
    observed = (
        runner.git_output(scratch_root, "rev-parse", "HEAD"),
        runner.git_output(scratch_root, "rev-parse", "HEAD^{tree}"),
    )
    if observed != (commit, tree):
        raise PublicationError(f"{role} adapter scratch source identity changed during build")
    runner.require_only_parity_overlay(
        scratch_root,
        f"{role} scratch source changed outside its parity include",
    )


def derive_helper(
    approved_helper: pathlib.Path = APPROVED_HELPER,
    owner_helper: pathlib.Path = OWNER_HELPER,
    output_directory: pathlib.Path | None = None,
    output_root: pathlib.Path | None = None,
) -> tuple[pathlib.Path, str, str, str]:
    runner = _load_publication_runner(ROOT / "dev/consolidation/w9-publication-run.py")
    approved_bytes = runner.read_regular_bytes(approved_helper)
    owner_bytes = runner.read_regular_bytes(owner_helper)
    if hashlib.sha256(approved_bytes).hexdigest() != APPROVED_HELPER_SHA256:
        raise PublicationError("approved W9 helper SHA-256 changed")
    if hashlib.sha256(owner_bytes).hexdigest() != OWNER_HELPER_SHA256:
        raise PublicationError("W9 owner helper SHA-256 changed")
    owner_source = owner_bytes.decode("utf-8")
    owner_source = _replace_once(
        owner_source,
        '    std::fs::create_dir_all(outputdir)?;\n'
        '    let path = outputdir.join(format!("owners-{index}.tsv"));\n'
        '    let mut output = std::io::BufWriter::new(std::fs::File::create(path)?);\n',
        '    let mut output = std::io::BufWriter::new(\n'
        '        w9_create_new_adapter_file(outputdir, &format!("owners-{index}.tsv"))?,\n'
        '    );\n',
    )
    owner_source = _replace_once(
        owner_source,
        '    output.flush()\n',
        '    output.flush()?;\n'
        '    output.get_ref().sync_all()\n',
    )
    source = approved_bytes.decode("utf-8")
    source = 'include!("w9-publication-owner-helper.rs");\n' + source
    source = _replace_once(
        source,
        '                let publication_facts = w9_canonical_debug(&format!("{checked:#?}"));\n',
        '                let publication_facts = w9_canonical_debug(&format!("{checked:?}"));\n',
    )
    source = _replace_once(
        source,
        '    let root = PathBuf::from(std::env::var("W9_INPUT_ROOT").expect("W9_INPUT_ROOT"));\n',
        '    if let Some(path) = std::env::var_os("W9_ADAPTER_HASH_ROOT") {\n'
        '        let count = std::env::var("W9_ADAPTER_HASH_INPUT_COUNT").expect("W9_ADAPTER_HASH_INPUT_COUNT").parse::<usize>().expect("numeric adapter input count");\n'
        '        w9_hash_adapter_tree(&PathBuf::from(path), count).unwrap_or_else(|error| panic!("hash raw adapter outcomes: {error}"));\n'
        '        return;\n'
        '    }\n'
        '    let root = PathBuf::from(std::env::var("W9_INPUT_ROOT").expect("W9_INPUT_ROOT"));\n',
    )
    source = _replace_once(
        source,
        '    let timing_path = PathBuf::from(std::env::var("W9_TIMING_OUTPUT").expect("W9_TIMING_OUTPUT"));\n',
        '    let timing_path = PathBuf::from(std::env::var("W9_TIMING_OUTPUT").expect("W9_TIMING_OUTPUT"));\n'
        '    let adapter_output = std::env::var_os("W9_ADAPTER_OUTPUT_DIR").map(PathBuf::from);\n'
        '',
    )
    source = _replace_once(
        source,
        '        let (sources, parsed) = parse_load_entry_source_arena_only(&file, entry, roots);\n',
        '        let (sources, parsed) = parse_load_entry_source_arena_only(&file, entry, roots);\n'
        '        if let Some(path) = adapter_output.as_ref() {\n'
        '            w9_write_adapter_owners(path, index, &parsed.arena, &sources)\n'
        '                .unwrap_or_else(|error| panic!("write owner index {index}: {error}"));\n'
        '        }\n',
    )
    source = _replace_once(
        source,
        '                let serialization_thread_cpu_end_ns = w9_thread_cpu_nanos();\n',
        '                if let Some(path) = adapter_output.as_ref() {\n'
        '                    w9_write_adapter_record(path, index, name, "checked", &diagnostic_facts, &rendered_publication_facts);\n'
        '                }\n'
        '                let serialization_thread_cpu_end_ns = w9_thread_cpu_nanos();\n',
    )
    source = _replace_once(
        source,
        '            for (name, _) in active_options.iter().copied() {\n',
        '            for (name, _) in active_options.iter().copied() {\n'
        '                if let Some(path) = adapter_output.as_ref() {\n'
        '                    w9_write_adapter_record(path, index, name, "load-error", &canonical, "");\n'
        '                }\n',
    )
    source += '''
fn w9_open_adapter_directory(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags, CWD};
    use std::path::Component;
    if !path.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "adapter path must be absolute",
        ));
    }
    let mut directory = std::fs::File::from(
        rustix::fs::openat(
            CWD,
            std::path::Path::new("/"),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = std::fs::File::from(
                    rustix::fs::openat(
                        &directory,
                        name,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(std::io::Error::from)?,
                );
            }
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "adapter output path must be absolute and contain no parent components")),
        }
    }
    Ok(directory)
}

fn w9_create_new_adapter_file(
    directory: &std::path::Path,
    name: &str,
) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags};
    let directory = w9_open_adapter_directory(directory)?;
    let file = rustix::fs::openat(
        &directory,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map(std::fs::File::from)
    .map_err(std::io::Error::from)?;
    directory.sync_all()?;
    Ok(file)
}

fn w9_create_adapter_directory(
    parent: &std::fs::File,
    name: &str,
) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags};
    rustix::fs::mkdirat(parent, name, Mode::from_raw_mode(0o700))
        .map_err(std::io::Error::from)?;
    parent.sync_all()?;
    rustix::fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(std::io::Error::from)
}

fn w9_create_adapter_file_at(
    directory: &std::fs::File,
    name: &str,
) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags};
    rustix::fs::openat(
        directory,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map(std::fs::File::from)
    .map_err(std::io::Error::from)
}

fn w9_write_adapter_file(
    directory: &std::fs::File,
    name: &str,
    contents: &[u8],
) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut output = w9_create_adapter_file_at(directory, name)?;
    output.write_all(contents)?;
    output.sync_all()
}

fn w9_write_adapter_record(
    root: &std::path::Path,
    index: usize,
    profile: &str,
    kind: &str,
    diagnostics: &str,
    publication: &str,
) {
    let directory_name = format!("record-{index}-{profile}");
    let root = w9_open_adapter_directory(root)
        .unwrap_or_else(|error| panic!("open adapter output directory: {error}"));
    let directory = w9_create_adapter_directory(&root, &directory_name)
        .unwrap_or_else(|error| panic!("create adapter record directory: {error}"));
    w9_write_adapter_file(&directory, "kind", kind.as_bytes())
        .unwrap_or_else(|error| panic!("write adapter outcome kind: {error}"));
    w9_write_adapter_file(&directory, "diagnostics", diagnostics.as_bytes())
        .unwrap_or_else(|error| panic!("write adapter diagnostics: {error}"));
    w9_write_adapter_file(&directory, "publication", publication.as_bytes())
        .unwrap_or_else(|error| panic!("write adapter publication: {error}"));
    let hashes = format!("{}\\t{}", blake3::hash(diagnostics.as_bytes()).to_hex(), blake3::hash(publication.as_bytes()).to_hex());
    w9_write_adapter_file(&directory, "receipt-hashes", hashes.as_bytes())
        .unwrap_or_else(|error| panic!("write adapter receipt hashes: {error}"));
}

fn w9_hash_adapter_tree(root_path: &std::path::Path, count: usize) -> std::io::Result<()> {
    use rustix::fs::{Dir, FileType, Mode, OFlags};
    use std::collections::BTreeSet;
    use std::io::Read as _;

    let mut multi_chunk = (0..4097).map(|index| (index % 251) as u8).collect::<Vec<_>>();
    let original_hash = blake3::hash(&multi_chunk);
    multi_chunk[2048] ^= 1;
    if original_hash == blake3::hash(&multi_chunk) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "BLAKE3 did not change after a multi-chunk input mutation",
        ));
    }
    let root = w9_open_adapter_directory(root_path)?;
    let mut expected_names = BTreeSet::from(["schema".to_owned()]);
    for index in 0..count {
        expected_names.insert(format!("owners-{index}.tsv"));
        for profile in ["default", "reveal", "execution", "check-cli"] {
            expected_names.insert(format!("record-{index}-{profile}"));
        }
    }
    let mut actual_names = BTreeSet::new();
    let mut entries = Dir::read_from(&root).map_err(std::io::Error::from)?;
    while let Some(entry) = entries.read() {
        let entry = entry.map_err(std::io::Error::from)?;
        let name = std::str::from_utf8(entry.file_name().to_bytes()).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error)
        })?;
        if name != "." && name != ".." && !actual_names.insert(name.to_owned()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("duplicate raw adapter sidecar {name}"),
            ));
        }
    }
    if actual_names != expected_names {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("raw adapter sidecar set differs: missing={:?} extra={:?}",
                expected_names.difference(&actual_names).collect::<Vec<_>>(),
                actual_names.difference(&expected_names).collect::<Vec<_>>()),
        ));
    }
    let schema = std::fs::File::from(
        rustix::fs::openat(
            &root,
            "schema",
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    let schema_metadata = rustix::fs::fstat(&schema).map_err(std::io::Error::from)?;
    if FileType::from_raw_mode(schema_metadata.st_mode as _) != FileType::RegularFile {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "raw adapter schema is not a regular file",
        ));
    }
    let mut schema_contents = String::new();
    let mut schema = schema;
    schema.read_to_string(&mut schema_contents)?;
    if schema_contents != "w9-publication-adapter\\t1\\n" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "raw adapter schema differs",
        ));
    }
    for index in 0..count {
        for profile in ["default", "reveal", "execution", "check-cli"] {
            let directory_name = format!("record-{index}-{profile}");
            let directory = std::fs::File::from(
                rustix::fs::openat(
                    &root,
                    directory_name.as_str(),
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(std::io::Error::from)?,
            );
            for field in ["diagnostics", "publication"] {
                let file = std::fs::File::from(
                    rustix::fs::openat(
                        &directory,
                        field,
                        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(std::io::Error::from)?,
                );
                let metadata = rustix::fs::fstat(&file).map_err(std::io::Error::from)?;
                if FileType::from_raw_mode(metadata.st_mode as _) != FileType::RegularFile {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("{directory_name}/{field} is not a regular file"),
                    ));
                }
                let mut contents = Vec::new();
                let mut file = file;
                file.read_to_end(&mut contents)?;
                println!("w9rawhash\\t{index}\\t{profile}\\t{field}\\t{}", blake3::hash(&contents).to_hex());
            }
        }
    }
    Ok(())
}
'''
    if output_directory is None:
        raise PublicationError("output directory is required")
    if output_root is None:
        output_root = ROOT
    output_root = runner.real_directory(output_root, "adapter helper work root")
    output_directory = pathlib.Path(os.path.abspath(output_directory))
    if not runner.under(output_directory, output_root / ".work"):
        raise PublicationError(f"adapter helper output must be under its source .work/: {output_directory}")
    _mkdir_new_directory(output_directory, runner)
    helper = output_directory / "w9-publication-adapter.rs"
    _write_new_file(helper, source.encode("utf-8"), runner)
    owner_copy = output_directory / "w9-publication-owner-helper.rs"
    _write_new_file(owner_copy, owner_source.encode("utf-8"), runner)
    return (
        helper,
        hashlib.sha256(source.encode()).hexdigest(),
        hashlib.sha256(owner_source.encode("utf-8")).hexdigest(),
        hashlib.sha256(owner_bytes).hexdigest(),
    )


def _candidate_approved_helper(candidate_root: pathlib.Path, runner) -> pathlib.Path:
    candidate_root = runner.real_directory(candidate_root, "candidate source")
    expected = candidate_root / "dev/consolidation/winddown/gate-tools/w9-repository-parity.rs"
    helper = runner.regular_file(expected, "candidate source-approved helper")
    if helper != expected or runner.require_sha256(
        helper, APPROVED_HELPER_SHA256, "candidate source-approved helper"
    ) != APPROVED_HELPER_SHA256:
        raise PublicationError("candidate source-approved helper identity differs")
    return helper


def _load_publication_runner(path: pathlib.Path):
    spec = importlib.util.spec_from_file_location("w9_publication_run", path)
    if spec is None or spec.loader is None:
        raise PublicationError(f"cannot load W9 publication runner {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _load_candidate_publication_runner(
    candidate_root: pathlib.Path,
    source_receipt: pathlib.Path,
    runner,
):
    candidate_root = runner.real_directory(candidate_root, "candidate source")
    source_receipt = runner.regular_file(source_receipt, "candidate source and binary receipt")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record.get("candidate_root", "")) != candidate_root:
        raise PublicationError("candidate source receipt names a different checkout")
    if runner.source_identity(candidate_root) != (
        source_record.get("candidate_commit"), source_record.get("candidate_tree")
    ):
        raise PublicationError("candidate source identity differs from its source receipt")
    candidate_runner = runner.regular_file(
        candidate_root / "dev/consolidation/w9-publication-run.py",
        "candidate publication runner",
    )
    if source_record.get("runner") != str(candidate_runner):
        raise PublicationError("candidate source receipt publication runner path differs")
    if not re.fullmatch(r"[0-9a-f]{64}", source_record.get("runner_sha256", "")):
        raise PublicationError("candidate source receipt publication runner hash is malformed")
    if runner.sha256_file(candidate_runner) != source_record["runner_sha256"]:
        raise PublicationError("candidate publication runner hash differs from its source receipt")
    loaded_runner = _load_publication_runner(candidate_runner)
    if loaded_runner.CONTROLLER_ROOT != runner.CONTROLLER_ROOT:
        raise PublicationError("candidate publication runner uses a different controller Git root")
    return loaded_runner


def _load_exact_owner_runner(runner):
    path = runner.regular_file(
        pathlib.Path(__file__).with_name("w9_publication_exact_owner_runner.py"),
        "controller exact-owner validator",
    )
    name = "_w9_publication_exact_owner_runner"
    loaded = sys.modules.get(name)
    if loaded is not None and pathlib.Path(loaded.__file__) == path:
        return loaded
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise PublicationError(f"could not load controller exact-owner validator: {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    try:
        spec.loader.exec_module(module)
    except BaseException:
        del sys.modules[name]
        raise
    return module


def _validated_exact_owners(proof: dict[str, object], role: str, count: int):
    owners = proof.get("owners")
    if not isinstance(owners, dict) or set(owners) != set(range(count)):
        raise PublicationError(f"exact-owner {role} proof does not cover all {count} inputs")
    for index, owner in owners.items():
        if (
            not isinstance(owner, tuple)
            or len(owner) != 2
            or not isinstance(owner[0], tuple)
            or len(owner[0]) != 4
            or not isinstance(owner[1], dict)
        ):
            raise PublicationError(f"exact-owner {role} proof has malformed owner {index}")
    return owners


def _require_exact_capture_binding(
    proof: dict[str, object], role: str, expected_capture: pathlib.Path, runner
) -> None:
    capture = proof.get("capture")
    if not isinstance(capture, dict):
        raise PublicationError(f"exact-owner {role} proof has no capture binding")
    capture_schema = capture.get("schema")
    if capture_schema == "2":
        capture_field = "adapter_capture_receipt"
        hash_field = "adapter_capture_receipt_sha256"
    elif capture_schema == "1":
        capture_field = "legacy_capture_receipt"
        hash_field = "legacy_capture_receipt_sha256"
    else:
        raise PublicationError(f"exact-owner {role} proof has an unsupported capture schema")
    recorded_capture = runner.regular_file(
        capture.get(capture_field, ""),
        f"exact-owner {role} bound adapter capture receipt",
    )
    if recorded_capture != expected_capture:
        raise PublicationError(f"exact-owner {role} proof names a different adapter capture receipt")
    recorded_hash = capture.get(hash_field, "")
    if not re.fullmatch(r"[0-9a-f]{64}", recorded_hash):
        raise PublicationError(f"exact-owner {role} proof has a malformed adapter capture hash")
    if runner.sha256_file(expected_capture) != recorded_hash:
        raise PublicationError(f"exact-owner {role} proof adapter capture hash differs")


def _mkdir_new_directory(path: pathlib.Path, runner) -> None:
    parent = runner.open_absolute_nofollow(path.parent, directory=True)
    try:
        os.mkdir(path.name, 0o700, dir_fd=parent)
        os.fsync(parent)
    finally:
        os.close(parent)


def _write_new_file(path: pathlib.Path, contents: bytes, runner) -> None:
    parent = runner.open_absolute_nofollow(path.parent, directory=True)
    descriptor = -1
    created = False
    try:
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        descriptor = os.open(path.name, flags, 0o600, dir_fd=parent)
        created = True
        with os.fdopen(descriptor, "wb") as output:
            descriptor = -1
            output.write(contents)
            output.flush()
            os.fsync(output.fileno())
        os.fsync(parent)
        created = False
    except BaseException:
        if descriptor >= 0:
            os.close(descriptor)
        if created:
            try:
                os.unlink(path.name, dir_fd=parent)
                os.fsync(parent)
            except FileNotFoundError:
                pass
        raise
    finally:
        os.close(parent)


def _read_regular_bytes_at(directory: int, name: str) -> bytes:
    if not name or name in {".", ".."} or "/" in name:
        raise PublicationError(f"invalid sidecar filename {name!r}")
    descriptor = -1
    try:
        flags = os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        descriptor = os.open(name, flags, dir_fd=directory)
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise PublicationError(f"sidecar member is not a regular file: {name}")
        contents = bytearray()
        while True:
            block = os.read(descriptor, 1024 * 1024)
            if not block:
                break
            contents.extend(block)
        return bytes(contents)
    except OSError as error:
        raise PublicationError(f"cannot read regular sidecar member {name!r}: {error}") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _read_regular_at(directory: int, name: str) -> str:
    try:
        return _read_regular_bytes_at(directory, name).decode("utf-8")
    except UnicodeError as error:
        raise PublicationError(f"sidecar member is not UTF-8: {name}") from error


def _open_directory_at(directory: int, name: str) -> int:
    if not name or name in {".", ".."} or "/" in name:
        raise PublicationError(f"invalid sidecar directory name {name!r}")
    descriptor = -1
    try:
        flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        descriptor = os.open(name, flags, dir_fd=directory)
        if not stat.S_ISDIR(os.fstat(descriptor).st_mode):
            raise PublicationError(f"sidecar member is not a directory: {name}")
        return descriptor
    except OSError as error:
        raise PublicationError(f"cannot open sidecar directory {name!r}: {error}") from error


def _record(root: int, key: tuple[int, str, str]) -> tuple[str, str, str, tuple[str, str]]:
    index, _relative_path, profile = key
    name = f"record-{index}-{profile}"
    directory = _open_directory_at(root, name)
    try:
        if set(os.listdir(directory)) != {"kind", "diagnostics", "publication", "receipt-hashes"}:
            raise PublicationError(f"{name}: outcome fields differ")
        kind = _read_regular_at(directory, "kind")
        diagnostics_bytes = _read_regular_bytes_at(directory, "diagnostics")
        publication_bytes = _read_regular_bytes_at(directory, "publication")
        diagnostics = diagnostics_bytes.decode("utf-8")
        publication = publication_bytes.decode("utf-8")
        hash_fields = _read_regular_at(directory, "receipt-hashes").split("\t")
        if kind not in {"checked", "load-error"}:
            raise PublicationError(f"{name}: invalid outcome kind {kind!r}")
        if len(hash_fields) != 2 or any(not re.fullmatch(r"[0-9a-f]{64}", digest) for digest in hash_fields):
            raise PublicationError(f"{name}: malformed receipt hashes")
        return kind, diagnostics, publication, (hash_fields[0], hash_fields[1])
    finally:
        os.close(directory)


def _parse_raw_hash_output(stdout: str, count: int) -> dict[tuple[int, str, str], str]:
    hashes: dict[tuple[int, str, str], str] = {}
    expected_profiles = set(PROFILES)
    for line in stdout.splitlines():
        if line.startswith("w9rawhash"):
            row = line
        elif "w9rawhash" in line:
            if not line.startswith(RAW_HASH_LIBTEST_PREFIX):
                raise PublicationError("native raw hash output has an unknown prefix")
            row = line[len(RAW_HASH_LIBTEST_PREFIX):]
        else:
            continue
        fields = row.split("\t")
        if len(fields) != 5 or fields[0] != "w9rawhash":
            raise PublicationError("native raw hash output has a malformed row")
        try:
            index = int(fields[1])
        except ValueError as error:
            raise PublicationError("native raw hash output has a malformed input index") from error
        profile, field, digest = fields[2:]
        if (
            index < 0
            or index >= count
            or profile not in expected_profiles
            or field not in {"diagnostics", "publication"}
            or not re.fullmatch(r"[0-9a-f]{64}", digest)
        ):
            raise PublicationError("native raw hash output contains an unexpected key or digest")
        key = (index, profile, field)
        if key in hashes:
            raise PublicationError(f"native raw hash output repeats {key}")
        hashes[key] = digest
    expected = {
        (index, profile, field)
        for index in range(count)
        for profile in PROFILES
        for field in ("diagnostics", "publication")
    }
    if set(hashes) != expected:
        raise PublicationError(
            f"native raw hash output coverage differs: missing={len(expected - set(hashes))} "
            f"extra={len(set(hashes) - expected)}"
        )
    return hashes


def _raw_hash_pair_matches(
    hashes: dict[tuple[int, str, str], str],
    index: int,
    profile: str,
    receipt_hashes: tuple[str, str],
) -> bool:
    return (
        hashes.get((index, profile, "diagnostics")),
        hashes.get((index, profile, "publication")),
    ) == receipt_hashes


def _hash_raw_outcomes(
    binary: pathlib.Path,
    raw_root: pathlib.Path,
    source_record: dict[str, str],
    count: int,
    capture_output: pathlib.Path,
    role: str,
    cpu: int,
    docker_environment: dict[str, str],
    runner,
) -> dict[tuple[int, str, str], str]:
    if not runner.under(raw_root, pathlib.Path(source_record["candidate_root"]) / ".work"):
        raise PublicationError(f"raw adapter output is outside candidate .work/: {raw_root}")
    job_dir = capture_output / "hashjobs" / role
    _mkdir_new_directory(job_dir, runner)
    container = f"w9adapter-{os.getpid()}-hash-{role}"
    if runner.existing_container(container, docker_environment):
        raise PublicationError(f"adapter hash Docker container already exists: {container}")
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA:
        command = [
            "timeout", "900s", source_record["builder_path"], source_record["candidate_root"], "run",
            f"W9_INPUT_ROOT={source_record['input_root']}",
            f"W9_CPU_SET={cpu}",
            f"W9_CONTAINER_NAME={container}",
            "env",
            f"W9_ADAPTER_HASH_ROOT={raw_root}",
            f"W9_ADAPTER_HASH_INPUT_COUNT={count}",
            "taskset", "-c", str(cpu), str(binary), "--exact",
            "sema::w9_repository_parity_receipt", "--nocapture",
        ]
        environment = {
            **docker_environment,
            "W9_SOURCE_COMMIT": source_record["candidate_commit"],
            "W9_SOURCE_TREE": source_record["candidate_tree"],
        }
        job_root = pathlib.Path(source_record["candidate_root"])
    else:
        wrapper = runner.regular_file(source_record["wrapper"], "pinned native test wrapper", executable=True)
        wrapper_root = runner.real_directory(source_record["wrapper_root"], "pinned wrapper root")
        runner.validate_wrapper_root(wrapper_root, pathlib.Path(source_record["candidate_root"]))
        runner.require_sha256(wrapper, runner.WRAPPER_SHA256, "pinned native test wrapper")
        shim_dir = runner.docker_shim(
            job_dir, container, cpu, pathlib.Path(source_record["input_root"])
        )
        environment = dict(docker_environment)
        environment["PATH"] = f"{shim_dir}:/usr/bin:/bin"
        command = [
            "timeout", "900s", str(wrapper), str(wrapper_root), "run", "env",
            f"W9_ADAPTER_HASH_ROOT={raw_root}",
            f"W9_ADAPTER_HASH_INPUT_COUNT={count}",
            "taskset", "-c", str(cpu), str(binary), "--exact",
            "sema::w9_repository_parity_receipt", "--nocapture",
        ]
        job_root = wrapper_root
    log = job_dir / "worker.log"
    log_descriptor = os.open(
        log,
        os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0),
        0o600,
    )
    log_output = os.fdopen(log_descriptor, "wb")
    log_output.write(
        f"hash_role={role} cpu={cpu} memory_limit=1GiB input_count={count} raw_root={raw_root} "
        f"binary_sha256={runner.sha256_file(binary)}\n".encode()
    )
    log_output.flush()
    job: dict[str, object] = {
        "worker": role,
        "cpu": cpu,
        "start": 0,
        "end": count,
        "container": container,
        "receipt": log,
        "timing": log,
        "log": log,
        "process": None,
        "output": log_output,
    }
    active: list[dict[str, object]] = []
    try:
        runner.start_and_register_job(
            job, active, command, job_root, environment
        )
        process = job["process"]
        while process.poll() is None:
            time.sleep(1)
        status = process.returncode
        if not log_output.closed:
            log_output.close()
        if status != 0:
            raise PublicationError(
                f"native {role} raw outcome hashing failed with status {status}; log={log}"
            )
        leftovers = runner.inspect_owned_containers(
            f"^w9adapter-{os.getpid()}-hash-{role}$", docker_environment
        )
        if leftovers:
            runner.stop_jobs(active, docker_environment)
            raise PublicationError(
                f"adapter hash Docker container remained after {role} hashing: {', '.join(leftovers)}"
            )
        active.remove(job)
        return _parse_raw_hash_output(runner.read_regular_text(log), count)
    except BaseException as primary_error:
        if not log_output.closed:
            log_output.close()
        if active:
            try:
                runner.stop_jobs(active, docker_environment)
            except runner.CleanupFailure as cleanup_error:
                raise runner.CleanupFailure(
                    f"{role} hash failed: {primary_error}; {cleanup_error}"
                ) from primary_error
        raise


def prepare_raw_output_directory(
    path: pathlib.Path,
    candidate_root: pathlib.Path,
    runner,
) -> pathlib.Path:
    output = pathlib.Path(os.path.abspath(path))
    if not runner.under(output, candidate_root / ".work"):
        raise PublicationError(f"raw sidecar directory must be under candidate source .work/: {output}")
    _mkdir_new_directory(output, runner)
    _write_new_file(output / "schema", RAW_SCHEMA.encode("utf-8"), runner)
    return output


def _create_consolidation_directory(root: pathlib.Path, runner) -> pathlib.Path:
    root_fd = runner.open_absolute_nofollow(root, directory=True)
    work_fd = -1
    consolidation_fd = -1
    try:
        work_fd = runner.open_or_create_child_directory(root_fd, ".work", "candidate .work")
        consolidation_fd = runner.open_or_create_child_directory(
            work_fd, "consolidation", "candidate .work/consolidation"
        )
    finally:
        if consolidation_fd >= 0:
            os.close(consolidation_fd)
        if work_fd >= 0:
            os.close(work_fd)
        os.close(root_fd)
    return runner.real_directory(root / ".work/consolidation", "candidate consolidation directory")


def _new_derived_helper_directory(root: pathlib.Path, runner) -> pathlib.Path:
    consolidation = _create_consolidation_directory(root, runner)
    attempt_directory = pathlib.Path(
        tempfile.mkdtemp(prefix="w9-publication-adapter-v1-", dir=consolidation)
    )
    attempt_directory = runner.real_directory(attempt_directory, "new adapter helper attempt")
    if not runner.under(attempt_directory, consolidation):
        raise PublicationError("adapter helper attempt escaped candidate consolidation")
    return attempt_directory / "derived"


def _adapter_build_output_paths(
    candidate_root: pathlib.Path,
    adapter_helper: pathlib.Path,
    role: str,
    commit: str,
    runner,
) -> tuple[pathlib.Path, pathlib.Path]:
    consolidation = _create_consolidation_directory(candidate_root, runner)
    attempt_directory = runner.real_directory(
        adapter_helper.parent.parent,
        f"{role} adapter build attempt directory",
    )
    if (
        not runner.under(attempt_directory, consolidation)
        or adapter_helper != attempt_directory / "derived/w9-publication-adapter.rs"
    ):
        raise PublicationError(f"{role} adapter helper does not belong to its retained build attempt")
    binary = runner.new_work_file_path(
        attempt_directory / f"w9-publication-adapter-{role}-{commit[:12]}",
        candidate_root,
        f"{role} adapter harness",
    )
    receipt = runner.new_work_file_path(
        attempt_directory / f"w9-publication-adapter-{role}-{commit[:12]}.receipt",
        candidate_root,
        f"{role} adapter build receipt",
    )
    return binary, receipt


def _planned_adapter_artifact_root(command, build_environment, runner):
    plan_command = [*command[:2], "plan-build", *command[3:]]
    try:
        result = subprocess.run(
            plan_command,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=build_environment,
        )
    except OSError as error:
        raise PublicationError(f"could not plan source-qualified adapter build: {error}") from error
    if result.returncode != 0:
        detail = result.stderr.decode(errors="replace").strip() or result.stdout.decode(errors="replace").strip()
        raise PublicationError(
            f"source-qualified adapter build planning failed with status {result.returncode}: {detail}"
        )
    return runner.cargo_target_directory_from_plan(result.stdout)


def _write_adapter_build_receipt(path: pathlib.Path, record: dict[str, str], runner) -> None:
    fields = {
        "1": ADAPTER_BUILD_FIELDS,
        "2": SOURCE_ADAPTER_BUILD_FIELDS,
    }.get(record.get("schema", ""))
    if fields is None or set(record) != set(fields):
        raise PublicationError("adapter build receipt fields differ")
    _write_new_file(
        path,
        "".join(f"{field}\t{record[field]}\n" for field in fields).encode("utf-8"),
        runner,
    )


def _parse_receipt_header(
    receipt: pathlib.Path,
    lines: list[str],
    fields: tuple[str, ...],
    label: str,
) -> tuple[dict[str, str], list[str]]:
    if len(lines) < len(fields):
        raise PublicationError(f"{receipt}: incomplete {label} receipt")
    record: dict[str, str] = {}
    for row_number, (line, expected) in enumerate(zip(lines, fields), 1):
        parts = line.split("\t")
        if len(parts) != 2 or parts[0] != expected or not parts[1]:
            raise PublicationError(f"{receipt}: malformed {label} receipt row {row_number}")
        record[parts[0]] = parts[1]
    return record, lines[len(fields):]


def _parse_adapter_build_receipt(
    path: pathlib.Path,
    role: str,
    runner,
    owner_helper_source: pathlib.Path | None = None,
) -> dict[str, str]:
    receipt = runner.regular_file(path, f"{role} adapter build receipt")
    lines = runner.read_regular_text(receipt).splitlines()
    if not lines:
        raise PublicationError(f"{receipt}: incomplete adapter build receipt")
    first = lines[0].split("\t")
    if len(first) != 2 or first[0] != "schema":
        raise PublicationError(f"{receipt}: malformed adapter build receipt schema row")
    if first[1] == "1":
        return _parse_historical_adapter_build_receipt(
            receipt, role, runner, owner_helper_source, lines
        )
    if first[1] == "2":
        return _parse_source_adapter_build_receipt(
            receipt, role, runner, owner_helper_source, lines
        )
    raise PublicationError(f"{receipt}: unsupported adapter build receipt schema {first[1]!r}")


def _parse_historical_adapter_build_receipt(
    path: pathlib.Path,
    role: str,
    runner,
    owner_helper_source: pathlib.Path | None = None,
    lines: list[str] | None = None,
) -> dict[str, str]:
    receipt = runner.regular_file(path, f"{role} adapter build receipt")
    record, remainder = _parse_receipt_header(
        receipt,
        runner.read_regular_text(receipt).splitlines() if lines is None else lines,
        ADAPTER_BUILD_FIELDS,
        "adapter build",
    )
    if remainder:
        raise PublicationError(f"{receipt}: unexpected adapter build receipt rows")
    if set(record) != set(ADAPTER_BUILD_FIELDS) or record["schema"] != "1" or record["role"] != role:
        raise PublicationError(f"{receipt}: adapter build receipt schema or role differs")
    for field in ("source_commit", "source_tree", "binary_sha256", "builder_sha256", "approved_helper_sha256", "helper_sha256", "owner_helper_sha256", "owner_helper_source_sha256", "overlay_sha256"):
        if not re.fullmatch(r"[0-9a-f]{40,64}", record[field]):
            raise PublicationError(f"{receipt}: malformed {field}")
    contract = runner.w9_build_contract()
    if (
        record["builder_image"] != contract["W9_CACHE_BUILD_IMAGE"]
        or record["build_target"] != contract["W9_CACHE_BUILD_TARGET"]
        or record["build_profile"] != contract["W9_CACHE_BUILD_PROFILE"]
    ):
        raise PublicationError(f"{receipt}: pinned build image, target, or profile differs")
    source_root = runner.real_directory(record["source_root"], f"{role} source root")
    if role == "reference" and record["source_commit"] != runner.REFERENCE_HEAD:
        raise PublicationError(f"{receipt}: reference source commit differs from its pin")
    if role == "reference" and record["source_tree"] != REFERENCE_TREE:
        raise PublicationError(f"{receipt}: reference source tree differs from its pin")
    if role == "reference" and source_root.name != "w9-adapter-ref-e240":
        raise PublicationError(f"{receipt}: reference source root is not the clean adapter clone")
    if role == "derived-reference":
        try:
            reference_correction.validate_source(
                source_root, reference_correction.PATCH_PATH, runner
            )
        except reference_correction.CorrectionError as error:
            raise PublicationError(f"{receipt}: {error}") from error
    binary = runner.regular_file(record["binary"], f"{role} adapter harness", executable=True)
    if runner.source_identity(source_root) != (record["source_commit"], record["source_tree"]):
        raise PublicationError(f"{receipt}: source identity changed after adapter build")
    if runner.sha256_file(binary) != record["binary_sha256"]:
        raise PublicationError(f"{receipt}: adapter harness hash changed")
    approved_helper = runner.regular_file(record["approved_helper_path"], "approved parity helper")
    if pathlib.Path(record["approved_helper_path"]) != runner.APPROVED_HELPER_SOURCE:
        raise PublicationError(f"{receipt}: approved helper path differs from its pin")
    if runner.sha256_file(approved_helper) != APPROVED_HELPER_SHA256 or record["approved_helper_sha256"] != APPROVED_HELPER_SHA256:
        raise PublicationError(f"{receipt}: approved helper hash differs from its pin")
    builder = runner.regular_file(record["builder_path"], "pinned adapter build runner", executable=True)
    if runner.require_sha256(builder, runner.PINNED_BUILD_RUNNER_SHA256, "pinned adapter build runner") != record["builder_sha256"]:
        raise PublicationError(f"{receipt}: build runner hash differs from its pin")
    helper_path = runner.regular_file(record["helper_path"], "derived W9 adapter helper")
    owner_helper_path = runner.regular_file(record["owner_helper_path"], "derived W9 owner helper")
    if runner.sha256_file(helper_path) != record["helper_sha256"]:
        raise PublicationError(f"{receipt}: derived helper hash differs")
    if runner.sha256_file(owner_helper_path) != record["owner_helper_sha256"]:
        raise PublicationError(f"{receipt}: derived owner helper hash differs")
    owner_helper_source = runner.regular_file(record["owner_helper_source_path"], "pinned W9 owner helper source")
    expected_owner_source = OWNER_HELPER if owner_helper_source is None else owner_helper_source
    if pathlib.Path(record["owner_helper_source_path"]) != expected_owner_source:
        raise PublicationError(f"{receipt}: owner helper source path differs from its pin")
    if runner.sha256_file(owner_helper_source) != OWNER_HELPER_SHA256 or record["owner_helper_source_sha256"] != OWNER_HELPER_SHA256:
        raise PublicationError(f"{receipt}: owner helper source hash differs from its pin")
    source_tests = runner.regular_file(source_root / "tests/sema.rs", f"{role} semantic tests")
    source_tests_bytes = runner.read_regular_bytes(source_tests)
    expected_overlay = hashlib.sha256(_adapter_overlay(source_tests_bytes, runner)).hexdigest()
    if record["overlay_sha256"] != expected_overlay:
        raise PublicationError(f"{receipt}: source-derived overlay hash differs")
    return record


def _parse_source_adapter_build_receipt(
    path: pathlib.Path,
    role: str,
    runner,
    owner_helper_source: pathlib.Path | None,
    lines: list[str],
) -> dict[str, str]:
    receipt = runner.regular_file(path, f"{role} adapter build receipt")
    record, remainder = _parse_receipt_header(
        receipt, lines, SOURCE_ADAPTER_BUILD_FIELDS, "source-qualified adapter build"
    )
    if remainder or set(record) != set(SOURCE_ADAPTER_BUILD_FIELDS):
        raise PublicationError(f"{receipt}: source-qualified adapter build receipt fields differ")
    if record["schema"] != "2" or record["role"] != role:
        raise PublicationError(f"{receipt}: source-qualified adapter build schema or role differs")
    for field in (
        "source_commit", "source_tree", "binary_sha256", "source_receipt_sha256",
        "input_manifest_sha256", "linker_sha256", "runner_sha256", "approved_helper_sha256",
        "helper_sha256", "owner_helper_sha256", "owner_helper_source_sha256", "overlay_sha256",
    ):
        if not re.fullmatch(r"[0-9a-f]{64}", record[field]) and not (
            field in {"source_commit", "source_tree"}
            and re.fullmatch(r"[0-9a-f]{40}", record[field])
        ):
            raise PublicationError(f"{receipt}: malformed {field}")
    source_root = runner.real_directory(record["source_root"], f"{role} source root")
    if str(source_root) != record["source_root"]:
        raise PublicationError(f"{receipt}: source root path is not canonical")
    if runner.source_identity(source_root) != (record["source_commit"], record["source_tree"]):
        raise PublicationError(f"{receipt}: source identity changed after adapter build")
    if role == "reference" and (
        record["source_commit"], record["source_tree"]
    ) != (runner.REFERENCE_HEAD, REFERENCE_TREE):
        raise PublicationError(f"{receipt}: reference source identity differs from its pin")
    if role == "reference" and source_root.name != "w9-adapter-ref-e240":
        raise PublicationError(f"{receipt}: reference source root is not the clean adapter clone")
    if role == "derived-reference":
        try:
            reference_correction.validate_source(
                source_root, reference_correction.PATCH_PATH, runner
            )
        except reference_correction.CorrectionError as error:
            raise PublicationError(f"{receipt}: {error}") from error
    binary = runner.regular_file(record["binary"], f"{role} adapter harness", executable=True)
    if str(binary) != record["binary"] or runner.sha256_file(binary) != record["binary_sha256"]:
        raise PublicationError(f"{receipt}: adapter harness identity differs")
    source_receipt = runner.regular_file(record["source_receipt"], "source-qualified cohort receipt")
    if str(source_receipt) != record["source_receipt"]:
        raise PublicationError(f"{receipt}: source receipt path is not canonical")
    if runner.sha256_file(source_receipt) != record["source_receipt_sha256"]:
        raise PublicationError(f"{receipt}: source receipt hash differs")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if source_record.get("schema") != runner.SOURCE_COHORT_SCHEMA:
        raise PublicationError(f"{receipt}: source receipt is not source-qualified")
    if role == "candidate" and source_root != pathlib.Path(source_record["candidate_root"]):
        raise PublicationError(f"{receipt}: candidate build source differs from its cohort receipt")
    if (
        record["input_root"] != source_record["input_root"]
        or record["input_manifest"] != source_record["input_manifest"]
        or record["input_manifest_sha256"] != source_record["input_manifest_sha256"]
        or record["corpus_blake3"] != source_record["corpus_blake3"]
    ):
        raise PublicationError(f"{receipt}: input cohort identity differs from its source receipt")
    for build_field, source_field in (
        ("builder_path", "builder_path"),
        ("builder_sha256", "builder_sha256"),
        ("builder_image", "builder_image"),
        ("build_target", "build_target"),
        ("build_profile", "build_profile"),
        ("rustflags", "rustflags"),
        ("linker_path", "linker_path"),
        ("linker_sha256", "linker_sha256"),
        ("runner_path", "runner"),
        ("runner_sha256", "runner_sha256"),
    ):
        if record[build_field] != source_record[source_field]:
            raise PublicationError(f"{receipt}: {build_field} differs from its source receipt")
    builder = runner.regular_file(record["builder_path"], "source-qualified adapter build runner", executable=True)
    if str(builder) != record["builder_path"] or runner.require_sha256(
        builder, runner.SOURCE_BUILDER_SHA256, "source-qualified adapter build runner"
    ) != record["builder_sha256"]:
        raise PublicationError(f"{receipt}: source-qualified build runner identity differs")
    linker = runner.regular_file(record["linker_path"], "source-qualified musl linker")
    if str(linker) != record["linker_path"] or runner.require_sha256(
        linker, runner.SOURCE_LINKER_SHA256, "source-qualified musl linker"
    ) != record["linker_sha256"]:
        raise PublicationError(f"{receipt}: source-qualified linker identity differs")
    input_root = runner.real_directory(record["input_root"], "source-qualified input root")
    input_manifest = runner.regular_file(record["input_manifest"], "source-qualified input manifest")
    if str(input_root) != record["input_root"] or str(input_manifest) != record["input_manifest"]:
        raise PublicationError(f"{receipt}: source-qualified input paths are not canonical")
    approved_helper = runner.regular_file(record["approved_helper_path"], "source-qualified approved helper")
    expected_approved_helper = runner.real_directory(
        source_record["candidate_root"], "candidate source"
    ) / "dev/consolidation/winddown/gate-tools/w9-repository-parity.rs"
    if approved_helper != expected_approved_helper or runner.require_sha256(
        approved_helper, APPROVED_HELPER_SHA256, "source-qualified approved helper"
    ) != record["approved_helper_sha256"]:
        raise PublicationError(f"{receipt}: source-qualified approved helper identity differs")
    helper_path = runner.regular_file(record["helper_path"], "derived W9 adapter helper")
    owner_helper_path = runner.regular_file(record["owner_helper_path"], "derived W9 owner helper")
    if runner.sha256_file(helper_path) != record["helper_sha256"]:
        raise PublicationError(f"{receipt}: derived helper hash differs")
    if runner.sha256_file(owner_helper_path) != record["owner_helper_sha256"]:
        raise PublicationError(f"{receipt}: derived owner helper hash differs")
    owner_helper_source = runner.regular_file(
        record["owner_helper_source_path"], "pinned W9 owner helper source"
    )
    expected_owner_source = runner.real_directory(
        source_record["candidate_root"], "candidate source"
    ) / "dev/consolidation/w9-publication-owner-helper.rs"
    if owner_helper_source != expected_owner_source or runner.require_sha256(
        owner_helper_source, OWNER_HELPER_SHA256, "pinned W9 owner helper source"
    ) != record["owner_helper_source_sha256"]:
        raise PublicationError(f"{receipt}: owner helper source identity differs")
    source_tests = runner.regular_file(source_root / "tests/sema.rs", f"{role} semantic tests")
    expected_overlay = hashlib.sha256(_adapter_overlay(runner.read_regular_bytes(source_tests), runner)).hexdigest()
    if record["overlay_sha256"] != expected_overlay:
        raise PublicationError(f"{receipt}: source-derived overlay hash differs")
    return record


def _write_capture_receipt(
    path: pathlib.Path,
    role: str,
    build_receipt: pathlib.Path,
    build: dict[str, str],
    capture_root: pathlib.Path,
    raw_root: pathlib.Path,
    receipt_directory: pathlib.Path,
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    range_receipts: dict[tuple[int, int], pathlib.Path],
    runner,
) -> None:
    build_schema = build.get("schema")
    if build_schema == "2":
        record = {
            "schema": "2",
            "role": role,
            "source_receipt": build["source_receipt"],
            "source_receipt_sha256": build["source_receipt_sha256"],
            "source_root": build["source_root"],
            "source_commit": build["source_commit"],
            "source_tree": build["source_tree"],
            "binary": build["binary"],
            "binary_sha256": build["binary_sha256"],
            "build_receipt": str(build_receipt),
            "build_receipt_sha256": runner.sha256_file(build_receipt),
            "builder_path": build["builder_path"],
            "builder_sha256": build["builder_sha256"],
            "runner_path": build["runner_path"],
            "runner_sha256": build["runner_sha256"],
            "linker_path": build["linker_path"],
            "linker_sha256": build["linker_sha256"],
            "helper_sha256": build["helper_sha256"],
            "owner_helper_sha256": build["owner_helper_sha256"],
            "input_root": str(input_root),
            "input_manifest": str(input_manifest),
            "input_manifest_sha256": build["input_manifest_sha256"],
            "corpus_blake3": build["corpus_blake3"],
            "capture_root": str(capture_root),
            "raw_root": str(raw_root),
            "receipt_directory": str(receipt_directory),
            "range_count": str(len(range_receipts)),
            "outcome_count": str(sum(end - start for start, end in range_receipts) * len(PROFILES)),
        }
        fields = SOURCE_CAPTURE_FIELDS
    elif build_schema == "1":
        record = {
            "schema": "1",
            "role": role,
            "source_root": build["source_root"],
            "source_commit": build["source_commit"],
            "source_tree": build["source_tree"],
            "binary": build["binary"],
            "binary_sha256": build["binary_sha256"],
            "build_receipt": str(build_receipt),
            "build_receipt_sha256": runner.sha256_file(build_receipt),
            "helper_sha256": build["helper_sha256"],
            "owner_helper_sha256": build["owner_helper_sha256"],
            "input_root": str(input_root),
            "input_manifest": str(input_manifest),
            "input_manifest_sha256": runner.INPUT_MANIFEST_SHA256,
            "corpus_blake3": runner.CORPUS_BLAKE3,
            "capture_root": str(capture_root),
            "raw_root": str(raw_root),
            "receipt_directory": str(receipt_directory),
            "range_count": str(len(range_receipts)),
            "outcome_count": str(sum(end - start for start, end in range_receipts) * len(PROFILES)),
        }
        fields = CAPTURE_FIELDS
    else:
        raise PublicationError("adapter build receipt has an unsupported schema")
    if set(record) != set(fields):
        raise PublicationError("adapter capture receipt fields differ")
    lines = [f"{field}\t{record[field]}\n" for field in fields]
    for (start, end), receipt in sorted(range_receipts.items()):
        lines.append(f"range\t{start}\t{end}\t{runner.sha256_file(receipt)}\n")
    _write_new_file(path, "".join(lines).encode("utf-8"), runner)


def _parse_capture_receipt(
    path: pathlib.Path,
    role: str,
    receipt_directory: pathlib.Path,
    capture_root: pathlib.Path,
    raw_root: pathlib.Path,
    expected_build: dict[str, str],
    expected_inputs: dict[int, object],
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    runner,
) -> dict[str, str]:
    receipt = runner.regular_file(path, f"{role} adapter capture receipt")
    lines = runner.read_regular_text(receipt).splitlines()
    if not lines:
        raise PublicationError(f"{receipt}: incomplete adapter capture receipt")
    first = lines[0].split("\t")
    if len(first) != 2 or first[0] != "schema":
        raise PublicationError(f"{receipt}: malformed adapter capture receipt schema row")
    if first[1] == "1":
        if expected_build.get("schema") != "1":
            raise PublicationError(f"{receipt}: adapter capture and build receipt schemas differ")
        return _parse_historical_capture_receipt(
            receipt, role, receipt_directory, capture_root, raw_root, expected_build,
            expected_inputs, input_root, input_manifest, runner, lines,
        )
    if first[1] == "2":
        if expected_build.get("schema") != "2":
            raise PublicationError(f"{receipt}: adapter capture and build receipt schemas differ")
        return _parse_source_capture_receipt(
            receipt, role, receipt_directory, capture_root, raw_root, expected_build,
            expected_inputs, input_root, input_manifest, runner, lines,
        )
    raise PublicationError(f"{receipt}: unsupported adapter capture receipt schema {first[1]!r}")


def _parse_historical_capture_receipt(
    path: pathlib.Path,
    role: str,
    receipt_directory: pathlib.Path,
    capture_root: pathlib.Path,
    raw_root: pathlib.Path,
    expected_build: dict[str, str],
    expected_inputs: dict[int, object],
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    runner,
    lines: list[str] | None = None,
) -> dict[str, str]:
    receipt = runner.regular_file(path, f"{role} adapter capture receipt")
    lines = runner.read_regular_text(receipt).splitlines() if lines is None else lines
    record, range_lines = _parse_receipt_header(receipt, lines, CAPTURE_FIELDS, "adapter capture")
    if set(record) != set(CAPTURE_FIELDS) or record["schema"] != "1" or record["role"] != role:
        raise PublicationError(f"{receipt}: capture receipt schema or role differs")
    for field in ("source_root", "source_commit", "source_tree", "binary", "binary_sha256", "helper_sha256", "owner_helper_sha256"):
        if record[field] != expected_build[field]:
            raise PublicationError(f"{receipt}: {field} differs from the adapter build receipt")
    build_receipt = runner.regular_file(record["build_receipt"], f"{role} adapter build receipt")
    if runner.sha256_file(build_receipt) != record["build_receipt_sha256"]:
        raise PublicationError(f"{receipt}: adapter build receipt hash differs")
    if record["input_root"] != str(input_root) or record["input_manifest"] != str(input_manifest):
        raise PublicationError(f"{receipt}: frozen input root or manifest path differs")
    if (
        record["capture_root"] != str(capture_root)
        or record["raw_root"] != str(raw_root)
        or record["receipt_directory"] != str(receipt_directory)
        or raw_root != capture_root / "raw"
        or receipt_directory != capture_root / "receipts"
    ):
        raise PublicationError(f"{receipt}: capture output paths differ")
    if record["input_manifest_sha256"] != runner.INPUT_MANIFEST_SHA256 or record["corpus_blake3"] != runner.CORPUS_BLAKE3:
        raise PublicationError(f"{receipt}: frozen corpus pins differ")
    count = len(expected_inputs)
    ranges = runner.expected_ranges(count)
    if record["range_count"] != str(len(ranges)) or record["outcome_count"] != str(count * len(PROFILES)):
        raise PublicationError(f"{receipt}: required source/profile coverage differs")
    range_rows: list[tuple[int, int, str]] = []
    for row_number, line in enumerate(range_lines, len(CAPTURE_FIELDS) + 1):
        fields = line.split("\t")
        if len(fields) != 4 or fields[0] != "range":
            raise PublicationError(f"{receipt}: malformed range receipt row {row_number}")
        try:
            start, end = int(fields[1]), int(fields[2])
        except ValueError as error:
            raise PublicationError(f"{receipt}: malformed range bounds at row {row_number}") from error
        if not re.fullmatch(r"[0-9a-f]{64}", fields[3]):
            raise PublicationError(f"{receipt}: malformed range receipt hash at row {row_number}")
        range_rows.append((start, end, fields[3]))
    if [(start, end) for start, end, _digest in range_rows] != ranges:
        raise PublicationError(f"{receipt}: range set differs from all required 1690-source shards")
    for (start, end), (_row_start, _row_end, digest) in zip(ranges, range_rows):
        shard = runner.candidate_receipt_path(receipt_directory, start, end)
        if runner.sha256_file(shard) != digest:
            raise PublicationError(f"{receipt}: range receipt hash differs for {start}..{end}")
    if not range_rows:
        raise PublicationError(f"{receipt}: no source ranges were recorded")
    return record


def _parse_source_capture_receipt(
    path: pathlib.Path,
    role: str,
    receipt_directory: pathlib.Path,
    capture_root: pathlib.Path,
    raw_root: pathlib.Path,
    expected_build: dict[str, str],
    expected_inputs: dict[int, object],
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    runner,
    lines: list[str],
) -> dict[str, str]:
    receipt = runner.regular_file(path, f"{role} adapter capture receipt")
    record, range_lines = _parse_receipt_header(
        receipt, lines, SOURCE_CAPTURE_FIELDS, "source-qualified adapter capture"
    )
    if len(record) != len(SOURCE_CAPTURE_FIELDS) or record["schema"] != "2" or record["role"] != role:
        raise PublicationError(f"{receipt}: source-qualified capture schema or role differs")
    for field in (
        "source_root", "source_commit", "source_tree", "binary", "binary_sha256",
        "builder_path", "builder_sha256", "runner_path", "runner_sha256", "linker_path",
        "linker_sha256", "helper_sha256", "owner_helper_sha256",
    ):
        if record[field] != expected_build[field]:
            raise PublicationError(f"{receipt}: {field} differs from the adapter build receipt")
    source_receipt = runner.regular_file(record["source_receipt"], "source-qualified cohort receipt")
    if str(source_receipt) != record["source_receipt"]:
        raise PublicationError(f"{receipt}: source receipt path is not canonical")
    if runner.sha256_file(source_receipt) != record["source_receipt_sha256"]:
        raise PublicationError(f"{receipt}: source receipt hash differs")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if source_record.get("schema") != runner.SOURCE_COHORT_SCHEMA:
        raise PublicationError(f"{receipt}: source receipt is not source-qualified")
    build_receipt = runner.regular_file(record["build_receipt"], f"{role} adapter build receipt")
    if str(build_receipt) != record["build_receipt"]:
        raise PublicationError(f"{receipt}: build receipt path is not canonical")
    if runner.sha256_file(build_receipt) != record["build_receipt_sha256"]:
        raise PublicationError(f"{receipt}: adapter build receipt hash differs")
    if (
        record["source_receipt"] != expected_build["source_receipt"]
        or record["source_receipt_sha256"] != expected_build["source_receipt_sha256"]
        or record["input_root"] != expected_build["input_root"]
        or record["input_manifest"] != expected_build["input_manifest"]
        or record["input_manifest_sha256"] != expected_build["input_manifest_sha256"]
        or record["corpus_blake3"] != expected_build["corpus_blake3"]
    ):
        raise PublicationError(f"{receipt}: source cohort identity differs from the adapter build receipt")
    if record["input_root"] != str(input_root) or record["input_manifest"] != str(input_manifest):
        raise PublicationError(f"{receipt}: frozen input root or manifest path differs")
    if (
        record["capture_root"] != str(capture_root)
        or record["raw_root"] != str(raw_root)
        or record["receipt_directory"] != str(receipt_directory)
        or raw_root != capture_root / "raw"
        or receipt_directory != capture_root / "receipts"
    ):
        raise PublicationError(f"{receipt}: capture output paths differ")
    if (
        record["input_manifest_sha256"] != source_record["input_manifest_sha256"]
        or record["corpus_blake3"] != source_record["corpus_blake3"]
    ):
        raise PublicationError(f"{receipt}: frozen corpus pins differ")
    ranges = runner.expected_ranges(len(expected_inputs))
    if record["range_count"] != str(len(ranges)) or record["outcome_count"] != str(len(expected_inputs) * len(PROFILES)):
        raise PublicationError(f"{receipt}: required source/profile coverage differs")
    range_rows: list[tuple[int, int, str]] = []
    for row_number, line in enumerate(range_lines, len(SOURCE_CAPTURE_FIELDS) + 1):
        fields = line.split("\t")
        if len(fields) != 4 or fields[0] != "range":
            raise PublicationError(f"{receipt}: malformed range receipt row {row_number}")
        try:
            start, end = int(fields[1]), int(fields[2])
        except ValueError as error:
            raise PublicationError(f"{receipt}: malformed range bounds at row {row_number}") from error
        if not re.fullmatch(r"[0-9a-f]{64}", fields[3]):
            raise PublicationError(f"{receipt}: malformed range receipt hash at row {row_number}")
        range_rows.append((start, end, fields[3]))
    if [(start, end) for start, end, _digest in range_rows] != ranges:
        raise PublicationError(f"{receipt}: range set differs from all required 1690-source shards")
    for (start, end), (_row_start, _row_end, digest) in zip(ranges, range_rows):
        shard = runner.candidate_receipt_path(receipt_directory, start, end)
        if runner.sha256_file(shard) != digest:
            raise PublicationError(f"{receipt}: range receipt hash differs for {start}..{end}")
    return record


def _reuse_reference_capture(
    capture_receipt: pathlib.Path,
    build_receipt: pathlib.Path,
    build: dict[str, str],
    expected_inputs: dict[int, object],
    reference,
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    runner,
) -> tuple[pathlib.Path, pathlib.Path, pathlib.Path, dict[tuple[int, int], pathlib.Path]]:
    receipt = runner.regular_file(capture_receipt, "reference adapter capture receipt")
    capture_root = pathlib.Path(_capture_field(receipt, "capture_root", runner))
    raw_root = pathlib.Path(_capture_field(receipt, "raw_root", runner))
    receipt_directory = pathlib.Path(_capture_field(receipt, "receipt_directory", runner))
    capture_root = runner.real_directory(capture_root, "reference adapter capture root")
    raw_root = runner.real_directory(raw_root, "reference adapter raw outcomes")
    receipt_directory = runner.real_directory(receipt_directory, "reference adapter range receipts")
    if receipt.parent != capture_root or receipt.name != "capture.receipt":
        raise PublicationError("reference capture receipt is outside its recorded capture root")
    record = _parse_capture_receipt(
        receipt,
        "reference",
        receipt_directory,
        capture_root,
        raw_root,
        build,
        expected_inputs,
        input_root,
        input_manifest,
        runner,
    )
    expected_build_receipt = runner.regular_file(build_receipt, "reference adapter build receipt")
    if pathlib.Path(record["build_receipt"]) != expected_build_receipt:
        raise PublicationError("reference capture names a different adapter build receipt")
    if runner.sha256_file(expected_build_receipt) != record["build_receipt_sha256"]:
        raise PublicationError("reference capture adapter build receipt hash differs")
    captured_reference = aggregate_candidate_receipts(
        runner, receipt_directory, expected_inputs, reference
    )
    if reference is not None and captured_reference.outcomes != reference.outcomes:
        raise PublicationError("reused reference harness outcomes differ from the pinned reference receipt")
    range_receipts = {
        interval: runner.candidate_receipt_path(receipt_directory, *interval)
        for interval in runner.expected_ranges(len(expected_inputs))
    }
    return raw_root, receipt_directory, receipt, range_receipts


def _capture_plan(
    count: int,
    runner,
    publication_bytes_by_range: dict[tuple[int, int], int] | None = None,
) -> tuple[list[tuple[int, int]], list[tuple[int, int]]]:
    ranges = runner.expected_ranges(count)
    next_start = 0
    for start, end in ranges:
        if start != next_start or end <= start:
            raise PublicationError("adapter capture plan does not cover each input exactly once")
        next_start = end
    if next_start != count:
        raise PublicationError("adapter capture plan does not cover each input exactly once")
    if publication_bytes_by_range is None:
        return ranges, list(ranges)
    if set(publication_bytes_by_range) != set(ranges) or any(
        type(size) is not int or size < 0 for size in publication_bytes_by_range.values()
    ):
        raise PublicationError("reference publication byte counts do not cover each capture range")
    return ranges, sorted(
        ranges,
        key=lambda interval: (-publication_bytes_by_range[interval], interval),
    )


def _publication_bytes_by_range(
    raw_root: pathlib.Path,
    count: int,
    ranges: list[tuple[int, int]],
    runner,
) -> dict[tuple[int, int], int]:
    if ranges != runner.expected_ranges(count):
        raise PublicationError("reference publication sizes require canonical capture ranges")
    range_by_index = {
        index: interval
        for interval in ranges
        for index in range(interval[0], interval[1])
    }
    if set(range_by_index) != set(range(count)):
        raise PublicationError("reference publication sizes do not cover every input")
    sizes = {interval: 0 for interval in ranges}
    root_descriptor = runner.open_absolute_nofollow(raw_root, directory=True)
    try:
        for index, interval in range_by_index.items():
            for profile in PROFILES:
                record_name = f"record-{index}-{profile}"
                record_descriptor = _open_directory_at(root_descriptor, record_name)
                try:
                    if set(os.listdir(record_descriptor)) != {
                        "kind", "diagnostics", "publication", "receipt-hashes"
                    }:
                        raise PublicationError(f"{record_name}: outcome fields differ")
                    try:
                        publication_descriptor = os.open(
                            "publication",
                            os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
                            | getattr(os, "O_CLOEXEC", 0),
                            dir_fd=record_descriptor,
                        )
                    except OSError as error:
                        raise PublicationError(
                            f"cannot open reference publication bytes for {record_name}: {error}"
                        ) from error
                    try:
                        publication_metadata = os.fstat(publication_descriptor)
                        if not stat.S_ISREG(publication_metadata.st_mode):
                            raise PublicationError(
                                f"{record_name}: publication is not a regular file"
                            )
                        sizes[interval] += publication_metadata.st_size
                    finally:
                        os.close(publication_descriptor)
                finally:
                    os.close(record_descriptor)
    finally:
        os.close(root_descriptor)
    return sizes


def _launch_capture_job(
    runner,
    source_record: dict[str, str],
    output: pathlib.Path,
    role: str,
    build_record: dict[str, str],
    binary: pathlib.Path,
    raw_root: pathlib.Path,
    receipt_directory: pathlib.Path,
    start: int,
    end: int,
    worker: str,
    active: list[dict[str, object]],
    docker_environment: dict[str, str],
) -> dict[str, object]:
    cpu = next((cpu for slot, cpu in CAPTURE_SLOTS if slot == worker), None)
    if cpu is None:
        raise PublicationError(f"unknown adapter capture worker {worker!r}")
    job_dir = output / role / "jobs" / worker / f"{start}-{end}"
    job_dir.mkdir(parents=True, exist_ok=False)
    receipt = runner.candidate_receipt_path(receipt_directory, start, end)
    timing = job_dir / "timing.txt"
    log = job_dir / "worker.log"
    if any(path.exists() or path.is_symlink() for path in (receipt, timing, log)):
        raise PublicationError(f"refusing to overwrite adapter shard artifact {role}/{worker}/{start}..{end}")
    container = f"w9adapter-{os.getpid()}-{role}-{worker}-{start}"
    if runner.existing_container(container, docker_environment):
        raise PublicationError(f"adapter Docker container already exists: {container}")
    source_qualified = source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA
    if source_qualified:
        command = [
            "timeout", "900s", source_record["builder_path"], source_record["candidate_root"], "run",
            f"W9_INPUT_ROOT={source_record['input_root']}",
            f"W9_CPU_SET={cpu}",
            f"W9_CONTAINER_NAME={container}",
            "env",
            f"W9_RECEIPT_OUTPUT={receipt}",
            f"W9_TIMING_OUTPUT={timing}",
            f"W9_ADAPTER_OUTPUT_DIR={raw_root}",
            f"W9_SHARD_INDEX={start // 100}",
            "W9_SHARD_SIZE=100",
            f"W9_START={start}",
            f"W9_END={end}",
            "taskset", "-c", str(cpu), str(binary),
            "--exact", "sema::w9_repository_parity_receipt", "--nocapture",
        ]
        environment = {
            **docker_environment,
            "W9_SOURCE_COMMIT": source_record["candidate_commit"],
            "W9_SOURCE_TREE": source_record["candidate_tree"],
        }
    else:
        shim_dir = runner.docker_shim(job_dir, container, cpu, pathlib.Path(source_record["input_root"]))
        command = [
            "timeout", "900s", source_record["wrapper"], source_record["wrapper_root"], "run", "env",
            f"W9_INPUT_ROOT={source_record['input_root']}",
            f"W9_RECEIPT_OUTPUT={receipt}",
            f"W9_TIMING_OUTPUT={timing}",
            f"W9_ADAPTER_OUTPUT_DIR={raw_root}",
            f"W9_SHARD_INDEX={start // 100}",
            "W9_SHARD_SIZE=100",
            f"W9_START={start}",
            f"W9_END={end}",
            "taskset", "-c", str(cpu), str(binary),
            "--exact", "sema::w9_repository_parity_receipt", "--nocapture",
        ]
        environment = dict(docker_environment)
        environment["PATH"] = f"{shim_dir}:/usr/bin:/bin"
    log_descriptor = os.open(
        log,
        os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0),
        0o600,
    )
    log_output = os.fdopen(log_descriptor, "wb")
    log_output.write(
        f"role={role} cpu={cpu} memory_limit=1GiB range={start}..{end} "
        f"source={build_record['source_commit']}:{build_record['source_tree']} "
        f"binary_sha256={build_record['binary_sha256']} helper_sha256={build_record['helper_sha256']} "
        f"corpus={runner.CORPUS_BLAKE3}\n".encode()
    )
    log_output.flush()
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
        "output": log_output,
    }
    try:
        runner.start_and_register_job(job, active, command, str(source_record["candidate_root"]), environment)
    except BaseException:
        if not log_output.closed:
            log_output.close()
        raise
    return job


def _run_capture_role(
    runner,
    source_record: dict[str, str],
    output: pathlib.Path,
    role: str,
    build_record: dict[str, str],
    binary: pathlib.Path,
    count: int,
    docker_environment: dict[str, str],
    *,
    publication_bytes_by_range: dict[tuple[int, int], int] | None = None,
) -> tuple[pathlib.Path, pathlib.Path, dict[tuple[int, int], pathlib.Path]]:
    role_root = output / role
    role_root.mkdir()
    raw_root = prepare_raw_output_directory(role_root / "raw", pathlib.Path(source_record["candidate_root"]), runner)
    receipt_directory = role_root / "receipts"
    receipt_directory.mkdir()
    (role_root / "jobs").mkdir()
    ranges, pending_ranges = _capture_plan(count, runner, publication_bytes_by_range)
    active: list[dict[str, object]] = []
    completed: list[tuple[int, int]] = []
    try:
        for worker, _cpu in CAPTURE_SLOTS:
            if not pending_ranges:
                break
            start, end = pending_ranges.pop(0)
            (role_root / "jobs" / worker).mkdir()
            _launch_capture_job(
                runner, source_record, output, role, build_record, binary, raw_root, receipt_directory,
                start, end, worker, active, docker_environment,
            )
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
                if status != 0:
                    raise PublicationError(
                        f"{role} adapter worker {job['worker']} range {job['start']}..{job['end']} "
                        f"failed with status {status}; log={job['log']}"
                    )
                runner.regular_file(job["receipt"], f"{role} adapter range receipt")
                runner.regular_file(job["timing"], f"{role} adapter range timing")
                completed.append((int(job["start"]), int(job["end"])))
                active.remove(job)
                worker = str(job["worker"])
                if pending_ranges:
                    start, end = pending_ranges.pop(0)
                    _launch_capture_job(
                        runner, source_record, output, role, build_record, binary, raw_root, receipt_directory,
                        start, end, worker, active, docker_environment,
                    )
            if not changed:
                time.sleep(1)
        leftovers = runner.inspect_owned_containers(f"^w9adapter-{os.getpid()}-{role}-", docker_environment)
        if leftovers:
            stranded = [{"container": name, "process": None, "output": None} for name in leftovers]
            runner.stop_jobs(stranded, docker_environment)
            raise PublicationError(f"adapter Docker containers remained after {role} capture: {', '.join(leftovers)}")
    except BaseException as primary_error:
        try:
            runner.stop_jobs(active, docker_environment)
        except runner.CleanupFailure as cleanup_error:
            raise runner.CleanupFailure(f"{role} capture failed: {primary_error}; {cleanup_error}") from primary_error
        raise
    if sorted(completed) != ranges:
        raise PublicationError(f"{role} adapter capture did not complete every range")
    runner.write_status(role_root, "complete", completed, f"outcomes={count * len(PROFILES)}")
    range_receipts = runner.discover_candidate_receipts(receipt_directory, count)
    return raw_root, receipt_directory, range_receipts


def execute_adapter_publication(args: argparse.Namespace, runner) -> int:
    if not getattr(args, "capture_only", False):
        raise PublicationError(
            "run requires --capture-only; then capture exact owners and use compare-captures"
        )
    candidate_root = runner.real_directory(args.candidate_root, "candidate source")
    source_receipt = runner.regular_file(args.candidate_source_receipt, "candidate source and binary receipt")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise PublicationError("candidate source receipt names a different checkout")
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA:
        expected_inputs, _paths_by_name = runner.validate_source_cohort_inputs(source_record)
        reference = None
    else:
        expected_inputs, _paths_by_name, reference = runner.validate_all_inputs(source_record)
    if len(expected_inputs) != 1690:
        raise PublicationError(f"candidate source receipt input count differs: {len(expected_inputs)}")
    reference_source_root = _prepare_reference_source(
        candidate_root, pathlib.Path(args.reference_source_root), runner
    )
    reference_build_path = runner.regular_file(args.reference_build_receipt, "reference adapter build receipt")
    candidate_build_path = runner.regular_file(args.candidate_build_receipt, "candidate adapter build receipt")
    owner_helper_source = candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
    reference_build = _parse_adapter_build_receipt(
        reference_build_path, "reference", runner, owner_helper_source
    )
    candidate_build = _parse_adapter_build_receipt(
        candidate_build_path, "candidate", runner, owner_helper_source
    )
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and (
        reference_build["schema"] != "2" or candidate_build["schema"] != "2"
    ):
        raise PublicationError("source-qualified capture requires source-qualified adapter builds")
    if pathlib.Path(reference_build["source_root"]) != reference_source_root:
        raise PublicationError("reference adapter build receipt names a different source root")
    if pathlib.Path(candidate_build["source_root"]) != candidate_root:
        raise PublicationError("candidate adapter build receipt names a different source root")
    if (candidate_build["source_commit"], candidate_build["source_tree"]) != (
        source_record["candidate_commit"], source_record["candidate_tree"]
    ):
        raise PublicationError("candidate adapter binary source differs from candidate source receipt")
    if reference_build["helper_sha256"] != candidate_build["helper_sha256"]:
        raise PublicationError("reference and candidate adapter helpers differ")
    if reference_build["owner_helper_sha256"] != candidate_build["owner_helper_sha256"]:
        raise PublicationError("reference and candidate owner helpers differ")
    reference_capture = None
    reference_capture_receipt = getattr(args, "reference_capture_receipt", None)
    if reference_capture_receipt is not None:
        reference_capture = _reuse_reference_capture(
            reference_capture_receipt,
            reference_build_path,
            reference_build,
            expected_inputs,
            reference,
            pathlib.Path(source_record["input_root"]),
            pathlib.Path(source_record["input_manifest"]),
            runner,
        )
    ranges, _pending_ranges = _capture_plan(len(expected_inputs), runner)
    output = runner.require_output_path(args.output, candidate_root)
    if args.dry_run:
        print(
            f"adapter preflight ok: reference={reference_build['source_commit']}:{reference_build['source_tree']} "
            f"candidate={candidate_build['source_commit']}:{candidate_build['source_tree']} "
            f"inputs={len(expected_inputs)} profiles={len(PROFILES)} outcomes={len(expected_inputs) * len(PROFILES)} "
            f"ranges={len(ranges)} workers={len(CAPTURE_SLOTS)} "
            f"cpus={','.join(str(cpu) for _worker, cpu in CAPTURE_SLOTS)} "
            f"memory_per_worker=1GiB output={output}"
        )
        return 0
    if source_record.get("schema") != runner.SOURCE_COHORT_SCHEMA:
        wrapper = runner.regular_file(source_record["wrapper"], "pinned native test wrapper", executable=True)
        wrapper_root = runner.real_directory(source_record["wrapper_root"], "pinned wrapper root")
        runner.validate_wrapper_root(wrapper_root, candidate_root)
        runner.require_sha256(wrapper, runner.WRAPPER_SHA256, "pinned native test wrapper")
    docker_environment = runner.local_docker_environment()
    completed: list[tuple[int, int]] = []
    with runner.claim_run_resources(candidate_root, output, completed):
        runner.write_status(output, "incomplete", completed)
        captures: dict[str, tuple[pathlib.Path, pathlib.Path, pathlib.Path, dict[tuple[int, int], pathlib.Path]]] = {}
        publication_bytes_by_range = None
        if reference_capture is not None:
            captures["reference"] = reference_capture
            publication_bytes_by_range = _publication_bytes_by_range(
                reference_capture[0], len(expected_inputs), ranges, runner
            )
            completed.extend(ranges)
            runner.write_status(output, "incomplete", completed, "reused_capture=reference")
        for role, build_path, build in (
            ("reference", reference_build_path, reference_build),
            ("candidate", candidate_build_path, candidate_build),
        ):
            if role == "reference" and reference_capture is not None:
                continue
            binary = runner.regular_file(build["binary"], f"{role} adapter harness", executable=True)
            raw_root, receipt_directory, range_receipts = _run_capture_role(
                runner,
                source_record,
                output,
                role,
                build,
                binary,
                len(expected_inputs),
                docker_environment,
                publication_bytes_by_range=publication_bytes_by_range,
            )
            capture_receipt = output / role / "capture.receipt"
            _write_capture_receipt(
                capture_receipt,
                role,
                build_path,
                build,
                capture_receipt.parent,
                raw_root,
                receipt_directory,
                pathlib.Path(source_record["input_root"]),
                pathlib.Path(source_record["input_manifest"]),
                range_receipts,
                runner,
            )
            captures[role] = (raw_root, receipt_directory, capture_receipt, range_receipts)
            completed.extend(ranges)
            runner.write_status(output, "incomplete", completed, f"completed_capture={role}")
        summary = f"capture-only corpus={len(expected_inputs)} outcomes={len(expected_inputs) * len(PROFILES)}"
        runner.write_status(output, "capture-complete", completed, summary)
        print(
            f"{summary}\nreference_capture={captures['reference'][2]}\n"
            f"candidate_capture={captures['candidate'][2]}\ncomparison=not_run"
        )
        return 0


def build_adapter_harness(
    source_root: pathlib.Path,
    candidate_root: pathlib.Path,
    source_receipt: pathlib.Path,
    source_record: dict[str, str],
    role: str,
    adapter_helper: pathlib.Path,
    helper_sha256: str,
    owner_helper_sha256: str,
    owner_helper_source_sha256: str,
    runner,
) -> tuple[pathlib.Path, pathlib.Path]:
    candidate_root = runner.real_directory(candidate_root, f"{role} candidate source")
    source_root = runner.real_directory(source_root, f"{role} source")
    source_receipt = runner.regular_file(source_receipt, "source-qualified cohort receipt")
    if source_record.get("schema") != runner.SOURCE_COHORT_SCHEMA:
        raise PublicationError("source-qualified adapter build requires a schema-3 cohort receipt")
    if runner.parse_candidate_receipt(source_receipt) != source_record:
        raise PublicationError("source-qualified cohort receipt changed before adapter build")
    approved_helper = _candidate_approved_helper(candidate_root, runner)
    commit, tree = runner.source_identity(source_root)
    if role == "reference" and commit != runner.REFERENCE_HEAD:
        raise PublicationError(f"reference source commit differs from the approved head: {commit}")
    if role == "derived-reference":
        try:
            reference_correction.validate_source(
                source_root, reference_correction.PATCH_PATH, runner
            )
        except reference_correction.CorrectionError as error:
            raise PublicationError(str(error)) from error
    helper_bytes = runner.read_regular_bytes(adapter_helper)
    if hashlib.sha256(helper_bytes).hexdigest() != helper_sha256:
        raise PublicationError("derived adapter helper changed before harness build")
    owner_bytes = runner.read_regular_bytes(adapter_helper.parent / "w9-publication-owner-helper.rs")
    if hashlib.sha256(owner_bytes).hexdigest() != owner_helper_sha256:
        raise PublicationError("owner helper changed before harness build")
    builder = runner.regular_file(source_record["builder_path"], "source-qualified adapter build runner", executable=True)
    builder_sha256 = runner.require_sha256(
        builder, source_record["builder_sha256"], "source-qualified adapter build runner"
    )
    binary_path, receipt_path = _adapter_build_output_paths(
        candidate_root, adapter_helper, role, commit, runner
    )
    scratch_parent = runner.candidate_scratch_parent(candidate_root)
    docker_environment = runner.local_docker_environment()
    with runner.scratch_directory(scratch_parent) as scratch:
        scratch_root = scratch / "source"
        clone = subprocess.run(
            ["git", "clone", "--quiet", "--no-local", "--no-checkout", "--depth=1", str(source_root), str(scratch_root)],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        if clone.returncode != 0:
            raise PublicationError(f"could not clone {role} source for adapter build: {clone.stderr.strip()}")
        runner.git_output(scratch_root, "checkout", "--quiet", "--detach", commit)
        if runner.source_identity(scratch_root) != (commit, tree):
            raise PublicationError(f"{role} adapter scratch checkout identity differs")
        runner.exclude_work_from_git(scratch_root)
        scratch_tests = runner.regular_file(scratch_root / "tests/sema.rs", f"{role} scratch semantic tests")
        source_tests_bytes = runner.read_regular_bytes(scratch_tests)
        overlay_bytes = _adapter_overlay(source_tests_bytes, runner)
        overlay_sha256 = hashlib.sha256(overlay_bytes).hexdigest()
        _create_consolidation_directory(scratch_root, runner)
        _write_new_file(scratch_root / ".work/consolidation/w9-publication-adapter.rs", helper_bytes, runner)
        _write_new_file(scratch_root / ".work/consolidation/w9-publication-owner-helper.rs", owner_bytes, runner)
        scratch_tests.write_bytes(overlay_bytes)
        if hashlib.sha256(runner.read_regular_bytes(scratch_tests)).hexdigest() != overlay_sha256:
            raise PublicationError(f"{role} scratch parity overlay hash differs")
        runner.require_only_parity_overlay(scratch_root, f"{role} scratch source changes escaped the parity include")
        command = [
            str(builder), str(scratch_root), "build", "cargo", "test", "--locked",
            "--profile", source_record["build_profile"],
            "--target", source_record["build_target"],
            "-p", "xsh", "--test", "integration", "--no-run",
            "--message-format=json-render-diagnostics",
        ]
        build_environment = {
            **docker_environment,
            "W9_SOURCE_COMMIT": commit,
            "W9_SOURCE_TREE": tree,
            "W9_ROLE": role,
            "W9_OVERLAY_SHA256": overlay_sha256,
            "W9_LINKER_PATH": source_record["linker_path"],
            "W9_LINKER_SHA256": source_record["linker_sha256"],
        }
        artifact_root = _planned_adapter_artifact_root(command, build_environment, runner)
        result = subprocess.run(
            command,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=build_environment,
        )
        if result.returncode != 0:
            raise PublicationError(f"pinned {role} adapter harness build failed: {result.stderr.strip() or result.stdout.strip()}")
        scratch_binary = runner.cargo_target_integration_executable(result.stdout, artifact_root)
        _require_scratch_overlay_identity(scratch_root, commit, tree, role, runner)
        if hashlib.sha256(runner.read_regular_bytes(scratch_tests)).hexdigest() != overlay_sha256:
            raise PublicationError(f"{role} parity overlay changed during adapter build")
        if hashlib.sha256(runner.read_regular_bytes(scratch_root / ".work/consolidation/w9-publication-adapter.rs")).hexdigest() != helper_sha256:
            raise PublicationError(f"{role} adapter helper changed during build")
        if hashlib.sha256(runner.read_regular_bytes(scratch_root / ".work/consolidation/w9-publication-owner-helper.rs")).hexdigest() != owner_helper_sha256:
            raise PublicationError(f"{role} owner helper changed during build")
        if role == "derived-reference":
            try:
                reference_correction.validate_source(
                    source_root, reference_correction.PATCH_PATH, runner
                )
            except reference_correction.CorrectionError as error:
                raise PublicationError(str(error)) from error
        runtime_commit, runtime_tree = runner.source_identity(candidate_root)
        runner.run_harness_test_listing(
            builder,
            candidate_root,
            scratch_binary,
            {
                **docker_environment,
                "W9_SOURCE_COMMIT": runtime_commit,
                "W9_SOURCE_TREE": runtime_tree,
            },
        )
        binary = runner.copy_executable(scratch_binary, binary_path)
    binary_sha256 = runner.sha256_file(binary)
    record = {
        "schema": "2",
        "role": role,
        "source_root": str(source_root),
        "source_commit": commit,
        "source_tree": tree,
        "binary": str(binary),
        "binary_sha256": binary_sha256,
        "source_receipt": str(source_receipt),
        "source_receipt_sha256": runner.sha256_file(source_receipt),
        "input_root": source_record["input_root"],
        "input_manifest": source_record["input_manifest"],
        "input_manifest_sha256": source_record["input_manifest_sha256"],
        "corpus_blake3": source_record["corpus_blake3"],
        "builder_path": str(builder),
        "builder_sha256": builder_sha256,
        "builder_image": source_record["builder_image"],
        "build_target": source_record["build_target"],
        "build_profile": source_record["build_profile"],
        "rustflags": source_record["rustflags"],
        "linker_path": source_record["linker_path"],
        "linker_sha256": source_record["linker_sha256"],
        "runner_path": source_record["runner"],
        "runner_sha256": source_record["runner_sha256"],
        "approved_helper_path": str(approved_helper),
        "approved_helper_sha256": APPROVED_HELPER_SHA256,
        "helper_path": str(adapter_helper),
        "helper_sha256": helper_sha256,
        "owner_helper_path": str(adapter_helper.parent / "w9-publication-owner-helper.rs"),
        "owner_helper_sha256": owner_helper_sha256,
        "owner_helper_source_path": str(
            candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
        ),
        "owner_helper_source_sha256": owner_helper_source_sha256,
        "overlay_sha256": overlay_sha256,
    }
    _write_adapter_build_receipt(receipt_path, record, runner)
    return binary, receipt_path


def build_adapter_harnesses(args: argparse.Namespace, runner) -> None:
    candidate_root = runner.real_directory(args.candidate_root, "candidate source")
    source_receipt = runner.regular_file(args.candidate_source_receipt, "candidate source receipt")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise PublicationError("candidate source receipt names a different checkout")
    if source_record.get("schema") != runner.SOURCE_COHORT_SCHEMA:
        raise PublicationError("adapter harness builds require a source-qualified cohort receipt")
    expected_inputs, _paths_by_name = runner.validate_source_cohort_inputs(source_record)
    if len(expected_inputs) != 1690:
        raise PublicationError(f"candidate source receipt input count differs: {len(expected_inputs)}")
    reference_root = _prepare_reference_source(
        candidate_root, pathlib.Path(args.reference_source_root), runner
    )
    if runner.source_identity(candidate_root) != (
        source_record["candidate_commit"], source_record["candidate_tree"]
    ):
        raise PublicationError("candidate source identity differs from its source receipt")
    runner.candidate_scratch_parent(candidate_root)
    reference_build_receipt = getattr(args, "reference_build_receipt", None)
    reference_build = None
    if reference_build_receipt is not None:
        reference_receipt = runner.regular_file(
            reference_build_receipt, "reference adapter build receipt"
        )
        owner_helper_source = candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
        reference_build = _parse_adapter_build_receipt(
            reference_receipt, "reference", runner, owner_helper_source
        )
        if reference_build["schema"] != "2":
            raise PublicationError("reused reference adapter build is not source-qualified")
        if pathlib.Path(reference_build["source_root"]) != reference_root:
            raise PublicationError("reference adapter build receipt names a different source root")
    derived_directory = _new_derived_helper_directory(candidate_root, runner)
    owner_helper_source = candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
    adapter_helper, helper_sha256, owner_helper_sha256, owner_helper_source_sha256 = derive_helper(
        approved_helper=_candidate_approved_helper(candidate_root, runner),
        owner_helper=owner_helper_source,
        output_directory=derived_directory,
        output_root=candidate_root,
    )
    if reference_build is None:
        reference_binary, reference_receipt = build_adapter_harness(
            reference_root,
            candidate_root,
            source_receipt,
            source_record,
            "reference",
            adapter_helper,
            helper_sha256,
            owner_helper_sha256,
            owner_helper_source_sha256,
            runner,
        )
    else:
        if reference_build["helper_sha256"] != helper_sha256:
            raise PublicationError("reused reference adapter helper differs from the current helper")
        if reference_build["owner_helper_sha256"] != owner_helper_sha256:
            raise PublicationError("reused reference owner helper differs from the current helper")
        reference_binary = pathlib.Path(reference_build["binary"])
    candidate_binary, candidate_receipt = build_adapter_harness(
        candidate_root,
        candidate_root,
        source_receipt,
        source_record,
        "candidate",
        adapter_helper,
        helper_sha256,
        owner_helper_sha256,
        owner_helper_source_sha256,
        runner,
    )
    print(
        f"reference_adapter_binary={reference_binary}\nreference_build_receipt={reference_receipt}\n"
        f"candidate_adapter_binary={candidate_binary}\ncandidate_build_receipt={candidate_receipt}\n"
        f"candidate_source={source_record['candidate_commit']}:{source_record['candidate_tree']}\n"
        f"reference_source_root={reference_root}"
    )


def build_derived_reference_harness(args: argparse.Namespace, runner) -> int:
    candidate_root = runner.real_directory(args.candidate_root, "candidate source")
    source_receipt = runner.regular_file(args.candidate_source_receipt, "candidate source receipt")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise PublicationError("candidate source receipt names a different checkout")
    if source_record.get("schema") != runner.SOURCE_COHORT_SCHEMA:
        raise PublicationError("derived reference builds require a source-qualified cohort receipt")
    expected_inputs, _paths_by_name = runner.validate_source_cohort_inputs(source_record)
    if len(expected_inputs) != 1690:
        raise PublicationError(f"candidate source receipt input count differs: {len(expected_inputs)}")
    derived_root = runner.real_directory(args.derived_reference_source_root, "derived reference source")
    try:
        reference_correction.validate_source(
            derived_root, reference_correction.PATCH_PATH, runner
        )
    except reference_correction.CorrectionError as error:
        raise PublicationError(str(error)) from error
    if runner.source_identity(candidate_root) != (
        source_record["candidate_commit"], source_record["candidate_tree"]
    ):
        raise PublicationError("candidate source identity differs from its source receipt")
    runner.candidate_scratch_parent(candidate_root)
    derived_directory = _new_derived_helper_directory(candidate_root, runner)
    owner_helper_source = candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
    helper, helper_sha256, owner_helper_sha256, owner_helper_source_sha256 = derive_helper(
        approved_helper=_candidate_approved_helper(candidate_root, runner),
        owner_helper=owner_helper_source,
        output_directory=derived_directory,
        output_root=candidate_root,
    )
    binary, build_receipt = build_adapter_harness(
        derived_root,
        candidate_root,
        source_receipt,
        source_record,
        "derived-reference",
        helper,
        helper_sha256,
        owner_helper_sha256,
        owner_helper_source_sha256,
        runner,
    )
    try:
        reference_correction.validate_source(
            derived_root, reference_correction.PATCH_PATH, runner
        )
    except reference_correction.CorrectionError as error:
        raise PublicationError(str(error)) from error
    print(
        f"derived_reference_binary={binary}\nderived_reference_build_receipt={build_receipt}\n"
        f"derived_reference_source={reference_correction.SOURCE_COMMIT}:{reference_correction.SOURCE_TREE}"
    )
    return 0


def capture_derived_reference(args: argparse.Namespace, runner) -> int:
    candidate_root = runner.real_directory(args.candidate_root, "candidate source")
    source_receipt = runner.regular_file(args.candidate_source_receipt, "candidate source receipt")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise PublicationError("candidate source receipt names a different checkout")
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA:
        expected_inputs, _paths_by_name = runner.validate_source_cohort_inputs(source_record)
        reference = None
    else:
        expected_inputs, _paths_by_name, reference = runner.validate_all_inputs(source_record)
    if len(expected_inputs) * len(PROFILES) != reference_correction.EXPECTED_OUTCOME_COUNT:
        raise PublicationError("derived reference corpus size differs from its pinned coverage")
    if runner.source_identity(candidate_root) != (
        source_record["candidate_commit"], source_record["candidate_tree"]
    ):
        raise PublicationError("candidate source identity differs from its source receipt")
    derived_root = runner.real_directory(args.derived_reference_source_root, "derived reference source")
    try:
        source_identity = reference_correction.validate_source(
            derived_root, reference_correction.PATCH_PATH, runner
        )
    except reference_correction.CorrectionError as error:
        raise PublicationError(str(error)) from error
    input_root = pathlib.Path(source_record["input_root"])
    input_manifest = pathlib.Path(source_record["input_manifest"])
    original_capture_path = runner.regular_file(
        args.original_capture_receipt, "immutable original reference capture receipt"
    )
    original_build_path = runner.regular_file(
        _capture_field(original_capture_path, "build_receipt", runner),
        "immutable original reference build receipt",
    )
    owner_helper_source = candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
    original_build = _parse_adapter_build_receipt(
        original_build_path, "reference", runner, owner_helper_source
    )
    if (original_build["source_commit"], original_build["source_tree"]) != (
        runner.REFERENCE_HEAD, REFERENCE_TREE
    ):
        raise PublicationError("original reference build differs from its immutable source pin")
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and original_build["schema"] != "2":
        raise PublicationError("source-qualified correction requires a fresh source-qualified e240 build")
    original_capture = _reuse_reference_capture(
        original_capture_path,
        original_build_path,
        original_build,
        expected_inputs,
        reference,
        input_root,
        input_manifest,
        runner,
    )
    original_exact_build_path = runner.regular_file(
        args.original_exact_build_receipt, "immutable original exact-owner build receipt"
    )
    original_exact_capture_path = runner.regular_file(
        args.original_exact_capture_receipt, "immutable original exact-owner capture receipt"
    )
    exact_owner_runner = _load_exact_owner_runner(runner)
    exact_build = _parse_adapter_build_receipt(
        original_exact_build_path, "reference", runner, owner_helper_source
    )
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and exact_build["schema"] != "2":
        raise PublicationError("source-qualified correction requires a fresh e240 exact-owner build")
    try:
        _exact_helper_path, _exact_helper_bytes, exact_helper_sha256 = exact_owner_runner._load_exact_helper(runner)
    except exact_owner_runner.ExactOwnerError as error:
        raise PublicationError(f"original exact-owner helper validation failed: {error}") from error
    try:
        exact_capture, _exact_ranges, _owner_hashes = exact_owner_runner._parse_exact_capture_receipt(
            original_exact_capture_path,
            "reference",
            original_exact_build_path,
            exact_build,
            expected_inputs,
            reference,
            input_root,
            input_manifest,
            exact_helper_sha256,
            runner,
        )
    except exact_owner_runner.ExactOwnerError as error:
        raise PublicationError(f"original exact-owner capture validation failed: {error}") from error
    if exact_capture.get("schema") == "2":
        exact_capture_field = "adapter_capture_receipt"
        exact_capture_hash_field = "adapter_capture_receipt_sha256"
    elif exact_capture.get("schema") == "1":
        exact_capture_field = "legacy_capture_receipt"
        exact_capture_hash_field = "legacy_capture_receipt_sha256"
    else:
        raise PublicationError("original exact-owner capture has an unsupported schema")
    if pathlib.Path(exact_capture[exact_capture_field]) != original_capture_path:
        raise PublicationError("original exact-owner proof names a different adapter capture")
    if runner.sha256_file(original_capture_path) != exact_capture[exact_capture_hash_field]:
        raise PublicationError("original exact-owner proof adapter capture hash differs")
    build_receipt_path = runner.regular_file(
        args.derived_reference_build_receipt, "derived reference build receipt"
    )
    build = _parse_adapter_build_receipt(
        build_receipt_path, "derived-reference", runner, owner_helper_source
    )
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and build["schema"] != "2":
        raise PublicationError("source-qualified correction requires a fresh derived adapter build")
    if pathlib.Path(build["source_root"]) != derived_root:
        raise PublicationError("derived reference build receipt names a different source root")
    if (build["source_commit"], build["source_tree"]) != (
        source_identity["source_commit"], source_identity["source_tree"]
    ):
        raise PublicationError("derived reference build identity differs from the approved correction")
    if (original_build["helper_sha256"], original_build["owner_helper_sha256"]) != (
        build["helper_sha256"], build["owner_helper_sha256"]
    ):
        raise PublicationError("original reference adapter helpers differ from the correction harness")
    output = runner.require_output_path(args.output, candidate_root)
    if source_record.get("schema") != runner.SOURCE_COHORT_SCHEMA:
        wrapper = runner.regular_file(source_record["wrapper"], "pinned native test wrapper", executable=True)
        wrapper_root = runner.real_directory(source_record["wrapper_root"], "pinned wrapper root")
        runner.validate_wrapper_root(wrapper_root, candidate_root)
        runner.require_sha256(wrapper, runner.WRAPPER_SHA256, "pinned native test wrapper")
    docker_environment = runner.local_docker_environment()
    completed: list[tuple[int, int]] = []
    ranges = runner.expected_ranges(len(expected_inputs))
    publication_bytes_by_range = _publication_bytes_by_range(
        original_capture[0], len(expected_inputs), ranges, runner
    )
    output_claimed = False
    try:
        with runner.claim_run_resources(candidate_root, output, completed):
            output_claimed = True
            runner.write_status(output, "incomplete", [])
            raw_root, receipt_directory, range_receipts = _run_capture_role(
                runner,
                source_record,
                output,
                "derived-reference",
                build,
                pathlib.Path(build["binary"]),
                len(expected_inputs),
                docker_environment,
                publication_bytes_by_range=publication_bytes_by_range,
            )
            completed.extend(ranges)
            capture_receipt = output / "derived-reference" / "capture.receipt"
            _write_capture_receipt(
                capture_receipt,
                "derived-reference",
                build_receipt_path,
                build,
                capture_receipt.parent,
                raw_root,
                receipt_directory,
                input_root,
                input_manifest,
                range_receipts,
                runner,
            )
            try:
                reference_correction.validate_source(
                    derived_root, reference_correction.PATCH_PATH, runner
                )
            except reference_correction.CorrectionError as error:
                raise PublicationError(str(error)) from error
            correction_record = reference_correction.create_receipt_record(
                source_identity,
                build_receipt_path,
                build,
                capture_receipt,
                original_capture_path,
                original_exact_capture_path,
                runner,
            )
            correction_receipt = output / "correction.receipt"
            _write_new_file(
                correction_receipt,
                reference_correction.receipt_bytes(correction_record),
                runner,
            )
            summary = (
                f"derived-reference capture outcomes={len(expected_inputs) * len(PROFILES)} "
                f"ranges={len(range_receipts)}"
            )
            runner.write_status(output, "capture-complete", completed, summary)
    except BaseException as error:
        if output_claimed:
            try:
                runner.write_status(output, "incomplete", completed, f"{type(error).__name__}: {error}")
            except BaseException as status_error:
                print(f"could not record incomplete correction capture status: {status_error}", file=sys.stderr)
        raise
    print(
        f"{summary}\nderived_reference_capture={capture_receipt}\n"
        f"correction_receipt={correction_receipt}\n"
        f"original_reference_capture={original_capture_path}\n"
        f"original_exact_owner_capture={original_exact_capture_path}"
    )
    return 0


def aggregate_candidate_receipts(
    runner,
    candidate_receipt_directory: pathlib.Path,
    expected_inputs: dict[int, object],
    reference=None,
):
    count = len(expected_inputs)
    candidates = runner.discover_candidate_receipts(candidate_receipt_directory, count)
    all_inputs: dict[int, object] = {}
    all_outcomes: dict[tuple[int, str, str], tuple[str, str]] = {}
    for (start, end), path in sorted(candidates.items()):
        receipt = runner.parse_receipt(path, expected_inputs)
        runner.validate_candidate_shard(
            path, receipt, start, end, count, runner.CORPUS_BLAKE3
        )
        for index, value in receipt.inputs.items():
            if index in all_inputs:
                raise PublicationError(f"duplicate candidate input index {index}")
            if reference is not None and value != reference.inputs[index]:
                raise PublicationError(f"candidate receipt input path or digest differs at input {index}")
            if value.relative_path != expected_inputs[index].relative_path:
                raise PublicationError(f"candidate receipt input path differs at input {index}")
            all_inputs[index] = value
        for key, value in receipt.outcomes.items():
            if key in all_outcomes:
                raise PublicationError(f"duplicate candidate outcome {key}")
            all_outcomes[key] = value
    if set(all_inputs) != set(range(count)):
        raise PublicationError("candidate input receipt coverage differs from the source cohort")
    expected_outcome_keys = {
        (index, expected_inputs[index].relative_path, profile)
        for index in range(count)
        for profile in PROFILES
    }
    if set(all_outcomes) != expected_outcome_keys:
        raise PublicationError("candidate receipt coverage differs from the 1690 by 4 source cohort")
    if reference is not None and all_inputs != reference.inputs:
        raise PublicationError("candidate input receipt coverage differs from the reference")
    if reference is not None and set(all_outcomes) != set(reference.outcomes):
        raise PublicationError("candidate receipt coverage differs from the 1690 by 4 reference")
    terminals = reference.terminals if reference is not None else ()
    header = reference.header if reference is not None else ()
    return runner.ParsedReceipt(header, all_inputs, all_outcomes, terminals)


def _align_profile_publications(
    reference_publications: list[str],
    candidate_publications: list[str],
    reference_owner: tuple[
        dict[int, int],
        dict[str, set[str]],
        dict[str, tuple[str, str, str]],
        dict[str, tuple[str, str]],
    ],
    candidate_owner: tuple[
        dict[int, int],
        dict[str, set[str]],
        dict[str, tuple[str, str, str]],
        dict[str, tuple[str, str]],
    ],
    reference_tui_source: bytes,
    candidate_tui_source: bytes,
):
    return _align_profile_publications_with_parsed(
        reference_publications,
        candidate_publications,
        reference_owner,
        candidate_owner,
        reference_tui_source,
        candidate_tui_source,
    ).aligned


def _align_profile_publications_with_parsed(
    reference_publications: list[str],
    candidate_publications: list[str],
    reference_owner: tuple[
        dict[int, int],
        dict[str, set[str]],
        dict[str, tuple[str, str, str]],
        dict[str, tuple[str, str]],
    ],
    candidate_owner: tuple[
        dict[int, int],
        dict[str, set[str]],
        dict[str, tuple[str, str, str]],
        dict[str, tuple[str, str]],
    ],
    reference_tui_source: bytes,
    candidate_tui_source: bytes,
):
    if len(reference_publications) != len(PROFILES) or len(candidate_publications) != len(PROFILES):
        raise PublicationError("TUI source alignment requires all four publication profiles")
    return source_alignment._align_tui_publications_with_parsed(
        reference_publications=reference_publications,
        candidate_publications=candidate_publications,
        reference_source_sizes=reference_owner[0],
        reference_source_owners=reference_owner[1],
        candidate_source_sizes=candidate_owner[0],
        candidate_source_owners=candidate_owner[1],
        reference_tui_source=reference_tui_source,
        candidate_tui_source=candidate_tui_source,
    )


def _report_comparison_progress(
    processed_inputs: int,
    total_inputs: int,
    total_outcomes: int,
    mismatch_count: int,
) -> None:
    if processed_inputs % 25 == 0 or processed_inputs == total_inputs:
        processed_outcomes = processed_inputs * len(PROFILES)
        print(
            f"comparison_progress inputs={processed_inputs}/{total_inputs} "
            f"outcomes={processed_outcomes}/{total_outcomes} mismatches={mismatch_count}",
            file=sys.stderr,
            flush=True,
        )


def _compare_publication_input(
    index: int,
    context: _PublicationComparisonContext,
) -> tuple[str, ...]:
    mismatches: list[str] = []
    owner_error = False
    try:
        reference_owner = adapter.parse_owner_index(
            _read_regular_at(context.reference_root, f"owners-{index}.tsv")
        )
        candidate_owner = adapter.parse_owner_index(
            _read_regular_at(context.candidate_root, f"owners-{index}.tsv")
        )
    except (adapter.AdapterError, PublicationError, UnicodeError) as error:
        owner_error = True
        mismatches.append(f"{index}\towner-index\t{type(error).__name__}\t{error}")
    if not owner_error:
        if reference_owner != context.reference_exact_owners[index][0]:
            owner_error = True
            mismatches.append(f"{index}\treference-exact-owner-legacy-projection")
        if candidate_owner != context.candidate_exact_owners[index][0]:
            owner_error = True
            mismatches.append(f"{index}\tcandidate-exact-owner-legacy-projection")
    profile_records = []
    profile_error = False
    for profile in PROFILES:
        key = (index, context.expected_inputs[index].relative_path, profile)
        try:
            reference_kind, reference_diagnostics, reference_publication, reference_hashes = _record(
                context.reference_root, key
            )
            candidate_kind, candidate_diagnostics, candidate_publication, candidate_hashes = _record(
                context.candidate_root, key
            )
            reference_diagnostic_hash, reference_publication_hash = (
                context.expected_reference_outcomes.outcomes[key]
            )
            candidate_diagnostic_hash, candidate_publication_hash = context.candidate_outcomes[key]
            if not _raw_hash_pair_matches(context.reference_raw_hashes, index, profile, reference_hashes):
                mismatches.append(f"{key}\treference-raw-bytes-hash")
            if not _raw_hash_pair_matches(context.candidate_raw_hashes, index, profile, candidate_hashes):
                mismatches.append(f"{key}\tcandidate-raw-bytes-hash")
            if reference_hashes != (reference_diagnostic_hash, reference_publication_hash):
                mismatches.append(f"{key}\treference-raw-receipt-hash")
            if candidate_hashes != (candidate_diagnostic_hash, candidate_publication_hash):
                mismatches.append(f"{key}\tcandidate-raw-receipt-hash")
            if reference_diagnostics != candidate_diagnostics:
                mismatches.append(
                    f"{key}\tdiagnostics\t{hashlib.sha256(reference_diagnostics.encode()).hexdigest()}"
                    f"\t{hashlib.sha256(candidate_diagnostics.encode()).hexdigest()}"
                )
            if reference_kind != candidate_kind:
                mismatches.append(f"{key}\tkind\t{reference_kind}\t{candidate_kind}")
                profile_error = True
                continue
            if reference_kind == "load-error":
                if reference_publication or candidate_publication:
                    mismatches.append(f"{key}\tload-error-has-publication")
                    profile_error = True
                continue
            profile_records.append(
                (
                    profile,
                    key,
                    reference_diagnostics,
                    reference_publication,
                    candidate_diagnostics,
                    candidate_publication,
                )
            )
        except (adapter.AdapterError, PublicationError, UnicodeError, KeyError) as error:
            mismatches.append(f"{key}\tadapter\t{type(error).__name__}\t{error}")
            profile_error = True
    if owner_error or profile_error or not profile_records:
        return tuple(mismatches)
    try:
        profile_by_name = {record[0]: record for record in profile_records}
        candidate_publications = []
        for profile in PROFILES:
            record = profile_by_name.get(profile)
            if record is None:
                candidate_publications.append("")
                continue
            candidate_publication = record[5]
            if context.candidate_assignment_proofs is not None:
                proof = context.candidate_assignment_proofs.get(index)
                if proof is None or context.assignment_facts_module is None:
                    raise PublicationError(
                        f"assignment facts proof is missing for checked input {index}"
                    )
                candidate_publication = adapter.project_assignment_facts(
                    candidate_publication,
                    proof,
                    context.assignment_facts_module,
                )
            candidate_publications.append(candidate_publication)
        publication_alignment = _align_profile_publications_with_parsed(
            [
                profile_by_name[profile][3] if profile in profile_by_name else ""
                for profile in PROFILES
            ],
            candidate_publications,
            reference_owner,
            candidate_owner,
            context.reference_tui_source,
            context.candidate_tui_source,
        )
        aligned = publication_alignment.aligned
        (
            aligned_candidate_exact_owners,
            aligned_candidate_statement_owners,
        ) = source_alignment._align_tui_owner_facts(
            exact_owners=context.candidate_exact_owners[index][1],
            statement_owners=context.candidate_exact_owners[index][0][3],
            tui_source_id=aligned.tui_source_id,
            reference_tui_source=context.reference_tui_source,
            candidate_tui_source=context.candidate_tui_source,
        )
    except (adapter.AdapterError, PublicationError, UnicodeError, KeyError) as error:
        mismatches.append(f"{index}\ttui-alignment\t{type(error).__name__}\t{error}")
        return tuple(mismatches)
    comparison_outcomes: dict[
        tuple[str, str, str, str], tuple[type[Exception], str] | None
    ] = {}
    for (
        profile,
        key,
        reference_diagnostics,
        _reference_publication,
        candidate_diagnostics,
        _candidate_publication,
    ) in profile_records:
        profile_index = PROFILES.index(profile)
        comparison_pair = (
            reference_diagnostics,
            aligned.reference_publications[profile_index],
            candidate_diagnostics,
            aligned.candidate_publications[profile_index],
        )
        if comparison_pair in comparison_outcomes:
            cached_error = comparison_outcomes[comparison_pair]
            if cached_error is not None:
                error_type, error_text = cached_error
                mismatches.append(f"{key}\tadapter\t{error_type.__name__}\t{error_text}")
            continue
        try:
            reference_parsed = publication_alignment.reference[profile_index]
            candidate_parsed = publication_alignment.candidate[profile_index]
            if reference_parsed is None or candidate_parsed is None:
                adapter.compare_publications(
                    reference_diagnostics,
                    aligned.reference_publications[profile_index],
                    reference_owner[0],
                    reference_owner[1],
                    reference_owner[2],
                    reference_owner[3],
                    candidate_diagnostics,
                    aligned.candidate_publications[profile_index],
                    aligned.candidate_source_sizes,
                    aligned.candidate_source_owners,
                    reference_exact_owners=context.reference_exact_owners[index][1],
                    candidate_exact_owners=aligned_candidate_exact_owners,
                    candidate_statement_owners=aligned_candidate_statement_owners,
                )
            else:
                adapter._compare_parsed_publications(
                    reference_diagnostics,
                    reference_parsed.prefix,
                    tuple(field.rendered for field in reference_parsed.fields),
                    reference_owner[0],
                    reference_owner[1],
                    reference_owner[2],
                    reference_owner[3],
                    candidate_diagnostics,
                    candidate_parsed.prefix,
                    tuple(field.rendered for field in candidate_parsed.fields),
                    aligned.candidate_source_sizes,
                    aligned.candidate_source_owners,
                    reference_exact_owners=context.reference_exact_owners[index][1],
                    candidate_exact_owners=aligned_candidate_exact_owners,
                    candidate_statement_owners=aligned_candidate_statement_owners,
                )
        except (adapter.AdapterError, PublicationError, UnicodeError, KeyError) as error:
            comparison_outcomes[comparison_pair] = (type(error), str(error))
            mismatches.append(f"{key}\tadapter\t{type(error).__name__}\t{error}")
        else:
            comparison_outcomes[comparison_pair] = None
    return tuple(mismatches)


def _publication_comparison_worker(
    connection,
    context: _PublicationComparisonContext,
) -> None:
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    signal.signal(signal.SIGTERM, signal.SIG_DFL)
    try:
        connection.send((None, None, None))
        while True:
            index = connection.recv()
            if index is None:
                return
            try:
                result = _compare_publication_input(index, context)
            except BaseException as error:
                connection.send((index, None, error))
                return
            connection.send((index, result, None))
    finally:
        connection.close()


def _stop_publication_comparison_workers(
    workers: list[tuple[multiprocessing.Process, object, object]],
    *,
    terminate: bool,
) -> None:
    errors = []
    if terminate:
        for process, _parent_connection, _child_connection in workers:
            try:
                if process.is_alive():
                    process.terminate()
            except BaseException as error:
                errors.append(error)
    else:
        for _process, connection, _child_connection in workers:
            try:
                connection.send(None)
            except BaseException as error:
                errors.append(error)
        if errors:
            for process, _parent_connection, _child_connection in workers:
                try:
                    if process.is_alive():
                        process.terminate()
                except BaseException as error:
                    errors.append(error)
    for process, connection, child_connection in workers:
        try:
            process.join(timeout=2)
            if process.is_alive():
                process.kill()
                process.join(timeout=2)
            if process.is_alive():
                errors.append(PublicationError("comparison worker could not be reaped"))
            elif not terminate and process.exitcode != 0:
                errors.append(
                    PublicationError(f"comparison worker exited with status {process.exitcode}")
                )
        except BaseException as error:
            errors.append(error)
        try:
            connection.close()
        except BaseException as error:
            errors.append(error)
        try:
            child_connection.close()
        except BaseException as error:
            errors.append(error)
    if errors:
        raise PublicationError(f"comparison worker cleanup failed: {errors[0]}") from errors[0]


def _run_parallel_publication_comparisons(
    context: _PublicationComparisonContext,
    count: int,
    on_result,
) -> None:
    process_context = multiprocessing.get_context("fork")
    workers: list[tuple[multiprocessing.Process, object, object]] = []
    previous_handlers = {}
    try:
        if threading.current_thread() is threading.main_thread():
            for signum in (signal.SIGINT, signal.SIGTERM):
                previous_handlers[signum] = signal.getsignal(signum)

                def interrupt(_signum, _frame):
                    previous_handler = previous_handlers[_signum]
                    if previous_handler is signal.SIG_IGN:
                        return
                    if callable(previous_handler):
                        return previous_handler(_signum, _frame)
                    if _signum == signal.SIGTERM and previous_handler is signal.SIG_DFL:
                        raise _PublicationComparisonTermination(_signum)
                    raise KeyboardInterrupt("publication comparison interrupted")

                signal.signal(signum, interrupt)
        for _ in range(min(3, count)):
            parent_connection, child_connection = process_context.Pipe()
            process = process_context.Process(
                target=_publication_comparison_worker,
                args=(child_connection, context),
            )
            try:
                process.start()
            except BaseException:
                if process.pid is not None:
                    workers.append((process, parent_connection, child_connection))
                child_connection.close()
                if process.pid is None:
                    parent_connection.close()
                raise
            workers.append((process, parent_connection, child_connection))
            child_connection.close()
        for process, connection, _child_connection in workers:
            try:
                ready = connection.recv()
            except (EOFError, OSError) as error:
                raise PublicationError(
                    f"comparison worker exited before startup with status {process.exitcode}"
                ) from error
            if ready != (None, None, None):
                raise PublicationError("comparison worker returned an invalid startup message")
        sentinels = {process.sentinel: process for process, _connection, _child in workers}
        batch_start = 0
        while batch_start < count:
            batch_end = min(batch_start + len(workers), count)
            batch_indices = tuple(range(batch_start, batch_end))
            active = {}
            for (process, connection, _child), index in zip(workers, batch_indices):
                connection.send(index)
                active[connection] = (process, index)
            batch_results = {}
            expected_error_exits = set()
            next_output = batch_start
            while len(batch_results) + next_output - batch_start < len(batch_indices):
                ready = multiprocessing.connection.wait([*active, *sentinels])
                for connection in (item for item in ready if item in active):
                    process, expected_index = active.pop(connection)
                    try:
                        index, result, error = connection.recv()
                    except (EOFError, OSError) as receive_error:
                        raise PublicationError(
                            f"comparison worker exited without an outcome for input {expected_index}"
                        ) from receive_error
                    if index != expected_index:
                        raise PublicationError(
                            f"comparison worker returned input {index} for assigned input {expected_index}"
                        )
                    batch_results[index] = error if error is not None else result
                    if error is not None:
                        expected_error_exits.add(process)
                for sentinel in (item for item in ready if item in sentinels):
                    process = sentinels[sentinel]
                    if process in expected_error_exits:
                        expected_error_exits.remove(process)
                        sentinels.pop(sentinel)
                        continue
                    connection = next(
                        (
                            connection
                            for connection, (active_process, _index) in active.items()
                            if active_process is process
                        ),
                        None,
                    )
                    if connection is not None and connection.poll():
                        process, expected_index = active.pop(connection)
                        try:
                            index, result, error = connection.recv()
                        except (EOFError, OSError) as receive_error:
                            raise PublicationError(
                                f"comparison worker exited without an outcome for input {expected_index}"
                            ) from receive_error
                        if index != expected_index:
                            raise PublicationError(
                                f"comparison worker returned input {index} for assigned input {expected_index}"
                            )
                        batch_results[index] = error if error is not None else result
                        if error is not None:
                            expected_error_exits.add(process)
                            sentinels.pop(sentinel)
                            continue
                    raise PublicationError(
                        f"comparison worker exited unexpectedly with status {process.exitcode}"
                    )
                while next_output in batch_results:
                    result = batch_results.pop(next_output)
                    if isinstance(result, BaseException):
                        raise result
                    on_result(next_output, result)
                    next_output += 1
            batch_start = batch_end
    except BaseException as error:
        try:
            _stop_publication_comparison_workers(workers, terminate=True)
        except BaseException as cleanup_error:
            if hasattr(error, "add_note"):
                error.add_note(f"comparison worker cleanup also failed: {cleanup_error}")
        raise
    else:
        try:
            _stop_publication_comparison_workers(workers, terminate=False)
        except BaseException as error:
            try:
                _stop_publication_comparison_workers(workers, terminate=True)
            except BaseException as cleanup_error:
                if hasattr(error, "add_note"):
                    error.add_note(f"comparison worker cleanup also failed: {cleanup_error}")
            raise
    finally:
        for signum, handler in previous_handlers.items():
            signal.signal(signum, handler)


def compare_full_publications(
    runner,
    reference_receipt_path: pathlib.Path | None,
    candidate_receipt_directory: pathlib.Path,
    reference_raw: pathlib.Path,
    candidate_raw: pathlib.Path,
    expected_inputs: dict[int, object],
    mismatch_path: pathlib.Path,
    reference_capture_receipt: pathlib.Path,
    candidate_capture_receipt: pathlib.Path,
    candidate_root: pathlib.Path,
    reference_source_root: pathlib.Path,
    candidate_source_record: dict[str, str],
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    docker_environment: dict[str, str],
    *,
    reference_exact_owners: dict[int, tuple[object, dict[str, object]]],
    candidate_exact_owners: dict[int, tuple[object, dict[str, object]]],
    corrected_reference: bool = False,
    original_capture_receipt: pathlib.Path | None = None,
    original_reference_source_root: pathlib.Path | None = None,
    candidate_assignment_proofs: dict[int, object] | None = None,
    assignment_facts_module: object | None = None,
) -> tuple[int, int, tuple[str, ...] | None]:
    count = len(expected_inputs)
    expected_owner_indexes = set(range(count))
    if (
        set(reference_exact_owners) != expected_owner_indexes
        or set(candidate_exact_owners) != expected_owner_indexes
    ):
        raise PublicationError("exact-owner proof coverage differs from the comparison corpus")
    if candidate_assignment_proofs is not None and (
        set(candidate_assignment_proofs) != expected_owner_indexes
        or assignment_facts_module is None
    ):
        raise PublicationError("assignment facts proof does not cover the comparison corpus")
    input_manifest = runner.regular_file(input_manifest, "approved input manifest")
    runner.require_sha256(input_manifest, runner.INPUT_MANIFEST_SHA256, "approved input manifest")
    source_qualified = candidate_source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA
    if source_qualified:
        if reference_receipt_path is not None:
            raise PublicationError("source-qualified comparison does not accept a historical reference receipt")
        runner.validate_source_cohort_inputs(candidate_source_record)
        reference = None
    else:
        if input_root != runner.REFERENCE_INPUT_ROOT:
            raise PublicationError("frozen input root differs from the approved reference path")
        if reference_receipt_path is None:
            raise PublicationError("historical comparison requires its pinned reference receipt")
        reference = runner.validate_reference_receipt(
            reference_receipt_path, expected_inputs, runner.CORPUS_BLAKE3
        )
    reference_build_path = runner.regular_file(
        _capture_field(reference_capture_receipt, "build_receipt", runner),
        "reference adapter build receipt",
    )
    candidate_build_path = runner.regular_file(
        _capture_field(candidate_capture_receipt, "build_receipt", runner),
        "candidate adapter build receipt",
    )
    owner_helper_source = candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
    reference_role = "derived-reference" if corrected_reference else "reference"
    reference_build = _parse_adapter_build_receipt(
        reference_build_path, reference_role, runner, owner_helper_source
    )
    candidate_build = _parse_adapter_build_receipt(
        candidate_build_path, "candidate", runner, owner_helper_source
    )
    if source_qualified and (
        reference_build["schema"] != "2" or candidate_build["schema"] != "2"
    ):
        raise PublicationError("source-qualified comparison requires source-qualified adapter builds")
    expected_reference_root = runner.real_directory(
        reference_source_root,
        "derived reference source" if corrected_reference else "clean reference source clone",
    )
    if pathlib.Path(reference_build["source_root"]) != expected_reference_root:
        raise PublicationError("reference adapter build receipt names a different checkout")
    if corrected_reference:
        try:
            reference_correction.validate_source(
                expected_reference_root, reference_correction.PATCH_PATH, runner
            )
        except reference_correction.CorrectionError as error:
            raise PublicationError(str(error)) from error
    if pathlib.Path(candidate_build["source_root"]) != candidate_root:
        raise PublicationError("candidate adapter build receipt names a different checkout")
    if (candidate_build["source_commit"], candidate_build["source_tree"]) != (
        candidate_source_record["candidate_commit"], candidate_source_record["candidate_tree"]
    ):
        raise PublicationError("candidate adapter harness source differs from its source receipt")
    if reference_build["helper_sha256"] != candidate_build["helper_sha256"]:
        raise PublicationError("reference and candidate adapter helper contents differ")
    if reference_build["owner_helper_sha256"] != candidate_build["owner_helper_sha256"]:
        raise PublicationError("reference and candidate owner helper contents differ")
    reference_capture_record = _parse_capture_receipt(
        reference_capture_receipt,
        reference_role,
        reference_capture_receipt.parent / "receipts",
        reference_capture_receipt.parent,
        reference_capture_receipt.parent / "raw",
        reference_build,
        expected_inputs,
        input_root,
        input_manifest,
        runner,
    )
    candidate_capture_record = _parse_capture_receipt(
        candidate_capture_receipt,
        "candidate",
        candidate_receipt_directory,
        candidate_capture_receipt.parent,
        candidate_capture_receipt.parent / "raw",
        candidate_build,
        expected_inputs,
        input_root,
        input_manifest,
        runner,
    )
    if source_qualified and (
        reference_capture_record["schema"] != "2"
        or candidate_capture_record["schema"] != "2"
    ):
        raise PublicationError("source-qualified comparison requires source-qualified adapter captures")
    captured_reference = aggregate_candidate_receipts(
        runner, reference_capture_receipt.parent / "receipts", expected_inputs, reference
    )
    if not corrected_reference and reference is not None and captured_reference.outcomes != reference.outcomes:
        raise PublicationError("derived reference harness outcomes differ from the pinned reference receipt")
    original_build: dict[str, str] | None = None
    original_capture: runner.ParsedReceipt | None = None
    original_raw: pathlib.Path | None = None
    if corrected_reference:
        if original_capture_receipt is None:
            raise PublicationError("derived reference comparison requires its immutable original capture")
        original_capture_receipt = runner.regular_file(
            original_capture_receipt, "immutable original reference capture receipt"
        )
        original_build_path = runner.regular_file(
            _capture_field(original_capture_receipt, "build_receipt", runner),
            "immutable original reference build receipt",
        )
        original_build = _parse_adapter_build_receipt(
            original_build_path, "reference", runner, owner_helper_source
        )
        original_capture_record = _parse_capture_receipt(
            original_capture_receipt,
            "reference",
            original_capture_receipt.parent / "receipts",
            original_capture_receipt.parent,
            original_capture_receipt.parent / "raw",
            original_build,
            expected_inputs,
            input_root,
            input_manifest,
            runner,
        )
        if source_qualified and (
            original_build["schema"] != "2" or original_capture_record["schema"] != "2"
        ):
            raise PublicationError("source-qualified correction requires fresh original adapter receipts")
        expected_original_source = runner.real_directory(
            original_reference_source_root
            if original_reference_source_root is not None
            else reference_source_root.parent / "w9-adapter-ref-e240",
            "clean original reference source",
        )
        if pathlib.Path(original_capture_record["source_root"]) != expected_original_source:
            raise PublicationError("immutable original reference capture source differs")
        original_capture = aggregate_candidate_receipts(
            runner, pathlib.Path(original_capture_record["receipt_directory"]), expected_inputs, reference
        )
        if reference is not None and original_capture.outcomes != reference.outcomes:
            raise PublicationError("immutable original capture differs from the pinned reference outcomes")
        original_raw = runner.real_directory(
            original_capture_record["raw_root"], "immutable original reference raw outcomes"
        )
    candidate = aggregate_candidate_receipts(
        runner, candidate_receipt_directory, expected_inputs, reference
    )
    outcome_baseline = reference if reference is not None else captured_reference
    keys = set(outcome_baseline.outcomes)
    if keys != set(candidate.outcomes):
        raise PublicationError("reference and candidate receipt outcome keys differ")
    if outcome_baseline.inputs != candidate.inputs:
        raise PublicationError("reference and candidate receipt input path or digest differs")
    mismatches: list[str] = []
    expected_reference_outcomes = captured_reference if corrected_reference or source_qualified else reference
    runner_for_paths = _load_publication_runner(
        candidate_root / "dev/consolidation/w9-publication-run.py"
    )
    reference_root = runner_for_paths.open_absolute_nofollow(reference_raw, directory=True)
    candidate_root = runner_for_paths.open_absolute_nofollow(candidate_raw, directory=True)
    original_root = (
        runner_for_paths.open_absolute_nofollow(original_raw, directory=True)
        if original_raw is not None
        else -1
    )
    try:
        reference_tui_path = runner_for_paths.regular_file(
            expected_reference_root / "stdlib/tui.xsh", "pinned reference TUI source"
        )
        candidate_source_root = pathlib.Path(candidate_build["source_root"])
        candidate_tui_path = runner_for_paths.regular_file(
            candidate_source_root / "stdlib/tui.xsh", "candidate TUI source"
        )
        reference_tui_source = runner_for_paths.read_regular_bytes(reference_tui_path)
        candidate_tui_source = runner_for_paths.read_regular_bytes(candidate_tui_path)
        _mkdir_new_directory(mismatch_path.parent / "hashjobs", runner_for_paths)
        reference_raw_hashes = _hash_raw_outcomes(
            pathlib.Path(reference_build["binary"]), reference_raw, candidate_source_record,
            count, mismatch_path.parent, reference_role, 0, docker_environment, runner,
        )
        candidate_raw_hashes = _hash_raw_outcomes(
            pathlib.Path(candidate_build["binary"]), candidate_raw, candidate_source_record,
            count, mismatch_path.parent, "candidate", 1, docker_environment, runner,
        )
        first_delta = None
        if corrected_reference:
            if original_build is None or original_capture is None or original_root < 0:
                raise PublicationError("immutable original reference capture validation is incomplete")
            original_raw_hashes = _hash_raw_outcomes(
                pathlib.Path(original_build["binary"]), original_raw, candidate_source_record,
                count, mismatch_path.parent, "reference", 2, docker_environment, runner,
            )

            def original_rows():
                for index in range(count):
                    for profile in PROFILES:
                        key = (index, expected_inputs[index].relative_path, profile)
                        kind, diagnostics, publication, hashes = _record(original_root, key)
                        if not _raw_hash_pair_matches(original_raw_hashes, index, profile, hashes):
                            raise PublicationError(f"{key}: immutable original raw bytes hash differs")
                        if hashes != original_capture.outcomes[key]:
                            raise PublicationError(f"{key}: immutable original receipt hash differs")
                        yield key, (kind, diagnostics, publication)

            def derived_rows():
                for index in range(count):
                    for profile in PROFILES:
                        key = (index, expected_inputs[index].relative_path, profile)
                        kind, diagnostics, publication, hashes = _record(reference_root, key)
                        if not _raw_hash_pair_matches(reference_raw_hashes, index, profile, hashes):
                            raise PublicationError(f"{key}: derived reference raw bytes hash differs")
                        if hashes != captured_reference.outcomes[key]:
                            raise PublicationError(f"{key}: derived reference receipt hash differs")
                        yield key, (kind, diagnostics, publication)

            first_delta = reference_correction.first_difference(original_rows(), derived_rows())
        if len(keys) != count * len(PROFILES):
            raise PublicationError(f"receipt outcome count differs: {len(keys)}")

        comparison_context = _PublicationComparisonContext(
            reference_root,
            candidate_root,
            expected_inputs,
            expected_reference_outcomes,
            candidate.outcomes,
            reference_raw_hashes,
            candidate_raw_hashes,
            reference_exact_owners,
            candidate_exact_owners,
            reference_tui_source,
            candidate_tui_source,
            candidate_assignment_proofs,
            assignment_facts_module,
        )

        def accept_input_result(index: int, input_mismatches: tuple[str, ...]) -> None:
            mismatches.extend(input_mismatches)
            _report_comparison_progress(index + 1, count, len(keys), len(mismatches))

        if count <= 1:
            for index in range(count):
                accept_input_result(index, _compare_publication_input(index, comparison_context))
        else:
            _run_parallel_publication_comparisons(
                comparison_context,
                count,
                accept_input_result,
            )
    finally:
        os.close(reference_root)
        os.close(candidate_root)
        if original_root >= 0:
            os.close(original_root)
    payload = "\n".join(mismatches) + ("\n" if mismatches else "")
    _write_new_file(mismatch_path, payload.encode("utf-8"), runner_for_paths)
    if corrected_reference:
        return len(keys), len(mismatches), first_delta
    return len(keys), len(mismatches)


def compare_captured_publications(args: argparse.Namespace, runner) -> int:
    candidate_root = runner.real_directory(args.candidate_root, "candidate source")
    source_receipt = runner.regular_file(args.candidate_source_receipt, "candidate source and binary receipt")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise PublicationError("candidate source receipt names a different checkout")
    if runner.source_identity(candidate_root) != (
        source_record["candidate_commit"], source_record["candidate_tree"]
    ):
        raise PublicationError("candidate source identity differs from its source receipt")
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA:
        expected_inputs, _paths_by_name = runner.validate_source_cohort_inputs(source_record)
        reference = None
    else:
        expected_inputs, _paths_by_name, reference = runner.validate_all_inputs(source_record)
    if len(expected_inputs) != 1690:
        raise PublicationError(f"candidate source receipt input count differs: {len(expected_inputs)}")
    reference_source_root = _prepare_reference_source(
        candidate_root, pathlib.Path(args.reference_source_root), runner
    )
    corrected_reference = getattr(args, "derived_reference_correction_receipt", None) is not None
    original_capture_path = None
    correction_record = None
    derived_reference_root = None
    if corrected_reference:
        derived_reference_root = runner.real_directory(
            args.derived_reference_source_root, "derived reference source"
        )
        try:
            correction_source = reference_correction.validate_source(
                derived_reference_root, reference_correction.PATCH_PATH, runner
            )
        except reference_correction.CorrectionError as error:
            raise PublicationError(str(error)) from error
        correction_receipt_path = runner.regular_file(
            args.derived_reference_correction_receipt, "derived reference correction receipt"
        )
        correction_record = reference_correction.parse_receipt(
            correction_receipt_path, runner.read_regular_text(correction_receipt_path)
        )
        for field, expected in correction_source.items():
            if correction_record[field] != expected:
                raise PublicationError(f"derived reference receipt {field} differs from its source")
        original_capture_path = runner.regular_file(
            args.original_capture_receipt, "immutable original reference capture receipt"
        )
        original_exact_capture_path = runner.regular_file(
            args.reference_exact_capture_receipt, "immutable original exact-owner capture receipt"
        )
        if pathlib.Path(correction_record["original_capture_receipt"]) != original_capture_path:
            raise PublicationError("derived reference receipt names a different original capture")
        if pathlib.Path(correction_record["original_exact_capture_receipt"]) != original_exact_capture_path:
            raise PublicationError("derived reference receipt names a different original exact-owner capture")
        derived_build_path = runner.regular_file(
            correction_record["build_receipt"], "derived reference adapter build receipt"
        )
        derived_capture_path = runner.regular_file(
            correction_record["capture_receipt"], "derived reference adapter capture receipt"
        )
        derived_build = _parse_adapter_build_receipt(
            derived_build_path,
            "derived-reference",
            runner,
            candidate_root / "dev/consolidation/w9-publication-owner-helper.rs",
        )
        if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and derived_build["schema"] != "2":
            raise PublicationError("source-qualified correction requires a fresh derived adapter build")
        if pathlib.Path(derived_build["source_root"]) != derived_reference_root:
            raise PublicationError("derived reference build receipt names a different source root")
        derived_capture_record = _parse_capture_receipt(
            derived_capture_path,
            "derived-reference",
            pathlib.Path(_capture_field(derived_capture_path, "receipt_directory", runner)),
            pathlib.Path(_capture_field(derived_capture_path, "capture_root", runner)),
            pathlib.Path(_capture_field(derived_capture_path, "raw_root", runner)),
            derived_build,
            expected_inputs,
            pathlib.Path(source_record["input_root"]),
            pathlib.Path(source_record["input_manifest"]),
            runner,
        )
        if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and derived_capture_record["schema"] != "2":
            raise PublicationError("source-qualified correction requires a fresh derived adapter capture")
        reference_correction.validate_receipt_associations(
            correction_record,
            derived_build_path,
            derived_build,
            derived_capture_path,
            derived_capture_record,
            original_capture_path,
            original_exact_capture_path,
            runner,
        )
        reference_capture_path = derived_capture_path
    else:
        reference_capture_path = runner.regular_file(
            args.reference_capture_receipt, "reference adapter capture receipt"
        )
    candidate_capture_path = runner.regular_file(
        args.candidate_capture_receipt, "candidate adapter capture receipt"
    )
    reference_exact_build_path = runner.regular_file(
        args.reference_exact_build_receipt, "reference exact-owner build receipt"
    )
    candidate_exact_build_path = runner.regular_file(
        args.candidate_exact_build_receipt, "candidate exact-owner build receipt"
    )
    reference_exact_capture_path = runner.regular_file(
        args.reference_exact_capture_receipt, "reference exact-owner capture receipt"
    )
    candidate_exact_capture_path = runner.regular_file(
        args.candidate_exact_capture_receipt, "candidate exact-owner capture receipt"
    )
    input_root = pathlib.Path(source_record["input_root"])
    input_manifest = pathlib.Path(source_record["input_manifest"])
    reference_raw_root = runner.real_directory(
        _capture_field(reference_capture_path, "raw_root", runner),
        "reference adapter raw outcomes",
    )
    candidate_raw_root = runner.real_directory(
        _capture_field(candidate_capture_path, "raw_root", runner),
        "candidate adapter raw outcomes",
    )
    candidate_receipt_directory = runner.real_directory(
        _capture_field(candidate_capture_path, "receipt_directory", runner),
        "candidate adapter range receipts",
    )
    output = runner.require_output_path(args.output, candidate_root)
    _mkdir_new_directory(output, runner)
    runner.write_status(output, "incomplete", [])
    reference_exact_output = output / "exact-reference"
    candidate_exact_output = output / "exact-candidate"
    comparison_output = output / "comparison"
    try:
        exact_owner_runner = _load_exact_owner_runner(runner)
        assignment_facts_proofs = None
        assignment_facts_module = None
        assignment_facts_receipt = getattr(args, "assignment_facts_capture_receipt", None)

        def validate_exact_owner_proof(
            role: str,
            build_receipt: pathlib.Path,
            exact_capture_receipt: pathlib.Path,
            proof_output: pathlib.Path,
        ) -> dict[str, object]:
            try:
                proof = exact_owner_runner.validate_proof(
                    candidate_root,
                    source_receipt,
                    reference_source_root,
                    role,
                    build_receipt,
                    exact_capture_receipt,
                    proof_output,
                    runner,
                    dry_run=False,
                )
            except exact_owner_runner.ExactOwnerError as error:
                raise PublicationError(f"{role} exact-owner proof failed: {error}") from error
            return proof

        reference_exact_proof = validate_exact_owner_proof(
            "reference",
            reference_exact_build_path,
            reference_exact_capture_path,
            reference_exact_output,
        )
        _require_exact_capture_binding(
            reference_exact_proof,
            "reference",
            original_capture_path if corrected_reference else reference_capture_path,
            runner,
        )
        reference_exact_owners = _validated_exact_owners(
            reference_exact_proof, "reference", len(expected_inputs)
        )
        candidate_exact_proof = validate_exact_owner_proof(
            "candidate",
            candidate_exact_build_path,
            candidate_exact_capture_path,
            candidate_exact_output,
        )
        _require_exact_capture_binding(
            candidate_exact_proof, "candidate", candidate_capture_path, runner
        )
        candidate_exact_owners = _validated_exact_owners(
            candidate_exact_proof, "candidate", len(expected_inputs)
        )
        if assignment_facts_receipt is not None:
            try:
                assignment_facts_proofs, assignment_facts_module, _assignment_facts_record = (
                    exact_owner_runner.validate_assignment_facts_capture(
                        candidate_root,
                        source_receipt,
                        candidate_capture_path,
                        candidate_exact_build_path,
                        candidate_exact_capture_path,
                        assignment_facts_receipt,
                        candidate_exact_proof,
                        runner,
                    )
                )
            except exact_owner_runner.ExactOwnerError as error:
                raise PublicationError(f"candidate assignment facts proof failed: {error}") from error

        comparison_output = runner.require_output_path(
            str(comparison_output), candidate_root
        )
        completed: list[tuple[int, int]] = []
        docker_environment = runner.local_docker_environment()
        with runner.claim_run_resources(candidate_root, comparison_output, completed):
            runner.write_status(comparison_output, "incomplete", completed)
            mismatch_path = runner.new_work_file_path(
                comparison_output / "mismatches.tsv",
                candidate_root,
                "adapter mismatch report",
            )
            compare_result = compare_full_publications(
                runner,
                None if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA else pathlib.Path(source_record["reference_receipt"]),
                candidate_receipt_directory,
                reference_raw_root,
                candidate_raw_root,
                expected_inputs,
                mismatch_path,
                reference_capture_path,
                candidate_capture_path,
                candidate_root,
                derived_reference_root if corrected_reference else reference_source_root,
                source_record,
                input_root,
                input_manifest,
                docker_environment,
                reference_exact_owners=reference_exact_owners,
                candidate_exact_owners=candidate_exact_owners,
                corrected_reference=corrected_reference,
                original_capture_receipt=original_capture_path,
                original_reference_source_root=reference_source_root,
                candidate_assignment_proofs=assignment_facts_proofs,
                assignment_facts_module=assignment_facts_module,
            )
            if corrected_reference:
                outcomes, mismatches, first_delta = compare_result
            else:
                outcomes, mismatches = compare_result
                first_delta = None
            if corrected_reference:
                delta_path = comparison_output / "original-to-derived-reference.tsv"
                _write_new_file(
                    delta_path,
                    reference_correction.first_difference_bytes(first_delta),
                    runner,
                )
                comparison_receipt = comparison_output / "derived-reference-comparison.receipt"
                delta_fields = (
                    first_delta
                    if first_delta is not None
                    else ("none", "none", "none", "none", "none", "none")
                )
                first_difference_value = "\t".join(delta_fields)
                comparison_lines = (
                    "schema\t1\n"
                    "role\toriginal-to-derived-reference-comparison\n"
                    f"original_capture_receipt\t{original_capture_path}\n"
                    f"original_capture_receipt_sha256\t{runner.sha256_file(original_capture_path)}\n"
                    f"derived_capture_receipt\t{reference_capture_path}\n"
                    f"derived_capture_receipt_sha256\t{runner.sha256_file(reference_capture_path)}\n"
                    f"correction_receipt\t{correction_receipt_path}\n"
                    f"correction_receipt_sha256\t{runner.sha256_file(correction_receipt_path)}\n"
                    f"original_exact_capture_receipt\t{reference_exact_capture_path}\n"
                    f"original_exact_capture_receipt_sha256\t{runner.sha256_file(reference_exact_capture_path)}\n"
                    f"delta_file\t{delta_path}\n"
                    f"delta_file_sha256\t{runner.sha256_file(delta_path)}\n"
                    f"outcome_count\t{reference_correction.EXPECTED_OUTCOME_COUNT}\n"
                    f"first_difference\t{first_difference_value}\n"
                ).encode("utf-8")
                _write_new_file(comparison_receipt, comparison_lines, runner)
            proof_state = "verified" if assignment_facts_proofs is not None else "missing"
            summary = (
                f"corpus={len(expected_inputs)} outcomes={outcomes} mismatches={mismatches} "
                f"assignment_facts_proof={proof_state}"
            )
            result_status = (
                "complete"
                if assignment_facts_proofs is not None and mismatches == 0
                else "diagnostic-only"
                if assignment_facts_proofs is None
                else "mismatches"
            )
            runner.write_status(comparison_output, result_status, completed, summary)
        runner.write_status(output, result_status, [], summary)
    except BaseException as error:
        try:
            runner.write_status(output, "incomplete", [], f"{type(error).__name__}: {error}")
        except BaseException as status_error:
            print(f"could not record incomplete adapter comparison status: {status_error}", file=sys.stderr)
        raise
    print(
        f"{summary}\nreference_capture={reference_capture_path}\n"
        f"candidate_capture={candidate_capture_path}\n"
        f"reference_exact_proof={reference_exact_output}\n"
        f"candidate_exact_proof={candidate_exact_output}\n"
        f"mismatch_file={comparison_output / 'mismatches.tsv'}"
        f"\nassignment_facts_proof={'verified' if assignment_facts_proofs is not None else 'missing'}"
        + (
            f"\nassignment_facts_capture={assignment_facts_receipt}"
            if assignment_facts_receipt is not None
            else ""
        )
        + (
            f"\noriginal_to_derived_delta={comparison_output / 'original-to-derived-reference.tsv'}"
            f"\nderived_reference_comparison_receipt={comparison_output / 'derived-reference-comparison.receipt'}"
            if corrected_reference
            else ""
        )
    )
    return 0 if assignment_facts_proofs is not None and mismatches == 0 else 1


def _capture_field(path: pathlib.Path, field: str, runner) -> str:
    receipt = runner.regular_file(path, "adapter capture receipt")
    lines = runner.read_regular_text(receipt).splitlines()
    if not lines:
        raise PublicationError(f"{receipt}: incomplete adapter capture receipt")
    schema = lines[0].split("\t")
    if len(schema) != 2 or schema[0] != "schema":
        raise PublicationError(f"{receipt}: malformed adapter capture receipt schema row")
    if schema[1] == "2":
        fields = SOURCE_CAPTURE_FIELDS
    elif schema[1] == "1":
        fields = CAPTURE_FIELDS
    else:
        raise PublicationError(f"{receipt}: unsupported adapter capture receipt schema {schema[1]!r}")
    for line in lines[:len(fields)]:
        fields = line.split("\t")
        if len(fields) == 2 and fields[0] == field:
            return fields[1]
    raise PublicationError(f"{receipt}: missing {field}")


def main(argv: list[str] | None = None) -> int:
    runner = _load_publication_runner(ROOT / "dev/consolidation/w9-publication-run.py")
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("build-harnesses")
    build.add_argument("--candidate-root", type=pathlib.Path, required=True)
    build.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    build.add_argument("--reference-source-root", type=pathlib.Path, required=True)
    build.add_argument("--reference-build-receipt", type=pathlib.Path)
    build_derived = commands.add_parser("build-derived-reference")
    build_derived.add_argument("--candidate-root", type=pathlib.Path, required=True)
    build_derived.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    build_derived.add_argument("--derived-reference-source-root", type=pathlib.Path, required=True)
    capture_derived = commands.add_parser("capture-derived-reference")
    capture_derived.add_argument("--candidate-root", type=pathlib.Path, required=True)
    capture_derived.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    capture_derived.add_argument("--derived-reference-source-root", type=pathlib.Path, required=True)
    capture_derived.add_argument("--derived-reference-build-receipt", type=pathlib.Path, required=True)
    capture_derived.add_argument("--original-capture-receipt", type=pathlib.Path, required=True)
    capture_derived.add_argument("--original-exact-build-receipt", type=pathlib.Path, required=True)
    capture_derived.add_argument("--original-exact-capture-receipt", type=pathlib.Path, required=True)
    capture_derived.add_argument("--output", required=True)
    run = commands.add_parser(
        "run",
        help="capture legacy outcomes before exact-owner comparison",
        description=(
            "Capture legacy outcomes with --capture-only, then capture or reuse exact-owner "
            "proofs and call compare-captures."
        ),
    )
    run.add_argument("--candidate-root", type=pathlib.Path, required=True)
    run.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    run.add_argument("--reference-source-root", type=pathlib.Path, required=True)
    run.add_argument("--reference-build-receipt", type=pathlib.Path, required=True)
    run.add_argument("--candidate-build-receipt", type=pathlib.Path, required=True)
    run.add_argument("--reference-capture-receipt", type=pathlib.Path)
    run.add_argument(
        "--capture-only",
        action="store_true",
        help="write capture receipts for a later exact-owner comparison",
    )
    run.add_argument("--output", required=True)
    run.add_argument("--dry-run", action="store_true")
    compare = commands.add_parser("compare-captures")
    compare.add_argument("--candidate-root", type=pathlib.Path, required=True)
    compare.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    compare.add_argument("--reference-source-root", type=pathlib.Path, required=True)
    compare.add_argument("--reference-exact-build-receipt", type=pathlib.Path, required=True)
    compare.add_argument("--candidate-exact-build-receipt", type=pathlib.Path, required=True)
    compare.add_argument("--reference-capture-receipt", type=pathlib.Path, required=True)
    compare.add_argument("--candidate-capture-receipt", type=pathlib.Path, required=True)
    compare.add_argument("--reference-exact-capture-receipt", type=pathlib.Path, required=True)
    compare.add_argument("--candidate-exact-capture-receipt", type=pathlib.Path, required=True)
    compare.add_argument("--assignment-facts-capture-receipt", type=pathlib.Path)
    compare.add_argument("--output", required=True)
    compare_derived = commands.add_parser("compare-derived-captures")
    compare_derived.add_argument("--candidate-root", type=pathlib.Path, required=True)
    compare_derived.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    compare_derived.add_argument("--reference-source-root", type=pathlib.Path, required=True)
    compare_derived.add_argument("--derived-reference-source-root", type=pathlib.Path, required=True)
    compare_derived.add_argument("--derived-reference-correction-receipt", type=pathlib.Path, required=True)
    compare_derived.add_argument("--original-capture-receipt", type=pathlib.Path, required=True)
    compare_derived.add_argument("--reference-exact-build-receipt", type=pathlib.Path, required=True)
    compare_derived.add_argument("--candidate-exact-build-receipt", type=pathlib.Path, required=True)
    compare_derived.add_argument("--candidate-capture-receipt", type=pathlib.Path, required=True)
    compare_derived.add_argument("--reference-exact-capture-receipt", type=pathlib.Path, required=True)
    compare_derived.add_argument("--candidate-exact-capture-receipt", type=pathlib.Path, required=True)
    compare_derived.add_argument("--assignment-facts-capture-receipt", type=pathlib.Path)
    compare_derived.add_argument("--output", required=True)
    args = parser.parse_args(argv)
    try:
        if args.command in {"compare-captures", "compare-derived-captures"}:
            runner = _load_candidate_publication_runner(
                args.candidate_root,
                args.candidate_source_receipt,
                runner,
            )
        if args.command == "build-harnesses":
            build_adapter_harnesses(args, runner)
            return 0
        if args.command == "build-derived-reference":
            return build_derived_reference_harness(args, runner)
        if args.command == "capture-derived-reference":
            return capture_derived_reference(args, runner)
        if args.command in {"compare-captures", "compare-derived-captures"}:
            return compare_captured_publications(args, runner)
        return execute_adapter_publication(args, runner)
    except (
        _PublicationComparisonTermination,
    ) as termination:
        os.kill(os.getpid(), termination.signum)
        raise
    except (
        PublicationError,
        reference_correction.CorrectionError,
        runner.PublicationError,
        runner.CleanupFailure,
        OSError,
        subprocess.SubprocessError,
    ) as error:
        print(f"W9 publication adapter failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
