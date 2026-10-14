from __future__ import annotations

import argparse
import hashlib
import importlib.util
import os
import pathlib
import re
import stat
import subprocess
import sys
import tempfile
import time

import w9_publication_adapter as adapter


ROOT = pathlib.Path(__file__).resolve().parents[2]
APPROVED_HELPER = pathlib.Path(
    "/home/josh/d/laputa-systems/xsh-consolidation-lanes/module-record-driver/"
    ".work/consolidation/w9-repository-parity.rs"
)
OWNER_HELPER = ROOT / "dev/consolidation/w9-publication-owner-helper.rs"
APPROVED_HELPER_SHA256 = "2ff5aaa74f8ae3d0597a3c14796d8643211a5b57b8b95147ba87205540ca95b0"
OWNER_HELPER_SHA256 = "23fafe5b4140396242543bb4c59b0bcc1e0d4b59280f1f0e8dabf0cbd8431562"
REFERENCE_TREE = "a1ef04ac7b6f759c272e8f841acee3a3b3a6ff08"
RECORD_NAME = re.compile(r"record-(0|[1-9][0-9]*)-(default|reveal|execution|check-cli)\Z")
PROFILES = ("default", "reveal", "execution", "check-cli")
RAW_SCHEMA = "w9-publication-adapter\t1\n"
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


class PublicationError(ValueError):
    pass


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
    use std::os::unix::ffi::OsStrExt as _;

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


def _load_publication_runner(path: pathlib.Path):
    spec = importlib.util.spec_from_file_location("w9_publication_run", path)
    if spec is None or spec.loader is None:
        raise PublicationError(f"cannot load W9 publication runner {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


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
        if not line.startswith("w9rawhash"):
            continue
        fields = line.split("\t")
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
    wrapper = runner.regular_file(source_record["wrapper"], "pinned native test wrapper", executable=True)
    wrapper_root = runner.real_directory(source_record["wrapper_root"], "pinned wrapper root")
    runner.validate_wrapper_root(wrapper_root, pathlib.Path(source_record["candidate_root"]))
    runner.require_sha256(wrapper, runner.WRAPPER_SHA256, "pinned native test wrapper")
    if not runner.under(raw_root, pathlib.Path(source_record["candidate_root"]) / ".work"):
        raise PublicationError(f"raw adapter output is outside candidate .work/: {raw_root}")
    job_dir = capture_output / "hashjobs" / role
    _mkdir_new_directory(job_dir, runner)
    container = f"w9adapter-{os.getpid()}-hash-{role}"
    if runner.existing_container(container, docker_environment):
        raise PublicationError(f"adapter hash Docker container already exists: {container}")
    shim_dir = runner.docker_shim(
        job_dir, container, cpu, pathlib.Path(source_record["input_root"])
    )
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
    environment = dict(docker_environment)
    environment["PATH"] = f"{shim_dir}:/usr/bin:/bin"
    command = [
        "timeout",
        "900s",
        str(wrapper),
        str(wrapper_root),
        "run",
        "env",
        f"W9_ADAPTER_HASH_ROOT={raw_root}",
        f"W9_ADAPTER_HASH_INPUT_COUNT={count}",
        "taskset",
        "-c",
        str(cpu),
        str(binary),
        "--exact",
        "sema::w9_repository_parity_receipt",
        "--nocapture",
    ]
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
            job, active, command, str(wrapper_root), environment
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


def _write_adapter_build_receipt(path: pathlib.Path, record: dict[str, str], runner) -> None:
    if set(record) != set(ADAPTER_BUILD_FIELDS):
        raise PublicationError("adapter build receipt fields differ")
    _write_new_file(
        path,
        "".join(f"{field}\t{record[field]}\n" for field in ADAPTER_BUILD_FIELDS).encode("utf-8"),
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


def _parse_adapter_build_receipt(path: pathlib.Path, role: str, runner) -> dict[str, str]:
    receipt = runner.regular_file(path, f"{role} adapter build receipt")
    record, remainder = _parse_receipt_header(
        receipt, runner.read_regular_text(receipt).splitlines(), ADAPTER_BUILD_FIELDS, "adapter build"
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
    if pathlib.Path(record["owner_helper_source_path"]) != OWNER_HELPER:
        raise PublicationError(f"{receipt}: owner helper source path differs from its pin")
    if runner.sha256_file(owner_helper_source) != OWNER_HELPER_SHA256 or record["owner_helper_source_sha256"] != OWNER_HELPER_SHA256:
        raise PublicationError(f"{receipt}: owner helper source hash differs from its pin")
    source_tests = runner.regular_file(source_root / "tests/sema.rs", f"{role} semantic tests")
    source_tests_bytes = runner.read_regular_bytes(source_tests)
    expected_overlay = hashlib.sha256(_adapter_overlay(source_tests_bytes, runner)).hexdigest()
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
    if set(record) != set(CAPTURE_FIELDS):
        raise PublicationError("adapter capture receipt fields differ")
    lines = [f"{field}\t{record[field]}\n" for field in CAPTURE_FIELDS]
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


def _capture_plan(count: int, runner) -> tuple[list[tuple[int, int]], dict[str, list[tuple[int, int]]]]:
    ranges = runner.expected_ranges(count)
    workers = {worker: ranges[index::4] for index, worker in enumerate(("A", "B", "C", "D"))}
    flattened = sorted(interval for worker_ranges in workers.values() for interval in worker_ranges)
    if flattened != ranges or sum(end - start for start, end in flattened) != count:
        raise PublicationError("adapter capture plan does not cover each input exactly once")
    return ranges, workers


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
    cpu = {"A": 0, "B": 1, "C": 2, "D": 3}[worker]
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
    shim_dir = runner.docker_shim(job_dir, container, cpu, pathlib.Path(source_record["input_root"]))
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
    environment = dict(docker_environment)
    environment["PATH"] = f"{shim_dir}:/usr/bin:/bin"
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
) -> tuple[pathlib.Path, pathlib.Path, dict[tuple[int, int], pathlib.Path]]:
    role_root = output / role
    role_root.mkdir()
    raw_root = prepare_raw_output_directory(role_root / "raw", pathlib.Path(source_record["candidate_root"]), runner)
    receipt_directory = role_root / "receipts"
    receipt_directory.mkdir()
    (role_root / "jobs").mkdir()
    ranges, queues = _capture_plan(count, runner)
    active: list[dict[str, object]] = []
    completed: list[tuple[int, int]] = []
    try:
        for worker in queues:
            start, end = queues[worker].pop(0)
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
                if queues[worker]:
                    start, end = queues[worker].pop(0)
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
    candidate_root = runner.real_directory(args.candidate_root, "candidate source")
    source_receipt = runner.regular_file(args.candidate_source_receipt, "candidate source and binary receipt")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise PublicationError("candidate source receipt names a different checkout")
    expected_inputs, _paths_by_name, reference = runner.validate_all_inputs(source_record)
    if len(expected_inputs) != 1690:
        raise PublicationError(f"candidate source receipt input count differs: {len(expected_inputs)}")
    reference_source_root = _prepare_reference_source(
        candidate_root, pathlib.Path(args.reference_source_root), runner
    )
    reference_build_path = runner.regular_file(args.reference_build_receipt, "reference adapter build receipt")
    candidate_build_path = runner.regular_file(args.candidate_build_receipt, "candidate adapter build receipt")
    reference_build = _parse_adapter_build_receipt(reference_build_path, "reference", runner)
    candidate_build = _parse_adapter_build_receipt(candidate_build_path, "candidate", runner)
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
    ranges, workers = _capture_plan(len(expected_inputs), runner)
    output = runner.require_output_path(args.output, candidate_root)
    if args.dry_run:
        print(
            f"adapter preflight ok: reference={reference_build['source_commit']}:{reference_build['source_tree']} "
            f"candidate={candidate_build['source_commit']}:{candidate_build['source_tree']} "
            f"inputs={len(expected_inputs)} profiles={len(PROFILES)} outcomes={len(expected_inputs) * len(PROFILES)} "
            f"ranges={len(ranges)} workers={len(workers)} cpus=0,1,2,3 memory_per_worker=1GiB output={output}"
        )
        return 0
    wrapper = runner.regular_file(source_record["wrapper"], "pinned native test wrapper", executable=True)
    wrapper_root = runner.real_directory(source_record["wrapper_root"], "pinned wrapper root")
    runner.validate_wrapper_root(wrapper_root, candidate_root)
    runner.require_sha256(wrapper, runner.WRAPPER_SHA256, "pinned native test wrapper")
    docker_environment = runner.local_docker_environment()
    completed: list[tuple[int, int]] = []
    with runner.claim_run_resources(candidate_root, output, completed):
        runner.write_status(output, "incomplete", completed)
        captures: dict[str, tuple[pathlib.Path, pathlib.Path, pathlib.Path, dict[tuple[int, int], pathlib.Path]]] = {}
        for role, build_path, build in (
            ("reference", reference_build_path, reference_build),
            ("candidate", candidate_build_path, candidate_build),
        ):
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
        mismatch_path = runner.new_work_file_path(output / "mismatches.tsv", candidate_root, "adapter mismatch report")
        outcomes, mismatches = compare_full_publications(
            runner,
            pathlib.Path(source_record["reference_receipt"]),
            captures["candidate"][1],
            captures["reference"][0],
            captures["candidate"][0],
            expected_inputs,
            mismatch_path,
            captures["reference"][2],
            captures["candidate"][2],
            candidate_root,
            reference_source_root,
            source_record,
            pathlib.Path(source_record["input_root"]),
            pathlib.Path(source_record["input_manifest"]),
            docker_environment,
        )
        summary = f"corpus={len(expected_inputs)} outcomes={outcomes} mismatches={mismatches}"
        runner.write_status(output, "complete", completed, summary)
        print(f"{summary}\nreference_capture={captures['reference'][2]}\ncandidate_capture={captures['candidate'][2]}\nmismatch_file={mismatch_path}")
        return 0 if mismatches == 0 else 1


def build_adapter_harness(
    source_root: pathlib.Path,
    candidate_root: pathlib.Path,
    role: str,
    adapter_helper: pathlib.Path,
    helper_sha256: str,
    owner_helper_sha256: str,
    owner_helper_source_sha256: str,
    runner,
) -> tuple[pathlib.Path, pathlib.Path]:
    source_root = runner.real_directory(source_root, f"{role} source")
    commit, tree = runner.source_identity(source_root)
    if role == "reference" and commit != runner.REFERENCE_HEAD:
        raise PublicationError(f"reference source commit differs from the approved head: {commit}")
    helper_bytes = runner.read_regular_bytes(adapter_helper)
    if hashlib.sha256(helper_bytes).hexdigest() != helper_sha256:
        raise PublicationError("derived adapter helper changed before harness build")
    owner_bytes = runner.read_regular_bytes(adapter_helper.parent / "w9-publication-owner-helper.rs")
    if hashlib.sha256(owner_bytes).hexdigest() != owner_helper_sha256:
        raise PublicationError("owner helper changed before harness build")
    builder = runner.regular_file(runner.APPROVED_BUILD_RUNNER, "pinned adapter build runner", executable=True)
    contract = runner.w9_build_contract()
    builder_sha256 = runner.require_sha256(
        builder, contract["W9_CACHE_BUILD_RUNNER_SHA256"], "pinned adapter build runner"
    )
    if runner.PINNED_IMAGE not in runner.read_regular_text(builder):
        raise PublicationError("pinned adapter build runner does not name the approved image")
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
            "--profile", contract["W9_CACHE_BUILD_PROFILE"],
            "--target", contract["W9_CACHE_BUILD_TARGET"],
            "-p", "xsh", "--test", "integration", "--no-run",
            "--message-format=json-render-diagnostics",
        ]
        result = subprocess.run(
            command,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=docker_environment,
        )
        if result.returncode != 0:
            raise PublicationError(f"pinned {role} adapter harness build failed: {result.stderr.strip() or result.stdout.strip()}")
        scratch_binary = runner.cargo_integration_executable(result.stdout, scratch_root)
        _require_scratch_overlay_identity(scratch_root, commit, tree, role, runner)
        if hashlib.sha256(runner.read_regular_bytes(scratch_tests)).hexdigest() != overlay_sha256:
            raise PublicationError(f"{role} parity overlay changed during adapter build")
        if hashlib.sha256(runner.read_regular_bytes(scratch_root / ".work/consolidation/w9-publication-adapter.rs")).hexdigest() != helper_sha256:
            raise PublicationError(f"{role} adapter helper changed during build")
        if hashlib.sha256(runner.read_regular_bytes(scratch_root / ".work/consolidation/w9-publication-owner-helper.rs")).hexdigest() != owner_helper_sha256:
            raise PublicationError(f"{role} owner helper changed during build")
        runner.run_harness_test_listing(builder, candidate_root, scratch_binary, docker_environment)
        binary = runner.copy_executable(scratch_binary, binary_path)
    binary_sha256 = runner.sha256_file(binary)
    record = {
        "schema": "1",
        "role": role,
        "source_root": str(source_root),
        "source_commit": commit,
        "source_tree": tree,
        "binary": str(binary),
        "binary_sha256": binary_sha256,
        "builder_path": str(builder),
        "builder_sha256": builder_sha256,
        "approved_helper_path": str(runner.APPROVED_HELPER_SOURCE),
        "approved_helper_sha256": APPROVED_HELPER_SHA256,
        "helper_path": str(adapter_helper),
        "helper_sha256": helper_sha256,
        "owner_helper_path": str(adapter_helper.parent / "w9-publication-owner-helper.rs"),
        "owner_helper_sha256": owner_helper_sha256,
        "owner_helper_source_path": str(OWNER_HELPER),
        "owner_helper_source_sha256": owner_helper_source_sha256,
        "overlay_sha256": overlay_sha256,
        "builder_image": contract["W9_CACHE_BUILD_IMAGE"],
        "build_target": contract["W9_CACHE_BUILD_TARGET"],
        "build_profile": contract["W9_CACHE_BUILD_PROFILE"],
    }
    _write_adapter_build_receipt(receipt_path, record, runner)
    return binary, receipt_path


def build_adapter_harnesses(args: argparse.Namespace, runner) -> None:
    candidate_root = runner.real_directory(args.candidate_root, "candidate source")
    source_receipt = runner.regular_file(args.candidate_source_receipt, "candidate source receipt")
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise PublicationError("candidate source receipt names a different checkout")
    expected_inputs, _paths_by_name, _reference = runner.validate_all_inputs(source_record)
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
    derived_directory = _new_derived_helper_directory(candidate_root, runner)
    adapter_helper, helper_sha256, owner_helper_sha256, owner_helper_source_sha256 = derive_helper(
        output_directory=derived_directory,
        output_root=candidate_root,
    )
    reference_binary, reference_receipt = build_adapter_harness(
        reference_root,
        candidate_root,
        "reference",
        adapter_helper,
        helper_sha256,
        owner_helper_sha256,
        owner_helper_source_sha256,
        runner,
    )
    candidate_binary, candidate_receipt = build_adapter_harness(
        candidate_root,
        candidate_root,
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


def aggregate_candidate_receipts(
    runner,
    candidate_receipt_directory: pathlib.Path,
    expected_inputs: dict[int, object],
    reference,
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
            if value != reference.inputs[index]:
                raise PublicationError(f"candidate receipt input path or digest differs at input {index}")
            all_inputs[index] = value
        for key, value in receipt.outcomes.items():
            if key in all_outcomes:
                raise PublicationError(f"duplicate candidate outcome {key}")
            all_outcomes[key] = value
    if all_inputs != reference.inputs:
        raise PublicationError("candidate input receipt coverage differs from the reference")
    if set(all_outcomes) != set(reference.outcomes):
        raise PublicationError("candidate receipt coverage differs from the 1690 by 4 reference")
    return runner.ParsedReceipt(reference.header, all_inputs, all_outcomes, reference.terminals)


def compare_full_publications(
    runner,
    reference_receipt_path: pathlib.Path,
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
) -> tuple[int, int]:
    count = len(expected_inputs)
    input_manifest = runner.regular_file(input_manifest, "approved input manifest")
    runner.require_sha256(input_manifest, runner.INPUT_MANIFEST_SHA256, "approved input manifest")
    if input_root != runner.REFERENCE_INPUT_ROOT:
        raise PublicationError("frozen input root differs from the approved reference path")
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
    reference_build = _parse_adapter_build_receipt(reference_build_path, "reference", runner)
    candidate_build = _parse_adapter_build_receipt(candidate_build_path, "candidate", runner)
    expected_reference_root = runner.real_directory(
        reference_source_root, "clean reference source clone"
    )
    if pathlib.Path(reference_build["source_root"]) != expected_reference_root:
        raise PublicationError("reference adapter build receipt names a different checkout")
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
    _parse_capture_receipt(
        reference_capture_receipt,
        "reference",
        reference_capture_receipt.parent / "receipts",
        reference_capture_receipt.parent,
        reference_capture_receipt.parent / "raw",
        reference_build,
        expected_inputs,
        input_root,
        input_manifest,
        runner,
    )
    _parse_capture_receipt(
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
    captured_reference = aggregate_candidate_receipts(
        runner, reference_capture_receipt.parent / "receipts", expected_inputs, reference
    )
    if captured_reference.outcomes != reference.outcomes:
        raise PublicationError("derived reference harness outcomes differ from the pinned reference receipt")
    candidate = aggregate_candidate_receipts(
        runner, candidate_receipt_directory, expected_inputs, reference
    )
    keys = set(reference.outcomes)
    if keys != set(candidate.outcomes):
        raise PublicationError("reference and candidate receipt outcome keys differ")
    if reference.inputs != candidate.inputs:
        raise PublicationError("reference and candidate receipt input path or digest differs")
    mismatches: list[str] = []
    runner_for_paths = _load_publication_runner(ROOT / "dev/consolidation/w9-publication-run.py")
    reference_root = runner_for_paths.open_absolute_nofollow(reference_raw, directory=True)
    candidate_root = runner_for_paths.open_absolute_nofollow(candidate_raw, directory=True)
    try:
        _mkdir_new_directory(mismatch_path.parent / "hashjobs", runner_for_paths)
        reference_raw_hashes = _hash_raw_outcomes(
            pathlib.Path(reference_build["binary"]), reference_raw, candidate_source_record,
            count, mismatch_path.parent, "reference", 0, docker_environment, runner,
        )
        candidate_raw_hashes = _hash_raw_outcomes(
            pathlib.Path(candidate_build["binary"]), candidate_raw, candidate_source_record,
            count, mismatch_path.parent, "candidate", 1, docker_environment, runner,
        )
        if len(keys) != count * len(PROFILES):
            raise PublicationError(f"receipt outcome count differs: {len(keys)}")
        for index in range(count):
            owner_error = False
            try:
                reference_owner = adapter.parse_owner_index(
                    _read_regular_at(reference_root, f"owners-{index}.tsv")
                )
                candidate_owner = adapter.parse_owner_index(
                    _read_regular_at(candidate_root, f"owners-{index}.tsv")
                )
                if reference_owner[:2] != candidate_owner[:2]:
                    raise PublicationError(f"owner source/span AST differs at input {index}")
            except (adapter.AdapterError, PublicationError, UnicodeError) as error:
                owner_error = True
                mismatches.append(f"{index}\towner-index\t{type(error).__name__}\t{error}")
            for profile in PROFILES:
                key = (index, expected_inputs[index].relative_path, profile)
                try:
                    reference_kind, reference_diagnostics, reference_publication, reference_hashes = _record(reference_root, key)
                    candidate_kind, candidate_diagnostics, candidate_publication, candidate_hashes = _record(candidate_root, key)
                    reference_diagnostic_hash, reference_publication_hash = reference.outcomes[key]
                    candidate_diagnostic_hash, candidate_publication_hash = candidate.outcomes[key]
                    if not _raw_hash_pair_matches(reference_raw_hashes, index, profile, reference_hashes):
                        mismatches.append(f"{key}\treference-raw-bytes-hash")
                    if not _raw_hash_pair_matches(candidate_raw_hashes, index, profile, candidate_hashes):
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
                        continue
                    if reference_kind == "load-error":
                        if reference_publication or candidate_publication:
                            mismatches.append(f"{key}\tload-error-has-publication")
                        continue
                    if owner_error:
                        continue
                    adapter.compare_publications(
                        reference_diagnostics,
                        reference_publication,
                        reference_owner[0],
                        reference_owner[1],
                        reference_owner[2],
                        reference_owner[3],
                        candidate_diagnostics,
                        candidate_publication,
                        candidate_owner[0],
                        candidate_owner[1],
                    )
                except (adapter.AdapterError, PublicationError, UnicodeError, KeyError) as error:
                    mismatches.append(f"{key}\tadapter\t{type(error).__name__}\t{error}")
    finally:
        os.close(reference_root)
        os.close(candidate_root)
    payload = "\n".join(mismatches) + ("\n" if mismatches else "")
    _write_new_file(mismatch_path, payload.encode("utf-8"), runner_for_paths)
    return len(keys), len(mismatches)


def _capture_field(path: pathlib.Path, field: str, runner) -> str:
    receipt = runner.regular_file(path, "adapter capture receipt")
    for line in runner.read_regular_text(receipt).splitlines()[:len(CAPTURE_FIELDS)]:
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
    run = commands.add_parser("run")
    run.add_argument("--candidate-root", type=pathlib.Path, required=True)
    run.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    run.add_argument("--reference-source-root", type=pathlib.Path, required=True)
    run.add_argument("--reference-build-receipt", type=pathlib.Path, required=True)
    run.add_argument("--candidate-build-receipt", type=pathlib.Path, required=True)
    run.add_argument("--output", required=True)
    run.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.command == "build-harnesses":
            build_adapter_harnesses(args, runner)
            return 0
        return execute_adapter_publication(args, runner)
    except (PublicationError, runner.PublicationError, runner.CleanupFailure, OSError, subprocess.SubprocessError) as error:
        print(f"W9 publication adapter failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
