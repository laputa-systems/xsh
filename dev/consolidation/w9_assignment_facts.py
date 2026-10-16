from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass
from typing import Mapping


SCHEMA_HEADER = "w9-assignment-facts\t1"
_IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")
_LOWER_HEX = re.compile(r"[0-9a-f]+\Z")


class AssignmentFactsError(ValueError):
    pass


@dataclass(frozen=True, order=True)
class OwnerKey:
    source_id: int
    statement_start: int
    statement_len: int


@dataclass(frozen=True)
class ProofBinding:
    source_commit: str
    source_tree: str
    helper_sha256: str
    corpus_digest: str
    input_index: int
    input_path: str


@dataclass(frozen=True, order=True)
class SourceIdentity:
    role: str
    source_path: str
    module_key: str | None = None
    module_name: str | None = None
    internal: bool | None = None


@dataclass(frozen=True)
class AssignmentOwner:
    key: OwnerKey
    identity: SourceIdentity
    names: frozenset[str]


@dataclass(frozen=True)
class AssignmentProof:
    binding: ProofBinding
    source_files: tuple[tuple[int, str, int], ...]
    owners: tuple[AssignmentOwner, ...]

    def owner_map(self) -> dict[OwnerKey, frozenset[str]]:
        result: dict[OwnerKey, frozenset[str]] = {}
        for owner in self.owners:
            names = result.get(owner.key)
            if names is not None and names != owner.names:
                raise AssignmentFactsError(
                    f"legacy owners disagree for {owner.key}"
                )
            result[owner.key] = owner.names
        return result


def _canonical_uint(value: str, label: str) -> int:
    if re.fullmatch(r"0|[1-9][0-9]*", value) is None:
        raise AssignmentFactsError(f"{label} is not a canonical unsigned integer")
    return int(value)


def _validate_binding(binding: ProofBinding) -> None:
    if re.fullmatch(r"[0-9a-f]{40}", binding.source_commit) is None:
        raise AssignmentFactsError("source commit is malformed")
    if re.fullmatch(r"[0-9a-f]{40}", binding.source_tree) is None:
        raise AssignmentFactsError("source tree is malformed")
    if re.fullmatch(r"[0-9a-f]{64}", binding.helper_sha256) is None:
        raise AssignmentFactsError("helper SHA-256 is malformed")
    if re.fullmatch(r"[0-9a-f]{64}", binding.corpus_digest) is None:
        raise AssignmentFactsError("corpus digest is malformed")
    if binding.input_index < 0:
        raise AssignmentFactsError("input index is negative")
    if not binding.input_path or "\0" in binding.input_path:
        raise AssignmentFactsError("input path is empty or contains NUL")


def _decode_hex(value: str, label: str) -> str:
    if not value or len(value) % 2 or _LOWER_HEX.fullmatch(value) is None:
        raise AssignmentFactsError(f"{label} is not canonical lowercase hex")
    try:
        return bytes.fromhex(value).decode("utf-8")
    except UnicodeDecodeError:
        raise AssignmentFactsError(f"{label} is not UTF-8") from None


def _parse_identity(value: str) -> SourceIdentity:
    parts = value.split("\0")
    if parts[0] == "root" and len(parts) == 2 and parts[1]:
        return SourceIdentity("root", parts[1])
    if (
        parts[0] == "module"
        and len(parts) == 5
        and all(parts[1:4])
        and parts[4] in {"0", "1"}
    ):
        return SourceIdentity(
            "module", parts[1], parts[2], parts[3], parts[4] == "1"
        )
    raise AssignmentFactsError("owner has a malformed semantic root identity")


def _parse_names(value: str) -> frozenset[str]:
    if not value:
        return frozenset()
    spellings = [_decode_hex(part, "assignment name") for part in value.split(",")]
    if any(_IDENTIFIER.fullmatch(name) is None for name in spellings):
        raise AssignmentFactsError("owner contains a malformed assignment name")
    if spellings != sorted(set(spellings)):
        raise AssignmentFactsError("owner names are duplicate or noncanonical")
    return frozenset(spellings)


def parse_assignment_proof(
    contents: str,
    expected_binding: ProofBinding,
    expected_sha256: str,
) -> AssignmentProof:
    _validate_binding(expected_binding)
    if re.fullmatch(r"[0-9a-f]{64}", expected_sha256) is None:
        raise AssignmentFactsError("proof SHA-256 is malformed")
    try:
        actual_sha256 = hashlib.sha256(contents.encode("ascii")).hexdigest()
    except UnicodeEncodeError:
        raise AssignmentFactsError("proof contains non-ASCII bytes") from None
    if actual_sha256 != expected_sha256:
        raise AssignmentFactsError("proof bytes differ from the capture receipt")
    if not contents.endswith("\n") or "\r" in contents:
        raise AssignmentFactsError("proof is not newline-terminated canonical TSV")
    lines = contents[:-1].split("\n")
    if not lines or lines[0] != SCHEMA_HEADER:
        raise AssignmentFactsError("proof schema is missing or unsupported")
    if len(lines) < 4:
        raise AssignmentFactsError("proof is incomplete")

    binding_fields = lines[1].split("\t")
    if len(binding_fields) != 7 or binding_fields[0] != "binding":
        raise AssignmentFactsError("proof binding row is malformed")
    source_commit, source_tree, helper_sha256, corpus_digest, raw_index, raw_path = (
        binding_fields[1:]
    )
    actual_binding = ProofBinding(
        source_commit,
        source_tree,
        helper_sha256,
        corpus_digest,
        _canonical_uint(raw_index, "input index"),
        _decode_hex(raw_path, "input path"),
    )
    _validate_binding(actual_binding)
    if actual_binding != expected_binding:
        raise AssignmentFactsError("proof source, helper, corpus, or input binding differs")

    source_rows: list[tuple[int, str, int]] = []
    index = 2
    while index < len(lines) and lines[index].startswith("source\t"):
        fields = lines[index].split("\t")
        if len(fields) != 4:
            raise AssignmentFactsError(f"source row {index - 1} is malformed")
        source_id = _canonical_uint(fields[1], "source id")
        source_path = _decode_hex(fields[2], "source path")
        byte_len = _canonical_uint(fields[3], "source byte length")
        source_rows.append((source_id, source_path, byte_len))
        index += 1
    if not source_rows:
        raise AssignmentFactsError("proof has no source rows")
    if [row[0] for row in source_rows] != list(range(len(source_rows))):
        raise AssignmentFactsError("proof source ids are missing, duplicated, or unordered")

    if index >= len(lines):
        raise AssignmentFactsError("proof owner counts are missing")
    count_fields = lines[index].split("\t")
    if len(count_fields) != 4 or count_fields[0] != "counts":
        raise AssignmentFactsError("proof owner counts are malformed")
    source_count, root_count, module_count = (
        _canonical_uint(value, f"{label} count")
        for value, label in zip(count_fields[1:], ("source", "root owner", "module owner"))
    )
    if source_count != len(source_rows) or root_count != 1:
        raise AssignmentFactsError("proof source or root owner count is incomplete")
    index += 1

    owners: list[AssignmentOwner] = []
    for line_index in range(index, len(lines)):
        fields = lines[line_index].split("\t")
        if len(fields) != 6 or fields[0] != "owner":
            raise AssignmentFactsError(f"owner row {line_index} is malformed")
        source_id, start, length = (
            _canonical_uint(fields[1], "owner source id"),
            _canonical_uint(fields[2], "owner statement start"),
            _canonical_uint(fields[3], "owner statement length"),
        )
        if source_id >= len(source_rows):
            raise AssignmentFactsError(f"owner row {line_index} names an unknown source")
        identity = _parse_identity(_decode_hex(fields[4], "owner identity"))
        if identity.source_path != source_rows[source_id][1]:
            raise AssignmentFactsError(f"owner row {line_index} has a changed source identity")
        names = _parse_names(fields[5])
        owners.append(
            AssignmentOwner(OwnerKey(source_id, start, length), identity, names)
        )

    if len(owners) != root_count + module_count:
        raise AssignmentFactsError("proof owner rows do not match their declared counts")
    roots = [owner for owner in owners if owner.identity.role == "root"]
    modules = [owner for owner in owners if owner.identity.role == "module"]
    if len(roots) != root_count or len(modules) != module_count:
        raise AssignmentFactsError("proof root and module owner counts differ")
    owner_order = [(owner.key, owner.identity) for owner in owners]
    if owner_order != sorted(owner_order):
        raise AssignmentFactsError("proof owner rows are not in canonical order")
    identities = [owner.identity for owner in owners]
    if len(set(identities)) != len(identities):
        raise AssignmentFactsError("proof contains a duplicate semantic owner")
    proof = AssignmentProof(
        actual_binding,
        tuple(source_rows),
        tuple(owners),
    )
    proof.owner_map()
    return proof


def parse_candidate_assignment_facts(rendered: str) -> dict[OwnerKey, frozenset[str]]:
    from w9_publication_adapter import (
        AdapterError,
        _debug_name_value,
        _fields,
        _map_entries,
        _set_items_ordered,
    )

    try:
        fields = _fields(rendered, "AssignmentFacts")
        if set(fields) != {"owners"}:
            raise AssignmentFactsError("candidate AssignmentFacts has unexpected fields")
        result: dict[OwnerKey, frozenset[str]] = {}
        for raw_owner, raw_names in _map_entries(fields["owners"]).items():
            owner_fields = _fields(raw_owner, "AssignmentFactOwner")
            if set(owner_fields) != {"source_id", "statement_start", "statement_len"}:
                raise AssignmentFactsError("candidate owner has unexpected fields")
            source = re.fullmatch(r"SourceId\((0|[1-9][0-9]*)\)", owner_fields["source_id"])
            if source is None:
                raise AssignmentFactsError("candidate owner source id is malformed")
            key = OwnerKey(
                int(source.group(1)),
                _canonical_uint(owner_fields["statement_start"], "candidate statement start"),
                _canonical_uint(owner_fields["statement_len"], "candidate statement length"),
            )
            spellings = []
            for raw_name in _set_items_ordered(raw_names):
                name = _debug_name_value(raw_name)
                if name is None or _IDENTIFIER.fullmatch(name) is None:
                    raise AssignmentFactsError("candidate owner has a malformed assignment name")
                spellings.append(name)
            if len(spellings) != len(set(spellings)):
                raise AssignmentFactsError("candidate owner names are duplicated")
            if key in result:
                raise AssignmentFactsError(f"candidate has a duplicate owner {key}")
            result[key] = frozenset(spellings)
        return result
    except AdapterError as error:
        raise AssignmentFactsError(str(error)) from None


def validate_assignment_facts(
    candidate: str | Mapping[OwnerKey, frozenset[str]], proof: AssignmentProof
) -> dict[OwnerKey, frozenset[str]]:
    if isinstance(candidate, str):
        actual = parse_candidate_assignment_facts(candidate)
    else:
        actual = {}
        for key, names in candidate.items():
            if not isinstance(key, OwnerKey):
                raise AssignmentFactsError("candidate contains a malformed owner key")
            if isinstance(names, str):
                raise AssignmentFactsError("candidate owner names are not a collection")
            normalized_names = frozenset(names)
            if any(_IDENTIFIER.fullmatch(name) is None for name in normalized_names):
                raise AssignmentFactsError("candidate contains a malformed assignment name")
            actual[key] = normalized_names
    expected = proof.owner_map()
    if actual.keys() != expected.keys():
        missing = sorted(expected.keys() - actual.keys())
        extra = sorted(actual.keys() - expected.keys())
        raise AssignmentFactsError(
            f"candidate owners differ from legacy proof; missing={missing}, extra={extra}"
        )
    for key in sorted(expected):
        if actual[key] != expected[key]:
            raise AssignmentFactsError(
                f"candidate names differ from legacy proof for {key}; "
                f"expected={sorted(expected[key])}, actual={sorted(actual[key])}"
            )
    return expected
