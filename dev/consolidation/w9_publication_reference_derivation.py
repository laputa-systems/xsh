from __future__ import annotations

import hashlib
import pathlib
import re
import subprocess
import tempfile

import w9_publication_reference_correction as retry_context


CHECKOUT_ROOT = pathlib.Path("/home/josh/d/laputa-systems/xsh")
SOURCE_ROOT = CHECKOUT_ROOT / ".work/wt/w9-derived-api-imports"
SOURCE_PARENT = "1fd9e5f2f366833b786e57d1fc54bcd9a238731e"
SOURCE_COMMIT = "74f1551ee9e3066b653ae190779e16d511c90293"
SOURCE_TREE = "3162d2f33076a8c9261d0ffb7a89a9952662f03f"
SOURCE_PATCH_SHA256 = "a9e4fcef54f5c68cf18fcde821ee07c83d2085f1eb4a415c47b5e219d2152bea"
API_IMPORT_FIX_COMMIT = "5366d950ad644fec8652b2a409148c75261a1358"
API_IMPORT_FIX_TREE = "185404824d6d5bdcb3be3c13153a6021ae18d3b9"
API_IMPORT_FIX_PARENT = "a6fab91e13c440022c48e5584dbc135d765b1092"
API_IMPORT_FIX_PATH = "src/sema/check/scopecheck.rs"
API_IMPORT_FIX_PATCH_SHA256 = "d0a66cb4db325b2f6b57d5c48e64519010e8a067db0da63a7d4235c3c301db73"
API_IMPORT_FIX_PATHS = (
    "src/sema/check/scopecheck.rs",
    "tests/xsh/checker-modules.xsh",
)
SOURCE_PATCH_PATHS = (API_IMPORT_FIX_PATH,)
PROFILES = retry_context.PROFILES
EXPECTED_RANGE_COUNT = retry_context.EXPECTED_RANGE_COUNT
EXPECTED_OUTCOME_COUNT = retry_context.EXPECTED_OUTCOME_COUNT
RECEIPT_FIELDS = (
    "schema",
    "role",
    "retry_context_base_commit",
    "retry_context_base_tree",
    "retry_context_patch_path",
    "retry_context_patch_sha256",
    "retry_context_source_root",
    "retry_context_source_parent",
    "retry_context_source_commit",
    "retry_context_source_tree",
    "api_import_fix_commit",
    "api_import_fix_tree",
    "api_import_fix_parent",
    "api_import_fix_path",
    "api_import_fix_patch_sha256",
    "source_root",
    "source_parent",
    "source_commit",
    "source_tree",
    "source_patch_sha256",
    "source_state",
    "build_receipt",
    "build_receipt_sha256",
    "binary_sha256",
    "helper_sha256",
    "owner_helper_sha256",
    "capture_receipt",
    "capture_receipt_sha256",
    "original_capture_receipt",
    "original_capture_receipt_sha256",
    "original_exact_capture_receipt",
    "original_exact_capture_receipt_sha256",
    "range_count",
    "outcome_count",
    "profiles",
)
HEX256 = re.compile(r"[0-9a-f]{64}\Z")
HEX40 = re.compile(r"[0-9a-f]{40}\Z")


class DerivationError(ValueError):
    pass


def _committed_patch_bytes(source_root: pathlib.Path, *diff_args: str) -> bytes:
    try:
        result = subprocess.run(
            ["git", "-C", str(source_root), "diff", "--binary", *diff_args],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise DerivationError(f"cannot read committed derivation patch: {error}") from error
    return result.stdout


def _committed_file_bytes(source_root: pathlib.Path, commit: str, path: str) -> bytes:
    try:
        result = subprocess.run(
            ["git", "-C", str(source_root), "show", f"{commit}:{path}"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise DerivationError(f"cannot read committed source file {path}: {error}") from error
    return result.stdout


def _approved_api_import_patch(runner) -> bytes:
    if runner.git_output(CHECKOUT_ROOT, "rev-parse", f"{API_IMPORT_FIX_COMMIT}^") != API_IMPORT_FIX_PARENT:
        raise DerivationError("approved API import fix parent differs from its pin")
    if runner.git_output(CHECKOUT_ROOT, "rev-parse", f"{API_IMPORT_FIX_COMMIT}^{{tree}}") != API_IMPORT_FIX_TREE:
        raise DerivationError("approved API import fix tree differs from its pin")
    parents = runner.git_output(
        CHECKOUT_ROOT, "rev-list", "--parents", "-n", "1", API_IMPORT_FIX_COMMIT
    ).split()
    if parents != [API_IMPORT_FIX_COMMIT, API_IMPORT_FIX_PARENT]:
        raise DerivationError("approved API import fix parent list differs from its pin")
    changed_paths = tuple(
        runner.git_output(
            CHECKOUT_ROOT, "diff", "--name-only", API_IMPORT_FIX_PARENT, API_IMPORT_FIX_COMMIT
        ).splitlines()
    )
    if changed_paths != API_IMPORT_FIX_PATHS:
        raise DerivationError("approved API import fix changed paths differ from its pin")
    source_paths = tuple(
        runner.git_output(
            CHECKOUT_ROOT,
            "diff",
            "--name-only",
            API_IMPORT_FIX_PARENT,
            API_IMPORT_FIX_COMMIT,
            "--",
            API_IMPORT_FIX_PATH,
        ).splitlines()
    )
    if source_paths != (API_IMPORT_FIX_PATH,):
        raise DerivationError("approved API import source patch paths differ from its pin")
    patch_bytes = _committed_patch_bytes(
        CHECKOUT_ROOT,
        API_IMPORT_FIX_PARENT,
        API_IMPORT_FIX_COMMIT,
        "--",
        API_IMPORT_FIX_PATH,
    )
    if hashlib.sha256(patch_bytes).hexdigest() != API_IMPORT_FIX_PATCH_SHA256:
        raise DerivationError("approved API import source patch SHA-256 differs from its pin")
    return patch_bytes


def _apply_api_import_patch(base_file: bytes, patch_bytes: bytes) -> bytes:
    scratch_parent = CHECKOUT_ROOT / ".work/tmp"
    try:
        scratch_parent.mkdir(parents=True, exist_ok=True)
        if scratch_parent.is_symlink() or not scratch_parent.is_dir():
            raise DerivationError("API import patch scratch parent is not a regular directory")
        if scratch_parent.resolve(strict=True) != scratch_parent.absolute():
            raise DerivationError("API import patch scratch parent is not canonical")
        with tempfile.TemporaryDirectory(prefix="w9-api-import-patch-", dir=scratch_parent) as scratch:
            scratch_root = pathlib.Path(scratch)
            source_file = scratch_root / API_IMPORT_FIX_PATH
            source_file.parent.mkdir(parents=True)
            source_file.write_bytes(base_file)
            initialized = subprocess.run(
                ["git", "init", "--quiet", str(scratch_root)],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            if initialized.returncode != 0:
                detail = initialized.stderr.decode("utf-8", errors="replace").strip()
                raise DerivationError(f"cannot initialize API import patch scratch source: {detail}")
            check = subprocess.run(
                ["git", "apply", "--check", "-"],
                cwd=scratch_root,
                check=False,
                input=patch_bytes,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            if check.returncode != 0:
                detail = check.stderr.decode("utf-8", errors="replace").strip()
                raise DerivationError(
                    f"approved API import patch does not apply to retry-context source: {detail}"
                )
            applied = subprocess.run(
                ["git", "apply", "-"],
                cwd=scratch_root,
                check=False,
                input=patch_bytes,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            if applied.returncode != 0:
                detail = applied.stderr.decode("utf-8", errors="replace").strip()
                raise DerivationError(f"cannot apply approved API import patch: {detail}")
            scratch_files = tuple(
                path.relative_to(scratch_root).as_posix()
                for path in scratch_root.rglob("*")
                if path.is_file() and ".git" not in path.relative_to(scratch_root).parts
            )
            if scratch_files != (API_IMPORT_FIX_PATH,):
                raise DerivationError("approved API import patch produced unexpected files")
            return source_file.read_bytes()
    except DerivationError:
        raise
    except OSError as error:
        raise DerivationError(f"cannot prepare API import patch scratch source: {error}") from error


def validate_source(source_root: pathlib.Path, runner) -> dict[str, str]:
    source_root = runner.real_directory(source_root, "combined derived reference source")
    if source_root != SOURCE_ROOT:
        raise DerivationError(f"combined derived reference source must be {SOURCE_ROOT}")
    if runner.git_output(source_root, "rev-parse", "--show-toplevel") != str(source_root):
        raise DerivationError("combined derived reference source is not its Git worktree root")
    try:
        context = retry_context.validate_source(
            retry_context.SOURCE_ROOT, retry_context.PATCH_PATH, runner
        )
    except retry_context.CorrectionError as error:
        raise DerivationError(f"retry-context correction validation failed: {error}") from error
    if context["base_commit"] != retry_context.BASE_COMMIT or context["base_tree"] != retry_context.BASE_TREE:
        raise DerivationError("retry-context base identity differs from its pin")
    if runner.git_output(source_root, "rev-parse", f"{retry_context.SOURCE_COMMIT}^{{tree}}") != retry_context.SOURCE_TREE:
        raise DerivationError("retry-context source tree differs from its pin")
    if runner.git_output(source_root, "rev-parse", "HEAD^") != SOURCE_PARENT:
        raise DerivationError("combined derived reference parent differs from its pin")
    parents = runner.git_output(source_root, "rev-list", "--parents", "-n", "1", "HEAD").split()
    if parents != [SOURCE_COMMIT, SOURCE_PARENT]:
        raise DerivationError("combined derived reference parent list differs from its pin")
    runner.git_output(source_root, "merge-base", "--is-ancestor", retry_context.BASE_COMMIT, SOURCE_COMMIT)
    if runner.git_output(source_root, "status", "--porcelain=v1", "--untracked-files=all"):
        raise DerivationError("combined derived reference source worktree is not clean")
    if runner.source_identity(source_root) != (SOURCE_COMMIT, SOURCE_TREE):
        raise DerivationError("combined derived reference source commit or tree differs from its pin")
    changed_paths = tuple(
        runner.git_output(source_root, "diff", "--name-only", SOURCE_PARENT, SOURCE_COMMIT).splitlines()
    )
    if changed_paths != SOURCE_PATCH_PATHS:
        raise DerivationError("combined derived reference changed paths differ from its pin")
    derived_patch = _committed_patch_bytes(source_root, SOURCE_PARENT, SOURCE_COMMIT)
    if hashlib.sha256(derived_patch).hexdigest() != SOURCE_PATCH_SHA256:
        raise DerivationError("combined derived reference source patch SHA-256 differs from its pin")
    approved_patch = _approved_api_import_patch(runner)
    base_file = _committed_file_bytes(source_root, SOURCE_PARENT, API_IMPORT_FIX_PATH)
    expected_file = _apply_api_import_patch(base_file, approved_patch)
    actual_file = _committed_file_bytes(source_root, SOURCE_COMMIT, API_IMPORT_FIX_PATH)
    if actual_file != expected_file:
        raise DerivationError("combined derived reference file is not the exact retry-context plus API import patch")
    return {
        "retry_context_base_commit": retry_context.BASE_COMMIT,
        "retry_context_base_tree": retry_context.BASE_TREE,
        "retry_context_patch_path": context["patch_path"],
        "retry_context_patch_sha256": context["patch_sha256"],
        "retry_context_source_root": context["source_root"],
        "retry_context_source_parent": context["source_parent"],
        "retry_context_source_commit": context["source_commit"],
        "retry_context_source_tree": context["source_tree"],
        "api_import_fix_commit": API_IMPORT_FIX_COMMIT,
        "api_import_fix_tree": API_IMPORT_FIX_TREE,
        "api_import_fix_parent": API_IMPORT_FIX_PARENT,
        "api_import_fix_path": API_IMPORT_FIX_PATH,
        "api_import_fix_patch_sha256": API_IMPORT_FIX_PATCH_SHA256,
        "source_root": str(source_root),
        "source_parent": SOURCE_PARENT,
        "source_commit": SOURCE_COMMIT,
        "source_tree": SOURCE_TREE,
        "source_patch_sha256": SOURCE_PATCH_SHA256,
        "source_state": "clean",
    }


def receipt_bytes(record: dict[str, str]) -> bytes:
    validate_receipt_record(record)
    return "".join(f"{field}\t{record[field]}\n" for field in RECEIPT_FIELDS).encode("utf-8")


def parse_receipt(path: pathlib.Path, contents: str) -> dict[str, str]:
    lines = contents.splitlines()
    if len(lines) != len(RECEIPT_FIELDS):
        raise DerivationError(f"{path}: combined derivation receipt field count differs")
    record: dict[str, str] = {}
    for row_number, (line, expected) in enumerate(zip(lines, RECEIPT_FIELDS), 1):
        fields = line.split("\t")
        if len(fields) != 2 or fields[0] != expected or not fields[1]:
            raise DerivationError(f"{path}: malformed combined derivation receipt row {row_number}")
        record[fields[0]] = fields[1]
    validate_receipt_record(record)
    return record


def validate_receipt_record(record: dict[str, str]) -> None:
    if set(record) != set(RECEIPT_FIELDS):
        raise DerivationError("combined derivation receipt fields differ")
    if record["schema"] != "3" or record["role"] != "combined-derived-reference":
        raise DerivationError("combined derivation receipt schema or role differs")
    expected_source = {
        "retry_context_base_commit": retry_context.BASE_COMMIT,
        "retry_context_base_tree": retry_context.BASE_TREE,
        "retry_context_patch_path": str(retry_context.PATCH_PATH),
        "retry_context_patch_sha256": retry_context.PATCH_SHA256,
        "retry_context_source_root": str(retry_context.SOURCE_ROOT),
        "retry_context_source_parent": retry_context.SOURCE_PARENT,
        "retry_context_source_commit": retry_context.SOURCE_COMMIT,
        "retry_context_source_tree": retry_context.SOURCE_TREE,
        "api_import_fix_commit": API_IMPORT_FIX_COMMIT,
        "api_import_fix_tree": API_IMPORT_FIX_TREE,
        "api_import_fix_parent": API_IMPORT_FIX_PARENT,
        "api_import_fix_path": API_IMPORT_FIX_PATH,
        "api_import_fix_patch_sha256": API_IMPORT_FIX_PATCH_SHA256,
        "source_root": str(SOURCE_ROOT),
        "source_parent": SOURCE_PARENT,
        "source_commit": SOURCE_COMMIT,
        "source_tree": SOURCE_TREE,
        "source_patch_sha256": SOURCE_PATCH_SHA256,
        "source_state": "clean",
        "range_count": str(EXPECTED_RANGE_COUNT),
        "outcome_count": str(EXPECTED_OUTCOME_COUNT),
        "profiles": ",".join(PROFILES),
    }
    for field, expected in expected_source.items():
        if record[field] != expected:
            raise DerivationError(f"combined derivation receipt {field} differs")
    for field in (
        "retry_context_patch_sha256",
        "api_import_fix_patch_sha256",
        "source_patch_sha256",
        "build_receipt_sha256",
        "binary_sha256",
        "helper_sha256",
        "owner_helper_sha256",
        "capture_receipt_sha256",
        "original_capture_receipt_sha256",
        "original_exact_capture_receipt_sha256",
    ):
        if HEX256.fullmatch(record[field]) is None:
            raise DerivationError(f"combined derivation receipt {field} is malformed")
    for field in (
        "retry_context_base_commit",
        "retry_context_base_tree",
        "retry_context_source_commit",
        "retry_context_source_parent",
        "retry_context_source_tree",
        "api_import_fix_commit",
        "api_import_fix_tree",
        "api_import_fix_parent",
        "source_parent",
        "source_commit",
        "source_tree",
    ):
        if HEX40.fullmatch(record[field]) is None:
            raise DerivationError(f"combined derivation receipt {field} is malformed")


def create_receipt_record(
    source: dict[str, str],
    build_receipt: pathlib.Path,
    build: dict[str, str],
    capture_receipt: pathlib.Path,
    original_capture_receipt: pathlib.Path,
    original_exact_capture_receipt: pathlib.Path,
    runner,
) -> dict[str, str]:
    record = {
        "schema": "3",
        "role": "combined-derived-reference",
        **source,
        "build_receipt": str(build_receipt),
        "build_receipt_sha256": runner.sha256_file(build_receipt),
        "binary_sha256": build["binary_sha256"],
        "helper_sha256": build["helper_sha256"],
        "owner_helper_sha256": build["owner_helper_sha256"],
        "capture_receipt": str(capture_receipt),
        "capture_receipt_sha256": runner.sha256_file(capture_receipt),
        "original_capture_receipt": str(original_capture_receipt),
        "original_capture_receipt_sha256": runner.sha256_file(original_capture_receipt),
        "original_exact_capture_receipt": str(original_exact_capture_receipt),
        "original_exact_capture_receipt_sha256": runner.sha256_file(original_exact_capture_receipt),
        "range_count": str(EXPECTED_RANGE_COUNT),
        "outcome_count": str(EXPECTED_OUTCOME_COUNT),
        "profiles": ",".join(PROFILES),
    }
    validate_receipt_record(record)
    return record


def validate_receipt_associations(
    record: dict[str, str],
    build_receipt: pathlib.Path,
    build: dict[str, str],
    capture_receipt: pathlib.Path,
    capture: dict[str, str],
    original_capture_receipt: pathlib.Path,
    original_exact_capture_receipt: pathlib.Path,
    runner,
) -> None:
    validate_receipt_record(record)
    paths_and_hashes = (
        ("build_receipt", "build_receipt_sha256", build_receipt),
        ("capture_receipt", "capture_receipt_sha256", capture_receipt),
        ("original_capture_receipt", "original_capture_receipt_sha256", original_capture_receipt),
        (
            "original_exact_capture_receipt",
            "original_exact_capture_receipt_sha256",
            original_exact_capture_receipt,
        ),
    )
    for path_field, hash_field, expected_path in paths_and_hashes:
        observed_path = runner.regular_file(record[path_field], f"combined derivation {path_field}")
        if observed_path != expected_path:
            raise DerivationError(f"combined derivation receipt {path_field} differs")
        if runner.sha256_file(observed_path) != record[hash_field]:
            raise DerivationError(f"combined derivation receipt {hash_field} differs")
    if build.get("role") != "derived-reference":
        raise DerivationError("combined derived reference build receipt role differs")
    if (
        build.get("source_root") != record["source_root"]
        or build.get("source_commit") != record["source_commit"]
        or build.get("source_tree") != record["source_tree"]
    ):
        raise DerivationError("combined derived reference build source identity differs")
    for field in ("binary_sha256", "helper_sha256", "owner_helper_sha256"):
        if build.get(field) != record[field]:
            raise DerivationError(f"combined derivation build {field} differs")
    if capture.get("role") != "derived-reference":
        raise DerivationError("combined derived reference capture receipt role differs")
    if (
        capture.get("source_root") != record["source_root"]
        or capture.get("source_commit") != record["source_commit"]
        or capture.get("source_tree") != record["source_tree"]
    ):
        raise DerivationError("combined derived reference capture source identity differs")
    for field in ("binary_sha256", "helper_sha256", "owner_helper_sha256"):
        if capture.get(field) != record[field]:
            raise DerivationError(f"combined derivation capture {field} differs")
    if capture.get("range_count") != str(EXPECTED_RANGE_COUNT) or capture.get("outcome_count") != str(EXPECTED_OUTCOME_COUNT):
        raise DerivationError("combined derived reference capture coverage differs")
