#!/usr/bin/env python3
"""Check the provisional formal inventory against its named Rust owners."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Any, Iterable


PLAN_PATH = "xsh-lean.md"
PROVISIONAL_SOURCE_STATE = "provisional-during-consolidation"
FROZEN_SOURCE_STATE = "frozen-post-consolidation"
GIT_OBJECT_ID = re.compile(r"[0-9a-f]{40}")
SHA256_DIGEST = re.compile(r"[0-9a-f]{64}")
TEST_IDENTIFIER = re.compile(r"[A-Za-z0-9_][A-Za-z0-9._:/-]*")
XSHT_DURATION = r"(?:[0-9]+ms|[0-9]+(?:\.[0-9])?s|[0-9]+m(?:[0-9]+s)?|[0-9]+h(?:[0-9]+m)?)"
CARGO_DURATION = r"[0-9]+(?:\.[0-9]+)?s"


@dataclass(frozen=True)
class OwnerSpec:
    source: str
    declaration: str
    extractor: str
    derivation: str | None = None
    boundary: str | None = None


ARENA_ENUMS = (
    "ArenaByteSpans",
    "ArenaTypeExprTag",
    "ArenaStmtTag",
    "ArenaStmtKind",
    "SugarForm",
    "ArenaSugarOperand",
    "ArenaChild",
    "SugarView",
    "ArenaSugar",
    "WaitPace",
    "ArenaTypeDefBody",
    "ArenaModuleContractEntryKind",
    "ArenaBindingTargetKind",
    "ArenaArmSpelling",
    "DeferTrigger",
    "ArenaExprOrRun",
    "ArenaAssignTargetKind",
    "ArenaPatternKind",
    "ArenaExprTag",
    "ArenaExprKind",
    "ArenaCompQualifier",
    "ContextScopeKind",
    "RetrySchedule",
    "ArenaFmtPart",
    "ArenaText",
    "ArenaTextTag",
    "ArenaFmtPartTag",
    "ArenaSpawnTarget",
    "ArenaRecordFieldInput",
    "ArenaRecordFieldKind",
    "ArenaCallArgInput",
    "ArenaCallArgKind",
    "ArenaPipeStageKind",
    "ArenaBuilderEntryKind",
    "ArenaCommand",
    "ArenaRedirectionTarget",
    "ArenaEnvAssignmentValue",
    "ArenaCommandArgKind",
    "ArenaWordPart",
    "ArenaWordPartTag",
)

NODE_ENUMS = (
    "Effect",
    "DurationUnit",
    "FormatSpecKind",
    "StreamStageKind",
    "UnaryOp",
    "BinaryOp",
    "AssignOp",
    "CoreCommand",
    "RunKind",
    "RedirectionKind",
    "CommandWordRefSegment",
)

RESOURCE_RULE_FIELDS = (
    "kind",
    "consuming",
    "repeat",
    "scope_release",
    "with_release",
)


OWNER_SPECS: dict[str, OwnerSpec] = {
    **{
        f"syntax.arena.{name}": OwnerSpec(
            "src/syntax/arena.rs", name, "rust-enum"
        )
        for name in ARENA_ENUMS
    },
    **{
        f"syntax.node.{name}": OwnerSpec(
            "src/syntax/node.rs", name, "rust-enum"
        )
        for name in NODE_ENUMS
    },
    "syntax.stream_stage_table": OwnerSpec(
        "src/syntax/grammar.rs", "STREAM_STAGES", "stream-stage-table"
    ),
    "types.semantic": OwnerSpec("src/sema/types.rs", "Type", "rust-enum"),
    "types.module_export": OwnerSpec(
        "src/sema/types.rs", "ModuleExportType", "rust-enum"
    ),
    "types.registry": OwnerSpec(
        "crates/xsh-registry/src/types.rs", "Type", "rust-enum"
    ),
    "types.registry_parameter": OwnerSpec(
        "crates/xsh-registry/src/types.rs", "BuiltinTypeParameter", "rust-enum"
    ),
    "types.builtin_names": OwnerSpec(
        "crates/xsh-registry/src/types.rs", "builtin_type_names!", "builtin-type-names"
    ),
    "types.lowered": OwnerSpec("src/runtime/eval.rs", "LoweredType", "rust-enum"),
    "types.lowered_function": OwnerSpec(
        "src/runtime/eval.rs", "LoweredFunctionKind", "rust-enum"
    ),
    "types.lowered_return": OwnerSpec(
        "src/runtime/eval.rs", "LoweredReturnKind", "rust-enum"
    ),
    "types.indexed": OwnerSpec(
        "src/runtime/eval/indexed/semantic.rs", "TypeTag", "rust-enum"
    ),
    "runtime.value": OwnerSpec(
        "src/runtime/value.rs",
        "Value",
        "rust-enum",
        "Rust enum variants and tuple/record payload types, parsed from the declaration.",
        "Describes the runtime value algebra only; it does not inventory evaluator behavior or prove source semantics.",
    ),
    "runtime.result_value": OwnerSpec(
        "src/runtime/value.rs",
        "ResultValue",
        "rust-enum",
        "Rust enum variants and payload types, parsed from the declaration.",
        "Describes the runtime result container only; it does not inventory error propagation behavior.",
    ),
    "constants.literal": OwnerSpec(
        "src/sema/constants.rs",
        "LiteralConstant",
        "rust-enum",
        "Rust enum variants and payload types, parsed from the declaration.",
        "Covers bounded prepared constants, not which expressions evaluate to constants or their evaluation semantics.",
    ),
    "types.validation": OwnerSpec(
        "src/sema/validated.rs",
        "Validation",
        "rust-enum",
        "Rust enum variants and payload types, parsed from the declaration.",
        "Covers validation identities only; validity rules and preservation behavior remain separate semantics.",
    ),
    "types.runtime_op": OwnerSpec(
        "crates/xsh-registry/src/runtime_op.rs",
        "RuntimeOp",
        "rust-enum",
        "All RuntimeOp variants are parsed in declaration order from the closed Rust enum.",
        "Covers operation identities only; API registration, dispatch, and host effects are outside this inventory.",
    ),
    "types.callable_contract": OwnerSpec(
        "src/sema/types.rs",
        "CallableType",
        "callable-contract",
        "Named struct fields and normalized token bodies of the checked callable construction and compatibility rules.",
        "Pins current source-level signature rules; it is not a proof that call lowering or runtime dispatch conforms.",
    ),
    "types.callable_encoding": OwnerSpec(
        "src/runtime/eval/indexed/semantic.rs",
        "intern_signature",
        "callable-encoding",
        "Parameter flag constants and normalized token bodies for signature interning, decoding, and effect normalization.",
        "Pins the indexed signature representation only; verifier soundness and execution behavior are not established.",
    ),
    "stream.parameters": OwnerSpec(
        "crates/xsh-registry/src/stream_parameters.rs",
        "stage_parameters",
        "stream-parameters",
        "Parameter enum variants, record fields, default constant, and normalized stage-configuration and callable-policy functions.",
        "Covers registry declarations and policy tables only; stream execution and delayed-effect charging are outside this inventory.",
    ),
    "ir.indexed_ids": OwnerSpec(
        "src/runtime/eval/indexed.rs",
        "ir_id!",
        "indexed-id-layout",
        "The sentinel, ID-generating macro body, and every macro invocation are parsed from their declarations.",
        "Describes the current nonzero, one-based ID encoding and sentinel; it does not prove every decoder checks bounds.",
    ),
    "ir.indexed_ranges": OwnerSpec(
        "src/runtime/eval/indexed.rs",
        "IrRange",
        "indexed-range-layout",
        "IrData, IrRange, IrLocation field layouts and the range-bound and span-conversion methods are parsed from Rust.",
        "Covers compact indexed offsets and locations only; broader source-map behavior and verifier checks are excluded.",
    ),
    "source.locations": OwnerSpec(
        "src/source.rs",
        "Span",
        "source-location-layout",
        "SourceId, Span, and SourceLocation field layouts plus span constructor and range methods are parsed from Rust.",
        "Covers byte-offset representation and local span operations only; source loading and diagnostics are not modeled.",
    ),
    "types.indexed_pool_schema": OwnerSpec(
        "src/runtime/eval/indexed/semantic.rs",
        "TypeTag",
        "indexed-type-pool-schema",
        "TypeTag and TypeKey variants, pool fields, payload-classification helpers, and type interning layout are parsed from Rust.",
        "Records the current pool encoding and classification source; it does not establish a verified round trip or semantic preservation.",
    ),
    "control.eval_flow": OwnerSpec(
        "src/runtime/eval.rs", "EvalFlow", "rust-enum"
    ),
    "control.statement_flow": OwnerSpec(
        "src/runtime/eval.rs", "StmtFlow", "rust-enum"
    ),
    "control.flow": OwnerSpec("src/runtime/eval.rs", "Flow", "rust-enum"),
    "effects.indexed": OwnerSpec(
        "src/runtime/eval/indexed/semantic.rs", "EffectCode", "rust-enum"
    ),
    "effects.all": OwnerSpec("src/syntax/node.rs", "ALL", "effect-all"),
    **{
        f"resources.{name}": OwnerSpec(
            "crates/xsh-registry/src/managed_resource.rs", name, "rust-enum"
        )
        for name in (
            "ResourceKind",
            "ResourceConsumption",
            "ResourceRepeat",
            "ResourceScopeRelease",
        )
    },
    "resources.managed_table": OwnerSpec(
        "crates/xsh-registry/src/managed_resource.rs",
        "resource_table!",
        "resource-table",
    ),
    "ir.instructions": OwnerSpec(
        "src/runtime/eval/indexed/full/schema.rs",
        "instruction_schema!",
        "instruction-schema",
    ),
    "ir.patterns": OwnerSpec(
        "src/runtime/eval/indexed/full/schema.rs",
        "pattern_schema!",
        "pattern-schema",
    ),
    "ir.drivers": OwnerSpec(
        "src/runtime/eval/indexed/full/schema.rs",
        "driver_schema!",
        "driver-schema",
    ),
    "ir.pipeline_stages": OwnerSpec(
        "src/runtime/eval/indexed/full.rs",
        "pipeline_stage_schema!",
        "pipeline-stage-schema",
    ),
    "ir.lowered_value": OwnerSpec(
        "src/runtime/eval.rs",
        "LoweredValue",
        "rust-enum",
        "Variant payload types are encoded as unambiguous token arrays before being reconciled with codec routes.",
        "Describes lowered values and payload types only; lowering behavior and codec correctness are not proved.",
    ),
    "ir.values": OwnerSpec(
        "src/runtime/eval/indexed/full.rs",
        "lowered_value_schema!",
        "lowered-value-schema",
        "Codec rows are parsed and reconciled with every live LoweredValue variant; custom row procedures are retained verbatim as normalized tokens.",
        "Covers persistent value codec routing, including Set encode/decode/verify procedures; it does not prove codec soundness or runtime semantics.",
    ),
}


class InventoryError(Exception):
    pass


@lru_cache(maxsize=12)
def lex_rust(source: str) -> list[str]:
    """Tokenize delimiters and identifiers needed by the named owner layouts."""
    tokens: list[str] = []
    index = 0
    size = len(source)
    multi = ("..=", "...", "::", "=>", "->", "..", "&&", "||", "<=", ">=", "==", "!=")

    while index < size:
        char = source[index]
        if char.isspace():
            index += 1
            continue
        if source.startswith("//", index):
            end = source.find("\n", index + 2)
            index = size if end < 0 else end + 1
            continue
        if source.startswith("/*", index):
            depth = 1
            index += 2
            while index < size and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            if depth:
                raise InventoryError("unterminated Rust block comment")
            continue

        raw_start = index
        if char in "br" or source.startswith("cr", index):
            prefix_end = index
            if source.startswith("br", index) or source.startswith("cr", index):
                prefix_end += 2
            elif char == "b":
                prefix_end += 1
            else:
                prefix_end += 1
            if prefix_end < size and source[prefix_end] == "r":
                prefix_end += 1
            if prefix_end > raw_start and prefix_end <= size and source[prefix_end - 1] == "r":
                hash_end = prefix_end
                while hash_end < size and source[hash_end] == "#":
                    hash_end += 1
                if hash_end < size and source[hash_end] == '"':
                    hashes = source[prefix_end:hash_end]
                    terminator = '"' + hashes
                    end = source.find(terminator, hash_end + 1)
                    if end < 0:
                        raise InventoryError("unterminated Rust raw string")
                    tokens.append(source[index : end + len(terminator)])
                    index = end + len(terminator)
                    continue

        if char == '"' or (char in "bc" and index + 1 < size and source[index + 1] == '"'):
            if char != '"':
                index += 1
            start = index
            index += 1
            while index < size:
                if source[index] == "\\":
                    index += 2
                elif source[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            else:
                raise InventoryError("unterminated Rust string")
            tokens.append(source[start:index])
            continue

        if char == "'":
            # Character literals can contain delimiters; lifetimes remain tokens.
            end = index + 1
            if end < size and source[end] == "\\":
                end += 2
                while end < size and source[end] not in "'\n":
                    end += 1
            else:
                end += 1
            if end < size and source[end] == "'":
                tokens.append(source[index : end + 1])
                index = end + 1
                continue
            tokens.append("'")
            index += 1
            continue

        if char.isalpha() or char == "_":
            end = index + 1
            while end < size and (source[end].isalnum() or source[end] == "_"):
                end += 1
            tokens.append(source[index:end])
            index = end
            continue
        if char.isdigit():
            end = index + 1
            while end < size and (source[end].isalnum() or source[end] == "_"):
                end += 1
            tokens.append(source[index:end])
            index = end
            continue

        matched = next((punct for punct in multi if source.startswith(punct, index)), None)
        if matched:
            tokens.append(matched)
            index += len(matched)
        else:
            tokens.append(char)
            index += 1
    return tokens


def matching(tokens: list[str], start: int) -> int:
    pairs = {"{": "}", "[": "]", "(": ")"}
    opener = tokens[start]
    if opener not in pairs:
        raise InventoryError(f"expected opening delimiter, found {opener!r}")
    stack = [pairs[opener]]
    index = start + 1
    while index < len(tokens):
        token = tokens[index]
        if token in pairs:
            stack.append(pairs[token])
        elif token in ("}", "]", ")"):
            if not stack or token != stack.pop():
                raise InventoryError(f"mismatched delimiter {token!r}")
            if not stack:
                return index
        index += 1
    raise InventoryError(f"unclosed delimiter {opener!r}")


def split_top_level(tokens: list[str], delimiter: str = ",") -> list[list[str]]:
    pieces: list[list[str]] = []
    start = 0
    stack: list[str] = []
    angle = 0
    pairs = {"{": "}", "[": "]", "(": ")"}
    for index, token in enumerate(tokens):
        if token in pairs:
            stack.append(pairs[token])
        elif token in ("}", "]", ")"):
            if not stack or stack.pop() != token:
                raise InventoryError(f"mismatched nested delimiter {token!r}")
        elif not stack:
            if token == "<":
                angle += 1
            elif token == ">" and angle:
                angle -= 1
            elif token == delimiter and angle == 0:
                pieces.append(tokens[start:index])
                start = index + 1
    if stack:
        raise InventoryError("unclosed nested delimiter in owner row")
    pieces.append(tokens[start:])
    return [piece for piece in pieces if piece]


def skip_attributes(tokens: list[str], index: int, end: int) -> int:
    while index < end and tokens[index] == "#":
        if index + 1 >= end or tokens[index + 1] != "[":
            raise InventoryError("unsupported attribute layout in owner schema")
        index = matching(tokens, index + 1) + 1
    return index


def self_variant_from_pattern(tokens: list[str], label: str) -> str:
    variants = [
        tokens[index + 2]
        for index in range(len(tokens) - 2)
        if tokens[index : index + 2] == ["Self", "::"]
        and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", tokens[index + 2])
    ]
    if len(variants) != 1:
        raise InventoryError(
            f"{label} must route exactly one Self variant, found={variants}"
        )
    return variants[0]


def ensure_unique(items: list[str], label: str) -> list[str]:
    seen: set[str] = set()
    duplicates: list[str] = []
    for item in items:
        if item in seen:
            duplicates.append(item)
        seen.add(item)
    if duplicates:
        raise InventoryError(f"duplicate {label} entries: {', '.join(duplicates)}")
    return items


def macro_invocation(
    source: str, name: str, opener: str = "{"
) -> tuple[list[str], int, int]:
    tokens = lex_rust(source)
    starts = [
        index
        for index in range(len(tokens) - 2)
        if tokens[index : index + 3] == [name, "!", opener]
    ]
    if len(starts) != 1:
        raise InventoryError(f"expected one {name} invocation, found {len(starts)}")
    start = starts[0] + 2
    end = matching(tokens, start)
    return tokens, start + 1, end


def parse_field_group(tokens: list[str], start: int, expected: str) -> tuple[list[str], int]:
    if start >= len(tokens) or tokens[start] != expected:
        raise InventoryError(f"expected {expected!r} field group")
    end = matching(tokens, start)
    fields: list[str] = []
    for raw in split_top_level(tokens[start + 1 : end]):
        colon = raw.index(":") if ":" in raw else -1
        if colon <= 0:
            raise InventoryError(f"unrecognized field row: {' '.join(raw)}")
        name = raw[0]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
            raise InventoryError(f"unrecognized field name {name!r}")
        tail = raw[colon + 1 :]
        override = None
        if "=>" in tail:
            pivot = tail.index("=>")
            override = token_fingerprint(tail[pivot + 1 :])
            tail = tail[:pivot]
        if not tail:
            raise InventoryError(f"field {name} has no type")
        encoded = f"{name}:{token_fingerprint(tail)}"
        if override:
            encoded += f"=>{override}"
        fields.append(encoded)
    return fields, end + 1


def token_fingerprint(tokens: Iterable[str]) -> str:
    """Encode a token sequence without losing boundaries between tokens."""
    return "tokens:" + json.dumps(list(tokens), ensure_ascii=True, separators=(",", ":"))


def token_fingerprint_at(value: str, start: int) -> tuple[list[str], int]:
    prefix = "tokens:"
    if not value.startswith(prefix, start):
        raise InventoryError("expected a token fingerprint")
    try:
        tokens, end = json.JSONDecoder().raw_decode(value, start + len(prefix))
    except json.JSONDecodeError as exc:
        raise InventoryError("malformed token fingerprint") from exc
    if not isinstance(tokens, list) or any(
        not isinstance(token, str) for token in tokens
    ):
        raise InventoryError("token fingerprint must contain a string array")
    return tokens, end


def enum_field_rows(tokens: list[str], start: int, delimiter: str) -> tuple[list[str], int]:
    if tokens[start] != delimiter:
        raise InventoryError(f"expected {delimiter!r} enum payload")
    end = matching(tokens, start)
    fields: list[str] = []
    for raw in split_top_level(tokens[start + 1 : end]):
        if delimiter == "(":
            fields.append(token_fingerprint(raw))
            continue
        name_end = raw.index(":") if ":" in raw else -1
        if name_end != 1 or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", raw[0]):
            raise InventoryError("unrecognized record enum payload field")
        fields.append(f"{raw[0]}:{token_fingerprint(raw[name_end + 1 :])}")
    return fields, end + 1


def extract_enum(source: str, enum_name: str) -> list[str]:
    tokens = lex_rust(source)
    found = [
        index
        for index in range(len(tokens) - 1)
        if tokens[index] == "enum" and tokens[index + 1] == enum_name
    ]
    if len(found) != 1:
        raise InventoryError(f"expected one enum {enum_name}, found {len(found)}")
    opening = None
    for index in range(found[0] + 2, len(tokens)):
        if tokens[index] in (";", "}"):
            raise InventoryError(f"unsupported enum declaration for {enum_name}")
        if tokens[index] == "{":
            opening = index
            break
    if opening is None:
        raise InventoryError(f"enum body missing for {enum_name}")
    end = matching(tokens, opening)
    items: list[str] = []
    index = opening + 1
    while index < end:
        index = skip_attributes(tokens, index, end)
        if index >= end:
            break
        name = tokens[index]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
            raise InventoryError(f"unrecognized {enum_name} variant at token {name!r}")
        item = name
        index += 1
        if index < end and tokens[index] in ("(", "{"):
            delimiter = tokens[index]
            fields, index = enum_field_rows(tokens, index, delimiter)
            item += f"{delimiter}{','.join(fields)}{')' if delimiter == '(' else '}'}"
        if index < end and tokens[index] == "=":
            index += 1
            value_start = index
            while index < end and tokens[index] != ",":
                if tokens[index] in ("(", "[", "{"):
                    index = matching(tokens, index) + 1
                else:
                    index += 1
            item += f"={token_fingerprint(tokens[value_start:index])}"
        items.append(item)
        if index < end:
            if tokens[index] != ",":
                raise InventoryError(f"unrecognized {enum_name} variant tail {tokens[index]!r}")
            index += 1
    return ensure_unique(items, enum_name)


def parse_instruction_schema(source: str) -> list[str]:
    tokens, index, end = macro_invocation(source, "instruction_schema")
    rows: list[str] = []
    seen_ordinals: set[str] = set()
    seen_tags: set[str] = set()
    while index < end:
        row_name = tokens[index]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", row_name):
            raise InventoryError(f"unrecognized instruction row {row_name!r}")
        index += 1
        if index >= end or tokens[index] != "=>":
            raise InventoryError(f"instruction row {row_name} lacks =>")
        index += 1
        view = tokens[index]
        index += 1
        if tokens[index] != "[":
            raise InventoryError(f"instruction row {row_name} lacks kind")
        kind_end = matching(tokens, index)
        kind_parts = tokens[index + 1 : kind_end]
        if len(kind_parts) != 1:
            raise InventoryError(f"unrecognized instruction kind in {row_name}")
        kind = kind_parts[0]
        index = kind_end + 1
        if tokens[index] != "{":
            raise InventoryError(f"instruction row {row_name} lacks body")
        row_end = matching(tokens, index)
        index += 1
        while index < row_end:
            index = skip_attributes(tokens, index, row_end)
            if index >= row_end:
                break
            variant = tokens[index]
            index += 1
            fields: list[str] = []
            if tokens[index] in ("(", "{"):
                delimiter = tokens[index]
                fields, index = parse_field_group(tokens, index, delimiter)
            if tokens[index] != "=>":
                raise InventoryError(f"instruction {variant} lacks tag mapping")
            index += 1
            tag = tokens[index]
            if tag in seen_tags:
                raise InventoryError(f"duplicate instruction tag {tag}")
            seen_tags.add(tag)
            index += 1
            if tokens[index] != "=":
                raise InventoryError(f"instruction {variant} lacks ordinal")
            ordinal = tokens[index + 1]
            if not ordinal.isdecimal():
                raise InventoryError(f"invalid instruction ordinal {ordinal!r}")
            if ordinal in seen_ordinals:
                raise InventoryError(f"duplicate instruction ordinal {ordinal}")
            seen_ordinals.add(ordinal)
            index += 2
            if tokens[index] != "[":
                raise InventoryError(f"instruction {variant} lacks metadata")
            meta_end = matching(tokens, index)
            meta = split_top_level(tokens[index + 1 : meta_end], ";")
            if len(meta) not in (3, 4) or any(len(part) != 1 for part in meta[1:]):
                raise InventoryError(f"unrecognized instruction metadata for {variant}")
            effects = token_fingerprint(meta[0])
            classification = ":" + meta[3][0] if len(meta) == 4 else ""
            rows.append(
                f"{tag}={ordinal}|{row_name}:{view}:{kind}|{variant}"
                f"({','.join(fields)})|{effects}|{meta[1][0]}|{meta[2][0]}{classification}"
            )
            index = meta_end + 1
            if tokens[index] != ";":
                raise InventoryError(f"instruction {variant} row lacks semicolon")
            index += 1
        index = row_end + 1
    return ensure_unique(rows, "instruction schema")


def parse_pattern_schema(source: str) -> list[str]:
    tokens, index, end = macro_invocation(source, "pattern_schema")
    if tokens[index : index + 2] != ["enum", "FullPatternTag"]:
        raise InventoryError("pattern schema owner header changed")
    index += 2
    if tokens[index] != "{":
        raise InventoryError("pattern schema body missing")
    body_end = matching(tokens, index)
    index += 1
    rows: list[str] = []
    ordinals: set[str] = set()
    while index < body_end:
        index = skip_attributes(tokens, index, body_end)
        if index >= body_end:
            break
        variant = tokens[index]
        index += 1
        if tokens[index] != "=":
            raise InventoryError(f"pattern {variant} lacks ordinal")
        ordinal = tokens[index + 1]
        if not ordinal.isdecimal() or ordinal in ordinals:
            raise InventoryError(f"invalid or duplicate pattern ordinal {ordinal!r}")
        ordinals.add(ordinal)
        index += 2
        fields: list[str] = []
        if tokens[index] in ("{", "("):
            delimiter = tokens[index]
            fields, index = parse_field_group(tokens, index, delimiter)
        if tokens[index] != "[":
            raise InventoryError(f"pattern {variant} lacks mode and constraint")
        meta_end = matching(tokens, index)
        metadata = tokens[index + 1 : meta_end]
        if len(metadata) != 3 or metadata[1] != ",":
            raise InventoryError(f"unrecognized pattern metadata for {variant}")
        rows.append(
            f"{variant}={ordinal}|{','.join(fields)}|{metadata[0]}|{metadata[2]}"
        )
        index = meta_end + 1
        if index < body_end and tokens[index] == ",":
            index += 1
        elif index < body_end:
            raise InventoryError(f"pattern {variant} row lacks comma")
    if body_end + 1 != end:
        raise InventoryError("pattern schema has trailing tokens")
    return ensure_unique(rows, "pattern schema")


def parse_driver_schema(source: str) -> list[str]:
    tokens, index, end = macro_invocation(source, "driver_schema")
    if index + 1 >= end or tokens[index + 1] != ";":
        raise InventoryError("driver schema owner header changed")
    index += 2
    rows: list[str] = []
    while index < end:
        variant = tokens[index]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", variant):
            raise InventoryError(f"unrecognized driver row {variant!r}")
        index += 1
        fields, index = parse_field_group(tokens, index, "{")
        rows.append(f"{variant}|{','.join(fields)}")
    return ensure_unique(rows, "driver schema")


def parse_pipeline_schema(source: str) -> list[str]:
    tokens, index, end = macro_invocation(source, "pipeline_stage_schema")
    if tokens[index] != "@" or tokens[index + 1] != "context":
        raise InventoryError("pipeline schema owner header changed")
    separator = index + 2
    while separator < end and tokens[separator] != ";":
        if tokens[separator] in ("{", "}"):
            raise InventoryError("pipeline schema context layout changed")
        separator += 1
    if separator >= end:
        raise InventoryError("pipeline schema context terminator missing")
    index = separator + 1
    rows: list[str] = []
    while index < end:
        variant = tokens[index]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", variant):
            raise InventoryError(f"unrecognized pipeline stage {variant!r}")
        kind = tokens[index + 1]
        if kind not in ("unit", "tuple", "struct"):
            raise InventoryError(f"unrecognized pipeline stage row kind {kind!r}")
        fields, index = parse_field_group(tokens, index + 2, "{")
        if tokens[index] != ";":
            raise InventoryError(f"pipeline stage {variant} row lacks semicolon")
        index += 1
        rows.append(f"{variant}|{kind}|{','.join(fields)}")
    return ensure_unique(rows, "pipeline schema")


def parse_resource_table(source: str) -> list[str]:
    tokens, index, end = macro_invocation(source, "resource_table")
    rows: list[str] = []
    while index < end:
        variant = tokens[index]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", variant):
            raise InventoryError(f"unrecognized resource row {variant!r}")
        if tokens[index + 1] != "=>":
            raise InventoryError(f"resource row {variant} lacks =>")
        index += 2
        if tokens[index] != "(":
            raise InventoryError(f"resource row {variant} has no type and rule tuple")
        tuple_end = matching(tokens, index)
        tuple_parts = split_top_level(tokens[index + 1 : tuple_end])
        if len(tuple_parts) != 2:
            raise InventoryError(f"resource row {variant} has malformed type and rule")
        type_expression = token_fingerprint(tuple_parts[0])
        rule = tuple_parts[1]
        if len(rule) < 3 or rule[0] != "ResourceRule" or rule[1] != "{":
            raise InventoryError(f"resource row {variant} has an unknown rule layout")
        rule_end = matching(rule, 1)
        if rule_end != len(rule) - 1:
            raise InventoryError(f"resource row {variant} has trailing rule tokens")
        fields: dict[str, str] = {}
        for raw in split_top_level(rule[2:rule_end]):
            colon = raw.index(":") if ":" in raw else -1
            if colon != 1:
                raise InventoryError(f"resource row {variant} has malformed rule field")
            fields[raw[0]] = token_fingerprint(raw[colon + 1 :])
        if tuple(fields) != RESOURCE_RULE_FIELDS:
            raise InventoryError(
                f"resource rule layout changed for {variant}: "
                f"expected={RESOURCE_RULE_FIELDS}, found={tuple(fields)}"
            )
        rules = ",".join(f"{key}={fields[key]}" for key in RESOURCE_RULE_FIELDS)
        rows.append(f"{variant}|type={type_expression}|{rules}")
        index = tuple_end + 1
        if index < end:
            if tokens[index] != ",":
                raise InventoryError(f"resource row {variant} lacks a comma")
            index += 1
    return ensure_unique(rows, "resource table")


def parse_stage_table(source: str) -> list[str]:
    tokens = lex_rust(source)
    marker = tokens.index("STREAM_STAGES")
    eq = tokens.index("=", marker)
    open_index = eq + 1
    if tokens[open_index] != "[":
        raise InventoryError("STREAM_STAGES is no longer an array")
    end = matching(tokens, open_index)
    entries: list[str] = []
    index = open_index + 1
    while index < end:
        if tokens[index] != "stage" or tokens[index + 1] != "(":
            raise InventoryError("unrecognized STREAM_STAGES row")
        row_end = matching(tokens, index + 1)
        row = tokens[index + 2 : row_end]
        if (
            len(row) != 9
            or row[0:2] != ["StreamStageKind", "::"]
            or row[3] != ","
            or row[5] != ","
            or row[7] != ","
        ):
            raise InventoryError(f"unrecognized stream stage row: {' '.join(row)}")
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", row[2]) or not row[4].startswith('"'):
            raise InventoryError("stream stage kind or name is malformed")
        if row[6] not in ("true", "false") or row[8] not in ("true", "false"):
            raise InventoryError(f"unrecognized stream stage flags for {row[2]}")
        entries.append(f"{row[2]}:{row[4]}:{row[6]}:{row[8]}")
        index = row_end + 1
        if index < end and tokens[index] == ",":
            index += 1
    return ensure_unique(entries, "stream stage table")


def parse_effect_all(source: str) -> list[str]:
    tokens = lex_rust(source)
    starts = [
        i
        for i in range(len(tokens) - 4)
        if tokens[i : i + 5] == ["const", "ALL", ":", "[", "Effect"]
    ]
    if len(starts) != 1:
        raise InventoryError(f"expected one Effect::ALL array, found {len(starts)}")
    eq = tokens.index("=", starts[0] + 5)
    if tokens[eq + 1] != "[":
        raise InventoryError("Effect::ALL is no longer an array")
    end = matching(tokens, eq + 1)
    array_tokens = tokens[eq + 2 : end]
    items: list[str] = []
    index = 0
    while index < len(array_tokens):
        if array_tokens[index : index + 2] != ["Self", "::"]:
            raise InventoryError("unrecognized Effect::ALL item")
        items.append(array_tokens[index + 2])
        index += 3
        if index < len(array_tokens):
            if array_tokens[index] != ",":
                raise InventoryError("unrecognized Effect::ALL separator")
            index += 1
    return ensure_unique(items, "Effect::ALL")


def parse_builtin_type_names(source: str) -> list[str]:
    tokens, index, end = macro_invocation(source, "builtin_type_names", "(")
    items: list[str] = []
    while index < end:
        if tokens[index] != "(":
            raise InventoryError("unrecognized builtin type name row")
        row_end = matching(tokens, index)
        fields = split_top_level(tokens[index + 1 : row_end])
        if (
            len(fields) != 2
            or len(fields[0]) != 1
            or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", fields[0][0])
            or len(fields[1]) != 1
            or not fields[1][0].startswith('"')
        ):
            raise InventoryError("malformed builtin type name row")
        items.append(f"{fields[0][0]}:{fields[1][0]}")
        index = row_end + 1
        if index < end:
            if tokens[index] != ",":
                raise InventoryError("builtin type name rows require commas")
            index += 1
    return ensure_unique(items, "builtin type name")


def parse_lowered_value_schema(source: str) -> list[str]:
    tokens, index, end = macro_invocation(source, "lowered_value_schema")
    try:
        rows_index = tokens.index("rows", index, end)
        rows_open = tokens.index("{", rows_index + 1, end)
        rows_end = matching(tokens, rows_open)
        aliases_index = tokens.index("aliases", rows_end + 1, end)
        aliases_open = tokens.index("{", aliases_index + 1, end)
        aliases_end = matching(tokens, aliases_open)
        custom_index = tokens.index("custom", aliases_end + 1, end)
        custom_open = tokens.index("{", custom_index + 1, end)
        custom_end = matching(tokens, custom_open)
        reject_index = tokens.index("reject", custom_end + 1, end)
        reject_open = tokens.index("{", reject_index + 1, end)
        reject_end = matching(tokens, reject_open)
    except ValueError as exc:
        raise InventoryError("lowered value schema section missing") from exc
    rows: list[str] = []
    seen_tags: set[str] = set()
    seen_ordinals: set[str] = set()
    cursor = rows_open + 1
    while cursor < rows_end:
        tag = tokens[cursor]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", tag) or tokens[cursor + 1] != "=":
            raise InventoryError(f"unrecognized lowered value row {tag!r}")
        ordinal = tokens[cursor + 2]
        if not ordinal.isdecimal() or tokens[cursor + 3] != "{":
            raise InventoryError(f"malformed lowered value row {tag}")
        if tag in seen_tags or ordinal in seen_ordinals:
            raise InventoryError(f"duplicate lowered value tag or ordinal {tag}={ordinal}")
        seen_tags.add(tag)
        seen_ordinals.add(ordinal)
        row_end = matching(tokens, cursor + 3)
        fields_index = tokens.index("fields", cursor + 4, row_end)
        fields_open = tokens.index("{", fields_index + 1, row_end)
        fields, fields_end = parse_field_group(tokens, fields_open, "{")
        encode_index = tokens.index("encode", fields_end, row_end)
        encode_open = tokens.index("{", encode_index + 1, row_end)
        encode_end = matching(tokens, encode_open)
        encode_arrow = find_top_level(tokens, encode_open + 1, encode_end, "=>")
        encoded_variant = self_variant_from_pattern(
            tokens[encode_open + 1 : encode_arrow], f"encoded lowered value row {tag}"
        )
        if encoded_variant != tag:
            raise InventoryError(
                f"lowered value row {tag} encodes {encoded_variant}"
            )
        rows.append(f"{tag}={ordinal}|{','.join(fields)}")
        cursor = row_end + 1

    cursor = aliases_open + 1
    while cursor < aliases_end:
        arrow = find_top_level(tokens, cursor, aliases_end, "=>")
        pattern_tokens = tokens[cursor:arrow]
        pattern = token_fingerprint(pattern_tokens)
        self_variant_from_pattern(pattern_tokens, "lowered value alias")
        tag = tokens[arrow + 1]
        if tag not in seen_tags:
            raise InventoryError(f"lowered value alias {pattern} names unknown tag {tag}")
        fields_key = tokens.index("fields", arrow + 2, aliases_end)
        fields_open = tokens.index("{", fields_key + 1, aliases_end)
        fields_end = matching(tokens, fields_open)
        fields = [
            token_fingerprint(part)
            for part in split_top_level(tokens[fields_open + 1 : fields_end])
        ]
        rows.append(f"alias:{pattern}=>{tag}|{','.join(fields)}")
        cursor = fields_end + 1
        if tokens[cursor] != "}":
            raise InventoryError(f"lowered value alias {pattern} has trailing tokens")
        cursor += 1

    cursor = custom_open + 1
    while cursor < custom_end:
        tag = tokens[cursor]
        if tokens[cursor + 1] != "=" or not tokens[cursor + 2].isdecimal():
            raise InventoryError(f"unrecognized custom lowered value row {tag!r}")
        ordinal = tokens[cursor + 2]
        if tag in seen_tags or ordinal in seen_ordinals:
            raise InventoryError(f"duplicate custom lowered value tag or ordinal {tag}={ordinal}")
        seen_tags.add(tag)
        seen_ordinals.add(ordinal)
        cursor += 3
        if tokens[cursor] != "{":
            raise InventoryError(f"custom lowered value row {tag} body missing")
        custom_end_row = matching(tokens, cursor)
        procedures: dict[str, list[str]] = {}
        section_cursor = cursor + 1
        while section_cursor < custom_end_row:
            procedure = tokens[section_cursor]
            if procedure not in {"encode", "decode", "verify"}:
                raise InventoryError(
                    f"unrecognized custom lowered value procedure {procedure!r}"
                )
            if procedure in procedures:
                raise InventoryError(
                    f"duplicate custom lowered value procedure {procedure}"
                )
            if tokens[section_cursor + 1] != "{":
                raise InventoryError(
                    f"custom lowered value procedure {procedure} body missing"
                )
            procedure_open = section_cursor + 1
            procedure_end = matching(tokens, procedure_open)
            procedures[procedure] = tokens[procedure_open : procedure_end + 1]
            section_cursor = procedure_end + 1
        if set(procedures) != {"encode", "decode", "verify"}:
            raise InventoryError(
                "custom lowered value route requires exactly one encode, decode, and verify procedure"
            )
        encode_tokens = procedures["encode"]
        encode_arrow = find_top_level(encode_tokens, 1, len(encode_tokens) - 1, "=>")
        encoded_variant = self_variant_from_pattern(
            encode_tokens[1:encode_arrow], f"custom lowered value row {tag}"
        )
        if encoded_variant != tag:
            raise InventoryError(
                f"custom lowered value row {tag} encodes {encoded_variant}"
            )
        if tag != "Set":
            raise InventoryError(f"unrecognized custom lowered value route {tag}")
        helper = function_body(source, "decode_set_payload")
        rows.append(
            f"{tag}={ordinal}|custom"
            f"|encode:{token_fingerprint(procedures['encode'])}"
            f"|decode:{token_fingerprint(procedures['decode'])}"
            f"|verify:{token_fingerprint(procedures['verify'])}"
            f"|decode_set_payload:{helper}"
        )
        cursor = custom_end_row + 1

    cursor = reject_open + 1
    while cursor < reject_end:
        arrow = find_top_level(tokens, cursor, reject_end, "=>")
        for pattern_tokens in split_top_level(tokens[cursor:arrow], "|"):
            self_variant_from_pattern(pattern_tokens, "rejected lowered value pattern")
            pattern = token_fingerprint(pattern_tokens)
            rows.append(f"reject:{pattern}")
        cursor = arrow + 1
        if tokens[cursor] != "{":
            raise InventoryError("lowered value reject row has no body")
        cursor = matching(tokens, cursor) + 1
        if cursor < reject_end:
            if tokens[cursor] != ",":
                raise InventoryError("lowered value reject rows require commas")
            cursor += 1
    return ensure_unique(rows, "lowered value schema")


def lowered_value_variant_name(item: str) -> str:
    name = re.match(r"[A-Za-z_][A-Za-z0-9_]*", item)
    if name is None:
        raise InventoryError(f"malformed LoweredValue variant {item!r}")
    return name.group(0)


def lowered_value_payload(item: str) -> str:
    name = lowered_value_variant_name(item)
    if item.startswith(name + "(") and item.endswith(")"):
        return item[len(name) + 1 : -1]
    if item.startswith(name + "{") and item.endswith("}"):
        return item[len(name) + 1 : -1]
    return ""


def reconcile_lowered_value_routes(
    enum_items: list[str], schema_items: list[str]
) -> list[str]:
    """Require one codec route for each live value variant and bind custom payloads."""
    routes: list[tuple[str, str]] = []
    for item in schema_items:
        if item.startswith("alias:"):
            pattern_start = len("alias:")
            pattern, pattern_end = token_fingerprint_at(item, pattern_start)
            if item[pattern_end : pattern_end + 2] != "=>":
                raise InventoryError(f"unrecognized lowered value alias route {item}")
            routes.append(
                (self_variant_from_pattern(pattern, "lowered value alias"), "alias")
            )
        elif item.startswith("reject:"):
            pattern, pattern_end = token_fingerprint_at(item, len("reject:"))
            if pattern_end != len(item):
                raise InventoryError(f"unrecognized lowered value reject route {item}")
            routes.append(
                (self_variant_from_pattern(pattern, "rejected lowered value"), "rejected")
            )
        else:
            row, separator, details = item.partition("|")
            variant, equals, ordinal = row.partition("=")
            if not separator or not equals or not ordinal.isdecimal():
                raise InventoryError(f"malformed lowered value schema route {item}")
            route = "custom" if details.startswith("custom") else "encoded"
            routes.append((variant, route))

    route_names = [variant for variant, _ in routes]
    ensure_unique(route_names, "LoweredValue codec routes")
    variants = [lowered_value_variant_name(item) for item in enum_items]
    ensure_unique(variants, "LoweredValue variants")
    if set(route_names) != set(variants):
        raise InventoryError(
            "LoweredValue codec routes differ from the live enum: "
            f"missing={sorted(set(variants) - set(route_names))}, "
            f"unknown={sorted(set(route_names) - set(variants))}"
        )

    enum_by_name = dict(zip(variants, enum_items, strict=True))
    reconciled: list[str] = []
    for item in schema_items:
        row, separator, details = item.partition("|")
        variant = row.split("=", 1)[0]
        if separator and details.startswith("custom"):
            payload = lowered_value_payload(enum_by_name[variant])
            if not payload:
                raise InventoryError(
                    f"custom LoweredValue route {variant} has no enum payload"
                )
            ordinal = row.split("=", 1)[1]
            procedure_details = details[len("custom") :]
            reconciled.append(
                f"{variant}={ordinal}|custom:{payload}{procedure_details}"
            )
        else:
            reconciled.append(item)
    return reconciled


def extract_struct_fields(source: str, struct_name: str) -> list[str]:
    tokens = lex_rust(source)
    found = [
        index
        for index in range(len(tokens) - 1)
        if tokens[index] == "struct" and tokens[index + 1] == struct_name
    ]
    if len(found) != 1:
        raise InventoryError(f"expected one struct {struct_name}, found {len(found)}")
    opening = next(
        (
            index
            for index in range(found[0] + 2, len(tokens))
            if tokens[index] in ("{", "(", ";")
        ),
        None,
    )
    if opening is None or tokens[opening] == ";":
        raise InventoryError(f"body missing for struct {struct_name}")
    closing = matching(tokens, opening)
    body = tokens[opening + 1 : closing]
    fields: list[str] = []
    for raw in split_top_level(body):
        if tokens[opening] == "(":
            if raw and raw[0] == "pub":
                raw = raw[1:]
            fields.append(f"{len(fields)}:{token_fingerprint(raw)}")
        else:
            raw = raw[1:] if raw and raw[0] == "pub" else raw
            colon = find_top_level(raw, 0, len(raw), ":")
            if colon != 1 or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", raw[0]):
                raise InventoryError(f"unrecognized field in struct {struct_name}")
            fields.append(f"{raw[0]}:{token_fingerprint(raw[colon + 1 :])}")
    return ensure_unique(fields, struct_name)


def function_body(
    source: str, function_name: str, impl_name: str | None = None
) -> str:
    return token_fingerprint(function_tokens(source, function_name, impl_name))


def function_tokens(
    source: str, function_name: str, impl_name: str | None = None
) -> list[str]:
    tokens = lex_rust(source)
    if impl_name is not None:
        impls = [
            index
            for index in range(len(tokens) - 1)
            if tokens[index] == "impl" and tokens[index + 1] == impl_name
        ]
        if len(impls) != 1:
            raise InventoryError(f"expected one impl {impl_name}, found {len(impls)}")
        impl_open = next(
            (index for index in range(impls[0] + 2, len(tokens)) if tokens[index] == "{"),
            None,
        )
        if impl_open is None:
            raise InventoryError(f"impl body missing for {impl_name}")
        scope = (impl_open + 1, matching(tokens, impl_open))
    else:
        scope = (0, len(tokens))
    found = [
        index
        for index in range(scope[0], scope[1] - 1)
        if tokens[index] == "fn" and tokens[index + 1] == function_name
    ]
    if len(found) != 1:
        raise InventoryError(
            f"expected one {function_name} function in {impl_name or 'module'}, found {len(found)}"
        )
    opening = next(
        (index for index in range(found[0] + 2, scope[1]) if tokens[index] in ("{", ";")),
        None,
    )
    if opening is None or tokens[opening] != "{":
        raise InventoryError(f"body missing for function {function_name}")
    return tokens[opening : matching(tokens, opening) + 1]


def constant_value(source: str, name: str) -> str:
    tokens = lex_rust(source)
    found = [
        index
        for index in range(len(tokens) - 1)
        if tokens[index] == "const" and tokens[index + 1] == name
    ]
    if len(found) != 1:
        raise InventoryError(f"expected one const {name}, found {len(found)}")
    equals = tokens.index("=", found[0] + 2)
    end = tokens.index(";", equals + 1)
    return token_fingerprint(tokens[equals + 1 : end])


def parse_callable_contract(source: str) -> list[str]:
    items = [
        f"CallableType{{{','.join(extract_struct_fields(source, 'CallableType'))}}}",
        f"CallableParamType{{{','.join(extract_struct_fields(source, 'CallableParamType'))}}}",
        f"TypedCallable{{{','.join(extract_struct_fields(source, 'TypedCallable'))}}}",
    ]
    items.extend(
        f"TypedCallable::{name}{function_body(source, name, 'TypedCallable')}"
        for name in ("from_type_expr", "type_expr_error", "mismatch")
    )
    items.append(f"callable_return_fits{function_body(source, 'callable_return_fits')}")
    return ensure_unique(items, "callable contract")


def parse_callable_encoding(source: str) -> list[str]:
    items = [
        f"{name}={constant_value(source, name)}"
        for name in ("PARAM_DEFAULTED", "PARAM_REST", "MODULE_EXPORT_OPTIONAL")
    ]
    items.extend(
        f"{name}{function_body(source, name)}"
        for name in (
            "intern_signature",
            "intern_signature_key",
            "to_signature",
            "normalized_effects",
            "signature_effect_count",
        )
    )
    return ensure_unique(items, "callable encoding")


def parse_stream_parameters(source: str) -> list[str]:
    items = [
        f"StageParameterType={','.join(extract_enum(source, 'StageParameterType'))}",
        f"StageParameterDefault={','.join(extract_enum(source, 'StageParameterDefault'))}",
        f"StageParameterValidation={','.join(extract_enum(source, 'StageParameterValidation'))}",
        f"StageParameter{{{','.join(extract_struct_fields(source, 'StageParameter'))}}}",
        f"DEFAULT_PAR_MAP_WORKERS={constant_value(source, 'DEFAULT_PAR_MAP_WORKERS')}",
        f"stage_parameters{function_body(source, 'stage_parameters')}",
        f"stage_accepts_callable{function_body(source, 'stage_accepts_callable')}",
    ]
    return ensure_unique(items, "stream parameter contract")


def parse_indexed_id_layout(source: str) -> list[str]:
    tokens = lex_rust(source)
    macro = [
        index
        for index in range(len(tokens) - 2)
        if tokens[index : index + 3] == ["macro_rules", "!", "ir_id"]
    ]
    if len(macro) != 1:
        raise InventoryError(f"expected one ir_id macro, found {len(macro)}")
    opening = tokens.index("{", macro[0] + 3)
    macro_body = token_fingerprint(tokens[opening : matching(tokens, opening) + 1])
    names = []
    for index in range(len(tokens) - 3):
        if tokens[index : index + 3] == ["ir_id", "!", "("]:
            name = tokens[index + 3]
            if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
                raise InventoryError(f"unrecognized ir_id invocation {name!r}")
            names.append(name)
    return ensure_unique(
        [
            f"IR_NONE={constant_value(source, 'IR_NONE')}",
            f"ir_id!={macro_body}",
            f"ids={','.join(names)}",
        ],
        "indexed ID layout",
    )


def parse_indexed_range_layout(source: str) -> list[str]:
    return ensure_unique(
        [
            f"IrData{{{','.join(extract_struct_fields(source, 'IrData'))}}}",
            f"IrRange{{{','.join(extract_struct_fields(source, 'IrRange'))}}}",
            f"IrLocation{{{','.join(extract_struct_fields(source, 'IrLocation'))}}}",
            f"IrData::ZERO={constant_value(source, 'ZERO')}",
            f"IrData::new{function_body(source, 'new', 'IrData')}",
            f"IrData::from_range{function_body(source, 'from_range', 'IrData')}",
            f"IrData::range{function_body(source, 'range', 'IrData')}",
            f"IrRange::new{function_body(source, 'new', 'IrRange')}",
            f"IrRange::bounds{function_body(source, 'bounds', 'IrRange')}",
            f"IrLocation::from_span{function_body(source, 'from_span', 'IrLocation')}",
        ],
        "indexed range layout",
    )


def parse_source_location_layout(source: str) -> list[str]:
    items = [
        f"SourceId{{{','.join(extract_struct_fields(source, 'SourceId'))}}}",
        f"Span{{{','.join(extract_struct_fields(source, 'Span'))}}}",
        f"SourceLocation{{{','.join(extract_struct_fields(source, 'SourceLocation'))}}}",
    ]
    items.extend(
        f"{impl}::{name}{function_body(source, name, impl)}"
        for impl, names in (
            ("SourceId", ("new", "raw")),
            (
                "Span",
                (
                    "new",
                    "at",
                    "set_start",
                    "set_end",
                    "shift_start",
                    "start",
                    "end",
                    "range",
                    "is_empty",
                ),
            ),
        )
        for name in names
    )
    return ensure_unique(items, "source location layout")


def parse_indexed_type_pool_schema(source: str) -> list[str]:
    helpers = ("has_no_payload", "has_one_type", "has_one_name")
    variants = extract_enum(source, "TypeTag")
    classified: dict[str, list[str]] = {}
    for helper in helpers:
        body = function_tokens(source, helper, "TypeTag")
        members = [
            body[index + 2]
            for index in range(len(body) - 2)
            if body[index : index + 2] == ["Self", "::"]
        ]
        ensure_unique(members, f"TypeTag::{helper}")
        unknown = sorted(set(members) - set(variants))
        if unknown:
            raise InventoryError(f"TypeTag::{helper} names unknown variants {unknown}")
        classified[helper] = members
    combined = [variant for members in classified.values() for variant in members]
    ensure_unique(combined, "TypeTag payload classes")
    special = [variant for variant in variants if variant not in set(combined)]
    items = [
        f"TypeTag={','.join(variants)}",
        "payload-buckets="
        + ";".join(
            f"{helper.removeprefix('has_')}:{','.join(classified[helper])}"
            for helper in helpers
        )
        + f";other:{','.join(special)}",
        f"TypeKey={','.join(extract_enum(source, 'TypeKey'))}",
        f"SemanticPools{{{','.join(extract_struct_fields(source, 'SemanticPools'))}}}",
    ]
    items.extend(
        f"TypeTag::{name}{function_body(source, name, 'TypeTag')}"
        for name in helpers
    )
    items.extend(
        f"{name}{function_body(source, name)}"
        for name in ("intern_type", "verify_type_raw", "to_type_inner")
    )
    items.append(f"SemanticPools::verify{function_body(source, 'verify', 'SemanticPools')}")
    return ensure_unique(items, "indexed type pool schema")


def find_top_level(tokens: list[str], start: int, end: int, target: str) -> int:
    stack: list[str] = []
    angle = 0
    pairs = {"{": "}", "[": "]", "(": ")"}
    index = start
    while index < end:
        token = tokens[index]
        if not stack and angle == 0 and token == target:
            return index
        if token in pairs:
            stack.append(pairs[token])
        elif token in ("}", "]", ")"):
            if not stack or stack.pop() != token:
                raise InventoryError(f"mismatched nested delimiter {token!r}")
        elif not stack:
            if token == "<":
                angle += 1
            elif token == ">" and angle:
                angle -= 1
        index += 1
    raise InventoryError(f"owner row missing delimiter {target!r}")


EXTRACTORS = {
    "rust-enum": extract_enum,
    "stream-stage-table": parse_stage_table,
    "instruction-schema": parse_instruction_schema,
    "pattern-schema": parse_pattern_schema,
    "driver-schema": parse_driver_schema,
    "pipeline-stage-schema": parse_pipeline_schema,
    "resource-table": parse_resource_table,
    "lowered-value-schema": parse_lowered_value_schema,
    "effect-all": parse_effect_all,
    "builtin-type-names": parse_builtin_type_names,
    "callable-contract": parse_callable_contract,
    "callable-encoding": parse_callable_encoding,
    "stream-parameters": parse_stream_parameters,
    "indexed-id-layout": parse_indexed_id_layout,
    "indexed-range-layout": parse_indexed_range_layout,
    "source-location-layout": parse_source_location_layout,
    "indexed-type-pool-schema": parse_indexed_type_pool_schema,
}


def discover_public_arena_enums(source: str) -> set[str]:
    return discover_public_enums(source)


def discover_public_enums(source: str) -> set[str]:
    tokens = lex_rust(source)
    names: set[str] = set()
    for index in range(len(tokens) - 2):
        if tokens[index : index + 2] == ["pub", "enum"]:
            names.add(tokens[index + 2])
    return names


def load_json(path: Path) -> Any:
    def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise InventoryError(f"duplicate JSON object key {key!r}")
            result[key] = value
        return result

    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicate_keys)
    except (OSError, json.JSONDecodeError) as exc:
        raise InventoryError(f"cannot read {path}: {exc}") from exc


def required_plan_obligations(plan: str) -> dict[str, str]:
    """Read uppercase theorem IDs only from the G0-G10 plan sections."""
    headings = list(re.finditer(r"^### G(?:\d+[a-z]?) — .*?$", plan, re.M))
    if not headings:
        raise InventoryError("Lean plan has no recognized G0-G10 sections")
    obligations: dict[str, str] = {}
    for position, heading in enumerate(headings):
        gate = heading.group(0).split()[1]
        section = plan[heading.end() : headings[position + 1].start() if position + 1 < len(headings) else len(plan)]
        for code in re.findall(r"`([^`]+)`", section):
            prefix = None
            for part in code.split("/"):
                explicit = re.search(r"\b[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+\b", part)
                if explicit:
                    identifier = explicit.group(0)
                    if identifier in obligations:
                        raise InventoryError(f"duplicate plan obligation {identifier}")
                    obligations[identifier] = gate
                    prefix = identifier.rsplit("-", 1)[0] + "-"
                elif prefix and re.fullmatch(r"[A-Z][A-Z0-9]*", part.strip()):
                    identifier = prefix + part.strip()
                    if identifier in obligations:
                        raise InventoryError(f"duplicate plan obligation {identifier}")
                    obligations[identifier] = gate
    return dict(sorted(obligations.items()))


def lean_module_sources(formal_root: Path) -> dict[str, Path]:
    return {
        ".".join(path.relative_to(formal_root).with_suffix("").parts): path
        for path in formal_root.rglob("*.lean")
        if "scripts" not in path.relative_to(formal_root).parts
    }


def lean_import_graph(module_sources: dict[str, Path]) -> dict[str, set[str]]:
    imports_by_module: dict[str, set[str]] = {}
    for module, path in module_sources.items():
        imports: set[str] = set()
        for line in path.read_text(encoding="utf-8").splitlines():
            declaration = line.split("--", 1)[0].strip()
            if declaration.startswith("import "):
                imports.update(declaration.removeprefix("import ").split())
        imports_by_module[module] = imports
    return imports_by_module


def reachable_lean_modules(module_sources: dict[str, Path]) -> set[str]:
    if "Xsh" not in module_sources:
        raise InventoryError("Lean root module Xsh is missing")
    imports_by_module = lean_import_graph(module_sources)
    reachable = {"Xsh"}
    pending = ["Xsh"]
    while pending:
        module = pending.pop()
        for imported in imports_by_module[module] & module_sources.keys():
            if imported not in reachable:
                reachable.add(imported)
                pending.append(imported)
    return reachable


def lean_theorem_declarations(source: str) -> set[str]:
    scopes: list[tuple[str, str | None]] = []
    declarations: set[str] = set()
    for line in source.splitlines():
        namespace_match = re.match(
            r"^\s*namespace\s+([A-Za-z_][A-Za-z0-9_.]*)\s*(?:--.*)?$", line
        )
        if namespace_match:
            name = namespace_match.group(1)
            namespaces = [value for kind, value in scopes if kind == "namespace" and value]
            if name.startswith("Xsh.") or name == "Xsh":
                qualified = name
            elif namespaces:
                qualified = f"{namespaces[-1]}.{name}"
            else:
                qualified = name
            scopes.append(("namespace", qualified))
            continue

        section_match = re.match(
            r"^\s*section(?:\s+([A-Za-z_][A-Za-z0-9_.]*))?\s*(?:--.*)?$", line
        )
        if section_match:
            scopes.append(("section", section_match.group(1)))
            continue

        if re.match(r"^\s*mutual\s*(?:--.*)?$", line):
            scopes.append(("mutual", None))
            continue

        end_match = re.match(r"^\s*end(?:\s+([A-Za-z_][A-Za-z0-9_.]*))?\s*(?:--.*)?$", line)
        if end_match:
            closing = end_match.group(1)
            if closing is None:
                if scopes:
                    scopes.pop()
            else:
                for index in range(len(scopes) - 1, -1, -1):
                    kind, name = scopes[index]
                    if name == closing or name and name.split(".")[-1] == closing:
                        del scopes[index:]
                        break
            continue

        theorem_match = re.match(
            r"^\s*theorem\s+([A-Za-z_][A-Za-z0-9_.'?!]*)\b", line
        )
        if theorem_match:
            name = theorem_match.group(1)
            namespaces = [value for kind, value in scopes if kind == "namespace" and value]
            declarations.add(f"{namespaces[-1]}.{name}" if namespaces else name)
    return declarations


def validate_fragment_roots(
    roots: Any,
    module_sources: dict[str, Path],
    reachable_modules: set[str],
) -> set[str]:
    if not isinstance(roots, list) or not roots:
        raise InventoryError("fragment_roots must be a nonempty array")

    root_keys = {
        "id",
        "module",
        "declaration",
        "evidence_class",
        "status",
        "scope",
        "boundary",
    }
    seen_ids: set[str] = set()
    seen_declarations: set[str] = set()
    seen_modules: set[str] = set()
    theorem_declarations: dict[str, set[str]] = {}
    expected_modules: set[str] = set()
    for module, path in module_sources.items():
        if module.startswith("Xsh."):
            declarations = lean_theorem_declarations(path.read_text(encoding="utf-8"))
            if declarations:
                expected_modules.add(module)
                theorem_declarations[module] = declarations

    unreachable_modules = expected_modules - reachable_modules
    if unreachable_modules:
        raise InventoryError(
            f"unreachable Lean theorem modules: {sorted(unreachable_modules)}"
        )

    for entry in roots:
        if not isinstance(entry, dict) or set(entry) != root_keys:
            raise InventoryError("fragment root row has an invalid shape")
        identifier = entry["id"]
        module = entry["module"]
        declaration = entry["declaration"]
        if not isinstance(identifier, str) or not identifier.strip():
            raise InventoryError("fragment root id must be nonempty text")
        if identifier in seen_ids:
            raise InventoryError(f"duplicate fragment root id {identifier}")
        seen_ids.add(identifier)
        if not isinstance(module, str) or not module.startswith("Xsh."):
            raise InventoryError(f"fragment root {identifier} must name an Xsh module")
        path = module_sources.get(module)
        if path is None:
            raise InventoryError(f"fragment root {identifier} names missing module {module}")
        if module not in reachable_modules:
            raise InventoryError(f"unreachable fragment root module {module}")
        if not isinstance(declaration, str) or not declaration.strip():
            raise InventoryError(f"fragment root {identifier} must name a qualified declaration")
        if declaration in seen_declarations:
            raise InventoryError(f"duplicate fragment root declaration {declaration}")
        seen_declarations.add(declaration)
        if declaration not in theorem_declarations.get(module, set()):
            raise InventoryError(
                f"fragment root {identifier} is not a public theorem in {module}: {declaration}"
            )
        if entry["evidence_class"] != "SPEC_PROVED":
            raise InventoryError(f"fragment root {identifier} must use SPEC_PROVED")
        if entry["status"] != "fragment":
            raise InventoryError(f"fragment root {identifier} must remain visibly fragmentary")
        for field in ("scope", "boundary"):
            value = entry[field]
            if not isinstance(value, str) or not value.strip():
                raise InventoryError(f"fragment root {identifier} needs explicit {field} text")
        seen_modules.add(module)

    missing_modules = expected_modules - seen_modules
    if missing_modules:
        raise InventoryError(f"missing fragment roots for Lean modules: {sorted(missing_modules)}")
    return seen_ids


def expected_owner_items(root: Path) -> dict[str, list[str]]:
    expected: dict[str, list[str]] = {}
    arena_path = root / "src/syntax/arena.rs"
    arena_names = discover_public_arena_enums(arena_path.read_text(encoding="utf-8"))
    if arena_names != set(ARENA_ENUMS):
        added = sorted(arena_names - set(ARENA_ENUMS))
        removed = sorted(set(ARENA_ENUMS) - arena_names)
        raise InventoryError(
            f"public arena enum owner layout changed; added={added}, removed={removed}"
        )
    node_path = root / "src/syntax/node.rs"
    node_names = discover_public_enums(node_path.read_text(encoding="utf-8"))
    if node_names != set(NODE_ENUMS):
        added = sorted(node_names - set(NODE_ENUMS))
        removed = sorted(set(NODE_ENUMS) - node_names)
        raise InventoryError(
            f"public syntax enum owner layout changed; added={added}, removed={removed}"
        )
    source_cache: dict[str, str] = {}
    for owner_id, spec in OWNER_SPECS.items():
        source = source_cache.get(spec.source)
        if source is None:
            source = (root / spec.source).read_text(encoding="utf-8")
            source_cache[spec.source] = source
        extractor = EXTRACTORS[spec.extractor]
        try:
            items = (
                extractor(source, spec.declaration)
                if spec.extractor == "rust-enum"
                else extractor(source)
            )
        except (IndexError, ValueError) as exc:
            raise InventoryError(f"unrecognized layout for {owner_id}: {exc}") from exc
        if not items:
            raise InventoryError(f"owner {owner_id} has no inventoried items")
        expected[owner_id] = items
    expected["ir.values"] = reconcile_lowered_value_routes(
        expected["ir.lowered_value"], expected["ir.values"]
    )
    return expected


def compare_owner_items(owner_id: str, inventoried: Any, live: list[str]) -> None:
    if not isinstance(inventoried, list) or not all(isinstance(item, str) for item in inventoried):
        raise InventoryError(f"owner items for {owner_id} must be strings")
    ensure_unique(inventoried, owner_id)
    if inventoried != live:
        missing = sorted(set(live) - set(inventoried))
        stale = sorted(set(inventoried) - set(live))
        raise InventoryError(
            f"owner drift in {owner_id}: missing={missing[:8]}, stale={stale[:8]}"
        )


def validate_obligation_entries(
    plan: str,
    obligations: Any,
    fragment_root_ids: set[str] | None = None,
) -> list[str]:
    required = required_plan_obligations(plan)
    if not isinstance(obligations, list):
        raise InventoryError("obligations must be an array")
    fragment_root_ids = fragment_root_ids or set()
    seen: set[str] = set()
    seen_root_ids: set[str] = set()
    for entry in obligations:
        if not isinstance(entry, dict) or set(entry) != {
            "id",
            "gate",
            "evidence_class",
            "status",
            "proof_root_id",
        }:
            raise InventoryError("obligation row has an invalid shape")
        identifier = entry["id"]
        if not isinstance(identifier, str) or identifier in seen:
            raise InventoryError(f"missing, malformed, or duplicate obligation id {identifier!r}")
        seen.add(identifier)
        root_id = entry["proof_root_id"]
        if not isinstance(root_id, str) or not root_id.strip():
            raise InventoryError(f"theorem obligation {identifier} needs a planned proof-root id")
        if root_id in fragment_root_ids:
            raise InventoryError(
                f"fragment root {root_id} cannot close theorem obligation {identifier}"
            )
        if root_id != f"full.{identifier}" or root_id in seen_root_ids:
            raise InventoryError(
                f"theorem obligation {identifier} has a missing or duplicate full proof-root id"
            )
        seen_root_ids.add(root_id)
        if entry["evidence_class"] != "SPEC_PROVED":
            raise InventoryError(f"theorem obligation {identifier} must use SPEC_PROVED")
        if entry["status"] != "planned":
            raise InventoryError(f"theorem obligation {identifier} must remain visibly planned")
        if entry["gate"] != required.get(identifier):
            raise InventoryError(f"theorem obligation {identifier} is bound to the wrong plan gate")
    if seen != set(required):
        raise InventoryError(
            f"plan obligation entries differ: missing={sorted(set(required) - seen)}, "
            f"unknown={sorted(seen - set(required))}"
        )
    return list(required)


def resolve_git_object(root: Path, revision: str, description: str) -> str:
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "--verify", "--end-of-options", revision],
            capture_output=True,
            check=False,
            text=True,
        )
    except OSError as exc:
        raise InventoryError(f"cannot verify frozen source {description}: {exc}") from exc
    if result.returncode != 0:
        raise InventoryError(f"frozen source {description} does not exist in this repository")
    return result.stdout.strip()


def validate_source_identity(
    root: Path,
    source_state: Any,
    source_identity: Any,
) -> None:
    if source_state == PROVISIONAL_SOURCE_STATE:
        if source_identity is not None:
            raise InventoryError("provisional source state cannot carry a frozen source identity")
        return
    if source_state != FROZEN_SOURCE_STATE:
        raise InventoryError(f"unknown source state {source_state!r}")
    if not isinstance(source_identity, dict) or set(source_identity) != {"commit", "tree"}:
        raise InventoryError("frozen source state requires exact commit and tree identities")
    for field in ("commit", "tree"):
        value = source_identity[field]
        if not isinstance(value, str) or GIT_OBJECT_ID.fullmatch(value) is None:
            raise InventoryError(f"frozen source {field} must be a full lowercase Git object id")
    commit = source_identity["commit"]
    resolved_commit = resolve_git_object(root, f"{commit}^{{commit}}", "commit")
    if resolved_commit != commit:
        raise InventoryError("frozen source commit does not resolve to its exact full object id")
    resolved_tree = resolve_git_object(root, f"{commit}^{{tree}}", "commit tree")
    if source_identity["tree"] != resolved_tree:
        raise InventoryError("frozen source tree does not match the commit root tree")


def validate_case_command(case: dict[str, Any], binary_sha256s: dict[str, Any]) -> None:
    name = case["name"]
    test_id = case["test_id"]
    if not isinstance(test_id, str) or TEST_IDENTIFIER.fullmatch(test_id) is None:
        raise InventoryError(f"finite conformance case {name} needs a stable test identifier")
    command = case["command"]
    if (
        not isinstance(command, list)
        or not command
        or not isinstance(command[0], str)
        or not command[0].strip()
        or any(not isinstance(argument, str) or "\0" in argument for argument in command)
    ):
        raise InventoryError(f"finite conformance case {name} needs an exact argv command")
    runner = Path(command[0]).name
    if runner not in {"xsht", "cargo"}:
        raise InventoryError(f"finite conformance case {name} uses an unsupported evidence runner")

    if runner == "xsht":
        if len(command) != 4 or command[1:3] != ["test", "--exact"]:
            raise InventoryError(f"finite conformance case {name} needs an exact xsht test command")
        if command[3] != test_id:
            raise InventoryError(f"finite conformance case {name} test identifier does not match argv")
        required_roles = {"xsh", "xsht"}
    else:
        common = ["test", "--profile", case["profile"], "--target", case["target"], "-p"]
        if command[1:7] != common or len(command) < 12:
            raise InventoryError(f"finite conformance case {name} needs a supported exact cargo test command")
        package = command[7]
        if not package:
            raise InventoryError(f"finite conformance case {name} needs one explicit Cargo package")
        if command[8] == "--lib" and len(command) == 12:
            test_target = "lib"
            test_index = 9
        elif command[8] == "--test" and len(command) == 13 and command[9]:
            test_target = command[9]
            test_index = 10
        else:
            raise InventoryError(f"finite conformance case {name} needs one Cargo test target")
        if command[test_index + 1 :] != ["--", "--exact"]:
            raise InventoryError(f"finite conformance case {name} needs an exact Rust test filter")
        if command[test_index] != test_id:
            raise InventoryError(f"finite conformance case {name} test identifier does not match argv")
        required_roles = {"cargo", f"cargo-test:{package}:{test_target}"}

    if set(binary_sha256s) != required_roles:
        raise InventoryError(
            f"finite conformance case {name} binary roles must be {sorted(required_roles)}"
        )


def validate_finite_conformance(
    finite_conformance: Any,
    source_state: str,
    source_identity: Any,
) -> None:
    if not isinstance(finite_conformance, dict) or set(finite_conformance) != {
        "evidence_class",
        "status",
        "cases",
    }:
        raise InventoryError("finite conformance has an invalid shape")
    if finite_conformance["evidence_class"] != "RUST_CONFORMANCE":
        raise InventoryError("finite conformance must use RUST_CONFORMANCE")
    cases = finite_conformance["cases"]
    if not isinstance(cases, list):
        raise InventoryError("finite conformance cases must be an array")
    if not cases:
        if finite_conformance["status"] != "planned":
            raise InventoryError("empty finite conformance must remain planned")
        return
    if source_state != FROZEN_SOURCE_STATE or source_identity is None:
        raise InventoryError("finite conformance cases require a frozen source identity")
    if finite_conformance["status"] != "passed":
        raise InventoryError("finite conformance with cases must have passed status")

    case_keys = {
        "name",
        "test_id",
        "evidence_class",
        "binary_sha256s",
        "profile",
        "target",
        "command",
        "expected_result",
        "actual_result",
        "status",
    }
    seen_names: set[str] = set()
    for case in cases:
        if not isinstance(case, dict) or set(case) != case_keys:
            raise InventoryError("finite conformance case has an invalid shape")
        name = case["name"]
        if not isinstance(name, str) or not name.strip():
            raise InventoryError("finite conformance case name must be nonempty text")
        if name in seen_names:
            raise InventoryError(f"duplicate finite conformance case name {name}")
        seen_names.add(name)
        if case["evidence_class"] != "RUST_CONFORMANCE":
            raise InventoryError(f"finite conformance case {name} must use RUST_CONFORMANCE")
        for field in ("profile", "target"):
            value = case[field]
            if not isinstance(value, str) or not value.strip():
                raise InventoryError(f"finite conformance case {name} needs a binary {field}")
        binary_sha256s = case["binary_sha256s"]
        if not isinstance(binary_sha256s, dict) or not binary_sha256s:
            raise InventoryError(f"finite conformance case {name} needs role-keyed binary identities")
        for role, digest in binary_sha256s.items():
            if not isinstance(role, str) or not role.strip():
                raise InventoryError(f"finite conformance case {name} has an empty binary role")
            if not isinstance(digest, str) or SHA256_DIGEST.fullmatch(digest) is None:
                raise InventoryError(f"finite conformance case {name} needs a binary SHA-256 per role")
        validate_case_command(case, binary_sha256s)
        if case["status"] != "passed":
            raise InventoryError(f"finite conformance case {name} must have passed status")
        validate_expected_result(
            case["expected_result"],
            name,
            case["test_id"],
            Path(case["command"][0]).name,
        )
        validate_command_result(case["actual_result"], name, "actual")
        actual = normalized_test_outcome(case)
        if actual != case["expected_result"]:
            raise InventoryError(f"finite conformance case {name} result does not match expectation")


def validate_expected_result(value: Any, case_name: str, test_id: str, runner: str) -> None:
    keys = {"test_id", "exit_code", "passed", "failed", "skipped", "filtered"}
    if not isinstance(value, dict) or set(value) != keys:
        raise InventoryError(
            f"finite conformance case {case_name} expected result must be a normalized test outcome"
        )
    if value["test_id"] != test_id or any(
        type(value[field]) is not int or value[field] < 0
        for field in ("exit_code", "passed", "failed", "skipped")
    ):
        raise InventoryError(
            f"finite conformance case {case_name} expected result has invalid normalized fields"
        )
    if (runner == "xsht" and value["filtered"] is not None) or (
        runner == "cargo"
        and (type(value["filtered"]) is not int or value["filtered"] < 0)
    ):
        raise InventoryError(
            f"finite conformance case {case_name} expected result has invalid filtered count"
        )
    if value != {
        "test_id": test_id,
        "exit_code": 0,
        "passed": 1,
        "failed": 0,
        "skipped": 0,
        "filtered": value["filtered"],
    }:
        raise InventoryError(
            f"finite conformance case {case_name} expected result must require one passing test"
        )


def validate_command_result(value: Any, case_name: str, field: str) -> None:
    if (
        not isinstance(value, dict)
        or set(value) != {"exit_code", "stdout", "stderr"}
        or type(value["exit_code"]) is not int
        or not isinstance(value["stdout"], str)
        or not isinstance(value["stderr"], str)
    ):
        raise InventoryError(
            f"finite conformance case {case_name} {field} result must record exit_code, stdout, and stderr"
        )


def normalized_test_outcome(case: dict[str, Any]) -> dict[str, Any]:
    name = case["name"]
    runner = Path(case["command"][0]).name
    actual_result = case["actual_result"]
    lines = actual_result["stdout"].splitlines()
    content_lines = [line for line in lines if line]
    header_lines = [line for line in lines if re.fullmatch(r"running [0-9]+ tests?", line)]
    summaries = [line for line in lines if line.startswith("test result:")]
    if runner == "xsht":
        result_lines = [line for line in lines if " ... " in line]
        if len(header_lines) != 1 or len(result_lines) != 1 or len(summaries) != 1:
            raise InventoryError(
                f"finite conformance case {name} output must show exactly one passing test"
            )
        if content_lines != [header_lines[0], result_lines[0], summaries[0]]:
            raise InventoryError(f"finite conformance case {name} has unexpected xsht result output")
        header_match = re.fullmatch(r"running ([0-9]+) tests", header_lines[0])
        result_match = re.fullmatch(
            rf"(?P<test_id>.+) \.\.\. (?P<status>ok|FAILED|TIMEOUT|skipped(?: .*)?) "
            rf"(?P<duration>{XSHT_DURATION})",
            result_lines[0],
        )
        summary_match = re.fullmatch(
            r"test result: (?P<result>ok|FAILED)\. (?P<passed>[0-9]+) passed; "
            r"(?P<failed>[0-9]+) failed; (?P<skipped>[0-9]+) skipped",
            summaries[0],
        )
        if header_match is None or result_match is None or summary_match is None:
            raise InventoryError(f"finite conformance case {name} has an invalid xsht result transcript")
        outcome = {
            "test_id": result_match["test_id"],
            "exit_code": actual_result["exit_code"],
            "passed": int(summary_match["passed"]),
            "failed": int(summary_match["failed"]),
            "skipped": int(summary_match["skipped"]),
            "filtered": None,
        }
        status_outcome = {
            "ok": (1, 0, 0),
            "FAILED": (0, 1, 0),
            "TIMEOUT": (0, 1, 0),
        }.get(result_match["status"])
        if result_match["status"].startswith("skipped"):
            status_outcome = (0, 0, 1)
        if (
            int(header_match[1]) != 1
            or (outcome["passed"], outcome["failed"], outcome["skipped"]) != (1, 0, 0)
            or summary_match["result"] != ("ok" if outcome["failed"] == 0 else "FAILED")
            or status_outcome != (outcome["passed"], outcome["failed"], outcome["skipped"])
        ):
            raise InventoryError(
                f"finite conformance case {name} output must show exactly one passing test"
            )
        return outcome

    result_lines = [line for line in lines if line.startswith("test ") and " ... " in line]
    if len(header_lines) != 1 or len(result_lines) != 1 or len(summaries) != 1:
        raise InventoryError(f"finite conformance case {name} output must show exactly one passing test")
    if content_lines != [header_lines[0], result_lines[0], summaries[0]]:
        raise InventoryError(f"finite conformance case {name} has unexpected cargo test output")
    header_match = re.fullmatch(r"running ([0-9]+) tests?", header_lines[0])
    result_match = re.fullmatch(r"test (?P<test_id>.+) \.\.\. (?P<status>ok|FAILED|ignored)", result_lines[0])
    summary_match = re.fullmatch(
        r"test result: (?P<result>ok|FAILED)\. (?P<passed>[0-9]+) passed; "
        r"(?P<failed>[0-9]+) failed; (?P<ignored>[0-9]+) ignored; "
        r"(?P<measured>[0-9]+) measured; (?P<filtered>[0-9]+) filtered out; "
        rf"finished in (?P<duration>{CARGO_DURATION})",
        summaries[0],
    )
    if header_match is None or result_match is None or summary_match is None:
        raise InventoryError(f"finite conformance case {name} has an invalid cargo test result transcript")
    outcome = {
        "test_id": result_match["test_id"],
        "exit_code": actual_result["exit_code"],
        "passed": int(summary_match["passed"]),
        "failed": int(summary_match["failed"]),
        "skipped": int(summary_match["ignored"]),
        "filtered": int(summary_match["filtered"]),
    }
    status_outcome = {"ok": (1, 0, 0), "FAILED": (0, 1, 0), "ignored": (0, 0, 1)}[
        result_match["status"]
    ]
    if (
        int(header_match[1]) != 1
        or int(summary_match["measured"]) != 0
        or (outcome["passed"], outcome["failed"], outcome["skipped"]) != (1, 0, 0)
        or summary_match["result"] != ("ok" if outcome["failed"] == 0 else "FAILED")
        or status_outcome != (outcome["passed"], outcome["failed"], outcome["skipped"])
    ):
        raise InventoryError(
            f"finite conformance case {name} output must show exactly one passing test"
        )
    return outcome


def validate_coverage(root: Path, coverage: Any) -> dict[str, Any]:
    if not isinstance(coverage, dict):
        raise InventoryError("coverage root must be an object")
    required_root_keys = {
        "schema",
        "source_state",
        "source_identity",
        "scope",
        "core_inventory",
        "fragment_roots",
        "universal_theorems",
        "finite_conformance",
        "evidence_classes",
    }
    if set(coverage) != required_root_keys:
        raise InventoryError(
            f"coverage root keys differ: missing={sorted(required_root_keys - set(coverage))}, "
            f"unknown={sorted(set(coverage) - required_root_keys)}"
        )
    if type(coverage["schema"]) is not int or coverage["schema"] != 2:
        raise InventoryError("unsupported coverage schema")
    validate_source_identity(root, coverage["source_state"], coverage["source_identity"])
    if not isinstance(coverage["scope"], str) or not coverage["scope"].strip():
        raise InventoryError("scope must state the inventory denominator")

    owners = coverage["core_inventory"]
    if not isinstance(owners, dict):
        raise InventoryError("owners must be an object")
    if set(owners) != set(OWNER_SPECS):
        raise InventoryError(
            f"owner entries differ: missing={sorted(set(OWNER_SPECS) - set(owners))}, "
            f"unknown={sorted(set(owners) - set(OWNER_SPECS))}"
        )
    live = expected_owner_items(root)
    for owner_id, spec in OWNER_SPECS.items():
        entry = owners[owner_id]
        expected_keys = {"source", "declaration", "extractor", "items"}
        if spec.derivation is not None or spec.boundary is not None:
            expected_keys.update({"derivation", "boundary"})
        if not isinstance(entry, dict) or set(entry) != expected_keys:
            raise InventoryError(f"owner entry {owner_id} has an invalid shape")
        if entry["source"] != spec.source or entry["declaration"] != spec.declaration or entry["extractor"] != spec.extractor:
            raise InventoryError(f"owner entry {owner_id} does not match its canonical owner")
        if spec.derivation is not None and entry["derivation"] != spec.derivation:
            raise InventoryError(f"owner entry {owner_id} has stale derivation metadata")
        if spec.boundary is not None and entry["boundary"] != spec.boundary:
            raise InventoryError(f"owner entry {owner_id} has stale boundary metadata")
        compare_owner_items(owner_id, entry["items"], live[owner_id])

    classes = coverage["evidence_classes"]
    class_names = {"SPEC_PROVED", "RUST_CONFORMANCE", "ARTIFACT_CERTIFIED", "EXTERNAL_ASSUMPTION"}
    if not isinstance(classes, list) or len(classes) != len(class_names):
        raise InventoryError("evidence_classes must name the four campaign evidence classes")
    class_map: dict[str, dict[str, Any]] = {}
    for entry in classes:
        if not isinstance(entry, dict) or set(entry) != {"class", "status"}:
            raise InventoryError("evidence class row has an invalid shape")
        name = entry["class"]
        if not isinstance(name, str) or name not in class_names or name in class_map:
            raise InventoryError(f"unknown or duplicate evidence class {name!r}")
        if entry["status"] != "planned":
            raise InventoryError(f"evidence class {name} must remain visibly planned")
        class_map[name] = entry
    if set(class_map) != class_names:
        raise InventoryError("one or more campaign evidence classes are missing")

    plan_path = root / PLAN_PATH
    try:
        plan = plan_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise InventoryError(f"cannot read formal plan {plan_path}: {exc}") from exc
    module_sources = lean_module_sources(root / "formal")
    reachable_modules = reachable_lean_modules(module_sources)
    fragment_root_ids = validate_fragment_roots(
        coverage["fragment_roots"], module_sources, reachable_modules
    )
    obligation_ids = validate_obligation_entries(
        plan, coverage["universal_theorems"], fragment_root_ids
    )
    validate_finite_conformance(
        coverage["finite_conformance"],
        coverage["source_state"],
        coverage["source_identity"],
    )
    return {
        "owners": live,
        "fragment_roots": fragment_root_ids,
        "obligations": obligation_ids,
    }


def main(argv: list[str] | None = None) -> int:
    script_root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=script_root)
    parser.add_argument("--coverage", type=Path)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    coverage_path = args.coverage or root / "formal/coverage.json"
    if not coverage_path.is_absolute():
        coverage_path = root / coverage_path
    try:
        result = validate_coverage(root, load_json(coverage_path))
    except (InventoryError, OSError) as exc:
        print(f"inventory error: {exc}", file=sys.stderr)
        return 1
    print(
        f"inventory ok: {len(result['owners'])} owner sets, "
        f"{len(result['fragment_roots'])} Lean fragment roots, "
        f"{len(result['obligations'])} planned theorem obligations"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
