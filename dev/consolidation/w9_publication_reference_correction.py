from __future__ import annotations

import hashlib
import pathlib
import re
import subprocess


CHECKOUT_ROOT = pathlib.Path("/home/josh/d/laputa-systems/xsh")
SOURCE_ROOT = CHECKOUT_ROOT / ".work/wt/w9-derived-reference-e240"
PATCH_PATH = CHECKOUT_ROOT / ".work/tmp/w9-derived-reference-e240/patch.diff"
BASE_COMMIT = "e240efad273fe35d66d91f4037b6a8193527da2e"
BASE_TREE = "a1ef04ac7b6f759c272e8f841acee3a3b3a6ff08"
SOURCE_PARENT = "d1c78b06ba8be5e5cfe483e3a0df6b136460fc28"
SOURCE_COMMIT = "d8c71d108d19d46967bcd7cff85e80624be153a7"
SOURCE_TREE = "f7e3930ee95e6676badaf0061010c4c3e0619df2"
PATCH_SHA256 = "5c2829943307c9de91b0df71a50ba30163ea186b12810f46c672901cd7ef3a61"
ALLOWED_PATCH_PATHS = (
    "src/sema/check/expr.rs",
    "src/sema/check/resource.rs",
    "src/sema/check/stmt.rs",
    "src/sema/types.rs",
    "tests/xsh/checker-streams.xsh",
    "tests/xsh/resource-ownership.xsh",
)
PROFILES = ("default", "reveal", "execution", "check-cli")
EXPECTED_RANGE_COUNT = 68
EXPECTED_OUTCOME_COUNT = 6760
RECEIPT_FIELDS = (
    "schema",
    "role",
    "base_commit",
    "base_tree",
    "patch_path",
    "patch_sha256",
    "source_root",
    "source_commit",
    "source_tree",
    "source_parent",
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


class CorrectionError(ValueError):
    pass


def _committed_patch_bytes(source_root: pathlib.Path) -> bytes:
    try:
        result = subprocess.run(
            ["git", "-C", str(source_root), "diff", "--binary", BASE_COMMIT, SOURCE_COMMIT],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise CorrectionError(f"cannot read committed derived reference patch: {error}") from error
    return result.stdout


def validate_source(source_root: pathlib.Path, patch_path: pathlib.Path, runner) -> dict[str, str]:
    source_root = runner.real_directory(source_root, "derived reference source")
    patch_path = runner.regular_file(patch_path, "derived reference patch")
    if source_root != SOURCE_ROOT:
        raise CorrectionError(f"derived reference source must be {SOURCE_ROOT}")
    if patch_path != PATCH_PATH:
        raise CorrectionError(f"derived reference patch must be {PATCH_PATH}")
    if runner.git_output(source_root, "rev-parse", "--show-toplevel") != str(source_root):
        raise CorrectionError("derived reference source is not its Git worktree root")
    base_tree = runner.git_output(source_root, "rev-parse", f"{BASE_COMMIT}^{{tree}}")
    if base_tree != BASE_TREE:
        raise CorrectionError("derived reference base tree differs from its pin")
    commit, tree = runner.source_identity(source_root)
    if (commit, tree) != (SOURCE_COMMIT, SOURCE_TREE):
        raise CorrectionError("derived reference source commit or tree differs from its pin")
    parent = runner.git_output(source_root, "rev-parse", "HEAD^")
    if parent != SOURCE_PARENT:
        raise CorrectionError("derived reference commit parent differs from its pin")
    parents = runner.git_output(source_root, "rev-list", "--parents", "-n", "1", "HEAD").split()
    if parents != [SOURCE_COMMIT, SOURCE_PARENT]:
        raise CorrectionError("derived reference commit parent list differs from its pin")
    runner.git_output(source_root, "merge-base", "--is-ancestor", BASE_COMMIT, SOURCE_COMMIT)
    if runner.git_output(source_root, "status", "--porcelain=v1", "--untracked-files=all"):
        raise CorrectionError("derived reference source worktree is not clean")
    changed_paths = tuple(
        runner.git_output(
            source_root,
            "diff",
            "--name-only",
            BASE_COMMIT,
            SOURCE_COMMIT,
        ).splitlines()
    )
    if changed_paths != ALLOWED_PATCH_PATHS:
        raise CorrectionError("derived reference changed paths differ from the approved patch")
    patch_bytes = runner.read_regular_bytes(patch_path)
    if hashlib.sha256(patch_bytes).hexdigest() != PATCH_SHA256:
        raise CorrectionError("derived reference patch SHA-256 differs from its pin")
    committed_patch = _committed_patch_bytes(source_root)
    if committed_patch != patch_bytes:
        raise CorrectionError("derived reference patch bytes differ from the committed source diff")
    return {
        "base_commit": BASE_COMMIT,
        "base_tree": BASE_TREE,
        "patch_path": str(patch_path),
        "patch_sha256": PATCH_SHA256,
        "source_root": str(source_root),
        "source_commit": commit,
        "source_tree": tree,
        "source_parent": parent,
        "source_state": "clean",
    }


def receipt_bytes(record: dict[str, str]) -> bytes:
    validate_receipt_record(record)
    return "".join(f"{field}\t{record[field]}\n" for field in RECEIPT_FIELDS).encode("utf-8")


def parse_receipt(path: pathlib.Path, contents: str) -> dict[str, str]:
    lines = contents.splitlines()
    if len(lines) != len(RECEIPT_FIELDS):
        raise CorrectionError(f"{path}: derived reference receipt field count differs")
    record: dict[str, str] = {}
    for row_number, (line, expected) in enumerate(zip(lines, RECEIPT_FIELDS), 1):
        fields = line.split("\t")
        if len(fields) != 2 or fields[0] != expected or not fields[1]:
            raise CorrectionError(f"{path}: malformed derived reference receipt row {row_number}")
        record[fields[0]] = fields[1]
    validate_receipt_record(record)
    return record


def validate_receipt_record(record: dict[str, str]) -> None:
    if set(record) != set(RECEIPT_FIELDS):
        raise CorrectionError("derived reference receipt fields differ")
    if record["schema"] != "2" or record["role"] != "derived-reference":
        raise CorrectionError("derived reference receipt schema or role differs")
    expected_source = {
        "base_commit": BASE_COMMIT,
        "base_tree": BASE_TREE,
        "patch_path": str(PATCH_PATH),
        "patch_sha256": PATCH_SHA256,
        "source_root": str(SOURCE_ROOT),
        "source_commit": SOURCE_COMMIT,
        "source_tree": SOURCE_TREE,
        "source_parent": SOURCE_PARENT,
        "source_state": "clean",
        "range_count": str(EXPECTED_RANGE_COUNT),
        "outcome_count": str(EXPECTED_OUTCOME_COUNT),
        "profiles": ",".join(PROFILES),
    }
    for field, expected in expected_source.items():
        if record[field] != expected:
            raise CorrectionError(f"derived reference receipt {field} differs")
    for field in (
        "patch_sha256",
        "build_receipt_sha256",
        "binary_sha256",
        "helper_sha256",
        "owner_helper_sha256",
        "capture_receipt_sha256",
        "original_capture_receipt_sha256",
        "original_exact_capture_receipt_sha256",
    ):
        if HEX256.fullmatch(record[field]) is None:
            raise CorrectionError(f"derived reference receipt {field} is malformed")
    if HEX40.fullmatch(record["source_commit"]) is None or HEX40.fullmatch(record["source_tree"]) is None:
        raise CorrectionError("derived reference receipt source identity is malformed")


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
        "schema": "2",
        "role": "derived-reference",
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
        observed_path = runner.regular_file(record[path_field], f"derived reference {path_field}")
        if observed_path != expected_path:
            raise CorrectionError(f"derived reference receipt {path_field} differs")
        if runner.sha256_file(observed_path) != record[hash_field]:
            raise CorrectionError(f"derived reference receipt {hash_field} differs")
    if build.get("role") != "derived-reference":
        raise CorrectionError("derived reference build receipt role differs")
    if build.get("source_root") != record["source_root"] or build.get("source_commit") != record["source_commit"] or build.get("source_tree") != record["source_tree"]:
        raise CorrectionError("derived reference build source identity differs")
    for field in ("binary_sha256", "helper_sha256", "owner_helper_sha256"):
        if build.get(field) != record[field]:
            raise CorrectionError(f"derived reference build {field} differs")
    if capture.get("role") != "derived-reference":
        raise CorrectionError("derived reference capture receipt role differs")
    if capture.get("source_root") != record["source_root"] or capture.get("source_commit") != record["source_commit"] or capture.get("source_tree") != record["source_tree"]:
        raise CorrectionError("derived reference capture source identity differs")
    for field in ("binary_sha256", "helper_sha256", "owner_helper_sha256"):
        if capture.get(field) != record[field]:
            raise CorrectionError(f"derived reference capture {field} differs")
    if capture.get("range_count") != str(EXPECTED_RANGE_COUNT) or capture.get("outcome_count") != str(EXPECTED_OUTCOME_COUNT):
        raise CorrectionError("derived reference capture coverage differs")


def first_difference(original_rows, derived_rows) -> tuple[str, ...] | None:
    seen = 0
    first = None
    for original, derived in zip(original_rows, derived_rows, strict=True):
        if original[0] != derived[0]:
            raise CorrectionError("original and derived reference outcome keys differ")
        key = original[0]
        old = original[1]
        new = derived[1]
        for field, index in (("kind", 0), ("diagnostics", 1), ("publication", 2)):
            old_value = old[index]
            new_value = new[index]
            if old_value != new_value and first is None:
                first = (
                    str(key[0]),
                    key[1],
                    key[2],
                    field,
                    hashlib.sha256(old_value.encode("utf-8")).hexdigest(),
                    hashlib.sha256(new_value.encode("utf-8")).hexdigest(),
                )
        seen += 1
    if seen != EXPECTED_OUTCOME_COUNT:
        raise CorrectionError(
            f"original-to-derived reference comparison covered {seen} outcomes, expected {EXPECTED_OUTCOME_COUNT}"
        )
    return first


def first_difference_bytes(difference: tuple[str, ...] | None) -> bytes:
    prefix = "schema\t1\nrole\toriginal-to-derived-reference\n"
    if difference is None:
        return (prefix + "difference\tnone\n").encode("utf-8")
    if len(difference) != 6 or not HEX256.fullmatch(difference[4]) or not HEX256.fullmatch(difference[5]):
        raise CorrectionError("original-to-derived first difference is malformed")
    return (prefix + "difference\t" + "\t".join(difference) + "\n").encode("utf-8")
