#!/usr/bin/env python3
"""Check the provisional formal inventory against its named Rust owners."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Any, Iterable


PLAN_PATH = "xsh-lean.md"


@dataclass(frozen=True)
class OwnerSpec:
    source: str
    declaration: str
    extractor: str


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
    "ir.values": OwnerSpec(
        "src/runtime/eval/indexed/full.rs",
        "lowered_value_schema!",
        "lowered-value-schema",
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


def extract_enum(source: str, enum_name: str) -> list[str]:
    tokens = lex_rust(source)
    found: list[int] = []
    for index in range(len(tokens) - 1):
        if tokens[index] == "enum" and tokens[index + 1] == enum_name:
            found.append(index)
    if len(found) != 1:
        raise InventoryError(
            f"expected one enum {enum_name}, found {len(found)}"
        )
    index = found[0] + 2
    while index < len(tokens) and tokens[index] != "{":
        if tokens[index] in (";", "}"):
            raise InventoryError(f"unsupported enum declaration for {enum_name}")
        index += 1
    if index == len(tokens):
        raise InventoryError(f"enum body missing for {enum_name}")
    end = matching(tokens, index)
    names: list[str] = []
    index += 1
    while index < end:
        index = skip_attributes(tokens, index, end)
        if index >= end:
            break
        name = tokens[index]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
            raise InventoryError(f"unrecognized {enum_name} variant at token {name!r}")
        item = name
        index += 1
        if index < end and tokens[index] == "(":
            tuple_end = matching(tokens, index)
            fields = split_top_level(tokens[index + 1 : tuple_end])
            item += f"({','.join(compact_tokens(field) for field in fields)})"
            index = tuple_end + 1
        elif index < end and tokens[index] == "{":
            fields, index = parse_field_group(tokens, index, "{")
            item += f"{{{','.join(fields)}}}"
        if index < end and tokens[index] == "=":
            index += 1
            value_start = index
            while index < end and tokens[index] != ",":
                if tokens[index] in ("(", "[", "{"):
                    index = matching(tokens, index) + 1
                else:
                    index += 1
            item += f"={compact_tokens(tokens[value_start:index])}"
        names.append(item)
        if index < end:
            if tokens[index] != ",":
                raise InventoryError(f"unrecognized {enum_name} variant tail {tokens[index]!r}")
            index += 1
    return ensure_unique(names, enum_name)


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
            override = compact_tokens(tail[pivot + 1 :])
            tail = tail[:pivot]
        if not tail:
            raise InventoryError(f"field {name} has no type")
        encoded = f"{name}:{compact_tokens(tail)}"
        if override:
            encoded += f"=>{override}"
        fields.append(encoded)
    return fields, end + 1


def compact_tokens(tokens: Iterable[str]) -> str:
    return "".join(tokens)


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
            effects = compact_tokens(meta[0])
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
        type_expression = compact_tokens(tuple_parts[0])
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
            fields[raw[0]] = compact_tokens(raw[colon + 1 :])
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
        fields, _ = parse_field_group(tokens, fields_open, "{")
        rows.append(f"{tag}={ordinal}|{','.join(fields)}")
        cursor = row_end + 1

    cursor = aliases_open + 1
    while cursor < aliases_end:
        arrow = find_top_level(tokens, cursor, aliases_end, "=>")
        pattern = compact_tokens(tokens[cursor:arrow])
        tag = tokens[arrow + 1]
        if tag not in seen_tags:
            raise InventoryError(f"lowered value alias {pattern} names unknown tag {tag}")
        fields_key = tokens.index("fields", arrow + 2, aliases_end)
        fields_open = tokens.index("{", fields_key + 1, aliases_end)
        fields_end = matching(tokens, fields_open)
        fields = [compact_tokens(part) for part in split_top_level(tokens[fields_open + 1 : fields_end])]
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
        rows.append(f"{tag}={ordinal}|custom")
        cursor += 3
        if tokens[cursor] != "{":
            raise InventoryError(f"custom lowered value row {tag} body missing")
        cursor = matching(tokens, cursor) + 1

    cursor = reject_open + 1
    while cursor < reject_end:
        arrow = find_top_level(tokens, cursor, reject_end, "=>")
        for pattern in split_top_level(tokens[cursor:arrow], "|"):
            rows.append(f"reject:{compact_tokens(pattern)}")
        cursor = arrow + 1
        if tokens[cursor] != "{":
            raise InventoryError("lowered value reject row has no body")
        cursor = matching(tokens, cursor) + 1
        if cursor < reject_end:
            if tokens[cursor] != ",":
                raise InventoryError("lowered value reject rows require commas")
            cursor += 1
    return ensure_unique(rows, "lowered value schema")


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


def validate_obligation_entries(plan: str, obligations: Any) -> list[str]:
    required = required_plan_obligations(plan)
    if not isinstance(obligations, list):
        raise InventoryError("obligations must be an array")
    seen: set[str] = set()
    for entry in obligations:
        if not isinstance(entry, dict) or set(entry) != {"id", "gate", "evidence_class", "status"}:
            raise InventoryError("obligation row has an invalid shape")
        identifier = entry["id"]
        if not isinstance(identifier, str) or identifier in seen:
            raise InventoryError(f"missing, malformed, or duplicate obligation id {identifier!r}")
        seen.add(identifier)
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


def validate_coverage(root: Path, coverage: Any) -> dict[str, Any]:
    if not isinstance(coverage, dict):
        raise InventoryError("coverage root must be an object")
    required_root_keys = {
        "schema",
        "source_state",
        "scope",
        "core_inventory",
        "universal_theorems",
        "finite_conformance",
        "evidence_classes",
    }
    if set(coverage) != required_root_keys:
        raise InventoryError(
            f"coverage root keys differ: missing={sorted(required_root_keys - set(coverage))}, "
            f"unknown={sorted(set(coverage) - required_root_keys)}"
        )
    if type(coverage["schema"]) is not int or coverage["schema"] != 1:
        raise InventoryError("unsupported coverage schema")
    if coverage["source_state"] != "provisional-during-consolidation":
        raise InventoryError("post-consolidation source inventory still requires reconciliation")
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
        if not isinstance(entry, dict) or set(entry) != {"source", "declaration", "extractor", "items"}:
            raise InventoryError(f"owner entry {owner_id} has an invalid shape")
        if entry["source"] != spec.source or entry["declaration"] != spec.declaration or entry["extractor"] != spec.extractor:
            raise InventoryError(f"owner entry {owner_id} does not match its canonical owner")
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
    obligation_ids = validate_obligation_entries(plan, coverage["universal_theorems"])
    finite_conformance = coverage["finite_conformance"]
    if (
        not isinstance(finite_conformance, dict)
        or set(finite_conformance) != {"evidence_class", "status", "cases"}
        or finite_conformance["evidence_class"] != "RUST_CONFORMANCE"
        or finite_conformance["status"] != "planned"
        or finite_conformance["cases"] != []
    ):
        raise InventoryError("finite conformance must remain a separate planned, empty case set")
    return {"owners": live, "obligations": obligation_ids}


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
        f"{len(result['obligations'])} planned theorem obligations"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
