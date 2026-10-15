#!/usr/bin/env python3
"""Generate a source-bound inventory of instruction routes and oracle IDs."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
from typing import Any


HERE = pathlib.Path(__file__).resolve().parent
CONFIG = HERE.parent / "semantic-route-matrix" / "evidence.json"
SOURCE_LOCK_RELATIVE = pathlib.Path(".work/final-review-evidence/semantic-route-matrix/source-lock.json")
OUTPUT_RELATIVE = pathlib.Path(".work/final-review-evidence/semantic-route-matrix-eager-atom/matrix.json")
AST_EXTRACTOR_MANIFEST = HERE / "ast-extractor/Cargo.toml"
CARGO_EXE = pathlib.Path.home() / ".cargo/bin/cargo"
SCHEMA_PATH = pathlib.Path("src/runtime/eval/indexed/full/schema.rs")
ARENA_PATH = pathlib.Path("src/syntax/arena.rs")
PIPELINE_STAGE_SOURCE_PATH = pathlib.Path("src/runtime/eval/indexed/full.rs")
LOWERED_RUN_SOURCE_PATH = pathlib.Path("src/runtime/eval/lowered_run.rs")
TYPED_SCALAR_VALUES_PATH = pathlib.Path("src/runtime/eval/lowered_run/indexed_run/values.rs")
OPERANDS_HELPER_PATH = pathlib.Path("src/runtime/eval/lowered_run/indexed_run/operands.rs")
OPERAND_MANIFEST = HERE.parent / "semantic-route-matrix" / "operand-order-manifest.tsv"
ORDINAL_HOLES = [50, 64, 95, 97, 98, 147]
EXPECTED_LIVE_COUNT = 152
EAGER_ATOM_PROVENANCE = {
    "Null": ("ExprNull", "none", "construct", "LoweredValue::Null"),
    "Unit": ("ExprUnit", "none", "construct", "LoweredValue::Unit"),
    "Int": ("ExprInt", "value", "copy", "LoweredValue::Int"),
    "Float": ("ExprFloat", "value", "copy", "LoweredValue::Float"),
    "Duration": ("ExprDuration", "value", "copy", "LoweredValue::Duration"),
    "Bool": ("ExprBool", "value", "copy", "LoweredValue::Bool"),
    "Str": ("ExprStr", "value", "copy_borrowed_string", "LoweredValue::Str"),
    "Bytes": ("ExprBytes", "value", "copy_borrowed_bytes", "LoweredValue::Bytes"),
    "PreparedRegex": ("ExprPreparedRegex", "value", "clone_box", "LoweredValue::Regex"),
    "PreparedConstant": ("ExprPreparedConstant", "value", "clone_prepared_value", "prepared_constant.value"),
    "Path": ("ExprPath", "value", "validate_path_bytes", "LoweredValue::Path"),
    "Param": ("ExprParam", "slot", "freeze_slot_then_clone", "slots[slot]"),
}
OPERAND_POLICIES = {"strict", "short_circuit", "selected", "iterative", "not_applicable"}
FRAME_CONSTRUCTOR_ROWS = {
    "ExprFunctionRef": ("FunctionRef", 29),
    "ExprPathFrom": ("PathFrom", 30),
    "ExprEmptyMap": ("EmptyMap", 49),
}
COLLECTION_CONSTRUCTION_ROWS = {
    "ExprRecord": ("Record", 44, "FrameControl"),
    "ExprMapLiteral": ("MapLiteral", 45, "FrameControl"),
    "ExprRecordUpdate": ("RecordUpdate", 46, "FrameControl"),
    "ExprList": ("List", 47, "FrameApply"),
    "ExprListBuild": ("ListBuild", 48, "FrameApply"),
}
RESULT_CONSTRUCTOR_ROWS = {
    "ExprOk": ("Ok", 104),
    "ExprErr": ("Err", 105),
}
CALL_FRAME_ROWS = {
    "ExprCall": ("Call", 108, "EFFECT_TRACE"),
    "ExprDirectPureCall": ("DirectPureCall", 109, "EFFECT_TRACE"),
    "ExprExternalCall": ("ExternalCall", 110, "EFFECT_TRACE"),
    "ExprDynamicCall": ("DynamicCall", 111, "EFFECT_DYNAMIC_CALL | EFFECT_TRACE"),
}
FIELD_INDEX_ROWS = {
    "ExprField": ("Field", 56),
    "ExprIndex": ("Index", 57),
    "ExprIndexFromEnd": ("IndexFromEnd", 58),
}

CUSTOM_CHILD_PATHS = {
    "LoweredAssignPath": ["[*].Index.index"],
    "LoweredCallArg": [".Single.value", ".Splice.value"],
    "LoweredCompQualifiers": ["[*].For.iter", "[*].If.condition"],
    "LoweredErrorExpr": [".Structured.fields[*].value"],
    "LoweredFmtPart": [".Expr.value"],
    "LoweredProcessCommandArgv": [
        ".target", ".argv", ".cwd?", ".env?", ".stdin?", ".stdout?",
        ".stderr?", ".stdout_append?", ".stderr_append?", ".timeout?",
        ".detach?", ".new_session?", ".ignore_hup?", ".cpu_max?",
        ".accept?", ".same_group?",
    ],
    "LoweredProcessCommandBuilderEntry": [
        ".Field.value", ".Run.target.value", ".Run.args[*].value",
        ".Run.env[*].value.value", ".Run.timeout?", ".Run.cpu_max?",
        ".Run.accept?",
    ],
    "LoweredRecordEntry": [".Field.value", ".Spread.value"],
    "LoweredRecordUpdates": ["[*].value"],
    "LoweredRunCapture": [
        ".target.value", ".args[*].value", ".env[*].value.value",
        ".redirections[*].target.value", ".timeout?", ".cpu_max?", ".accept?",
    ],
    "LoweredRunEnv": [".value.value"],
    "LoweredRunPipelineSegment": [
        ".target.value", ".args[*].value", ".env[*].value.value",
        ".redirections[*].target.value", ".timeout?", ".cpu_max?", ".accept?",
    ],
    "LoweredSpawnRun": [
        ".target.value", ".args[*].value", ".env[*].value.value",
        ".redirections[*].target.value", ".timeout?", ".cpu_max?", ".accept?",
    ],
}

# Unit variants have no child paths, so checking the closed set also makes new
# pipeline stage variants visible even when they add no evaluator children.
PIPELINE_STAGE_VARIANTS = {
    "TextLines", "JsonLines", "Where", "WhereBlock", "Map", "MapBlock",
    "FlatMap", "FlatMapBlock", "BytesChunks", "BatchCount", "BatchMaxArgv",
    "BatchMaxBytes", "BatchLimits", "Shuffle", "Fold", "ReduceBy",
    "ReduceByConfigured", "ParMap", "ParMapBlock", "ParMapFlatMapReduceBy",
    "Tee", "Each", "TablePrint", "TablePrintConfigured", "Enumerate", "Zip",
    "Sort", "SortBy", "GroupBy", "CountBy", "Any", "AnyBlock", "All",
    "AllBlock", "UniqueBy", "Count", "Sum", "Collect", "First", "Last",
    "Min", "Max", "Take", "Drop", "Repeat", "Range",
}

LEAF_SCHEMA_TYPES = {
    "Arc", "AssignOp", "BinaryOp", "BuildBoolId", "BuildExprId", "BuildIntId",
    "BuildPatternId", "BuildStmtId", "DurationValue", "FloatValue", "FunctionName",
    "LoweredCompTarget", "LoweredFunctionKey", "LoweredStrPredicate", "LoweredType",
    "LoweredTypeCheck", "LoweredValue", "Name", "NameText", "PathValue",
    "PreparedConstantValue", "QualifiedName", "RegexValue", "ReduceByOp", "RuntimeOp",
    "ScanBytes", "ScanCheck", "Span", "String", "Type", "bool", "i64", "str",
    "usize", "[u8]",
    "crate::runtime::eval::EndDistance", "crate::runtime::eval::ModuleCallPlan",
    "crate::sema::wire_enums::WireEnumMapping", "crate::syntax::arena::ContextScopeKind",
    "crate::syntax::arena::RetrySchedule",
}

ARENA_SHAPE_CONTRACTS = {
    "expr_binary": (
        "ArenaExprKind::Binary { left, right, .. } => { "
        "visit(Child::Expr(left)); visit(Child::Expr(right)); }",
        "for_each_expr_child",
    ),
    "expr_comparison_chain": (
        "ArenaExprKind::ComparisonChain(pairs) => { "
        "for expr in self.comparison_chain_operands(pairs) { visit(Child::Expr(expr)); } }",
        "for_each_expr_child",
    ),
    "expr_if": (
        "ArenaExprKind::If { branches, else_value } => { "
        "for branch in self.if_expr_branches(branches) { "
        "visit(Child::Expr(branch.condition)); visit(Child::Expr(branch.value)); } "
        "visit(Child::Expr(else_value)); }",
        "for_each_expr_child",
    ),
    "expr_list": (
        "ArenaExprKind::List(items) | ArenaExprKind::Set(items) => { "
        "for expr in self.list_element_exprs(items) { visit(Child::Expr(expr)); } }",
        "for_each_expr_child",
    ),
    "expr_match": (
        "ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms } "
        "| ArenaExprKind::PatternCondition { value, arms } => { visit(Child::Expr(value)); "
        "for arm in self.match_expr_arms(arms) { visit(Child::Pattern(arm.pattern)); "
        "if let Some(guard) = arm.guard { visit(Child::Expr(guard)); } "
        "visit(Child::Expr(arm.value)); } }",
        "for_each_expr_child",
    ),
    "expr_pipeline": (
        "ArenaExprKind::Pipeline { input, stages } => { visit(Child::Expr(input)); "
        "for stage in self.pipe_stages(stages) { match &stage.kind { "
        "ArenaPipeStageKind::Expr(expr) => visit(Child::Expr(*expr)), "
        "ArenaPipeStageKind::Stream(stage) => self.for_each_stream_child(stage, &mut visit), } } }",
        "for_each_expr_child",
    ),
    "stmt_if": (
        "ArenaStmtKind::If { branches, else_block } => { "
        "for branch in self.if_branches(branches) { visit(Child::Expr(branch.condition)); "
        "visit(Child::Block(branch.block)); } "
        "if let Some(block) = else_block { visit(Child::Block(block)); } }",
        "for_each_stmt_child",
    ),
    "stmt_match": (
        "ArenaStmtKind::Match { value, arms } => { visit(Child::Expr(value)); "
        "for arm in self.match_arms(arms) { visit(Child::Pattern(arm.pattern)); "
        "if let Some(guard) = arm.guard { visit(Child::Expr(guard)); } "
        "visit(Child::Block(arm.block)); } }",
        "for_each_stmt_child",
    ),
}


class EvidenceError(RuntimeError):
    pass


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: pathlib.Path) -> str:
    return sha256_bytes(path.read_bytes())


def git_value(root: pathlib.Path, *args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=root, check=True, text=True, capture_output=True
    ).stdout.strip()


def source_snapshot(root: pathlib.Path, source_paths: list[str]) -> dict[str, Any]:
    status = git_value(root, "status", "--porcelain=v1", "--untracked-files=all")
    return {
        "head": git_value(root, "rev-parse", "HEAD"),
        "tree": git_value(root, "rev-parse", "HEAD^{tree}"),
        "clean": status == "",
        "status": status,
        "files": {
            name: sha256_file(root / name)
            for name in sorted(set(source_paths))
        },
    }


def source_lock_status(actual: dict[str, Any], lock: dict[str, Any]) -> dict[str, Any]:
    locked_files = lock.get("files", {})
    actual_files = actual["files"]
    file_mismatches = sorted(
        name for name, digest in locked_files.items() if actual_files.get(name) != digest
    )
    missing_locked_files = sorted(set(locked_files) - set(actual_files))
    unsealed_files = sorted(set(actual_files) - set(locked_files))
    head_matches = actual["head"] == lock.get("head")
    tree_matches = actual["tree"] == lock.get("tree")
    matches = (
        head_matches
        and tree_matches
        and not file_mismatches
        and not missing_locked_files
        and not unsealed_files
    )
    return {
        "status": "matched" if matches else "stale",
        "expected_commit": lock.get("head"),
        "expected_tree": lock.get("tree"),
        "commit_matches": head_matches,
        "tree_matches": tree_matches,
        "file_mismatches": file_mismatches,
        "missing_locked_files": missing_locked_files,
        "unsealed_files": unsealed_files,
    }


def committed_xsh_test_ids(
    root: pathlib.Path, commit: str, test_files: list[str]
) -> set[str]:
    ids: set[str] = set()
    for relative in sorted(set(test_files)):
        try:
            source = subprocess.run(
                ["git", "show", f"{commit}:{relative}"],
                cwd=root,
                check=True,
                text=True,
                capture_output=True,
            ).stdout
        except subprocess.CalledProcessError as error:
            raise EvidenceError(
                f"candidate oracle source is absent at {commit}:{relative}"
            ) from error
        for name in re.findall(r"(?m)^\s*test\s+([A-Za-z][A-Za-z0-9_]*)\b", source):
            ids.add(f"{relative}::{name}")
    return ids


def parse_schema(source: str) -> list[dict[str, Any]]:
    try:
        body = source.split("instruction_schema! {", 1)[1].split(
            "\n}\n\npattern_schema!", 1
        )[0]
    except IndexError as error:
        raise EvidenceError("could not isolate instruction_schema! rows") from error

    rows: list[dict[str, Any]] = []
    current_kind: str | None = None
    current: list[str] | None = None
    current_variant: str | None = None

    def finish() -> None:
        nonlocal current, current_variant
        if current is None or current_variant is None or current_kind is None:
            raise EvidenceError("instruction row ended without a row header")
        raw = "\n".join(current)
        route = re.search(
            r"=>\s*(?P<tag>[A-Za-z][A-Za-z0-9_]*)\s*=\s*(?P<ordinal>\d+)\s*"
            r"\[(?P<effects>[^;]+);\s*(?P<class>Operands|Scheduled);\s*"
            r"(?P<strategy>FrameApply|FrameControl)(?:;\s*(?P<classification>EagerAtom))?\];",
            raw,
            re.S,
        )
        if route is None:
            raise EvidenceError(f"could not parse route metadata for {current_variant}")
        rows.append(
            {
                "variant": current_variant,
                "tag": route.group("tag"),
                "ordinal": int(route.group("ordinal")),
                "kind": current_kind,
                "effects": " ".join(route.group("effects").split()),
                "in_place": route.group("class"),
                "frame_strategy": route.group("strategy"),
                "eager_atom": route.group("classification") == "EagerAtom",
                "schema_row": raw.strip(),
                "schema_fields": schema_fields(raw, current_variant),
            }
        )
        current = None
        current_variant = None

    for line in body.splitlines():
        group = re.match(r"^ {4}\w+ => \w+ \[(Int|Bool|Expr|Stmt)\] \{$", line)
        if group:
            if current is not None:
                raise EvidenceError("row group began before previous instruction ended")
            current_kind = group.group(1)
            continue

        if current is None:
            if len(line) - len(line.lstrip(" ")) == 8:
                row_start = re.match(r"^ {8}([A-Z][A-Za-z0-9_]*)\b", line)
                if row_start:
                    current = [line]
                    current_variant = row_start.group(1)
                    if line.rstrip().endswith("];"):
                        finish()
            continue

        current.append(line)
        if line.rstrip().endswith("];" ):
            finish()

    if current is not None:
        raise EvidenceError(f"unterminated instruction row {current_variant}")
    if not rows:
        raise EvidenceError("no instruction rows were parsed")
    tags = [row["tag"] for row in rows]
    ordinals = [row["ordinal"] for row in rows]
    if len(tags) != len(set(tags)):
        duplicates = sorted(tag for tag in set(tags) if tags.count(tag) > 1)
        raise EvidenceError(f"duplicate instruction tags: {duplicates}")
    if len(ordinals) != len(set(ordinals)):
        duplicates = sorted(number for number in set(ordinals) if ordinals.count(number) > 1)
        raise EvidenceError(f"duplicate instruction ordinals: {duplicates}")
    return rows


def split_top_level_fields(text: str) -> list[str]:
    fields: list[str] = []
    start = 0
    depth = {"<": 0, "(": 0, "[": 0, "{": 0}
    closing = {">": "<", ")": "(", "]": "[", "}": "{"}
    for index, char in enumerate(text):
        if char in depth:
            depth[char] += 1
        elif char in closing:
            depth[closing[char]] -= 1
        elif char == "," and not any(depth.values()):
            fields.append(text[start:index].strip())
            start = index + 1
    tail = text[start:].strip()
    if tail:
        fields.append(tail)
    return fields


def schema_fields(raw: str, variant: str) -> list[dict[str, str]]:
    """Return payload fields in schema order; this does not claim runtime order."""
    code = "\n".join(line.split("//", 1)[0] for line in raw.splitlines())
    header = re.search(rf"\b{re.escape(variant)}\b", code)
    route = re.search(r"=>\s*[A-Za-z][A-Za-z0-9_]*\s*=", code)
    if header is None or route is None or header.end() > route.start():
        raise EvidenceError(f"could not isolate payload fields for {variant}")
    payload = code[header.end():route.start()].strip()
    if not payload:
        return []
    opener = payload[0]
    if opener not in "({":
        raise EvidenceError(f"unknown payload form for {variant}: {payload}")
    closer = ")" if opener == "(" else "}"
    if not payload.endswith(closer):
        raise EvidenceError(f"unclosed payload for {variant}")
    fields: list[dict[str, str]] = []
    for item in split_top_level_fields(payload[1:-1]):
        pair = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.+)$", item, re.S)
        if pair is None:
            raise EvidenceError(f"could not parse field in {variant}: {item}")
        fields.append({"name": pair.group(1), "type": " ".join(pair.group(2).split())})
    return fields


def schema_child_paths(
    field_type: str,
    prefix: str,
    pipeline_stage_paths: list[str] | None = None,
) -> list[str]:
    """Project schema field types to possible evaluator-child paths only."""
    value = field_type.strip()
    if value in {"BuildExprId", "BuildStmtId", "BuildIntId", "BuildBoolId", "BuildPatternId"}:
        return [prefix]
    if value == "LoweredPipelineStage":
        if pipeline_stage_paths is None:
            raise EvidenceError("missing LoweredPipelineStage child inventory")
        return [prefix + path for path in pipeline_stage_paths]

    if value.startswith("(") and value.endswith(")"):
        items = split_top_level_fields(value[1:-1])
        return [
            path
            for index, item in enumerate(items)
            for path in schema_child_paths(item, f"{prefix}[{index}]", pipeline_stage_paths)
        ]

    generic = re.fullmatch(r"([A-Za-z_][A-Za-z0-9_:]*)\s*<(.*)>", value, re.S)
    if generic is not None:
        name = generic.group(1)
        args = split_top_level_fields(generic.group(2))
        if name in {"Option", "Box", "Arc"} and len(args) == 1:
            suffix = "?" if name == "Option" else ""
            return schema_child_paths(args[0], prefix + suffix, pipeline_stage_paths)
        if name == "Vec" and len(args) == 1:
            return schema_child_paths(args[0], prefix + "[*]", pipeline_stage_paths)
        if name == "FxHashMap" and len(args) == 2:
            keys = schema_child_paths(args[0], prefix + ".keys[*]", pipeline_stage_paths)
            values = schema_child_paths(args[1], prefix + ".values[*]", pipeline_stage_paths)
            return keys + values
        raise EvidenceError(f"unknown schema child container: {field_type}")

    custom_paths = CUSTOM_CHILD_PATHS.get(value)
    if custom_paths is not None:
        return [prefix + suffix for suffix in custom_paths]
    if value in LEAF_SCHEMA_TYPES:
        return []
    raise EvidenceError(f"unclassified schema payload type: {field_type}")


def mask_rust_non_code(source: str) -> str:
    """Blank comments and literals while preserving source offsets and newlines."""
    masked = list(source)
    raw_string_prefix = re.compile(r"(?:br|r)(?P<hashes>#{0,})\"")

    def blank(start: int, end: int) -> None:
        for index in range(start, end):
            if source[index] != "\n":
                masked[index] = " "

    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index)
            if end < 0:
                end = len(source)
            blank(index, end)
            index = end
            continue
        if source.startswith("/*", index):
            start = index
            depth = 1
            index += 2
            while index < len(source) and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            if depth:
                raise EvidenceError("unterminated Rust block comment")
            blank(start, index)
            continue

        raw = (
            raw_string_prefix.match(source, index)
            if source[index] in {"b", "r"}
            else None
        )
        if raw is not None:
            start = index
            closing = '"' + raw.group("hashes")
            content_start = raw.end()
            end = source.find(closing, content_start)
            if end < 0:
                raise EvidenceError("unterminated Rust raw string")
            index = end + len(closing)
            blank(start, index)
            continue

        if source[index] == '"':
            start = index
            index += 1
            while index < len(source):
                if source[index] == "\\":
                    index += 2
                elif source[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            else:
                raise EvidenceError("unterminated Rust string literal")
            blank(start, index)
            continue

        if source[index] == "'":
            character = re.match(
                r"'(?:\\(?:[0-7]{1,3}|x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f_]+\}|[^\n])|[^\\'\n])'",
                source[index:],
            )
            if character is not None:
                end = index + character.end()
                blank(index, end)
                index = end
                continue
        index += 1
    return "".join(masked)


def matching_delimiter(source: str, opening: int, opener: str, closer: str) -> int:
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == opener:
            depth += 1
        elif source[index] == closer:
            depth -= 1
            if depth == 0:
                return index
    raise EvidenceError(f"unclosed Rust delimiter {opener}")


def pipeline_stage_child_inventory(source: str) -> dict[str, Any]:
    code = mask_rust_non_code(source)
    matches = list(re.finditer(r"\bpipeline_stage_schema!\s*\{\s*@context\b", code))
    if len(matches) != 1:
        raise EvidenceError("expected one LoweredPipelineStage schema invocation")
    opening = code.find("{", matches[0].start())
    closing = matching_delimiter(code, opening, "{", "}")
    body = code[opening + 1:closing]
    context_end = body.find(";")
    if context_end < 0:
        raise EvidenceError("LoweredPipelineStage schema has no context terminator")

    variants: set[str] = set()
    child_paths: list[str] = []
    cursor = context_end + 1
    while True:
        while cursor < len(body) and body[cursor].isspace():
            cursor += 1
        if cursor == len(body):
            break
        name_match = re.match(r"[A-Za-z_][A-Za-z0-9_]*", body[cursor:])
        if name_match is None:
            raise EvidenceError("unrecognized LoweredPipelineStage variant declaration")
        name = name_match.group(0)
        cursor += name_match.end()
        while cursor < len(body) and body[cursor].isspace():
            cursor += 1
        shape_match = re.match(r"(unit|struct|tuple)\b", body[cursor:])
        if shape_match is None:
            raise EvidenceError(f"unrecognized LoweredPipelineStage shape: {name}")
        shape = shape_match.group(1)
        cursor += shape_match.end()
        while cursor < len(body) and body[cursor].isspace():
            cursor += 1
        if cursor >= len(body) or body[cursor] != "{":
            raise EvidenceError(f"missing LoweredPipelineStage fields: {name}")
        field_open = cursor
        field_close = matching_delimiter(body, field_open, "{", "}")
        field_source = body[field_open + 1:field_close].strip()
        if shape == "unit" and field_source:
            raise EvidenceError(f"unit LoweredPipelineStage has fields: {name}")
        if shape != "unit" and not field_source:
            raise EvidenceError(f"field-bearing LoweredPipelineStage is empty: {name}")
        if name in variants:
            raise EvidenceError(f"duplicate LoweredPipelineStage variant: {name}")
        variants.add(name)

        fields: list[tuple[str, str]] = []
        for item in split_top_level_fields(field_source) if field_source else []:
            field = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.+)$", item, re.S)
            if field is None:
                raise EvidenceError(f"could not parse LoweredPipelineStage field: {name}.{item}")
            field_name = field.group(1)
            if any(existing == field_name for existing, _type in fields):
                raise EvidenceError(f"duplicate LoweredPipelineStage field: {name}.{field_name}")
            fields.append((field_name, " ".join(field.group(2).split())))

        for field_name, field_type in fields:
            child_paths.extend(
                schema_child_paths(field_type, f".{name}.{field_name}")
            )

        cursor = field_close + 1
        while cursor < len(body) and body[cursor].isspace():
            cursor += 1
        if cursor >= len(body) or body[cursor] != ";":
            raise EvidenceError(f"missing LoweredPipelineStage terminator: {name}")
        cursor += 1

    added = sorted(variants - PIPELINE_STAGE_VARIANTS)
    missing = sorted(PIPELINE_STAGE_VARIANTS - variants)
    if added or missing:
        raise EvidenceError(
            f"LoweredPipelineStage variant set changed: added={added}; missing={missing}"
        )
    if len(child_paths) != len(set(child_paths)):
        raise EvidenceError("duplicate LoweredPipelineStage evaluator-child path")
    return {
        "variant_count": len(variants),
        "child_paths": child_paths,
        "child_path_count": len(child_paths),
    }


def operand_policy(tag: str, field: str, field_type: str, child_paths: list[str]) -> str:
    """Classify the child shape without treating it as runtime-order evidence."""
    if not child_paths:
        return "not_applicable"
    if tag in {"BoolAnd", "BoolOr"}:
        return "short_circuit"
    if tag == "ExprComparisonChain":
        return "short_circuit"
    if (tag, field) in {
        ("ExprResultFallback", "right"),
        ("ExprIf", "branches"), ("ExprIf", "else_value"),
        ("ExprPatternIf", "branches"), ("ExprPatternIf", "else_value"),
        ("ExprMatch", "arms"), ("ExprStrMatch", "arms"),
        ("ExprStrMatch", "fallback"), ("ExprTagMatch", "arms"),
        ("ExprTagMatch", "fallback"), ("ExprRetry", "pattern"),
        ("StmtAssert", "message"), ("StmtGuard", "else_body"),
        ("StmtWith", "body"), ("StmtWith", "else_body"),
        ("StmtIf", "branches"), ("StmtIf", "else_body"),
        ("StmtIfBool", "branches"), ("StmtIfBool", "else_body"),
        ("StmtPatternIf", "branches"), ("StmtPatternIf", "else_body"),
        ("StmtMatch", "arms"), ("StmtStrMatch", "arms"),
        ("StmtStrMatch", "fallback"), ("StmtTagMatch", "arms"),
        ("StmtTagMatch", "fallback"), ("StmtDefer", "value"),
    }:
        return "selected"
    if (tag, field) in {
        ("ExprListComp", "value"), ("ExprListComp", "qualifiers"),
        ("ExprMapComp", "key"), ("ExprMapComp", "value"),
        ("ExprMapComp", "qualifiers"),
    }:
        return "iterative"
    if "[*]" in " ".join(child_paths) or field_type.startswith(("Vec<", "FxHashMap<")):
        return "iterative"
    return "strict"


def parse_operand_manifest(source: str) -> dict[str, Any]:
    reader = csv.DictReader(source.splitlines(), delimiter="\t")
    expected_headers = [
        "record", "tag", "ordinal", "kind", "variant", "field", "type", "policy", "child_paths"
    ]
    if reader.fieldnames != expected_headers:
        raise EvidenceError("operand manifest has an unknown or incomplete header")

    result: dict[str, Any] = {}
    row_markers: set[str] = set()
    for line_number, record in enumerate(reader, start=2):
        kind = record["record"]
        tag = record["tag"]
        if kind not in {"row", "field"} or not tag:
            raise EvidenceError(f"invalid operand manifest record at line {line_number}")
        try:
            ordinal = int(record["ordinal"])
        except ValueError as error:
            raise EvidenceError(f"invalid operand manifest ordinal at line {line_number}") from error
        row = result.setdefault(
            tag,
            {"ordinal": ordinal, "kind": record["kind"], "variant": record["variant"], "fields": []},
        )
        if (row["ordinal"], row["kind"], row["variant"]) != (
            ordinal, record["kind"], record["variant"]
        ):
            raise EvidenceError(f"conflicting operand manifest row metadata: {tag}")
        if kind == "row":
            if tag in row_markers:
                raise EvidenceError(f"duplicate operand manifest row: {tag}")
            if any(record[name] for name in ("field", "type", "policy", "child_paths")):
                raise EvidenceError(f"operand manifest row marker has field data: {tag}")
            row_markers.add(tag)
            continue

        field = record["field"]
        if not field or field == "@row":
            raise EvidenceError(f"invalid operand manifest field at line {line_number}")
        paths = record["child_paths"].split(";") if record["child_paths"] else []
        if len(paths) != len(set(paths)):
            raise EvidenceError(f"duplicate child paths for {tag}.{field}")
        policy = record["policy"]
        if policy not in OPERAND_POLICIES:
            raise EvidenceError(f"unknown operand policy for {tag}.{field}: {policy}")
        if bool(paths) == (policy == "not_applicable"):
            raise EvidenceError(f"operand policy and child paths disagree for {tag}.{field}")
        if any(not path for path in paths):
            raise EvidenceError(f"empty child path for {tag}.{field}")
        if any(item["name"] == field for item in row["fields"]):
            raise EvidenceError(f"duplicate operand manifest field: {tag}.{field}")
        row["fields"].append(
            {"name": field, "type": record["type"], "policy": policy, "child_paths": paths}
        )

    missing_markers = sorted(set(result) - row_markers)
    if missing_markers:
        raise EvidenceError(f"operand manifest rows lack row markers: {missing_markers}")
    return result


def validate_operand_inventory(
    schema_rows: list[dict[str, Any]],
    manifest_source: str,
    arena_source: str,
    pipeline_stage_source: str | None = None,
) -> dict[str, Any]:
    manifest = parse_operand_manifest(manifest_source)
    pipeline_stages = (
        pipeline_stage_child_inventory(pipeline_stage_source)
        if pipeline_stage_source is not None
        else None
    )
    schema_tags = [row["tag"] for row in schema_rows]
    if len(schema_tags) != len(set(schema_tags)):
        raise EvidenceError("duplicate schema rows in operand inventory")
    schema_set = set(schema_tags)
    manifest_set = set(manifest)
    added = sorted(schema_set - manifest_set)
    missing = sorted(manifest_set - schema_set)
    if added or missing:
        raise EvidenceError(f"operand row inventory mismatch: added={added}; missing={missing}")

    result: dict[str, Any] = {}
    for row in schema_rows:
        tag = row["tag"]
        recorded = manifest[tag]
        metadata = (row["ordinal"], row["kind"], row["variant"])
        recorded_metadata = (recorded["ordinal"], recorded["kind"], recorded["variant"])
        if metadata != recorded_metadata:
            raise EvidenceError(f"operand manifest row shape changed: {tag}")
        expected_fields = row["schema_fields"]
        actual_fields = recorded["fields"]
        actual_names = [field["name"] for field in actual_fields]
        expected_names = [field["name"] for field in expected_fields]
        if actual_names != expected_names:
            missing_fields = sorted(set(expected_names) - set(actual_names))
            extra_fields = sorted(set(actual_names) - set(expected_names))
            raise EvidenceError(
                f"operand fields are unclassified for {tag}: missing={missing_fields}; extra={extra_fields}"
            )
        classified: list[dict[str, Any]] = []
        for expected, actual in zip(expected_fields, actual_fields):
            name = expected["name"]
            if actual["type"] != expected["type"]:
                raise EvidenceError(f"operand field type changed: {tag}.{name}")
            expected_paths = schema_child_paths(
                expected["type"],
                name,
                pipeline_stages["child_paths"] if pipeline_stages is not None else None,
            )
            if actual["child_paths"] != expected_paths:
                raise EvidenceError(f"operand child paths changed: {tag}.{name}")
            if bool(expected_paths) == (actual["policy"] == "not_applicable"):
                raise EvidenceError(f"operand policy and child paths disagree for {tag}.{name}")
            expected_policy = operand_policy(tag, name, expected["type"], expected_paths)
            if actual["policy"] != expected_policy:
                raise EvidenceError(f"operand policy changed: {tag}.{name}")
            classified.append(
                {
                    "name": name,
                    "type": expected["type"],
                    "policy": actual["policy"],
                    "child_paths": expected_paths,
                }
            )
        result[tag] = {
            "status": "syntax_schema_only",
            "fields": classified,
            "child_paths": [
                path for field in classified for path in field["child_paths"]
            ],
        }

    arena_checks = validate_arena_child_shapes(arena_source)
    return {
        "status": "syntax_schema_only",
        "policy_values": sorted(OPERAND_POLICIES),
        "rows": result,
        "arena_shape_checks": arena_checks,
        "pipeline_stage_shape_checks": pipeline_stages,
        "payload_rows": sum(bool(row["schema_fields"]) for row in schema_rows),
        "payload_fields": sum(len(row["schema_fields"]) for row in schema_rows),
        "evaluable_child_paths": sum(len(value["child_paths"]) for value in result.values()),
        "runtime_order_status": "unresolved",
    }


def rust_function_body(source: str, function_name: str) -> str:
    code = mask_rust_non_code(source)
    declaration = re.compile(rf"\bfn\s+{re.escape(function_name)}\b")
    matches = list(declaration.finditer(code))
    if len(matches) != 1:
        raise EvidenceError(
            f"expected one Rust function {function_name}, found {len(matches)}"
        )
    cursor = matches[0].end()
    while cursor < len(code) and code[cursor].isspace():
        cursor += 1
    if cursor >= len(code) or code[cursor] != "(":
        raise EvidenceError(f"unparseable Rust function signature: {function_name}")
    parameters_end = matching_delimiter(code, cursor, "(", ")")
    body_open = parameters_end + 1
    while body_open < len(code) and code[body_open].isspace():
        body_open += 1
    if body_open >= len(code) or code[body_open] != "{":
        raise EvidenceError(f"unparseable Rust function body: {function_name}")
    body_end = matching_delimiter(code, body_open, "{", "}")
    return code[body_open + 1:body_end]


def validate_arena_child_shapes(source: str) -> dict[str, str]:
    checked: dict[str, str] = {}
    owners = {function_name for _shape, function_name in ARENA_SHAPE_CONTRACTS.values()}
    normalized = {
        function_name: " ".join(rust_function_body(source, function_name).split())
        for function_name in owners
    }
    for name, (shape, function_name) in ARENA_SHAPE_CONTRACTS.items():
        if normalized[function_name].count(shape) != 1:
            raise EvidenceError(
                f"recognized syntax child shape drift: {function_name}/{name}"
            )
        checked[name] = "present_once"
    return checked


def source_tag_set(source: str, function_name: str) -> set[str]:
    start = source.find(f"fn {function_name}")
    if start < 0:
        raise EvidenceError(f"missing route function {function_name}")
    opening = source.find("{", start)
    depth = 0
    end = None
    for index in range(opening, len(source)):
        char = source[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                end = index + 1
                break
    if end is None:
        raise EvidenceError(f"unclosed route function {function_name}")
    return set(re.findall(r"FullTag::([A-Z][A-Za-z0-9_]*)", source[opening:end]))


def explicit_expression_routes(source: str) -> tuple[set[str], set[str]]:
    start = source.find("fn eval_expr_view")
    if start < 0:
        raise EvidenceError("missing explicit expression dispatch")
    unsupported = source.find("ExprViewKind::Null |", start)
    error_return = source.find("return Err(frame_strategy_error(tag, span));", unsupported)
    if unsupported < 0 or error_return < 0:
        raise EvidenceError("missing explicit expression unsupported arm")
    supported_text = source[start:unsupported]
    unsupported_text = source[unsupported:error_return]
    return (
        set(re.findall(r"ExprViewKind::([A-Z][A-Za-z0-9_]*)", supported_text)),
        set(re.findall(r"ExprViewKind::([A-Z][A-Za-z0-9_]*)", unsupported_text)),
    )


def statement_ast_inventory(
    root: pathlib.Path,
    rows: list[dict[str, Any]],
    schema_path: pathlib.Path,
    dispatch_path: str,
) -> dict[str, Any]:
    inventory = run_ast_extractor(
        root,
        [str(root / schema_path), str(root / dispatch_path), dispatch_path],
        "statement",
    )
    expected_tags = [row["tag"] for row in rows if row["kind"] == "Stmt"]
    if inventory.get("schema_tags") != expected_tags:
        raise EvidenceError("statement AST schema rows differ from the matrix schema rows")
    branches = inventory.get("branches", [])
    branch_tags = [branch.get("tag") for branch in branches]
    if sorted(branch_tags) != sorted(expected_tags) or len(branch_tags) != len(set(branch_tags)):
        raise EvidenceError("statement AST branches do not cover each statement tag once")
    fallbacks = inventory.get("fallbacks", [])
    if len(fallbacks) != 1 or fallbacks[0].get("outcome") != "frame_strategy_error":
        raise EvidenceError("statement AST inventory has no exact frame-strategy fallback")
    return inventory


def run_ast_extractor(
    root: pathlib.Path,
    arguments: list[str],
    subject: str,
) -> dict[str, Any]:
    scratch_tmp = root / ".work/tmp"
    if not scratch_tmp.is_dir():
        raise EvidenceError(f"scratch temporary directory is missing: {scratch_tmp}")
    if not CARGO_EXE.is_file():
        raise EvidenceError(f"explicit cargo executable is missing: {CARGO_EXE}")
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(AST_EXTRACTOR_MANIFEST.parent / "target")
    env["TMPDIR"] = str(scratch_tmp)
    try:
        result = subprocess.run(
            [
                str(CARGO_EXE),
                "run",
                "--quiet",
                "--offline",
                "--manifest-path",
                str(AST_EXTRACTOR_MANIFEST),
                "--",
                *arguments,
            ],
            cwd=root,
            check=False,
            text=True,
            capture_output=True,
            env=env,
        )
    except OSError as error:
        raise EvidenceError(f"could not run {subject} AST extractor: {error}") from error
    if result.returncode != 0:
        raise EvidenceError(
            f"{subject} AST extractor failed: "
            + (result.stderr.strip() or f"cargo exited with {result.returncode}")
        )
    try:
        inventory = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise EvidenceError(f"{subject} AST extractor emitted invalid JSON: {error}") from error
    return inventory


def typed_scalar_ast_inventory(
    root: pathlib.Path,
    rows: list[dict[str, Any]],
    indexed_path: str,
    explicit_path: str,
) -> dict[str, Any]:
    inventory = run_ast_extractor(
        root,
        [
            "--typed-scalars",
            str(root / SCHEMA_PATH),
            str(root / indexed_path),
            str(root / TYPED_SCALAR_VALUES_PATH),
            str(root / explicit_path),
            str(SCHEMA_PATH),
            indexed_path,
            str(TYPED_SCALAR_VALUES_PATH),
            explicit_path,
        ],
        "typed scalar",
    )
    return validate_typed_scalar_inventory(rows, inventory)


def eager_atom_ast_inventory(
    root: pathlib.Path,
    rows: list[dict[str, Any]],
    indexed_path: str,
    explicit_path: str,
) -> dict[str, Any]:
    inventory = run_ast_extractor(
        root,
        [
            "--eager-atoms",
            str(root / SCHEMA_PATH),
            str(root / indexed_path),
            str(root / explicit_path),
            str(root / PIPELINE_STAGE_SOURCE_PATH),
            str(root / LOWERED_RUN_SOURCE_PATH),
            str(SCHEMA_PATH),
            indexed_path,
            explicit_path,
            str(PIPELINE_STAGE_SOURCE_PATH),
            str(LOWERED_RUN_SOURCE_PATH),
        ],
        "EagerAtom",
    )
    return validate_eager_atom_inventory(rows, inventory)


def frame_constructor_ast_inventory(
    root: pathlib.Path,
    rows: list[dict[str, Any]],
    indexed_path: str,
    explicit_path: str,
) -> dict[str, Any]:
    inventory = run_ast_extractor(
        root,
        [
            "--frame-constructors",
            str(root / SCHEMA_PATH),
            str(root / indexed_path),
            str(root / explicit_path),
            str(SCHEMA_PATH),
            indexed_path,
            explicit_path,
        ],
        "FrameApply constructors",
    )
    return validate_frame_constructor_inventory(rows, inventory)


def validate_frame_constructor_inventory(
    rows: list[dict[str, Any]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    expected_rows = [row for row in rows if row["tag"] in FRAME_CONSTRUCTOR_ROWS]
    if len(expected_rows) != 3:
        raise EvidenceError(
            f"FrameApply constructor cohort must contain 3 rows, found {len(expected_rows)}"
        )
    expected = {
        (tag, variant, ordinal)
        for tag, (variant, ordinal) in FRAME_CONSTRUCTOR_ROWS.items()
    }
    actual_rows = [
        (row["tag"], row["variant"], row["ordinal"])
        for row in expected_rows
    ]
    if set(actual_rows) != expected or len(actual_rows) != len(expected):
        raise EvidenceError("FrameApply constructor matrix rows differ from the fixed cohort")
    if any(
        row["kind"] != "Expr"
        or row["effects"] != "0"
        or row["in_place"] != "Scheduled"
        or row["frame_strategy"] != "FrameApply"
        or row["eager_atom"]
        for row in expected_rows
    ):
        raise EvidenceError("FrameApply constructor metadata differs from the schema contract")

    source_paths = inventory.get("source_paths", {})
    if set(source_paths) != {"schema", "normal_dispatch", "forced_dispatch"} or any(
        not isinstance(path, str) or not path for path in source_paths.values()
    ):
        raise EvidenceError("FrameApply constructor AST inventory has incomplete source paths")

    schema_rows = inventory.get("schema_rows", [])
    if not isinstance(schema_rows, list) or any(not isinstance(row, dict) for row in schema_rows):
        raise EvidenceError("FrameApply constructor AST schema rows are malformed")
    actual_schema = [
        (row.get("tag"), row.get("variant"), row.get("ordinal"))
        for row in schema_rows
    ]
    if set(actual_schema) != expected or len(actual_schema) != len(expected):
        raise EvidenceError("FrameApply constructor AST schema rows differ from the fixed cohort")
    row_by_tag = {row["tag"]: row for row in expected_rows}
    for schema_row in schema_rows:
        matrix_row = row_by_tag[schema_row["tag"]]
        if (
            schema_row.get("effects") != matrix_row["effects"]
            or schema_row.get("class") != matrix_row["in_place"]
            or schema_row.get("strategy") != matrix_row["frame_strategy"]
            or not isinstance(schema_row.get("line"), int)
            or schema_row["line"] < 1
        ):
            raise EvidenceError("FrameApply constructor AST schema metadata differs from matrix rows")

    classifiers = inventory.get("classifiers", {})
    if set(classifiers) != {"FrameApply", "prepared_operands"}:
        raise EvidenceError("FrameApply constructor classifiers are incomplete")
    frame_branches = classifiers["FrameApply"]
    prepared_branches = classifiers["prepared_operands"]
    if (
        not isinstance(frame_branches, list)
        or not isinstance(prepared_branches, list)
        or any(not isinstance(branch, dict) for branch in frame_branches + prepared_branches)
    ):
        raise EvidenceError("FrameApply constructor classifier branches are malformed")
    frame_tags = [branch.get("tag") for branch in frame_branches]
    prepared_tags = [branch.get("tag") for branch in prepared_branches]
    known_tags = {row["tag"] for row in rows}
    if any(not isinstance(tag, str) or tag not in known_tags for tag in frame_tags + prepared_tags):
        raise EvidenceError("FrameApply constructor classifier names an unknown schema tag")
    if len(frame_tags) != len(set(frame_tags)) or not set(FRAME_CONSTRUCTOR_ROWS).issubset(frame_tags):
        raise EvidenceError("FrameApply constructor rows are absent or duplicated in the route classifier")
    if (
        len(prepared_tags) != len(set(prepared_tags))
        or set(prepared_tags) & set(FRAME_CONSTRUCTOR_ROWS)
        or not set(prepared_tags).issubset(frame_tags)
    ):
        raise EvidenceError("FrameApply constructor rows unexpectedly use prepared host operands")

    for name, mode in (("normal_route", "normal"), ("forced_route", "forced")):
        route = inventory.get(name)
        if not isinstance(route, dict) or route.get("mode") != mode:
            raise EvidenceError(f"FrameApply constructor inventory is missing {mode} route")
        for field in (
            "line", "helper_line", "statement_index", "fallback_line", "fallback_statement_index"
        ):
            if not isinstance(route.get(field), int) or route[field] < 0:
                raise EvidenceError(f"FrameApply constructor {mode} route has invalid {field}")
        if route["statement_index"] >= route["fallback_statement_index"]:
            raise EvidenceError(f"FrameApply constructor {mode} route occurs after its fallback")
        if route["line"] >= route["fallback_line"]:
            raise EvidenceError(f"FrameApply constructor {mode} route line occurs after its fallback")

    expected_child = {
        "FunctionRef": "no_child",
        "PathFrom": "one_child",
        "EmptyMap": "no_child",
    }
    expected_value = {
        "FunctionRef": "indexed_function_ref_value",
        "PathFrom": "indexed_path_from_value",
        "EmptyMap": "indexed_empty_map_value",
    }
    expected_helpers = {
        "child_arms": "indexed_frame_apply_child",
        "value_arms": "indexed_frame_apply_value",
    }
    for name, expected_arms in (("child_arms", expected_child), ("value_arms", expected_value)):
        arms = inventory.get(name, [])
        actual_arms = {arm.get("variant"): arm.get("behavior") for arm in arms}
        if (
            len(arms) != 3
            or actual_arms != expected_arms
            or any(arm.get("helper") != expected_helpers[name] for arm in arms)
        ):
            raise EvidenceError(f"FrameApply constructor {name} differ from the fixed view arms")
        if any(
            not isinstance(arm.get("line"), int)
            or arm["line"] < 1
            or not isinstance(arm.get("arm_index"), int)
            or arm["arm_index"] < 0
            or not isinstance(arm.get("helper"), str)
            for arm in arms
        ):
            raise EvidenceError(f"FrameApply constructor {name} have invalid source refs")

    fallbacks = inventory.get("helper_fallbacks", [])
    expected_fallback_helpers = {"indexed_frame_apply_child", "indexed_frame_apply_value"}
    if (
        not isinstance(fallbacks, list)
        or any(not isinstance(item, dict) for item in fallbacks)
        or len(fallbacks) != 2
        or {item.get("helper") for item in fallbacks} != expected_fallback_helpers
    ):
        raise EvidenceError("FrameApply constructor helper fallbacks are incomplete")
    if any(
        item.get("outcome") != "unreachable!: frame apply tag has a matching typed view"
        or not isinstance(item.get("line"), int)
        or item["line"] < 1
        for item in fallbacks
    ):
        raise EvidenceError("FrameApply constructor helper wildcard outcome changed")
    explicit_error = inventory.get("explicit_error_group", {})
    if (
        explicit_error.get("helper") != "eval_expr_view"
        or explicit_error.get("outcome") != "frame_strategy_error(tag, span)"
        or not isinstance(explicit_error.get("line"), int)
        or explicit_error["line"] < 1
    ):
        raise EvidenceError("FrameApply constructor defensive error group changed")

    value_boundaries = inventory.get("value_boundaries", {})
    if set(value_boundaries) != set(expected_value) or any(
        not isinstance(line, int) or line < 1 for line in value_boundaries.values()
    ):
        raise EvidenceError("FrameApply constructor value boundaries differ from the row helpers")
    if not isinstance(inventory.get("value_decode_line"), int) or inventory["value_decode_line"] < 1:
        raise EvidenceError("FrameApply constructor value decode boundary is missing")
    if inventory.get("runtime_error_paths", {}).get("status") != "unreviewed":
        raise EvidenceError("FrameApply constructor runtime errors must remain unreviewed")
    return inventory


def collection_construction_ast_inventory(
    root: pathlib.Path,
    rows: list[dict[str, Any]],
    indexed_path: str,
    explicit_path: str,
) -> dict[str, Any]:
    inventory = run_ast_extractor(
        root,
        [
            "--collection-constructors",
            str(root / SCHEMA_PATH),
            str(root / indexed_path),
            str(root / explicit_path),
            str(root / OPERANDS_HELPER_PATH),
            str(SCHEMA_PATH),
            indexed_path,
            explicit_path,
            str(OPERANDS_HELPER_PATH),
        ],
        "collection constructors",
    )
    return validate_collection_construction_inventory(rows, inventory)


def validate_collection_construction_inventory(
    rows: list[dict[str, Any]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    expected = {
        (tag, variant, ordinal, strategy)
        for tag, (variant, ordinal, strategy) in COLLECTION_CONSTRUCTION_ROWS.items()
    }
    expected_rows = [row for row in rows if row["tag"] in COLLECTION_CONSTRUCTION_ROWS]
    actual_rows = {
        (row["tag"], row["variant"], row["ordinal"], row["frame_strategy"])
        for row in expected_rows
    }
    if len(expected_rows) != 5 or actual_rows != expected:
        raise EvidenceError("collection construction matrix rows differ from the fixed cohort")
    if any(
        row["kind"] != "Expr"
        or row["effects"] != "0"
        or row["in_place"] != "Operands"
        or row["eager_atom"]
        for row in expected_rows
    ):
        raise EvidenceError("collection construction metadata differs from the schema contract")

    expected_fields = {
        "ExprRecord": [("entries", "Vec<LoweredRecordEntry>")],
        "ExprMapLiteral": [("entries", "Vec<(Option<BuildExprId>, BuildExprId, Span)>")],
        "ExprRecordUpdate": [
            ("base", "BuildExprId"),
            ("updates", "LoweredRecordUpdates"),
            ("span", "Span"),
        ],
        "ExprList": [("values", "Vec<BuildExprId>")],
        "ExprListBuild": [("values", "Vec<(bool, BuildExprId, Span)>")],
    }
    if any(
        row["tag"] not in expected_fields
        or not isinstance(row.get("schema_fields"), list)
        or any(not isinstance(field, dict) for field in row["schema_fields"])
        or [(field.get("name"), field.get("type")) for field in row["schema_fields"]]
        != expected_fields[row["tag"]]
        for row in expected_rows
    ):
        raise EvidenceError("collection construction schema fields differ from the fixed rows")

    source_paths = inventory.get("source_paths", {})
    if set(source_paths) != {"schema", "normal_dispatch", "forced_dispatch", "operand_helpers"} or any(
        not isinstance(path, str) or not path for path in source_paths.values()
    ):
        raise EvidenceError("collection constructor AST inventory has incomplete source paths")

    schema_rows = inventory.get("schema_rows", [])
    actual_schema = {
        (row.get("tag"), row.get("variant"), row.get("ordinal"), row.get("strategy"))
        for row in schema_rows
    }
    if len(schema_rows) != 5 or actual_schema != expected:
        raise EvidenceError("collection constructor AST schema rows differ from the fixed cohort")
    matrix_by_tag = {row["tag"]: row for row in expected_rows}
    for schema_row in schema_rows:
        matrix_row = matrix_by_tag[schema_row["tag"]]
        if (
            schema_row.get("effects") != matrix_row["effects"]
            or schema_row.get("class") != matrix_row["in_place"]
            or not isinstance(schema_row.get("line"), int)
            or schema_row["line"] < 1
        ):
            raise EvidenceError("collection constructor AST schema metadata differs from matrix rows")

    classifier = inventory.get("frame_apply_classifier", {})
    classifier_tags = classifier.get("tags", []) if isinstance(classifier, dict) else []
    known_tags = {row["tag"] for row in rows}
    if (
        not isinstance(classifier, dict)
        or not isinstance(classifier.get("line"), int)
        or classifier["line"] < 1
        or not isinstance(classifier_tags, list)
        or any(not isinstance(tag, str) for tag in classifier_tags)
        or len(classifier_tags) != len(set(classifier_tags))
        or not set(classifier_tags).issubset(known_tags)
        or set(classifier_tags) & set(COLLECTION_CONSTRUCTION_ROWS)
    ):
        raise EvidenceError("collection constructor FrameApply classifier exclusion is malformed")

    expected_normal = {
        "ExprRecord": "evaluate_each_entry_then_append_record_field_or_spread",
        "ExprMapLiteral": "evaluate_optional_key_before_value_then_append_entry",
        "ExprRecordUpdate": "evaluate_base_then_update_values_before_batch_replacement",
        "ExprList": "evaluate_each_item_then_append_without_splicing",
        "ExprListBuild": "evaluate_each_item_then_append_with_encoded_splice_policy",
    }
    expected_forced = {
        "ExprRecord": "record_items_frame_step",
        "ExprMapLiteral": "map_literal_frame_step",
        "ExprRecordUpdate": "record_update_base_continuation",
        "ExprList": "indexed_list_literal_frame_continuation",
        "ExprListBuild": "indexed_list_build_frame_continuation",
    }
    for key, behaviors in (("normal_arms", expected_normal), ("forced_arms", expected_forced)):
        arms = inventory.get(key, [])
        if not isinstance(arms, list) or any(not isinstance(arm, dict) for arm in arms):
            raise EvidenceError(f"collection constructor {key} are malformed")
        actual = {arm.get("tag"): arm.get("behavior") for arm in arms}
        if len(arms) != 5 or actual != behaviors or any(
            not isinstance(arm.get("line"), int)
            or arm["line"] < 1
            or not isinstance(arm.get("arm_index"), int)
            or arm["arm_index"] < 0
            or arm.get("tag") not in COLLECTION_CONSTRUCTION_ROWS
            or arm.get("variant") != COLLECTION_CONSTRUCTION_ROWS[arm["tag"]][0]
            for arm in arms
        ):
            raise EvidenceError(f"collection constructor {key} differ from their exact routes")

    forced_route = inventory.get("forced_route", {})
    route_fields = (
        "frame_apply_line",
        "frame_apply_statement_index",
        "view_kind_line",
        "view_kind_statement_index",
        "eval_expr_view_line",
    )
    if any(not isinstance(forced_route.get(field), int) or forced_route[field] < 0 for field in route_fields):
        raise EvidenceError("collection constructor forced route is incomplete")
    if (
        forced_route["frame_apply_statement_index"] >= forced_route["view_kind_statement_index"]
        or forced_route["frame_apply_line"] >= forced_route["view_kind_line"]
        or forced_route["view_kind_line"] >= forced_route["eval_expr_view_line"]
    ):
        raise EvidenceError("collection constructor forced route selection order changed")

    normal_fallback = inventory.get("normal_fallback", {})
    if (
        normal_fallback.get("helper") != "eval_indexed_expr_inner"
        or normal_fallback.get("outcome") != "RuntimeError(indexed-ir: direct indexed evaluator does not support tag)"
        or not isinstance(normal_fallback.get("line"), int)
        or normal_fallback["line"] < 1
        or not isinstance(normal_fallback.get("arm_index"), int)
        or normal_fallback["arm_index"] < 0
    ):
        raise EvidenceError("collection constructor normal dispatch fallback changed")

    expected_normal_views = {
        "Record": "unreachable!: record literal tag has a typed record view",
        "MapLiteral": "unreachable!: map literal tag has a typed map view",
        "List": "unreachable!: list literal tag has a typed list view",
        "ListBuild": "unreachable!: list build tag has a typed list build view",
    }
    normal_views = inventory.get("normal_view_fallbacks", [])
    if not isinstance(normal_views, list) or any(not isinstance(item, dict) for item in normal_views):
        raise EvidenceError("collection constructor normal typed-view fallbacks are malformed")
    normal_view_names = [
        item.get("scope", "").split()[0]
        if isinstance(item.get("scope"), str) and item.get("scope")
        else ""
        for item in normal_views
    ]
    if (
        len(normal_views) != len(expected_normal_views)
        or len(normal_view_names) != len(set(normal_view_names))
        or set(normal_view_names) != set(expected_normal_views)
        or any(
            item.get("helper") != "eval_indexed_expr_inner"
            or item.get("outcome") != expected_normal_views[item.get("scope", "").split()[0]]
            or not isinstance(item.get("line"), int)
            or item["line"] < 1
            or not isinstance(item.get("arm_index"), int)
            or item["arm_index"] < 0
            for item in normal_views
        )
    ):
        raise EvidenceError("collection constructor normal typed-view fallbacks differ")

    expected_helper_fallbacks = {
        "indexed_list_literal_view": "Err(indexed-ir: list literal view is invalid)",
        "indexed_map_literal_view": "Err(indexed-ir: map literal view is invalid)",
        "indexed_record_literal_view": "Err(indexed-ir: record literal view is invalid)",
        "decode_record_updates": "unreachable!: record-update tag has a matching borrowed expression view",
    }
    helper_fallbacks = inventory.get("helper_fallbacks", [])
    if not isinstance(helper_fallbacks, list) or any(
        not isinstance(item, dict) for item in helper_fallbacks
    ):
        raise EvidenceError("collection constructor typed-view helper fallbacks are malformed")
    if (
        len(helper_fallbacks) != len(expected_helper_fallbacks)
        or {item.get("helper"): item.get("outcome") for item in helper_fallbacks} != expected_helper_fallbacks
        or any(
            not isinstance(item.get("line"), int)
            or item["line"] < 1
            or not isinstance(item.get("arm_index"), int)
            or item["arm_index"] < 0
            for item in helper_fallbacks
        )
    ):
        raise EvidenceError("collection constructor typed-view helper fallbacks differ")
    explicit_error = inventory.get("explicit_error_group", {})
    if (
        explicit_error.get("helper") != "eval_expr_view"
        or explicit_error.get("outcome") != "frame_strategy_error(tag, span)"
        or not isinstance(explicit_error.get("line"), int)
        or explicit_error["line"] < 1
        or not isinstance(explicit_error.get("arm_index"), int)
        or explicit_error["arm_index"] < 0
    ):
        raise EvidenceError("collection constructor defensive frame-strategy group changed")
    if inventory.get("runtime_error_paths", {}).get("status") != "unreviewed":
        raise EvidenceError("collection constructor runtime errors must remain unreviewed")
    return inventory


def result_constructor_ast_inventory(
    root: pathlib.Path,
    rows: list[dict[str, Any]],
    indexed_path: str,
    explicit_path: str,
) -> dict[str, Any]:
    inventory = run_ast_extractor(
        root,
        [
            "--result-constructors",
            str(root / SCHEMA_PATH),
            str(root / indexed_path),
            str(root / explicit_path),
            str(SCHEMA_PATH),
            indexed_path,
            explicit_path,
        ],
        "result constructors",
    )
    return validate_result_constructor_inventory(rows, inventory)


def validate_result_constructor_inventory(
    rows: list[dict[str, Any]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    expected = {
        (tag, variant, ordinal)
        for tag, (variant, ordinal) in RESULT_CONSTRUCTOR_ROWS.items()
    }
    expected_rows = [row for row in rows if row["tag"] in RESULT_CONSTRUCTOR_ROWS]
    actual = {(row["tag"], row["variant"], row["ordinal"]) for row in expected_rows}
    if len(expected_rows) != 2 or actual != expected:
        raise EvidenceError("result constructor matrix rows differ from the fixed cohort")
    if any(
        row["kind"] != "Expr"
        or row["effects"] != "0"
        or row["in_place"] != "Operands"
        or row["frame_strategy"] != "FrameControl"
        or row["eager_atom"]
        for row in expected_rows
    ):
        raise EvidenceError("result constructor metadata differs from the schema contract")
    expected_fields = {
        "ExprOk": [("value", "BuildExprId")],
        "ExprErr": [("value", "BuildExprId"), ("cause", "Option<BuildExprId>")],
    }
    if any(
        row["tag"] not in expected_fields
        or not isinstance(row.get("schema_fields"), list)
        or any(not isinstance(field, dict) for field in row["schema_fields"])
        or [(field.get("name"), field.get("type")) for field in row["schema_fields"]]
        != expected_fields[row["tag"]]
        for row in expected_rows
    ):
        raise EvidenceError("result constructor schema fields differ from the fixed rows")

    source_paths = inventory.get("source_paths", {})
    if set(source_paths) != {"schema", "normal_dispatch", "forced_dispatch"} or any(
        not isinstance(path, str) or not path for path in source_paths.values()
    ):
        raise EvidenceError("result constructor AST inventory has incomplete source paths")
    schema_rows = inventory.get("schema_rows", [])
    actual_schema = {
        (row.get("tag"), row.get("variant"), row.get("ordinal"))
        for row in schema_rows
    }
    if len(schema_rows) != 2 or actual_schema != expected:
        raise EvidenceError("result constructor AST schema rows differ from the fixed cohort")
    matrix_by_tag = {row["tag"]: row for row in expected_rows}
    for schema_row in schema_rows:
        matrix_row = matrix_by_tag[schema_row["tag"]]
        if (
            schema_row.get("effects") != matrix_row["effects"]
            or schema_row.get("class") != matrix_row["in_place"]
            or schema_row.get("strategy") != matrix_row["frame_strategy"]
            or not isinstance(schema_row.get("line"), int)
            or schema_row["line"] < 1
        ):
            raise EvidenceError("result constructor AST schema metadata differs from matrix rows")

    expected_normal = {
        "ExprOk": "evaluate_child_then_wrap_result_ok",
        "ExprErr": "evaluate_error_then_optional_cause_before_constructing_error",
    }
    expected_forced = {
        "ExprOk": "push_child_with_wrap_ok_continuation",
        "ExprErr": "push_value_with_optional_cause_wrap_err_continuation",
    }
    for key, behaviors in (("normal_arms", expected_normal), ("forced_arms", expected_forced)):
        arms = inventory.get(key, [])
        if not isinstance(arms, list) or any(not isinstance(arm, dict) for arm in arms):
            raise EvidenceError(f"result constructor {key} are malformed")
        actual_arms = {arm.get("tag"): arm.get("behavior") for arm in arms}
        if len(arms) != 2 or actual_arms != behaviors or any(
            not isinstance(arm.get("line"), int)
            or arm["line"] < 1
            or not isinstance(arm.get("arm_index"), int)
            or arm["arm_index"] < 0
            or arm.get("tag") not in RESULT_CONSTRUCTOR_ROWS
            or arm.get("variant") != RESULT_CONSTRUCTOR_ROWS[arm["tag"]][0]
            for arm in arms
        ):
            raise EvidenceError(f"result constructor {key} differ from their exact routes")
    normal_fallback = inventory.get("normal_fallback", {})
    if (
        normal_fallback.get("helper") != "eval_indexed_expr_inner"
        or normal_fallback.get("outcome")
        != "RuntimeError(indexed-ir: direct indexed evaluator does not support tag)"
        or not isinstance(normal_fallback.get("line"), int)
        or normal_fallback["line"] < 1
        or not isinstance(normal_fallback.get("arm_index"), int)
        or normal_fallback["arm_index"] < 0
        or any(arm["arm_index"] >= normal_fallback["arm_index"] for arm in inventory["normal_arms"])
    ):
        raise EvidenceError("result constructor normal dispatch fallback changed")

    expected_normal_views = {
        "Ok": "unreachable!: ok tag has an ok view",
        "Err": "unreachable!: err tag has an err view",
    }
    expected_forced_views = {
        "Ok": "unreachable!: ok tag has an ok view",
        "Err": "unreachable!: err tag has an err view",
    }
    for key, helper, outcomes in (
        ("normal_view_fallbacks", "eval_indexed_expr_inner", expected_normal_views),
        ("forced_view_fallbacks", "eval_expr_view", expected_forced_views),
    ):
        fallbacks = inventory.get(key, [])
        if not isinstance(fallbacks, list) or any(not isinstance(item, dict) for item in fallbacks):
            raise EvidenceError(f"result constructor {key} are malformed")
        variants = [
            item.get("scope", "").split()[0]
            if isinstance(item.get("scope"), str) and item.get("scope")
            else ""
            for item in fallbacks
        ]
        if (
            len(fallbacks) != 2
            or len(variants) != len(set(variants))
            or set(variants) != set(outcomes)
            or any(
                item.get("helper") != helper
                or item.get("outcome") != outcomes[variant]
                or not isinstance(item.get("line"), int)
                or item["line"] < 1
                or not isinstance(item.get("arm_index"), int)
                or item["arm_index"] < 0
                for variant, item in zip(variants, fallbacks)
            )
        ):
            raise EvidenceError(f"result constructor {key} differ")

    forced_route = inventory.get("forced_route", {})
    route_fields = (
        "frame_apply_line",
        "frame_apply_statement_index",
        "view_kind_line",
        "view_kind_statement_index",
        "eval_expr_view_line",
    )
    if any(not isinstance(forced_route.get(field), int) or forced_route[field] < 0 for field in route_fields):
        raise EvidenceError("result constructor forced route is incomplete")
    if (
        forced_route["frame_apply_statement_index"] >= forced_route["view_kind_statement_index"]
        or forced_route["frame_apply_line"] >= forced_route["view_kind_line"]
        or forced_route["view_kind_line"] >= forced_route["eval_expr_view_line"]
    ):
        raise EvidenceError("result constructor forced route selection order changed")
    explicit_error = inventory.get("explicit_error_group", {})
    if (
        explicit_error.get("helper") != "eval_expr_view"
        or explicit_error.get("outcome") != "frame_strategy_error(tag, span)"
        or not isinstance(explicit_error.get("line"), int)
        or explicit_error["line"] < 1
        or not isinstance(explicit_error.get("arm_index"), int)
        or explicit_error["arm_index"] < 0
        or any(arm["arm_index"] >= explicit_error["arm_index"] for arm in inventory["forced_arms"])
    ):
        raise EvidenceError("result constructor defensive frame-strategy group changed")
    if inventory.get("runtime_error_paths", {}).get("status") != "unreviewed":
        raise EvidenceError("result constructor runtime errors must remain unreviewed")
    return inventory


def call_frame_ast_inventory(
    root: pathlib.Path,
    rows: list[dict[str, Any]],
    indexed_path: str,
    explicit_path: str,
) -> dict[str, Any]:
    inventory = run_ast_extractor(
        root,
        [
            "--call-frames",
            str(root / SCHEMA_PATH),
            str(root / indexed_path),
            str(root / explicit_path),
            str(SCHEMA_PATH),
            indexed_path,
            explicit_path,
        ],
        "call frames",
    )
    return validate_call_frame_inventory(rows, inventory)


def validate_call_frame_inventory(
    rows: list[dict[str, Any]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    expected = {
        (tag, variant, ordinal)
        for tag, (variant, ordinal, _) in CALL_FRAME_ROWS.items()
    }
    expected_rows = [row for row in rows if row["tag"] in CALL_FRAME_ROWS]
    actual = {(row["tag"], row["variant"], row["ordinal"]) for row in expected_rows}
    if len(expected_rows) != 4 or actual != expected:
        raise EvidenceError("call frame matrix rows differ from the fixed cohort")
    if any(
        row["kind"] != "Expr"
        or row["effects"].replace(" ", "") != CALL_FRAME_ROWS[row["tag"]][2].replace(" ", "")
        or row["in_place"] != "Scheduled"
        or row["frame_strategy"] != "FrameControl"
        or row["eager_atom"]
        for row in expected_rows
    ):
        raise EvidenceError("call frame metadata differs from the schema contract")
    expected_fields = {
        "ExprCall": [("function", "LoweredFunctionKey"), ("args", "Vec<LoweredCallArg>"), ("span", "Span")],
        "ExprDirectPureCall": [("function", "LoweredFunctionKey"), ("args", "Vec<LoweredCallArg>"), ("span", "Span")],
        "ExprExternalCall": [("function", "QualifiedName"), ("args", "Vec<LoweredCallArg>"), ("span", "Span")],
        "ExprDynamicCall": [("callee", "BuildExprId"), ("args", "Vec<LoweredCallArg>"), ("span", "Span")],
    }
    if any(
        not isinstance(row.get("schema_fields"), list)
        or any(not isinstance(field, dict) for field in row["schema_fields"])
        or [(field.get("name"), field.get("type")) for field in row["schema_fields"]]
        != expected_fields[row["tag"]]
        for row in expected_rows
    ):
        raise EvidenceError("call frame schema fields differ from the fixed rows")

    source_paths = inventory.get("source_paths", {})
    if set(source_paths) != {"schema", "normal_dispatch", "forced_dispatch"} or any(
        not isinstance(path, str) or not path for path in source_paths.values()
    ):
        raise EvidenceError("call frame AST inventory has incomplete source paths")
    schema_rows = inventory.get("schema_rows", [])
    if not isinstance(schema_rows, list) or any(not isinstance(row, dict) for row in schema_rows):
        raise EvidenceError("call frame AST schema rows are malformed")
    actual_schema = {
        (row.get("tag"), row.get("variant"), row.get("ordinal"))
        for row in schema_rows
    }
    if len(schema_rows) != 4 or actual_schema != expected:
        raise EvidenceError("call frame AST schema rows differ from the fixed cohort")
    matrix_by_tag = {row["tag"]: row for row in expected_rows}
    for schema_row in schema_rows:
        matrix_row = matrix_by_tag[schema_row["tag"]]
        if (
            schema_row.get("effects", "").replace(" ", "") != matrix_row["effects"].replace(" ", "")
            or schema_row.get("class") != matrix_row["in_place"]
            or schema_row.get("strategy") != matrix_row["frame_strategy"]
            or not isinstance(schema_row.get("line"), int)
            or schema_row["line"] < 1
        ):
            raise EvidenceError("call frame AST schema metadata differs from matrix rows")

    route = inventory.get("normal_route", {})
    route_fields = (
        "scheduled_predicate_line",
        "frame_schedule_line",
        "frame_helper_line",
        "inner_helper_line",
        "view_kind_line",
        "eval_expr_view_line",
    )
    if any(not isinstance(route.get(field), int) or route[field] < 1 for field in route_fields):
        raise EvidenceError("call frame normal and forced routes are incomplete")
    if not (
        route["scheduled_predicate_line"] < route["frame_schedule_line"] < route["frame_helper_line"]
        and route["frame_schedule_line"] < route["inner_helper_line"]
        and route["view_kind_line"] < route["eval_expr_view_line"]
    ):
        raise EvidenceError("call frame route order changed")

    expected_arms = {
        "ExprCall": ("Call", ["Call", "SelfCall", "DirectPureCall"], "decode_static_call_and_schedule_arguments"),
        "ExprDirectPureCall": ("DirectPureCall", ["Call", "SelfCall", "DirectPureCall"], "decode_static_call_and_schedule_arguments"),
        "ExprExternalCall": ("ExternalCall", ["ExternalCall"], "resolve_external_kind_then_schedule_arguments"),
        "ExprDynamicCall": ("DynamicCall", ["DynamicCall", "TypedCall"], "evaluate_callee_before_scheduling_dynamic_arguments"),
    }
    arms = inventory.get("forced_arms", [])
    if not isinstance(arms, list) or any(not isinstance(arm, dict) for arm in arms):
        raise EvidenceError("call frame forced arms are malformed")
    actual_arms = {arm.get("tag"): arm for arm in arms}
    if len(arms) != 4 or set(actual_arms) != set(expected_arms) or any(
        actual_arms[tag].get("variant") != variant
        or actual_arms[tag].get("view_group") != view_group
        or actual_arms[tag].get("behavior") != behavior
        or not isinstance(actual_arms[tag].get("line"), int)
        or actual_arms[tag]["line"] < 1
        or not isinstance(actual_arms[tag].get("arm_index"), int)
        or actual_arms[tag]["arm_index"] < 0
        for tag, (variant, view_group, behavior) in expected_arms.items()
    ):
        raise EvidenceError("call frame forced handlers differ from their exact view groups")
    if (
        actual_arms["ExprCall"]["line"] != actual_arms["ExprDirectPureCall"]["line"]
        or actual_arms["ExprCall"]["arm_index"] != actual_arms["ExprDirectPureCall"]["arm_index"]
    ):
        raise EvidenceError("static call variants no longer share the static-call handler")

    expected_fallbacks = {
        "Call": "unreachable!: static call tag has a typed call view",
        "DirectPureCall": "unreachable!: static call tag has a typed call view",
        "ExternalCall": "unreachable!: external call tag has a typed call view",
        "DynamicCall": "unreachable!: dynamic call tag has a typed call view",
    }
    fallbacks = inventory.get("typed_view_fallbacks", [])
    if not isinstance(fallbacks, list) or any(not isinstance(item, dict) for item in fallbacks):
        raise EvidenceError("call frame typed-view fallbacks are malformed")
    fallback_variants = [
        item.get("scope", "").split()[0]
        if isinstance(item.get("scope"), str) and item.get("scope")
        else ""
        for item in fallbacks
    ]
    if (
        len(fallbacks) != 4
        or len(fallback_variants) != len(set(fallback_variants))
        or set(fallback_variants) != set(expected_fallbacks)
        or any(
            not isinstance(item, dict)
            or item.get("helper") != "eval_expr_view"
            or item.get("outcome") != expected_fallbacks[variant]
            or not isinstance(item.get("line"), int)
            or item["line"] < 1
            or not isinstance(item.get("arm_index"), int)
            or item["arm_index"] < 0
            for variant, item in zip(fallback_variants, fallbacks)
        )
    ):
        raise EvidenceError("call frame typed-view fallbacks differ")

    normal_fallback = inventory.get("normal_fallback", {})
    if (
        normal_fallback.get("helper") != "eval_indexed_expr_inner"
        or normal_fallback.get("outcome") != "RuntimeError(indexed-ir: direct indexed evaluator does not support tag)"
        or not isinstance(normal_fallback.get("line"), int)
        or normal_fallback["line"] < 1
        or not isinstance(normal_fallback.get("arm_index"), int)
        or normal_fallback["arm_index"] < 0
    ):
        raise EvidenceError("call frame direct evaluator fallback changed")
    explicit_error = inventory.get("explicit_error_group", {})
    if (
        explicit_error.get("helper") != "eval_expr_view"
        or explicit_error.get("outcome") != "frame_strategy_error(tag, span)"
        or not isinstance(explicit_error.get("line"), int)
        or explicit_error["line"] < 1
        or not isinstance(explicit_error.get("arm_index"), int)
        or explicit_error["arm_index"] < 0
        or any(arm["arm_index"] >= explicit_error["arm_index"] for arm in arms)
    ):
        raise EvidenceError("call frame defensive error group changed")
    if inventory.get("runtime_error_paths", {}).get("status") != "unreviewed":
        raise EvidenceError("call frame runtime errors must remain unreviewed")
    return inventory


def field_index_ast_inventory(
    root: pathlib.Path,
    rows: list[dict[str, Any]],
    indexed_path: str,
    explicit_path: str,
) -> dict[str, Any]:
    inventory = run_ast_extractor(
        root,
        [
            "--field-index",
            str(root / SCHEMA_PATH),
            str(root / indexed_path),
            str(root / explicit_path),
            str(SCHEMA_PATH),
            indexed_path,
            explicit_path,
        ],
        "field/index expressions",
    )
    return validate_field_index_inventory(rows, inventory)


def validate_field_index_inventory(
    rows: list[dict[str, Any]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    expected = {
        (tag, variant, ordinal)
        for tag, (variant, ordinal) in FIELD_INDEX_ROWS.items()
    }
    expected_rows = [row for row in rows if row["tag"] in FIELD_INDEX_ROWS]
    actual = {(row["tag"], row["variant"], row["ordinal"]) for row in expected_rows}
    if len(expected_rows) != 3 or actual != expected:
        raise EvidenceError("field/index matrix rows differ from the fixed cohort")
    if any(
        row["kind"] != "Expr"
        or row["effects"] != "0"
        or row["in_place"] != "Operands"
        or row["frame_strategy"] != "FrameApply"
        or row["eager_atom"]
        for row in expected_rows
    ):
        raise EvidenceError("field/index metadata differs from the schema contract")
    expected_fields = {
        "ExprField": [("base", "BuildExprId"), ("name", "NameText"), ("span", "Span")],
        "ExprIndex": [("base", "BuildExprId"), ("index", "BuildExprId"), ("span", "Span")],
        "ExprIndexFromEnd": [
            ("base", "BuildExprId"),
            ("distance", "crate::runtime::eval::EndDistance"),
            ("span", "Span"),
        ],
    }
    if any(
        row["tag"] not in expected_fields
        or not isinstance(row.get("schema_fields"), list)
        or any(not isinstance(field, dict) for field in row["schema_fields"])
        or [(field.get("name"), field.get("type")) for field in row["schema_fields"]]
        != expected_fields[row["tag"]]
        for row in expected_rows
    ):
        raise EvidenceError("field/index schema fields differ from the fixed rows")

    source_paths = inventory.get("source_paths", {})
    if set(source_paths) != {"schema", "normal_dispatch", "forced_dispatch"} or any(
        not isinstance(path, str) or not path for path in source_paths.values()
    ):
        raise EvidenceError("field/index AST inventory has incomplete source paths")
    schema_rows = inventory.get("schema_rows", [])
    if not isinstance(schema_rows, list) or any(not isinstance(row, dict) for row in schema_rows):
        raise EvidenceError("field/index AST schema rows are malformed")
    actual_schema = {
        (row.get("tag"), row.get("variant"), row.get("ordinal"))
        for row in schema_rows
    }
    if len(schema_rows) != 3 or actual_schema != expected:
        raise EvidenceError("field/index AST schema rows differ from the fixed cohort")
    matrix_by_tag = {row["tag"]: row for row in expected_rows}
    for schema_row in schema_rows:
        matrix_row = matrix_by_tag[schema_row["tag"]]
        if (
            schema_row.get("effects") != matrix_row["effects"]
            or schema_row.get("class") != matrix_row["in_place"]
            or schema_row.get("strategy") != matrix_row["frame_strategy"]
            or not isinstance(schema_row.get("line"), int)
            or schema_row["line"] < 1
        ):
            raise EvidenceError("field/index AST schema metadata differs from matrix rows")

    if not isinstance(inventory.get("frame_apply_classifier_line"), int) or inventory["frame_apply_classifier_line"] < 1:
        raise EvidenceError("field/index FrameApply classifier is missing")
    normal_route = inventory.get("normal_route", {})
    normal_route_fields = (
        "scheduled_predicate_line", "frame_schedule_line", "frame_helper_line", "inner_helper_line",
    )
    if any(not isinstance(normal_route.get(field), int) or normal_route[field] < 1 for field in normal_route_fields):
        raise EvidenceError("field/index normal wrapper route is incomplete")
    if not (
        normal_route["scheduled_predicate_line"] < normal_route["frame_schedule_line"]
        and normal_route["frame_schedule_line"] <= normal_route["frame_helper_line"]
        and normal_route["frame_helper_line"] < normal_route["inner_helper_line"]
    ):
        raise EvidenceError("field/index normal wrapper route order changed")
    expected_normal = {
        "ExprField": "try_record_field_chain_then_evaluate_base_and_lookup_field",
        "ExprIndex": "evaluate_base_then_index_and_lookup",
        "ExprIndexFromEnd": "evaluate_base_then_lookup_from_end_distance",
    }
    expected_forced = {
        "ExprField": "try_record_field_chain_then_schedule_base",
        "ExprIndex": "schedule_base_then_index_continuation",
        "ExprIndexFromEnd": "schedule_base_then_from_end_continuation",
    }
    for key, behaviors in (("normal_arms", expected_normal), ("forced_arms", expected_forced)):
        arms = inventory.get(key, [])
        if not isinstance(arms, list) or any(not isinstance(arm, dict) for arm in arms):
            raise EvidenceError(f"field/index {key} are malformed")
        actual_arms = {arm.get("tag"): arm for arm in arms}
        if len(arms) != 3 or set(actual_arms) != set(behaviors) or any(
            actual_arms[tag].get("variant") != matrix_by_tag[tag]["variant"]
            or actual_arms[tag].get("behavior") != behavior
            or not isinstance(actual_arms[tag].get("line"), int)
            or actual_arms[tag]["line"] < 1
            or not isinstance(actual_arms[tag].get("arm_index"), int)
            or actual_arms[tag]["arm_index"] < 0
            for tag, behavior in behaviors.items()
        ):
            raise EvidenceError(f"field/index {key} differ from their exact routes")

    expected_view_fallbacks = {
        "Field": "unreachable!: field tag has a field view",
        "Index": "unreachable!: index tag has an index view",
        "IndexFromEnd": "unreachable!: index-from-end tag has an index-from-end view",
    }
    for key, helper in (
        ("normal_view_fallbacks", "eval_indexed_expr_inner"),
        ("forced_view_fallbacks", "eval_expr_view"),
    ):
        fallbacks = inventory.get(key, [])
        if not isinstance(fallbacks, list) or any(not isinstance(item, dict) for item in fallbacks):
            raise EvidenceError(f"field/index {key} are malformed")
        variants = [
            item.get("scope", "").split()[0]
            if isinstance(item.get("scope"), str) and item.get("scope")
            else ""
            for item in fallbacks
        ]
        if (
            len(fallbacks) != 3
            or len(variants) != len(set(variants))
            or set(variants) != set(expected_view_fallbacks)
            or any(
                item.get("helper") != helper
                or item.get("outcome") != expected_view_fallbacks[variant]
                or not isinstance(item.get("line"), int)
                or item["line"] < 1
                or not isinstance(item.get("arm_index"), int)
                or item["arm_index"] < 0
                for variant, item in zip(variants, fallbacks)
            )
        ):
            raise EvidenceError(f"field/index {key} differ")

    normal_fallback = inventory.get("normal_fallback", {})
    normal_arms = inventory["normal_arms"]
    if (
        normal_fallback.get("helper") != "eval_indexed_expr_inner"
        or normal_fallback.get("outcome")
        != "RuntimeError(indexed-ir: direct indexed evaluator does not support tag)"
        or not isinstance(normal_fallback.get("line"), int)
        or normal_fallback["line"] < 1
        or not isinstance(normal_fallback.get("arm_index"), int)
        or normal_fallback["arm_index"] < 0
        or any(arm["arm_index"] >= normal_fallback["arm_index"] for arm in normal_arms)
    ):
        raise EvidenceError("field/index normal dispatch fallback changed")
    route = inventory.get("forced_route", {})
    route_fields = (
        "frame_apply_line", "frame_apply_statement_index", "view_kind_line",
        "view_kind_statement_index", "eval_expr_view_line",
    )
    if any(not isinstance(route.get(field), int) or route[field] < 0 for field in route_fields):
        raise EvidenceError("field/index forced route is incomplete")
    if not (
        route["frame_apply_statement_index"] < route["view_kind_statement_index"]
        and route["frame_apply_line"] < route["view_kind_line"] < route["eval_expr_view_line"]
    ):
        raise EvidenceError("field/index forced route selection order changed")
    explicit_error = inventory.get("explicit_error_group", {})
    if (
        explicit_error.get("helper") != "eval_expr_view"
        or explicit_error.get("outcome") != "frame_strategy_error(tag, span)"
        or not isinstance(explicit_error.get("line"), int)
        or explicit_error["line"] < 1
        or not isinstance(explicit_error.get("arm_index"), int)
        or explicit_error["arm_index"] < 0
        or any(arm["arm_index"] >= explicit_error["arm_index"] for arm in inventory["forced_arms"])
    ):
        raise EvidenceError("field/index defensive frame-strategy group changed")
    if inventory.get("runtime_error_paths", {}).get("status") != "unreviewed":
        raise EvidenceError("field/index runtime errors must remain unreviewed")
    return inventory


def collection_construction_fallback_path_map(
    row: dict[str, Any],
    source_refs: dict[str, list[str]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    tag = row["tag"]
    schema = next(
        (
            item for item in inventory["schema_rows"]
            if item["tag"] == tag and item["variant"] == row["variant"]
        ),
        None,
    )
    if schema is None or schema["ordinal"] != row["ordinal"]:
        raise EvidenceError(f"collection constructor AST inventory has no exact schema row for {tag}")
    normal_arm = next((arm for arm in inventory["normal_arms"] if arm["tag"] == tag), None)
    forced_arm = next((arm for arm in inventory["forced_arms"] if arm["tag"] == tag), None)
    if normal_arm is None or forced_arm is None:
        raise EvidenceError(f"collection constructor inventory has incomplete routes for {tag}")

    paths = inventory["source_paths"]
    schema_ref = f"{paths['schema']}:{schema['line']}"
    normal_ref = f"{paths['normal_dispatch']}:{normal_arm['line']}"
    forced_ref = f"{paths['forced_dispatch']}:{forced_arm['line']}"
    normal_fallback_ref = f"{paths['normal_dispatch']}:{inventory['normal_fallback']['line']}"
    explicit_error_ref = f"{paths['forced_dispatch']}:{inventory['explicit_error_group']['line']}"
    helper_fallbacks = [
        (paths["normal_dispatch"], item)
        for item in inventory["normal_view_fallbacks"]
    ] + [
        (paths["operand_helpers"] if item["helper"] == "decode_record_updates" else paths["forced_dispatch"], item)
        for item in inventory["helper_fallbacks"]
    ]
    helper_fallback_refs = [f"{path}:{item['line']}" for path, item in helper_fallbacks]
    row_fallbacks = [
        {
            "scope": "direct normal evaluator tag not supported",
            "outcome": inventory["normal_fallback"]["outcome"],
            "source_ref": normal_fallback_ref,
            "arm_index": inventory["normal_fallback"]["arm_index"],
            "applies_to_row": False,
        }
    ]
    row_fallbacks.extend(
        {
            "scope": fallback["scope"],
            "outcome": fallback["outcome"],
            "source_ref": f"{path}:{fallback['line']}",
            "arm_index": fallback["arm_index"],
            "applies_to_row": row["tag"] in collection_fallback_tags(fallback),
        }
        for path, fallback in helper_fallbacks
    )
    row_fallbacks.append({
        "scope": (
            inventory["explicit_error_group"]["scope"]
            + "; collection constructor has a dedicated forced handler"
        ),
        "outcome": inventory["explicit_error_group"]["outcome"],
        "source_ref": explicit_error_ref,
        "arm_index": inventory["explicit_error_group"]["arm_index"],
        "applies_to_row": False,
    })
    runtime_error_paths = {
        "status": "unreviewed",
        "source_refs": [schema_ref, normal_ref, forced_ref, *helper_fallback_refs],
        "note": (
            "typed view decoding, child evaluation, and row-specific record, map, or list construction "
            "errors remain outside the dispatch inventory"
        ),
    }
    return {
        "review_status": "source_exhaustive_dispatch_path",
        "scope": (
            "exact schema membership, normal collection evaluation arms, forced FrameApply route selection, "
            "and typed-view mismatch guards; operation errors remain unreviewed"
        ),
        "branches": [{
            "tag": tag,
            "variant": row["variant"],
            "ordinal": row["ordinal"],
            "normal_behavior": normal_arm["behavior"],
            "normal_route_ref": normal_ref,
            "forced_behavior": forced_arm["behavior"],
            "forced_route_ref": forced_ref,
        }],
        "branches_exhaustive": True,
        "no_fallback": None,
        "fallbacks": row_fallbacks,
        "runtime_error_paths": runtime_error_paths,
        "source_refs": {
            **source_refs,
            "collection_constructor_schema": [schema_ref],
            "collection_constructor_normal_dispatch": [normal_ref, normal_fallback_ref],
            "collection_constructor_forced_dispatch": [forced_ref, explicit_error_ref],
            "collection_constructor_typed_view_fallbacks": helper_fallback_refs,
        },
        "unreviewed_branch_behavior": runtime_error_paths["note"],
    }


def collection_fallback_tags(fallback: dict[str, Any]) -> list[str]:
    helper_tags = {
        "indexed_list_literal_view": ["ExprList", "ExprListBuild"],
        "indexed_map_literal_view": ["ExprMapLiteral"],
        "indexed_record_literal_view": ["ExprRecord"],
        "decode_record_updates": ["ExprRecordUpdate"],
    }
    direct_variant_tags = {
        "Record": "ExprRecord",
        "MapLiteral": "ExprMapLiteral",
        "List": "ExprList",
        "ListBuild": "ExprListBuild",
    }
    helper = fallback.get("helper")
    if helper in helper_tags:
        return helper_tags[helper]
    variant = fallback.get("scope", "").split()[0]
    return [direct_variant_tags[variant]] if variant in direct_variant_tags else []


def result_constructor_fallback_path_map(
    row: dict[str, Any],
    source_refs: dict[str, list[str]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    tag = row["tag"]
    schema = next(
        (
            item for item in inventory["schema_rows"]
            if item["tag"] == tag and item["variant"] == row["variant"]
        ),
        None,
    )
    if schema is None or schema["ordinal"] != row["ordinal"]:
        raise EvidenceError(f"result constructor AST inventory has no exact schema row for {tag}")
    normal_arm = next((arm for arm in inventory["normal_arms"] if arm["tag"] == tag), None)
    forced_arm = next((arm for arm in inventory["forced_arms"] if arm["tag"] == tag), None)
    if normal_arm is None or forced_arm is None:
        raise EvidenceError(f"result constructor inventory has incomplete routes for {tag}")
    paths = inventory["source_paths"]
    schema_ref = f"{paths['schema']}:{schema['line']}"
    normal_ref = f"{paths['normal_dispatch']}:{normal_arm['line']}"
    forced_ref = f"{paths['forced_dispatch']}:{forced_arm['line']}"
    normal_fallback_ref = f"{paths['normal_dispatch']}:{inventory['normal_fallback']['line']}"
    normal_view = next(
        (item for item in inventory["normal_view_fallbacks"] if item["scope"].startswith(row["variant"] + " ")),
        None,
    )
    forced_view = next(
        (item for item in inventory["forced_view_fallbacks"] if item["scope"].startswith(row["variant"] + " ")),
        None,
    )
    if normal_view is None or forced_view is None:
        raise EvidenceError(f"result constructor inventory has incomplete typed-view fallbacks for {tag}")
    normal_view_ref = f"{paths['normal_dispatch']}:{normal_view['line']}"
    forced_view_ref = f"{paths['forced_dispatch']}:{forced_view['line']}"
    explicit_error_ref = f"{paths['forced_dispatch']}:{inventory['explicit_error_group']['line']}"
    row_fallbacks = [
        {
            "scope": "direct normal evaluator tag not supported",
            "outcome": inventory["normal_fallback"]["outcome"],
            "source_ref": normal_fallback_ref,
            "arm_index": inventory["normal_fallback"]["arm_index"],
            "applies_to_row": False,
        },
        {
            "scope": normal_view["scope"],
            "outcome": normal_view["outcome"],
            "source_ref": normal_view_ref,
            "arm_index": normal_view["arm_index"],
            "applies_to_row": True,
        },
        {
            "scope": forced_view["scope"],
            "outcome": forced_view["outcome"],
            "source_ref": forced_view_ref,
            "arm_index": forced_view["arm_index"],
            "applies_to_row": True,
        },
        {
            "scope": (
                inventory["explicit_error_group"]["scope"]
                + "; result constructor has a dedicated forced handler"
            ),
            "outcome": inventory["explicit_error_group"]["outcome"],
            "source_ref": explicit_error_ref,
            "arm_index": inventory["explicit_error_group"]["arm_index"],
            "applies_to_row": False,
        },
    ]
    runtime_error_paths = {
        "status": "unreviewed",
        "source_refs": [schema_ref, normal_ref, forced_ref],
        "note": (
            "typed view decoding, child evaluation, and row-specific result construction errors "
            "remain outside the dispatch inventory"
        ),
    }
    return {
        "review_status": "source_exhaustive_dispatch_path",
        "scope": (
            "exact schema membership, normal result-constructor arms, forced continuation selection, "
            "and typed-view mismatch guards; operation errors remain unreviewed"
        ),
        "branches": [{
            "tag": tag,
            "variant": row["variant"],
            "ordinal": row["ordinal"],
            "normal_behavior": normal_arm["behavior"],
            "normal_route_ref": normal_ref,
            "forced_behavior": forced_arm["behavior"],
            "forced_route_ref": forced_ref,
        }],
        "branches_exhaustive": True,
        "no_fallback": None,
        "fallbacks": row_fallbacks,
        "runtime_error_paths": runtime_error_paths,
        "source_refs": {
            **source_refs,
            "result_constructor_schema": [schema_ref],
            "result_constructor_normal_dispatch": [normal_ref, normal_fallback_ref, normal_view_ref],
            "result_constructor_forced_dispatch": [forced_ref, forced_view_ref, explicit_error_ref],
        },
        "unreviewed_branch_behavior": runtime_error_paths["note"],
    }


def call_frame_fallback_path_map(
    row: dict[str, Any],
    source_refs: dict[str, list[str]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    tag = row["tag"]
    schema = next(
        (
            item for item in inventory["schema_rows"]
            if item["tag"] == tag and item["variant"] == row["variant"]
        ),
        None,
    )
    if schema is None or schema["ordinal"] != row["ordinal"]:
        raise EvidenceError(f"call frame AST inventory has no exact schema row for {tag}")
    arm = next((item for item in inventory["forced_arms"] if item["tag"] == tag), None)
    if arm is None:
        raise EvidenceError(f"call frame inventory has no forced handler for {tag}")
    paths = inventory["source_paths"]
    route = inventory["normal_route"]
    schema_ref = f"{paths['schema']}:{schema['line']}"
    schedule_ref = f"{paths['normal_dispatch']}:{route['frame_schedule_line']}"
    frame_helper_ref = f"{paths['normal_dispatch']}:{route['frame_helper_line']}"
    forced_ref = f"{paths['forced_dispatch']}:{arm['line']}"
    fallbacks = [{
        "scope": "direct normal evaluator tag not supported",
        "outcome": inventory["normal_fallback"]["outcome"],
        "source_ref": f"{paths['normal_dispatch']}:{inventory['normal_fallback']['line']}",
        "arm_index": inventory["normal_fallback"]["arm_index"],
        "applies_to_row": False,
    }]
    for fallback in inventory["typed_view_fallbacks"]:
        variant = fallback["scope"].split()[0]
        applies = (
            (tag in {"ExprCall", "ExprDirectPureCall"} and variant in {"Call", "DirectPureCall"})
            or (tag == "ExprExternalCall" and variant == "ExternalCall")
            or (tag == "ExprDynamicCall" and variant == "DynamicCall")
        )
        fallbacks.append({
            "scope": fallback["scope"],
            "outcome": fallback["outcome"],
            "source_ref": f"{paths['forced_dispatch']}:{fallback['line']}",
            "arm_index": fallback["arm_index"],
            "applies_to_row": applies,
        })
    explicit_error_ref = f"{paths['forced_dispatch']}:{inventory['explicit_error_group']['line']}"
    fallbacks.append({
        "scope": inventory["explicit_error_group"]["scope"] + "; call row has a dedicated handler",
        "outcome": inventory["explicit_error_group"]["outcome"],
        "source_ref": explicit_error_ref,
        "arm_index": inventory["explicit_error_group"]["arm_index"],
        "applies_to_row": False,
    })
    typed_view_refs = [
        f"{paths['forced_dispatch']}:{fallback['line']}"
        for fallback in inventory["typed_view_fallbacks"]
    ]
    runtime_error_paths = {
        "status": "unreviewed",
        "source_refs": [schema_ref, schedule_ref, frame_helper_ref, forced_ref, *typed_view_refs],
        "note": inventory["runtime_error_paths"]["note"],
    }
    return {
        "review_status": "source_exhaustive_dispatch_path",
        "scope": (
            "exact call schema membership, normal scheduled FrameControl route, dedicated forced call view group, "
            "and typed-view mismatch guards; call failures remain unreviewed"
        ),
        "branches": [{
            "tag": tag,
            "variant": row["variant"],
            "ordinal": row["ordinal"],
            "normal_behavior": "scheduled_frame_control_routes_to_explicit_machine",
            "normal_route_ref": schedule_ref,
            "normal_helper_ref": frame_helper_ref,
            "forced_behavior": arm["behavior"],
            "forced_route_ref": forced_ref,
            "forced_view_group": arm["view_group"],
        }],
        "branches_exhaustive": True,
        "no_fallback": None,
        "fallbacks": fallbacks,
        "runtime_error_paths": runtime_error_paths,
        "source_refs": {
            **source_refs,
            "call_frame_schema": [schema_ref],
            "call_frame_normal_route": [schedule_ref, frame_helper_ref],
            "call_frame_forced_dispatch": [forced_ref, explicit_error_ref],
            "call_frame_typed_view_fallbacks": typed_view_refs,
        },
        "unreviewed_branch_behavior": runtime_error_paths["note"],
    }
def validate_eager_atom_inventory(
    rows: list[dict[str, Any]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    expected_rows = [row for row in rows if row["eager_atom"]]
    if len(expected_rows) != 12:
        raise EvidenceError(f"EagerAtom matrix cohort must contain 12 rows, found {len(expected_rows)}")
    source_paths = inventory.get("source_paths", {})
    if set(source_paths) != {
        "schema", "normal_dispatch", "forced_dispatch", "slot_codec", "freeze_helper"
    } or any(
        not isinstance(path, str) or not path for path in source_paths.values()
    ):
        raise EvidenceError("EagerAtom AST inventory has incomplete source paths")
    expected_schema = {
        (
            row["variant"],
            row["tag"],
            row["ordinal"],
            row["effects"],
            row["in_place"],
            row["frame_strategy"],
            "EagerAtom",
        )
        for row in expected_rows
    }
    schema_rows = inventory.get("schema_rows", [])
    actual_schema = [
        (
            row.get("variant"),
            row.get("tag"),
            row.get("ordinal"),
            row.get("effects"),
            row.get("class"),
            row.get("strategy"),
            row.get("classification"),
        )
        for row in schema_rows
    ]
    if set(actual_schema) != expected_schema or len(actual_schema) != len(expected_schema):
        raise EvidenceError("EagerAtom AST schema rows differ from matrix metadata")
    if any(not isinstance(row.get("line"), int) or row["line"] < 1 for row in schema_rows):
        raise EvidenceError("EagerAtom AST schema row has an invalid source line")
    schema_by_tag = {row["tag"]: row for row in schema_rows}
    for row in expected_rows:
        schema_fields = schema_by_tag[row["tag"]].get("fields")
        expected_fields = [(field["name"], field["type"]) for field in row["schema_fields"]]
        actual_fields = [
            (field.get("name"), field.get("type")) for field in schema_fields or []
        ]
        if actual_fields != expected_fields:
            raise EvidenceError(f"EagerAtom schema fields differ for {row['tag']}")

    rules = inventory.get("classification_rules", {})
    for name in ("default_false_line", "eager_true_line", "expansion_line"):
        if not isinstance(rules.get(name), int) or rules[name] < 1:
            raise EvidenceError(f"EagerAtom classification rule has an invalid {name}")
    if rules["default_false_line"] == rules["eager_true_line"]:
        raise EvidenceError("EagerAtom default and positive classification rules share a source line")

    expected_routes = {"normal", "forced"}
    routes = [inventory.get("normal_route"), inventory.get("forced_route")]
    if {route.get("mode") for route in routes if isinstance(route, dict)} != expected_routes:
        raise EvidenceError("EagerAtom route inventory must include normal and forced dispatch")
    route_by_mode = {route["mode"]: route for route in routes}
    for mode in expected_routes:
        route = route_by_mode[mode]
        for name in (
            "line", "helper_line", "statement_index", "generic_line", "generic_statement_index"
        ):
            if not isinstance(route.get(name), int) or route[name] < 0:
                raise EvidenceError(f"EagerAtom {mode} route has an invalid {name}")
        if route["line"] >= route["helper_line"]:
            raise EvidenceError(f"EagerAtom {mode} route does not precede its helper call")
        if route["statement_index"] >= route["generic_statement_index"] or route["line"] >= route["generic_line"]:
            raise EvidenceError(f"EagerAtom {mode} route occurs after generic FrameApply")

    expected_variants = {row["variant"] for row in expected_rows}
    helper_arms = inventory.get("helper_arms", [])
    helper_variants = [arm.get("variant") for arm in helper_arms]
    if len(helper_variants) != len(set(helper_variants)) or set(helper_variants) != expected_variants:
        raise EvidenceError("EagerAtom helper view arms differ from schema")
    if len(helper_arms) != 12 or any(
        not isinstance(arm.get("line"), int) or arm["line"] < 1 for arm in helper_arms
    ):
        raise EvidenceError("EagerAtom helper view arm inventory is incomplete")
    helper_by_variant = {arm["variant"]: arm for arm in helper_arms}
    for variant, (tag, input_name, transformation, output) in EAGER_ATOM_PROVENANCE.items():
        arm = helper_by_variant.get(variant)
        if arm is None or (
            arm.get("tag"), arm.get("input"), arm.get("transformation"), arm.get("output")
        ) != (tag, input_name, transformation, output):
            raise EvidenceError(f"EagerAtom value provenance differs for {variant}")
        if not isinstance(arm.get("arm_index"), int) or arm["arm_index"] < 0:
            raise EvidenceError(f"EagerAtom value provenance has an invalid arm index for {variant}")

    helper_fallback = inventory.get("helper_fallback", {})
    if helper_fallback.get("outcome") != "unreachable!: only eager atom tags reach this value helper":
        raise EvidenceError("EagerAtom helper wildcard outcome changed")
    explicit_fallback = inventory.get("explicit_error_group", {})
    if explicit_fallback.get("outcome") != "frame_strategy_error(tag, span)":
        raise EvidenceError("EagerAtom explicit defensive error group changed")
    for fallback in (helper_fallback, explicit_fallback):
        if not isinstance(fallback.get("line"), int) or fallback["line"] < 1:
            raise EvidenceError("EagerAtom fallback has an invalid source line")

    expected_boundaries = {
        ("ExprView decode", frozenset(row["tag"] for row in expected_rows)),
        ("PathValue conversion", frozenset({"ExprPath"})),
        ("ExprParam slot access and freeze", frozenset({"ExprParam"})),
    }
    boundaries = inventory.get("helper_boundaries", [])
    actual_boundaries = {
        (boundary.get("kind"), frozenset(boundary.get("tags", [])))
        for boundary in boundaries
    }
    if len(boundaries) != 3 or actual_boundaries != expected_boundaries:
        raise EvidenceError("EagerAtom helper boundary inventory differs from its operations")
    for boundary in boundaries:
        references = boundary.get("references")
        if not isinstance(references, list) or not references:
            raise EvidenceError("EagerAtom helper boundary has no source references")
        for reference in references:
            if (
                reference.get("source") not in source_paths
                or not isinstance(reference.get("line"), int)
                or reference["line"] < 1
            ):
                raise EvidenceError("EagerAtom helper boundary has an invalid source reference")

    decode = next(boundary for boundary in boundaries if boundary["kind"] == "ExprView decode")
    path = next(boundary for boundary in boundaries if boundary["kind"] == "PathValue conversion")
    param = next(boundary for boundary in boundaries if boundary["kind"] == "ExprParam slot access and freeze")
    if any(reference["source"] != "normal_dispatch" for reference in decode["references"] + path["references"]):
        raise EvidenceError("EagerAtom decode and path boundaries must point to the value helper")
    slot_decode_line = inventory.get("slot_decode_line")
    slot_bounds_line = inventory.get("slot_decode_bounds_line")
    if any(not isinstance(line, int) or line < 1 for line in (slot_decode_line, slot_bounds_line)):
        raise EvidenceError("EagerAtom slot decoder has an invalid source line")
    freeze = inventory.get("freeze_contract", {})
    for name in ("line", "threshold_line", "list_take_line", "shared_list_line"):
        if not isinstance(freeze.get(name), int) or freeze[name] < 1:
            raise EvidenceError(f"EagerAtom freeze contract has an invalid {name}")
    if freeze.get("shared_threshold") != "LOWERED_SHARED_LIST_THRESHOLD":
        raise EvidenceError("EagerAtom freeze threshold changed")
    required_param_refs = {
        ("slot_codec", slot_decode_line),
        ("slot_codec", slot_bounds_line),
        ("freeze_helper", freeze["line"]),
        ("freeze_helper", freeze["threshold_line"]),
        ("freeze_helper", freeze["list_take_line"]),
        ("freeze_helper", freeze["shared_list_line"]),
    }
    param_refs = {(reference["source"], reference["line"]) for reference in param["references"]}
    if not required_param_refs <= param_refs:
        raise EvidenceError("ExprParam slot/freeze boundary is missing source evidence")
    value_contract = inventory.get("value_contract", {})
    for name, expected in {
        "decode_error": "indexed_error(error, span)",
        "path_error": "indexed_error(IrVerifyError { message: error.message }, span)",
        "slot_bounds": "slot >= decoder.slot_count",
        "freeze": "before slots[slot].clone()",
    }.items():
        if expected not in value_contract.get(name, ""):
            raise EvidenceError(f"EagerAtom {name} boundary changed")
    if inventory.get("runtime_error_paths", {}).get("status") != "source_exhaustive_boundary_inventory":
        raise EvidenceError("EagerAtom helper error boundaries are not source-exhaustive")
    return inventory


def validate_eager_atom_operand_structure(
    rows: list[dict[str, Any]],
    inventory: dict[str, Any],
    operand_inventory: dict[str, Any],
) -> dict[str, dict[str, Any]]:
    schema_by_tag = {row["tag"]: row for row in inventory["schema_rows"]}
    syntax_rows = operand_inventory.get("rows", {})
    proof: dict[str, dict[str, Any]] = {}
    for row in rows:
        if not row["eager_atom"]:
            continue
        schema = schema_by_tag[row["tag"]]
        fields = [(field["name"], field["type"]) for field in schema["fields"]]
        if fields != [(field["name"], field["type"]) for field in row["schema_fields"]]:
            raise EvidenceError(f"EagerAtom source schema does not match operand row {row['tag']}")
        syntax = syntax_rows.get(row["tag"])
        if syntax is None or syntax.get("child_paths") != []:
            raise EvidenceError(f"EagerAtom row {row['tag']} has evaluator child paths")
        if any(
            field.get("child_paths") or field.get("policy") != "not_applicable"
            for field in syntax.get("fields", [])
        ):
            raise EvidenceError(f"EagerAtom row {row['tag']} has an applicable operand-order policy")
        if row["tag"] == "ExprParam" and fields != [("slot", "usize")]:
            raise EvidenceError("ExprParam must contain only its checked slot index")
        provenance = next(
            arm for arm in inventory["helper_arms"] if arm["tag"] == row["tag"]
        )
        proof[row["tag"]] = {
            "status": "not_applicable",
            "child_paths": [],
            "schema_fields": [{"name": name, "type": ty} for name, ty in fields],
            "value_arm_index": provenance["arm_index"],
            "reason": (
                "Schema fields are data or a local slot index; no field has a BuildExprId child, "
                "and the exact eager view arm constructs or reads a value without evaluating an expression operand."
            ),
        }
    return proof


def validate_typed_scalar_inventory(
    rows: list[dict[str, Any]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    expected_rows = [row for row in rows if row["kind"] in {"Int", "Bool"}]
    if len(expected_rows) != 18:
        raise EvidenceError(f"typed scalar matrix cohort must contain 18 rows, found {len(expected_rows)}")
    source_paths = inventory.get("source_paths", {})
    if set(source_paths) != {"schema", "normal_dispatch", "typed_values", "forced_dispatch"} or any(
        not isinstance(path, str) or not path for path in source_paths.values()
    ):
        raise EvidenceError("typed scalar AST inventory has incomplete source paths")
    expected_schema = {
        (row["kind"], row["variant"], row["tag"], row["ordinal"])
        for row in expected_rows
    }
    actual_schema = [
        (row.get("family"), row.get("variant"), row.get("tag"), row.get("ordinal"))
        for row in inventory.get("schema_rows", [])
    ]
    if set(actual_schema) != expected_schema or len(actual_schema) != len(expected_schema):
        raise EvidenceError("typed scalar AST schema rows differ from the matrix schema rows")

    for family in ("Int", "Bool"):
        expected_tags = {row["tag"] for row in expected_rows if row["kind"] == family}
        classifier = inventory.get("classifiers", {}).get(family)
        if not isinstance(classifier, list):
            raise EvidenceError(f"typed scalar AST inventory has no {family} classifier")
        classifier_tags = [branch.get("tag") for branch in classifier]
        if len(classifier_tags) != len(set(classifier_tags)) or set(classifier_tags) != expected_tags:
            raise EvidenceError(f"typed scalar {family} classifier coverage differs from schema")
        if any(not isinstance(branch.get("line"), int) or branch["line"] < 1 for branch in classifier):
            raise EvidenceError(f"typed scalar {family} classifier has an invalid source line")

    checks = inventory.get("normal_checks", [])
    if [check.get("family") for check in checks] != ["Int", "Bool"]:
        raise EvidenceError("typed scalar normal checks must be ordered Int then Bool")
    for check, family in zip(checks, ("Int", "Bool")):
        if (
            check.get("wrapper") != f"eval_indexed_typed_{family.lower()}"
            or check.get("result_variant") != family
            or not isinstance(check.get("line"), int)
            or check["line"] < 1
        ):
            raise EvidenceError(f"typed scalar normal wrapper differs for {family}")
    forced_entry = inventory.get("forced_entry", {})
    if (
        forced_entry.get("family") != "FrameApply"
        or forced_entry.get("wrapper") != "eval_typed_scalar_expr"
        or not isinstance(forced_entry.get("line"), int)
        or forced_entry["line"] < 1
    ):
        raise EvidenceError("typed scalar forced-frame entry is not source-validated")

    view_arms = inventory.get("view_arms", [])
    for family in ("Int", "Bool"):
        expected_variants = {row["variant"] for row in expected_rows if row["kind"] == family}
        family_arms = [arm for arm in view_arms if arm.get("family") == family]
        variants = [arm.get("variant") for arm in family_arms]
        if len(variants) != len(set(variants)) or set(variants) != expected_variants:
            raise EvidenceError(f"typed scalar {family} view arms differ from schema")
        if any(not isinstance(arm.get("line"), int) or arm["line"] < 1 for arm in family_arms):
            raise EvidenceError(f"typed scalar {family} view arm has an invalid source line")
    if len(view_arms) != 18:
        raise EvidenceError(f"typed scalar view inventory must contain 18 arms, found {len(view_arms)}")

    expected_frame_groups = {
        "frame_operation": {"IntBinary", "IntStrByteAtSlot", "BoolNot", "BoolAnd", "BoolOr", "BoolIntCompare"},
        "verified_in_place_typed_delegation": {row["tag"] for row in expected_rows}
        - {"IntBinary", "IntStrByteAtSlot", "BoolNot", "BoolAnd", "BoolOr", "BoolIntCompare"},
    }
    group_coverage: dict[str, str] = {}
    groups = inventory.get("frame_groups", [])
    for group in groups:
        behavior = group.get("behavior")
        if behavior not in expected_frame_groups:
            raise EvidenceError(f"unknown typed scalar frame behavior {behavior}")
        tags = group.get("tags", [])
        if not tags or len(tags) != len(set(tags)):
            raise EvidenceError("typed scalar frame group is empty or duplicated")
        if not isinstance(group.get("line"), int) or group["line"] < 1:
            raise EvidenceError("typed scalar frame group has an invalid source line")
        for tag in tags:
            if tag in group_coverage:
                raise EvidenceError(f"typed scalar frame tag appears twice: {tag}")
            group_coverage[tag] = behavior
    for behavior, expected_tags in expected_frame_groups.items():
        actual_tags = {tag for tag, actual_behavior in group_coverage.items() if actual_behavior == behavior}
        if actual_tags != expected_tags:
            raise EvidenceError(f"typed scalar {behavior} group differs from the audited cohort")
    if set(group_coverage) != {row["tag"] for row in expected_rows}:
        raise EvidenceError("typed scalar frame groups do not cover exactly the schema cohort")

    required_fallbacks = {
        "frame_strategy_error: decoded IntView does not match tag",
        "frame_strategy_error: decoded BoolView does not match tag",
        "frame_strategy_error: unmatched tag",
        "frame_strategy_error: verifier in-place proof absent",
    }
    fallbacks = inventory.get("fallbacks", [])
    if len(fallbacks) != len(required_fallbacks) or {fallback.get("outcome") for fallback in fallbacks} != required_fallbacks:
        raise EvidenceError("typed scalar explicit-frame fallbacks differ from the audited branches")
    if any(not isinstance(fallback.get("line"), int) or fallback["line"] < 1 for fallback in fallbacks):
        raise EvidenceError("typed scalar fallback has an invalid source line")

    expected_union_errors = {
        ("Int", "typed integer evaluator produced Bool"),
        ("Bool", "typed boolean evaluator produced Int"),
    }
    wrong_unions = inventory.get("wrong_union_errors", [])
    actual_union_errors = {(error.get("family"), error.get("outcome")) for error in wrong_unions}
    if actual_union_errors != expected_union_errors or len(wrong_unions) != 2:
        raise EvidenceError("typed scalar defensive wrong-union errors differ from their wrappers")
    if any(not isinstance(error.get("line"), int) or error["line"] < 1 for error in wrong_unions):
        raise EvidenceError("typed scalar wrong-union error has an invalid source line")
    return inventory


def route_rows(
    rows: list[dict[str, Any]],
    indexed_source: str,
    explicit_source: str,
) -> list[dict[str, Any]]:
    frame_apply = source_tag_set(indexed_source, "indexed_frame_apply_tag")
    ordinary_special = source_tag_set(indexed_source, "eval_indexed_expr")
    explicit_supported, explicit_unsupported = explicit_expression_routes(explicit_source)
    direct = {
        row["tag"]
        for row in rows
        if row["kind"] == "Expr"
        and row["in_place"] == "Scheduled"
        and row["frame_strategy"] == "FrameControl"
    } | ordinary_special

    routed: list[dict[str, Any]] = []
    for row in rows:
        tag = row["tag"]
        if row["kind"] == "Stmt":
            normal = "eval_statement"
            forced = "eval_statement"
        elif row["kind"] in ("Int", "Bool"):
            normal = "typed_recursive_scalar"
            forced = "typed_scalar_frame_arm"
        else:
            if row["eager_atom"]:
                normal = "recursive_eager_atom_read"
                forced = "eager_atom_frame_fast_path"
            elif tag in direct:
                normal = "direct_explicit_frames"
                forced = "generic_operand_apply_path" if tag in frame_apply else "explicit_expression_arm"
            elif tag in frame_apply:
                normal = "recursive_bridge_to_explicit_frames"
                forced = "generic_operand_apply_path"
            else:
                normal = "recursive_expression_arm"
                variant = row["variant"]
                if variant in explicit_supported:
                    forced = "explicit_expression_arm"
                elif variant in explicit_unsupported:
                    forced = "frame_strategy_error_fallback"
                else:
                    raise EvidenceError(
                        f"unclassified forced expression row {tag} / ExprViewKind::{variant}"
                    )
        normal_routes = [{"name": "normal", "route": normal}]
        forced_routes = [{"name": "forced", "route": forced}]
        if tag in {"ExprIf", "ExprMatch"}:
            normal = "verifier_selected_in_place_or_explicit_frames"
            forced = "forced_explicit_frames"
            verifier_test_ids = {
                "ExprIf": [
                    "runtime::eval::indexed::full::tests::direct_if_children_are_marked_and_selected",
                    "runtime::eval::indexed::full::tests::in_place_marks_follow_verified_operands_and_selection_runs_directly",
                ],
                "ExprMatch": [
                    "runtime::eval::indexed::full::tests::in_place_marks_follow_verified_operands_and_selection_runs_directly",
                ],
            }[tag]
            normal_routes = [
                {
                    "name": "verifier_approved_in_place_recursive_selection",
                    "route": "recursive_if_or_match_selection",
                    "selected_when": "execution.is_in_place(instruction)",
                    "source_refs": [
                        "src/runtime/eval/indexed/full/in_place.rs:28",
                        "src/runtime/eval/indexed/full.rs:14328",
                        "src/runtime/eval/lowered_run/indexed_run.rs:1388",
                    ],
                    "test_ids": verifier_test_ids,
                },
                {
                    "name": "non_in_place_explicit_frame_selection",
                    "route": "eval_indexed_expr_with_frames",
                    "selected_when": "execution.is_in_place(instruction) is false",
                    "source_refs": [
                        "src/runtime/eval/lowered_run/indexed_run.rs:1653",
                        "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:6670",
                        "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:6690",
                    ],
                    "test_ids": (
                        ["runtime::eval::indexed::full::tests::frame_apply_child_keeps_selection_on_the_explicit_machine"]
                        if tag == "ExprIf"
                        else []
                    ),
                },
            ]
            forced_routes = [
                {
                    "name": "forced_explicit_frame_selection",
                    "route": "explicit_expression_arm",
                    "selected_when": "force_frames is true",
                    "source_refs": [
                        "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:6321",
                        "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:6670",
                        "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:6690",
                    ],
                    "test_ids": [],
                }
            ]
        row_with_route = dict(row)
        row_with_route["normal_route"] = normal
        row_with_route["forced_route"] = forced
        row_with_route["normal_routes"] = normal_routes
        row_with_route["forced_routes"] = forced_routes
        row_with_route["declared_field_order"] = [field["name"] for field in row["schema_fields"]]
        row_with_route["runtime_operand_order"] = {
            "status": "unverified",
            "fields": [],
            "reason": "schema declaration order is not proof of evaluation order",
        }
        row_with_route["fallback_path_map"] = []
        if forced == "frame_strategy_error_fallback":
            row_with_route["fallback_path_map"].append({
                "path": "forced unsupported-expression strategy error",
                "source": "eval_expr_view unsupported ExprViewKind arm",
                "proof": "source_structure_only",
            })
        routed.append(row_with_route)
    return routed


def source_line_refs(
    source: str,
    relative_path: str,
    needles: list[str],
    fallback_functions: list[str],
) -> list[str]:
    references = [
        f"{relative_path}:{line_number}"
        for line_number, line in enumerate(source.splitlines(), start=1)
        if any(needle in line for needle in needles)
    ]
    if references:
        return references
    for function in fallback_functions:
        match = re.search(rf"(?m)^\s*(?:pub\([^)]*\)\s+)?fn\s+{re.escape(function)}\b", source)
        if match is not None:
            line_number = source.count("\n", 0, match.start()) + 1
            return [f"{relative_path}:{line_number}"]
    return []


def fallback_source_refs(
    row: dict[str, Any],
    schema_source: str,
    indexed_source: str,
    explicit_source: str,
    indexed_path: str,
    explicit_path: str,
    statement_inventory: dict[str, Any] | None = None,
) -> dict[str, list[str]]:
    tag = row["tag"]
    variant = row["variant"]
    schema_refs = source_line_refs(
        schema_source,
        "src/runtime/eval/indexed/full/schema.rs",
        [f"=> {tag} ="],
        [],
    )
    if row["kind"] == "Stmt":
        if statement_inventory is None:
            raise EvidenceError(f"missing AST statement inventory for {tag}")
        branch = next(
            (item for item in statement_inventory["branches"] if item["tag"] == tag),
            None,
        )
        if branch is None:
            raise EvidenceError(f"statement AST inventory has no exact branch for {tag}")
        owner = statement_inventory["owner"]
        branch_ref = f"{owner}:{branch['line']}"
        fallback_refs = [
            f"{owner}:{fallback['line']}"
            for fallback in statement_inventory["fallbacks"]
        ]
        normal_refs = [branch_ref, *fallback_refs]
        forced_refs = normal_refs
    else:
        normal_refs = source_line_refs(
            indexed_source,
            indexed_path,
            [f"FullTag::{tag}"],
            ["eval_indexed_expr", "eval_indexed_expr_inner"],
        )
        forced_refs = source_line_refs(
            explicit_source,
            explicit_path,
            [f"FullTag::{tag}", f"ExprViewKind::{variant}"],
            ["eval_indexed_expr_with_frames", "eval_expr_view"],
        )
    return {
        "schema_row": schema_refs,
        "normal_dispatch": normal_refs,
        "forced_dispatch": forced_refs,
        **(
            {"statement_match": [f"{statement_inventory['owner']}:{statement_inventory['match_line']}"]}
            if row["kind"] == "Stmt" and statement_inventory is not None
            else {}
        ),
    }


def field_index_fallback_path_map(
    row: dict[str, Any],
    source_refs: dict[str, list[str]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    tag = row["tag"]
    schema = next(
        (
            item for item in inventory["schema_rows"]
            if item["tag"] == tag and item["variant"] == row["variant"]
        ),
        None,
    )
    if schema is None or schema["ordinal"] != row["ordinal"]:
        raise EvidenceError(f"field/index AST inventory has no exact schema row for {tag}")
    normal_arm = next((arm for arm in inventory["normal_arms"] if arm["tag"] == tag), None)
    forced_arm = next((arm for arm in inventory["forced_arms"] if arm["tag"] == tag), None)
    if normal_arm is None or forced_arm is None:
        raise EvidenceError(f"field/index inventory has incomplete routes for {tag}")
    variant = row["variant"]
    normal_view = next(
        (item for item in inventory["normal_view_fallbacks"] if item["scope"].startswith(variant + " ")),
        None,
    )
    forced_view = next(
        (item for item in inventory["forced_view_fallbacks"] if item["scope"].startswith(variant + " ")),
        None,
    )
    if normal_view is None or forced_view is None:
        raise EvidenceError(f"field/index inventory has incomplete typed-view fallbacks for {tag}")

    paths = inventory["source_paths"]
    schema_ref = f"{paths['schema']}:{schema['line']}"
    normal_ref = f"{paths['normal_dispatch']}:{normal_arm['line']}"
    normal_fallback_ref = f"{paths['normal_dispatch']}:{inventory['normal_fallback']['line']}"
    normal_view_ref = f"{paths['normal_dispatch']}:{normal_view['line']}"
    forced_ref = f"{paths['forced_dispatch']}:{forced_arm['line']}"
    forced_view_ref = f"{paths['forced_dispatch']}:{forced_view['line']}"
    explicit_error_ref = f"{paths['forced_dispatch']}:{inventory['explicit_error_group']['line']}"
    classifier_ref = f"{paths['normal_dispatch']}:{inventory['frame_apply_classifier_line']}"
    normal_wrapper_ref = f"{paths['normal_dispatch']}:{inventory['normal_route']['frame_schedule_line']}"
    fallbacks = [
        {
            "scope": "direct normal evaluator tag not supported",
            "outcome": inventory["normal_fallback"]["outcome"],
            "source_ref": normal_fallback_ref,
            "arm_index": inventory["normal_fallback"]["arm_index"],
            "applies_to_row": False,
        },
        {
            "scope": normal_view["scope"],
            "outcome": normal_view["outcome"],
            "source_ref": normal_view_ref,
            "arm_index": normal_view["arm_index"],
            "applies_to_row": True,
        },
        {
            "scope": forced_view["scope"],
            "outcome": forced_view["outcome"],
            "source_ref": forced_view_ref,
            "arm_index": forced_view["arm_index"],
            "applies_to_row": True,
        },
        {
            "scope": (
                inventory["explicit_error_group"]["scope"]
                + "; field/index row has a dedicated forced handler"
            ),
            "outcome": inventory["explicit_error_group"]["outcome"],
            "source_ref": explicit_error_ref,
            "arm_index": inventory["explicit_error_group"]["arm_index"],
            "applies_to_row": False,
        },
    ]
    runtime_error_paths = {
        "status": "unreviewed",
        "source_refs": [schema_ref, normal_ref, forced_ref],
        "note": inventory["runtime_error_paths"]["note"],
    }
    return {
        "review_status": "source_exhaustive_dispatch_path",
        "scope": (
            "exact field/index schema membership, normal base/index evaluation order, forced child continuation, "
            "and typed-view mismatch guards; lookup errors remain unreviewed"
        ),
        "branches": [{
            "tag": tag,
            "variant": variant,
            "ordinal": row["ordinal"],
            "normal_behavior": normal_arm["behavior"],
            "normal_route_ref": normal_ref,
            "forced_behavior": forced_arm["behavior"],
            "forced_route_ref": forced_ref,
            "normal_wrapper_route_ref": normal_wrapper_ref,
            "frame_apply_classifier_ref": classifier_ref,
        }],
        "branches_exhaustive": True,
        "no_fallback": None,
        "fallbacks": fallbacks,
        "runtime_error_paths": runtime_error_paths,
        "source_refs": {
            **source_refs,
            "field_index_schema": [schema_ref],
            "field_index_normal_dispatch": [normal_wrapper_ref, normal_ref, normal_fallback_ref, normal_view_ref, classifier_ref],
            "field_index_forced_dispatch": [forced_ref, forced_view_ref, explicit_error_ref],
        },
        "unreviewed_branch_behavior": runtime_error_paths["note"],
    }


def build_fallback_path_map(
    row: dict[str, Any],
    source_refs: dict[str, list[str]],
    statement_inventory: dict[str, Any] | None = None,
    typed_scalar_inventory: dict[str, Any] | None = None,
    eager_atom_inventory: dict[str, Any] | None = None,
    frame_constructor_inventory: dict[str, Any] | None = None,
    collection_construction_inventory: dict[str, Any] | None = None,
    result_constructor_inventory: dict[str, Any] | None = None,
    call_frame_inventory: dict[str, Any] | None = None,
    field_index_inventory: dict[str, Any] | None = None,
) -> dict[str, Any]:
    if row["kind"] == "Stmt":
        if statement_inventory is None:
            raise EvidenceError(f"missing AST statement inventory for {row['tag']}")
        branch = next(
            (item for item in statement_inventory["branches"] if item["tag"] == row["tag"]),
            None,
        )
        if branch is None:
            raise EvidenceError(f"statement AST inventory has no exact branch for {row['tag']}")
        owner = statement_inventory["owner"]
        return {
            "review_status": "source_exhaustive_top_level_tag_dispatch",
            "scope": "eval_statement top-level FullTag match; arm-body semantics remain in runtime evidence",
            "branches": [
                {
                    "tag": branch["tag"],
                    "pattern": branch["pattern"],
                    "source_ref": f"{owner}:{branch['line']}",
                    "arm_index": branch["arm_index"],
                }
            ],
            "branches_exhaustive": True,
            "no_fallback": True,
            "no_fallback_scope": "valid statement tags",
            "fallbacks": [
                {
                    "pattern": fallback["pattern"],
                    "outcome": fallback["outcome"],
                    "source_ref": f"{owner}:{fallback['line']}",
                    "arm_index": fallback["arm_index"],
                    "reachable_for_valid_statement_tag": False,
                }
                for fallback in statement_inventory["fallbacks"]
            ],
            "source_refs": source_refs,
            "unreviewed_branch_behavior": "nested branch and arm-body behavior are outside this tag-dispatch inventory",
        }
    if row["kind"] in {"Int", "Bool"}:
        if typed_scalar_inventory is None:
            raise EvidenceError(f"missing AST typed scalar inventory for {row['tag']}")
        return typed_scalar_fallback_path_map(row, source_refs, typed_scalar_inventory)
    if row.get("eager_atom"):
        if eager_atom_inventory is None:
            raise EvidenceError(f"missing AST EagerAtom inventory for {row['tag']}")
        return eager_atom_fallback_path_map(row, source_refs, eager_atom_inventory)
    if row["tag"] in FRAME_CONSTRUCTOR_ROWS:
        if frame_constructor_inventory is None:
            raise EvidenceError(f"missing AST FrameApply constructor inventory for {row['tag']}")
        return frame_constructor_fallback_path_map(row, source_refs, frame_constructor_inventory)
    if row["tag"] in COLLECTION_CONSTRUCTION_ROWS:
        if collection_construction_inventory is None:
            raise EvidenceError(f"missing AST collection constructor inventory for {row['tag']}")
        return collection_construction_fallback_path_map(
            row, source_refs, collection_construction_inventory
        )
    if row["tag"] in RESULT_CONSTRUCTOR_ROWS:
        if result_constructor_inventory is None:
            raise EvidenceError(f"missing AST result constructor inventory for {row['tag']}")
        return result_constructor_fallback_path_map(row, source_refs, result_constructor_inventory)
    if row["tag"] in CALL_FRAME_ROWS:
        if call_frame_inventory is None:
            raise EvidenceError(f"missing AST call frame inventory for {row['tag']}")
        return call_frame_fallback_path_map(row, source_refs, call_frame_inventory)
    if row["tag"] in FIELD_INDEX_ROWS:
        if field_index_inventory is None:
            raise EvidenceError(f"missing AST field/index inventory for {row['tag']}")
        return field_index_fallback_path_map(row, source_refs, field_index_inventory)
    known_branches = row.get("fallback_path_map", [])
    return {
        "review_status": "specific_path_mapped" if known_branches else "unreviewed",
        "branches": known_branches,
        "branches_exhaustive": False,
        "no_fallback": None,
        "source_refs": source_refs,
        "unreviewed_branch_behavior": "fallback branches have not been exhaustively classified from the row's dispatch body",
    }


def typed_scalar_fallback_path_map(
    row: dict[str, Any],
    source_refs: dict[str, list[str]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    tag = row["tag"]
    family = row["kind"]
    schema = next((item for item in inventory["schema_rows"] if item["tag"] == tag), None)
    if schema is None or schema["family"] != family or schema["variant"] != row["variant"]:
        raise EvidenceError(f"typed scalar AST inventory has no exact schema row for {tag}")
    classifier = next(
        (item for item in inventory["classifiers"][family] if item["tag"] == tag),
        None,
    )
    if classifier is None:
        raise EvidenceError(f"typed scalar AST inventory has no exact {family} classifier for {tag}")
    view_arm = next(
        (
            item for item in inventory["view_arms"]
            if item["family"] == family and item["variant"] == row["variant"]
        ),
        None,
    )
    if view_arm is None:
        raise EvidenceError(f"typed scalar AST inventory has no exact {family} view arm for {tag}")
    frame_group = next(
        (item for item in inventory["frame_groups"] if tag in item["tags"]),
        None,
    )
    if frame_group is None:
        raise EvidenceError(f"typed scalar AST inventory has no explicit-frame group for {tag}")
    normal_check = next((item for item in inventory["normal_checks"] if item["family"] == family), None)
    if normal_check is None:
        raise EvidenceError(f"typed scalar AST inventory has no normal wrapper for {tag}")
    wrong_union = next((item for item in inventory["wrong_union_errors"] if item["family"] == family), None)
    if wrong_union is None:
        raise EvidenceError(f"typed scalar AST inventory has no defensive wrong-union error for {tag}")

    paths = inventory["source_paths"]
    path_for_source = {
        "explicit_dispatch": paths["forced_dispatch"],
        "normal_dispatch": paths["normal_dispatch"],
        "typed_values": paths["typed_values"],
    }
    row_fallbacks = []
    for fallback in inventory["fallbacks"]:
        applies_to_row = tag in fallback["applies_to"]
        if applies_to_row or fallback["scope"] == "tag outside the typed scalar cohort":
            source_path = path_for_source[fallback["source"]]
            row_fallbacks.append({
                "scope": fallback["scope"],
                "outcome": fallback["outcome"],
                "source_ref": f"{source_path}:{fallback['line']}",
                "arm_index": fallback["arm_index"],
                "applies_to_row": applies_to_row,
            })
    value_error = {
        "family": wrong_union["family"],
        "outcome": wrong_union["outcome"],
        "source_ref": f"{paths['typed_values']}:{wrong_union['line']}",
        "scope": "defensive typed result-union mismatch",
    }
    normal_refs = [
        f"{paths['normal_dispatch']}:{classifier['line']}",
        f"{paths['normal_dispatch']}:{normal_check['line']}",
        f"{paths['typed_values']}:{view_arm['line']}",
        f"{paths['typed_values']}:{wrong_union['line']}",
    ]
    forced_refs = [
        f"{paths['forced_dispatch']}:{inventory['forced_entry']['line']}",
        f"{paths['forced_dispatch']}:{frame_group['line']}",
        *[fallback["source_ref"] for fallback in row_fallbacks],
    ]
    runtime_error_paths = {
        "status": "unreviewed",
        "source_refs": [f"{paths['typed_values']}:{view_arm['line']}"],
        "note": (
            "typed view selection is source-exhaustive; decode, slot/type, checked operation, "
            "string helper, and typed work-stack errors are not classified per row"
        ),
    }
    return {
        "review_status": "source_exhaustive_dispatch_path",
        "scope": (
            "normal typed classifier and view dispatch plus forced-frame operation/delegation selection; "
            "does not claim absence of runtime errors or semantic equivalence"
        ),
        "branches": [{
            "tag": tag,
            "family": family,
            "view_variant": row["variant"],
            "normal_wrapper": normal_check["wrapper"],
            "normal_classifier_ref": f"{paths['normal_dispatch']}:{classifier['line']}",
            "normal_check_ref": f"{paths['normal_dispatch']}:{normal_check['line']}",
            "typed_view_ref": f"{paths['typed_values']}:{view_arm['line']}",
            "forced_behavior": frame_group["behavior"],
            "forced_group_tags": frame_group["tags"],
            "forced_entry_ref": f"{paths['forced_dispatch']}:{inventory['forced_entry']['line']}",
            "forced_group_ref": f"{paths['forced_dispatch']}:{frame_group['line']}",
        }],
        "branches_exhaustive": True,
        "no_fallback": None,
        "fallbacks": row_fallbacks,
        "defensive_wrong_union": [value_error],
        "runtime_error_paths": runtime_error_paths,
        "source_refs": {
            **source_refs,
            "normal_typed_dispatch": normal_refs,
            "forced_typed_dispatch": forced_refs,
            "typed_view": [f"{paths['typed_values']}:{view_arm['line']}"],
            "defensive_wrong_union": [value_error["source_ref"]],
        },
        "unreviewed_branch_behavior": runtime_error_paths["note"],
    }


def eager_atom_fallback_path_map(
    row: dict[str, Any],
    source_refs: dict[str, list[str]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    tag = row["tag"]
    schema = next(
        (
            item for item in inventory["schema_rows"]
            if item["tag"] == tag and item["variant"] == row["variant"]
        ),
        None,
    )
    if schema is None or schema["ordinal"] != row["ordinal"]:
        raise EvidenceError(f"EagerAtom AST inventory has no exact schema row for {tag}")
    helper_arm = next(
        (arm for arm in inventory["helper_arms"] if arm["variant"] == row["variant"] and arm["tag"] == tag),
        None,
    )
    if helper_arm is None:
        raise EvidenceError(f"EagerAtom AST inventory has no exact helper arm for {tag}")
    boundaries = inventory["helper_boundaries"]
    decode = next(boundary for boundary in boundaries if boundary["kind"] == "ExprView decode")
    row_boundaries = [decode]
    if tag == "ExprPath":
        row_boundaries.append(next(boundary for boundary in boundaries if boundary["kind"] == "PathValue conversion"))
    if tag == "ExprParam":
        row_boundaries.append(next(boundary for boundary in boundaries if boundary["kind"] == "ExprParam slot access and freeze"))

    paths = inventory["source_paths"]
    normal_ref = f"{paths['normal_dispatch']}:{inventory['normal_route']['line']}"
    normal_helper_ref = f"{paths['normal_dispatch']}:{inventory['normal_route']['helper_line']}"
    normal_generic_ref = f"{paths['normal_dispatch']}:{inventory['normal_route']['generic_line']}"
    forced_ref = f"{paths['forced_dispatch']}:{inventory['forced_route']['line']}"
    forced_helper_ref = f"{paths['forced_dispatch']}:{inventory['forced_route']['helper_line']}"
    forced_generic_ref = f"{paths['forced_dispatch']}:{inventory['forced_route']['generic_line']}"
    helper_view_ref = f"{paths['normal_dispatch']}:{helper_arm['line']}"
    helper_fallback_ref = f"{paths['normal_dispatch']}:{inventory['helper_fallback']['line']}"
    explicit_error_ref = f"{paths['forced_dispatch']}:{inventory['explicit_error_group']['line']}"
    classification = inventory["classification_rules"]
    classification_refs = [
        f"{paths['schema']}:{classification['default_false_line']}",
        f"{paths['schema']}:{classification['eager_true_line']}",
        f"{paths['schema']}:{classification['expansion_line']}",
    ]
    row_fallbacks = [
        {
            "scope": inventory["helper_fallback"]["scope"],
            "outcome": inventory["helper_fallback"]["outcome"],
            "source_ref": helper_fallback_ref,
            "arm_index": inventory["helper_fallback"]["arm_index"],
            "applies_to_row": False,
        },
        {
            "scope": inventory["explicit_error_group"]["scope"],
            "outcome": inventory["explicit_error_group"]["outcome"],
            "source_ref": explicit_error_ref,
            "arm_index": inventory["explicit_error_group"]["arm_index"],
            "applies_to_row": False,
        },
    ]
    boundary_evidence = [
        {
            "kind": boundary["kind"],
            "outcome": boundary["outcome"],
            "source_refs": [
                f"{paths[reference['source']]}:{reference['line']}"
                for reference in boundary["references"]
            ],
        }
        for boundary in row_boundaries
    ]
    boundary_refs = [
        source_ref
        for boundary in boundary_evidence
        for source_ref in boundary["source_refs"]
    ]
    runtime_error_paths = {
        "status": "source_exhaustive_boundary_inventory",
        "boundaries": boundary_evidence,
        "source_refs": boundary_refs,
        "note": "The source inventory classifies helper error conversion and slot mutation; runtime conformance still needs matching normal and forced receipts.",
    }
    schema_ref = f"{paths['schema']}:{schema['line']}"
    value_provenance = {
        "schema_fields": schema["fields"],
        "input": helper_arm["input"],
        "transformation": helper_arm["transformation"],
        "output": helper_arm["output"],
        "source_ref": helper_view_ref,
    }
    operand_liveness = {
        "status": "not_applicable",
        "child_paths": [],
        "schema_fields": schema["fields"],
        "reason": (
            "Schema fields are data or a local slot index; no field has a BuildExprId child, "
            "and the exact eager view arm constructs or reads a value without evaluating an expression operand."
        ),
        "source_refs": [schema_ref, helper_view_ref],
    }
    return {
        "review_status": "source_exhaustive_dispatch_path",
        "scope": (
            "schema-generated EagerAtom classification, normal recursive value selection, forced-frame eager fast path, "
            "row-specific value provenance, and helper error boundaries"
        ),
        "branches": [{
            "tag": tag,
            "variant": row["variant"],
            "ordinal": row["ordinal"],
            "normal_behavior": "recursive_eager_atom_value",
            "normal_route_ref": normal_ref,
            "normal_helper_ref": normal_helper_ref,
            "normal_generic_frame_apply_ref": normal_generic_ref,
            "classification_refs": classification_refs,
            "helper_view_ref": helper_view_ref,
            "value_provenance": value_provenance,
            "forced_behavior": "eager_atom_frame_fast_path",
            "forced_route_ref": forced_ref,
            "forced_helper_ref": forced_helper_ref,
            "forced_generic_frame_apply_ref": forced_generic_ref,
        }],
        "branches_exhaustive": True,
        "no_fallback": None,
        "fallbacks": row_fallbacks,
        "value_provenance": value_provenance,
        "operand_liveness": operand_liveness,
        "runtime_error_paths": runtime_error_paths,
        "source_refs": {
            **source_refs,
            "eager_atom_normal_dispatch": [normal_ref, normal_helper_ref, normal_generic_ref],
            "eager_atom_forced_dispatch": [forced_ref, forced_helper_ref, forced_generic_ref],
            "eager_atom_classification": classification_refs,
            "eager_atom_helper_view": [helper_view_ref],
            "eager_atom_value_provenance": [helper_view_ref],
            "eager_atom_error_boundaries": boundary_refs,
            "eager_atom_no_child_operands": [schema_ref, helper_view_ref],
            "eager_atom_fallbacks": [helper_fallback_ref, explicit_error_ref],
        },
        "unreviewed_branch_behavior": runtime_error_paths["note"],
    }


def frame_constructor_fallback_path_map(
    row: dict[str, Any],
    source_refs: dict[str, list[str]],
    inventory: dict[str, Any],
) -> dict[str, Any]:
    tag = row["tag"]
    schema = next(
        (
            item for item in inventory["schema_rows"]
            if item["tag"] == tag and item["variant"] == row["variant"]
        ),
        None,
    )
    if schema is None or schema["ordinal"] != row["ordinal"]:
        raise EvidenceError(f"FrameApply constructor AST inventory has no exact schema row for {tag}")
    paths = inventory["source_paths"]
    normal_route = inventory["normal_route"]
    forced_route = inventory["forced_route"]
    frame_classifier = next(
        (branch for branch in inventory["classifiers"]["FrameApply"] if branch["tag"] == tag),
        None,
    )
    if frame_classifier is None:
        raise EvidenceError(f"FrameApply constructor classifier has no branch for {tag}")
    child_arm = next(
        (arm for arm in inventory["child_arms"] if arm["variant"] == row["variant"]),
        None,
    )
    value_arm = next(
        (arm for arm in inventory["value_arms"] if arm["variant"] == row["variant"]),
        None,
    )
    if child_arm is None or value_arm is None:
        raise EvidenceError(f"FrameApply constructor inventory has incomplete helper arms for {tag}")

    normal_ref = f"{paths['normal_dispatch']}:{normal_route['line']}"
    normal_helper_ref = f"{paths['normal_dispatch']}:{normal_route['helper_line']}"
    classifier_ref = f"{paths['normal_dispatch']}:{frame_classifier['line']}"
    forced_ref = f"{paths['forced_dispatch']}:{forced_route['line']}"
    forced_helper_ref = f"{paths['forced_dispatch']}:{forced_route['helper_line']}"
    child_ref = f"{paths['normal_dispatch']}:{child_arm['line']}"
    value_ref = f"{paths['normal_dispatch']}:{value_arm['line']}"
    value_decode_ref = f"{paths['normal_dispatch']}:{inventory['value_decode_line']}"
    explicit_error_ref = (
        f"{paths['forced_dispatch']}:{inventory['explicit_error_group']['line']}"
    )
    row_fallbacks = [
        {
            "scope": fallback["scope"],
            "outcome": fallback["outcome"],
            "source_ref": f"{paths['normal_dispatch']}:{fallback['line']}",
            "arm_index": fallback["arm_index"],
            "applies_to_row": False,
        }
        for fallback in inventory["helper_fallbacks"]
    ]
    row_fallbacks.append({
        "scope": inventory["explicit_error_group"]["scope"],
        "outcome": inventory["explicit_error_group"]["outcome"],
        "source_ref": explicit_error_ref,
        "arm_index": inventory["explicit_error_group"]["arm_index"],
        "applies_to_row": False,
    })
    runtime_error_paths = {
        "status": "unreviewed",
        "source_refs": [value_decode_ref, value_ref],
        "note": (
            "ExprView decode and row-specific function reference or path construction outcomes "
            "remain outside the dispatch inventory"
        ),
    }
    return {
        "review_status": "source_exhaustive_dispatch_path",
        "scope": (
            "exact schema membership, normal generic FrameApply selection, forced child/value helper "
            "selection, and the defensive ExprViewKind error group; value construction errors remain unreviewed"
        ),
        "branches": [{
            "tag": tag,
            "variant": row["variant"],
            "ordinal": row["ordinal"],
            "normal_behavior": "indexed_frame_apply",
            "normal_route_ref": normal_ref,
            "normal_helper_ref": normal_helper_ref,
            "frame_apply_classifier_ref": classifier_ref,
            "forced_behavior": "explicit_frame_apply",
            "forced_route_ref": forced_ref,
            "forced_child_helper_ref": forced_helper_ref,
            "child_view_ref": child_ref,
            "child_behavior": child_arm["behavior"],
            "value_view_ref": value_ref,
            "value_behavior": value_arm["behavior"],
        }],
        "branches_exhaustive": True,
        "no_fallback": None,
        "fallbacks": row_fallbacks,
        "runtime_error_paths": runtime_error_paths,
        "source_refs": {
            **source_refs,
            "frame_constructor_normal_dispatch": [normal_ref, normal_helper_ref, classifier_ref],
            "frame_constructor_forced_dispatch": [forced_ref, forced_helper_ref],
            "frame_constructor_child_view": [child_ref],
            "frame_constructor_value_view": [value_ref, value_decode_ref],
            "frame_constructor_fallbacks": [
                *[f"{paths['normal_dispatch']}:{fallback['line']}" for fallback in inventory["helper_fallbacks"]],
                explicit_error_ref,
            ],
        },
        "unreviewed_branch_behavior": runtime_error_paths["note"],
    }


def xsh_test_ids(root: pathlib.Path, test_files: list[str]) -> set[str]:
    ids: set[str] = set()
    for relative in sorted(set(test_files)):
        path = root / relative
        if not path.is_file():
            raise EvidenceError(f"oracle source file is missing: {relative}")
        for name in re.findall(r"(?m)^\s*test\s+([A-Za-z][A-Za-z0-9_]*)\b", path.read_text()):
            ids.add(f"{relative}::{name}")
    return ids


def rust_test_ids(root: pathlib.Path, source_paths: list[str], modules: dict[str, str]) -> set[str]:
    ids: set[str] = set()
    for relative in sorted(set(source_paths)):
        path = root / relative
        if not path.is_file():
            raise EvidenceError(f"Rust oracle source file is missing: {relative}")
        source = path.read_text()
        module = modules.get(relative)
        if module is None:
            raise EvidenceError(f"Rust oracle module path is missing for {relative}")
        for name in re.findall(r"(?m)^\s*#\[test\]\s*\n\s*fn\s+([A-Za-z][A-Za-z0-9_]*)\s*\(", source):
            ids.add(f"{module}::{name}")
    return ids


def validate_oracle_id(oracle_id: str, known_ids: set[str]) -> None:
    if oracle_id not in known_ids:
        raise EvidenceError(f"oracle ID does not exist in source: {oracle_id}")


def validate_mapping_coverage(
    schema_tags: list[str], runtime_tags: set[str], static_tags: list[str]
) -> None:
    if len(schema_tags) != len(set(schema_tags)):
        raise EvidenceError("duplicate schema row tags")
    if len(static_tags) != len(set(static_tags)):
        raise EvidenceError("duplicate static-or-rejection row mappings")
    schema_set = set(schema_tags)
    static_set = set(static_tags)
    overlap = sorted(runtime_tags & static_set)
    missing = sorted(schema_set - runtime_tags - static_set)
    stale_static = sorted(static_set - schema_set)
    stale_runtime = sorted(runtime_tags - schema_set)
    if overlap or missing or stale_static or stale_runtime:
        raise EvidenceError(
            "row proof mapping mismatch: "
            f"overlap={overlap}; unmapped={missing}; "
            f"stale_static={stale_static}; stale_runtime={stale_runtime}"
        )


def native_commands(oracle_id: str) -> dict[str, list[str]]:
    executable = "target/x86_64-unknown-linux-musl/verification/xsht"
    return {
        "normal": [executable, "test", "--exact", oracle_id],
        "forced": [
            "env",
            "XSH_INTERNAL_TEST_FORCE_FRAMES=1",
            executable,
            "test",
            "--exact",
            oracle_id,
        ],
    }


def rust_test_command(oracle_id: str) -> list[str]:
    return [
        "cargo",
        "test",
        "-p",
        "xsh",
        "--lib",
        "--target",
        "x86_64-unknown-linux-musl",
        oracle_id,
        "--",
        "--exact",
    ]


def receipt_step_matches(
    receipt: dict[str, Any],
    step: dict[str, Any],
    *,
    expected_head: str,
    expected_tree: str,
    oracle_id: str,
    mode: str,
    expected_xsht_sha256: str,
) -> bool:
    identity = receipt.get("source_expected", {})
    if identity.get("commit") != expected_head or identity.get("tree") != expected_tree:
        return False
    if step.get("exit_code") != 0:
        return False
    hashes = receipt.get("hashes", {})
    before = hashes.get("binaries_before_tests", {}).get("xsht-verification")
    after = hashes.get("binaries_after_tests", {}).get("xsht-verification")
    if not expected_xsht_sha256 or before != expected_xsht_sha256 or after != before:
        return False
    argv = step.get("argv", [])
    if not isinstance(argv, list) or any(not isinstance(argument, str) for argument in argv):
        return False
    candidate_suffix = "target/x86_64-unknown-linux-musl/verification/xsht"
    binary_indexes = [
        index
        for index, argument in enumerate(argv)
        if pathlib.PurePath(argument).name == "xsht"
        and pathlib.PurePath(argument).as_posix().endswith(candidate_suffix)
    ]
    if len(binary_indexes) != 1 or argv[binary_indexes[0] + 1 :] != ["test", "--exact", oracle_id]:
        return False
    step_id = step.get("id")
    if not step_id:
        return False
    source_check_rows = receipt.get("source_checks", [])
    if not isinstance(source_check_rows, list):
        return False
    source_checks: dict[str, dict[str, Any]] = {}
    for check in source_check_rows:
        if not isinstance(check, dict):
            return False
        where = check.get("where")
        if not isinstance(where, str) or not where or where in source_checks:
            return False
        source_checks[where] = check
    for phase in ("before", "after"):
        check = source_checks.get(f"{phase}:{step_id}", {})
        if (
            check.get("head") != expected_head
            or check.get("tree") != expected_tree
            or check.get("clean") is not True
        ):
            return False
    env = step.get("env", {})
    markers = [
        value.split("=", 1)[1]
        for value in argv
        if value.startswith("XSH_INTERNAL_TEST_FORCE_FRAMES=")
    ]
    if "XSH_INTERNAL_TEST_FORCE_FRAMES" in env:
        markers.append(env["XSH_INTERNAL_TEST_FORCE_FRAMES"])
    if len(markers) > 1 or any(value != "1" for value in markers):
        return False
    force_is_set = bool(markers)
    return force_is_set == (mode == "forced") and bool(step.get("log_sha256"))


def receipt_steps_are_unique(steps: list[dict[str, Any]]) -> bool:
    ids = [step.get("id") for step in steps]
    commands = [
        (tuple(step.get("argv", [])), json.dumps(step.get("env", {}), sort_keys=True))
        for step in steps
    ]
    return (
        all(ids)
        and len(ids) == len(set(ids))
        and len(commands) == len(set(commands))
    )


def receipt_corpus_steps_are_unique(receipts: list[dict[str, Any]]) -> bool:
    return receipt_steps_are_unique(
        [step for receipt in receipts for step in receipt.get("steps", [])]
    )


def defensive_path_gaps(paths: list[dict[str, Any]]) -> list[dict[str, Any]]:
    if not paths:
        raise EvidenceError("defensive paths must be inventoried explicitly")
    required = {"tag", "path", "modes", "reason", "source", "structural_note", "route_behaviors"}
    for path in paths:
        missing = sorted(required - path.keys())
        if missing or not path["modes"] or not path["source"]:
            raise EvidenceError(f"incomplete defensive path mapping: {path}; missing={missing}")
        if set(path["modes"]) != set(path["route_behaviors"]):
            raise EvidenceError(f"defensive path route modes do not match: {path}")
    return [
        {
            "tag": path["tag"],
            "path": path["path"],
            "mode": mode,
            "reason": path["reason"],
            "source": path["source"],
            "structural_note": path["structural_note"],
            "route_behavior": path["route_behaviors"][mode],
            "status": "uncovered_defensive_path",
        }
        for path in paths
        for mode in path["modes"]
    ]


def final_correctness_receipts(
    root: pathlib.Path,
    expected_head: str,
    expected_tree: str,
    rows: list[dict[str, Any]],
    expected_xsht_sha256: str,
) -> dict[str, list[dict[str, str]]]:
    """Credit only successful exact-ID steps from a source-identical launch receipt."""
    reusable: dict[str, list[dict[str, str]]] = {row["tag"]: [] for row in rows}
    receipt_root = root / ".work/final-review-evidence/final-correctness-gates"
    if not receipt_root.is_dir():
        return reusable
    imported: list[tuple[pathlib.Path, dict[str, Any]]] = []
    for path in sorted(receipt_root.glob("*/receipt.json")):
        try:
            receipt = json.loads(path.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        if isinstance(receipt, dict) and isinstance(receipt.get("steps"), list):
            imported.append((path, receipt))
    if not receipt_corpus_steps_are_unique([receipt for _, receipt in imported]):
        return reusable
    for path, receipt in imported:
        for row in rows:
            if row.get("proof_kind") != "runtime":
                continue
            for mode in ("normal", "forced"):
                for oracle_id in row["oracles"]["runtime"]:
                    for step in receipt.get("steps", []):
                        if not receipt_step_matches(
                            receipt,
                            step,
                            expected_head=expected_head,
                            expected_tree=expected_tree,
                            oracle_id=oracle_id,
                            mode=mode,
                            expected_xsht_sha256=expected_xsht_sha256,
                        ):
                            continue
                        log_name = step.get("log")
                        if not log_name:
                            continue
                        log_path = pathlib.Path(log_name)
                        if not log_path.is_absolute():
                            log_path = root / log_path
                        try:
                            resolved_log = log_path.resolve()
                            if not resolved_log.is_relative_to(root.resolve()):
                                continue
                            if sha256_file(resolved_log) != step["log_sha256"]:
                                continue
                        except OSError:
                            continue
                        reusable[row["tag"]].append(
                            {
                                "mode": mode,
                                "receipt": str(path.relative_to(root)),
                                "step": step.get("id", ""),
                                "log_sha256": step["log_sha256"],
                                "oracle_id": oracle_id,
                            }
                        )
    return reusable


def build_matrix(root: pathlib.Path, config: dict[str, Any], lock: dict[str, Any]) -> dict[str, Any]:
    source_paths = source_lock_paths(config)
    actual = source_snapshot(root, source_paths)
    if not actual["clean"]:
        raise EvidenceError(f"source worktree is dirty:\n{actual['status']}")
    source_lock = source_lock_status(actual, lock)

    schema_text = (root / SCHEMA_PATH).read_text()
    rows = parse_schema(schema_text)
    if len(rows) != EXPECTED_LIVE_COUNT:
        raise EvidenceError(f"expected {EXPECTED_LIVE_COUNT} live rows, found {len(rows)}")
    ordinals = {row["ordinal"] for row in rows}
    if sorted(set(range(158)) - ordinals) != ORDINAL_HOLES:
        raise EvidenceError("instruction ordinal holes differ from the six retired ordinals")
    arena_text = (root / ARENA_PATH).read_text()
    pipeline_stage_source = (root / PIPELINE_STAGE_SOURCE_PATH).read_text()
    operand_inventory = validate_operand_inventory(
        rows, OPERAND_MANIFEST.read_text(), arena_text, pipeline_stage_source
    )

    indexed_source = (root / config["indexed_dispatch"]).read_text()
    explicit_source = (root / config["explicit_dispatch"]).read_text()
    statement_inventory = statement_ast_inventory(
        root,
        rows,
        SCHEMA_PATH,
        config["explicit_dispatch"],
    )
    scalar_inventory = typed_scalar_ast_inventory(
        root,
        rows,
        config["indexed_dispatch"],
        config["explicit_dispatch"],
    )
    eager_inventory = eager_atom_ast_inventory(
        root,
        rows,
        config["indexed_dispatch"],
        config["explicit_dispatch"],
    )
    eager_operand_proof = validate_eager_atom_operand_structure(
        rows, eager_inventory, operand_inventory
    )
    frame_constructor_inventory = frame_constructor_ast_inventory(
        root,
        rows,
        config["indexed_dispatch"],
        config["explicit_dispatch"],
    )
    collection_construction_inventory = collection_construction_ast_inventory(
        root,
        rows,
        config["indexed_dispatch"],
        config["explicit_dispatch"],
    )
    result_constructor_inventory = result_constructor_ast_inventory(
        root,
        rows,
        config["indexed_dispatch"],
        config["explicit_dispatch"],
    )
    call_frame_inventory = call_frame_ast_inventory(
        root,
        rows,
        config["indexed_dispatch"],
        config["explicit_dispatch"],
    )
    field_index_inventory = field_index_ast_inventory(
        root,
        rows,
        config["indexed_dispatch"],
        config["explicit_dispatch"],
    )
    routed = route_rows(rows, indexed_source, explicit_source)
    for row in routed:
        syntax_inventory = operand_inventory["rows"][row["tag"]]
        row["syntax_operand_inventory"] = syntax_inventory
        if row["eager_atom"]:
            structural_justification = eager_operand_proof[row["tag"]]
            row["runtime_operand_order"] = {
                "status": "not_applicable",
                "fields": [],
                "child_paths": [],
                "reason": structural_justification["reason"],
                "structural_justification": structural_justification,
            }
        else:
            row["runtime_operand_order"] = {
                "status": "unresolved" if row["schema_fields"] else "not_applicable",
                "fields": [field["name"] for field in syntax_inventory["fields"] if field["child_paths"]],
                "child_paths": syntax_inventory["child_paths"],
                "reason": (
                    "schema child links and syntax arena shapes identify possible children, "
                    "but do not prove evaluator order"
                    if row["schema_fields"]
                    else "instruction row has no payload fields"
                ),
            }

    xsh_ids = xsh_test_ids(root, config["xsh_oracle_files"])
    rust_ids = rust_test_ids(root, config["rust_oracle_files"], config["rust_test_modules"])
    runtime_ids = xsh_ids | rust_ids
    for row in routed:
        for route in row["normal_routes"] + row["forced_routes"]:
            for oracle_id in route.get("test_ids", []):
                validate_oracle_id(oracle_id, rust_ids)
            route["test_commands"] = {
                oracle_id: rust_test_command(oracle_id)
                for oracle_id in route.get("test_ids", [])
            }
    candidate_ids_by_source: dict[tuple[str, tuple[str, ...]], set[str]] = {}
    for tag, candidate in config.get("candidate_oracles", {}).items():
        if tag not in {row["tag"] for row in routed}:
            raise EvidenceError(f"candidate oracle maps an unknown instruction row: {tag}")
        commit = candidate.get("source_commit")
        ids = candidate.get("ids", [])
        if not commit or not ids:
            raise EvidenceError(f"candidate oracle needs a source commit and IDs: {tag}")
        paths = [oracle_id.split("::", 1)[0] for oracle_id in ids]
        source_key = (commit, tuple(sorted(set(paths))))
        if source_key not in candidate_ids_by_source:
            candidate_ids_by_source[source_key] = committed_xsh_test_ids(root, commit, paths)
        missing = sorted(set(ids) - candidate_ids_by_source[source_key])
        if missing:
            raise EvidenceError(f"candidate oracle IDs are absent at {commit}: {missing}")
    order_candidates = config.get("source_order_candidates", {})
    for tag, evidence in order_candidates.items():
        if tag not in {row["tag"] for row in routed}:
            raise EvidenceError(f"source-order evidence names an unknown instruction row: {tag}")
        ids = evidence.get("ids", [])
        commit = evidence.get("source_commit")
        known_ids = runtime_ids
        if commit:
            paths = [oracle_id.split("::", 1)[0] for oracle_id in ids]
            source_key = (commit, tuple(sorted(set(paths))))
            if source_key not in candidate_ids_by_source:
                candidate_ids_by_source[source_key] = committed_xsh_test_ids(root, commit, paths)
            known_ids = candidate_ids_by_source[source_key]
        missing = sorted(set(ids) - known_ids)
        if not ids or missing:
            raise EvidenceError(f"invalid source-order oracle IDs for {tag}: {missing}")
    oracle_map = config["runtime_oracles"]
    static_tags = config["static_or_rejection_tags"]
    static_tag_set = set(static_tags)
    static_reasons = config.get("static_or_rejection_reasons")
    if (
        not isinstance(static_reasons, dict)
        or set(static_reasons) != static_tag_set
        or any(not isinstance(reason, str) or not reason.strip() for reason in static_reasons.values())
    ):
        raise EvidenceError("static/rejection reasons must name every and only static schema row")
    static_id = config["static_oracle_id"]
    validate_oracle_id(static_id, rust_ids)
    unknown_oracle_rows = sorted(set(oracle_map) - {row["tag"] for row in routed})
    if unknown_oracle_rows:
        raise EvidenceError(f"runtime mappings name rows outside the current schema: {unknown_oracle_rows}")
    all_tags = [row["tag"] for row in routed]
    validate_mapping_coverage(all_tags, set(oracle_map), static_tags)

    seen_assignments: set[str] = set()
    focus_tags = set(config["review_focus_tags"])
    unknown_focus = sorted(focus_tags - {row["tag"] for row in routed})
    if unknown_focus:
        raise EvidenceError(f"review-focus rows do not exist in current schema: {unknown_focus}")

    route_gaps: list[dict[str, Any]] = []
    if source_lock["status"] != "matched":
        route_gaps.append({
            "tag": "source",
            "path": "source_lock",
            "reason": (
                "current source commit/tree or locked file hashes differ from the preserved source lock; "
                "decisive sources are recorded but not sealed"
            ),
            "source_lock": source_lock,
        })
    for row in routed:
        tag = row["tag"]
        row["review_focus"] = tag in focus_tags
        if tag in seen_assignments:
            raise EvidenceError(f"duplicate row mapping: {tag}")
        seen_assignments.add(tag)
        oracle = oracle_map.get(tag)
        if oracle is None:
            if tag not in static_tag_set:
                raise EvidenceError(f"instruction row has no proof mapping: {tag}")
            row["proof_kind"] = "static_or_rejection_only"
            row["oracles"] = {"static": [static_id]}
            row["commands"] = {
                "static": [
                    "cargo",
                    "test",
                    "-p",
                    "xsh",
                    "--lib",
                    "--target",
                    "x86_64-unknown-linux-musl",
                    static_id,
                    "--",
                    "--exact",
                ]
            }
            row["path_status"] = "static_schema_only"
            row["static_or_rejection_reason"] = static_reasons[tag]
            continue

        oracle_ids = oracle.get("ids", [])
        if not oracle_ids:
            raise EvidenceError(f"empty runtime oracle list for {tag}")
        for oracle_id in oracle_ids:
            validate_oracle_id(oracle_id, runtime_ids)
        row["proof_kind"] = oracle.get("proof_kind", "runtime")
        row["oracles"] = {"runtime": oracle_ids}
        row["commands"] = {
            oracle_id: native_commands(oracle_id)
            for oracle_id in oracle_ids
        }
        row["oracle_scope"] = oracle.get("scope", "")
        row["oracle_mapping_status"] = "candidate_requires_tag_path_witness_and_receipt"
        row["path_status"] = "candidate_unwitnessed_and_unexecuted"

    for tag, candidate in config.get("candidate_oracles", {}).items():
        row = next(item for item in routed if item["tag"] == tag)
        candidate_commit = candidate["source_commit"]
        current_ids = [oracle_id for oracle_id in candidate["ids"] if oracle_id in runtime_ids]
        row.setdefault("candidate_oracles", []).append({
            "ids": candidate["ids"],
            "source_commit": candidate_commit,
            "scope": candidate.get("scope", ""),
            "status": "available_in_current_sources" if len(current_ids) == len(candidate["ids"]) else "not_in_current_head",
            "commands": {oracle_id: native_commands(oracle_id) for oracle_id in candidate["ids"]},
        })

    all_tags_set = set(all_tags)
    for row in routed:
        tag = row["tag"]
        order_candidate = order_candidates.get(tag)
        if order_candidate is not None:
            commit = order_candidate.get("source_commit")
            row["source_order_evidence"] = {
                "status": "current_test_source_only" if commit is None else "candidate_commit_source_only",
                "oracle_ids": order_candidate["ids"],
                "source_commit": commit,
                "scope": order_candidate.get("scope", ""),
                "fields": order_candidate.get("fields", []),
            }
            row["runtime_operand_order"]["source_order_candidate"] = {
                "oracle_ids": order_candidate["ids"],
                "source_commit": commit,
                "scope": order_candidate.get("scope", ""),
                "fields": order_candidate.get("fields", []),
            }
        row["normal_routes"] = [
            {
                **route,
                "oracle_ids": row.get("oracles", {}).get("runtime", []) + route.get("test_ids", []),
                "mapping_status": (
                    "row_candidate_requires_route_witness_and_receipt"
                    if row.get("oracles", {}).get("runtime")
                    else "source_test_candidate_pending_receipt"
                    if route.get("test_ids")
                    else "unmapped"
                ),
            }
            for route in row["normal_routes"]
        ]
        row["forced_routes"] = [
            {
                **route,
                "oracle_ids": row.get("oracles", {}).get("runtime", []) + route.get("test_ids", []),
                "mapping_status": (
                    "row_candidate_requires_route_witness_and_receipt"
                    if row.get("oracles", {}).get("runtime")
                    else "source_test_candidate_pending_receipt"
                    if route.get("test_ids")
                    else "unmapped"
                ),
            }
            for route in row["forced_routes"]
        ]
        row["fallback_path_map"].extend(
            dict(path) for path in config["unmapped_paths"] if path["tag"] == tag
        )
        row["fallback_path_map"] = build_fallback_path_map(
            row,
            fallback_source_refs(
                row,
                schema_text,
                indexed_source,
                explicit_source,
                config["indexed_dispatch"],
                config["explicit_dispatch"],
                statement_inventory if row["kind"] == "Stmt" else None,
            ),
            statement_inventory if row["kind"] == "Stmt" else None,
            scalar_inventory if row["kind"] in {"Int", "Bool"} else None,
            eager_inventory if row["eager_atom"] else None,
            frame_constructor_inventory if row["tag"] in FRAME_CONSTRUCTOR_ROWS else None,
            collection_construction_inventory
            if row["tag"] in COLLECTION_CONSTRUCTION_ROWS
            else None,
            result_constructor_inventory
            if row["tag"] in RESULT_CONSTRUCTOR_ROWS
            else None,
            call_frame_inventory
            if row["tag"] in CALL_FRAME_ROWS
            else None,
            field_index_inventory
            if row["tag"] in FIELD_INDEX_ROWS
            else None,
        )
        if row["eager_atom"]:
            proof = eager_operand_proof[tag]
            row["fallback_path_map"]["operand_liveness"] = proof
            row["fallback_path_map"]["source_refs"]["eager_atom_no_child_operands"] = [
                f"{eager_inventory['source_paths']['schema']}:{next(item['line'] for item in eager_inventory['schema_rows'] if item['tag'] == tag)}",
                f"{eager_inventory['source_paths']['normal_dispatch']}:{next(arm['line'] for arm in eager_inventory['helper_arms'] if arm['tag'] == tag)}",
            ]
        row["execution_paths"] = {
            "normal": row["normal_routes"],
            "forced": row["forced_routes"],
            "status": row["path_status"],
        }
        if row["proof_kind"] == "static_or_rejection_only":
            for mode in ("normal", "forced"):
                for route in row[f"{mode}_routes"]:
                    route_gaps.append({
                        "tag": tag,
                        "path": mode,
                        "route": route["name"],
                        "reason": (
                            "mapped route test has no source-matched final-correctness receipt"
                            if route.get("test_ids")
                            else "no exact row-specific executable oracle maps this route"
                        ),
                    })
        else:
            for mode in ("normal", "forced"):
                for route in row[f"{mode}_routes"]:
                    route_gaps.append({
                        "tag": tag,
                        "path": mode,
                        "route": route["name"],
                        "reason": "candidate test IDs lack an explicit source witness for this tag and route",
                    })
        if row["kind"] != "Stmt" and not row["fallback_path_map"].get("branches_exhaustive"):
            route_gaps.append({
                "tag": tag,
                "path": "fallback_branch_inventory",
                "reason": (
                    "fallback branches are not exhaustively reviewed; "
                    f"review_status={row['fallback_path_map']['review_status']}"
                ),
                "source_refs": row["fallback_path_map"]["source_refs"],
            })
        runtime_error_paths = row["fallback_path_map"].get("runtime_error_paths")
        if runtime_error_paths is not None and runtime_error_paths.get("status") not in {
            "reviewed", "source_exhaustive_boundary_inventory"
        }:
            route_gaps.append({
                "tag": tag,
                "path": "runtime_error_path_inventory",
                "reason": "source-exhaustive dispatch leaves row-specific runtime errors unreviewed",
                "source_refs": runtime_error_paths["source_refs"],
            })
        if row["runtime_operand_order"]["status"] == "unresolved" and row["declared_field_order"] and order_candidate is None:
            route_gaps.append({
                "tag": tag,
                "path": "runtime_operand_order",
                "reason": (
                    "schema field order is recorded but runtime evaluation order is not source-verified: "
                    + ", ".join(row["declared_field_order"])
                ),
            })
        elif row["runtime_operand_order"]["status"] == "unresolved" and row["declared_field_order"]:
            route_gaps.append({
                "tag": tag,
                "path": "runtime_operand_order",
                "reason": "candidate order test source has no source-matched execution receipt",
                "candidate_oracle_ids": order_candidate["ids"],
            })

    # Defensive paths are independent of the valid path's test mapping.
    for path in defensive_path_gaps(config["unmapped_paths"]):
        if path["tag"] not in all_tags_set:
            raise EvidenceError(f"defensive path names an unknown instruction row: {path['tag']}")
        if path not in route_gaps:
            route_gaps.append(path)

    missing_rows = sorted({row["tag"] for row in routed} - seen_assignments)
    extra_rows = sorted(seen_assignments - {row["tag"] for row in routed})
    if missing_rows or extra_rows:
        raise EvidenceError(f"row mapping mismatch: missing={missing_rows}; extra={extra_rows}")
    if len(seen_assignments) != len(routed):
        raise EvidenceError("row mapping count mismatch")

    xsht_path = root / "target/x86_64-unknown-linux-musl/verification/xsht"
    expected_xsht_sha256 = sha256_file(xsht_path) if xsht_path.is_file() else ""
    reusable = final_correctness_receipts(
        root, actual["head"], actual["tree"], routed, expected_xsht_sha256
    )
    for row in routed:
        row["reused_final_correctness_receipts"] = reusable[row["tag"]]
    # Candidate commands remain red until both modes have source-identical receipts.
    for row in routed:
        if row.get("proof_kind") != "runtime":
            continue
        receipts = row["reused_final_correctness_receipts"]
        for mode in ("normal", "forced"):
            if not any(receipt["mode"] == mode for receipt in receipts):
                route_gaps.append({
                    "tag": row["tag"],
                    "path": mode,
                    "reason": "candidate oracle lacks a verified final-correctness receipt",
                })
    return {
        "schema": 1,
        "source_lock": source_lock,
        "source": {
            "commit": actual["head"],
            "tree": actual["tree"],
            "clean": actual["clean"],
            "file_sha256": actual["files"],
            "schema_sha256": sha256_bytes(schema_text.encode()),
            "generator_sha256": sha256_file(HERE / "generate.py"),
            "evidence_config_sha256": sha256_file(CONFIG),
        },
        "inventory": {
            "live_rows": len(routed),
            "ordinal_min": min(row["ordinal"] for row in routed),
            "ordinal_max": max(row["ordinal"] for row in routed),
            "retired_ordinals": ORDINAL_HOLES,
            "review_focus_rows": len(focus_tags),
            "runtime_rows": sum(row["proof_kind"] == "runtime" for row in routed),
            "static_or_rejection_only_rows": sum(
                row["proof_kind"] == "static_or_rejection_only" for row in routed
            ),
            "operand_payload_rows": operand_inventory["payload_rows"],
            "operand_schema_fields": operand_inventory["payload_fields"],
            "operand_child_paths": operand_inventory["evaluable_child_paths"],
            "unresolved_runtime_operand_rows": sum(
                row["runtime_operand_order"]["status"] == "unresolved" for row in routed
            ),
            "typed_scalar_dispatch_rows": sum(
                row["kind"] in {"Int", "Bool"}
                and row["fallback_path_map"].get("review_status") == "source_exhaustive_dispatch_path"
                for row in routed
            ),
            "typed_scalar_runtime_error_rows_unreviewed": sum(
                row["kind"] in {"Int", "Bool"}
                and
                row["fallback_path_map"].get("runtime_error_paths", {}).get("status") == "unreviewed"
                for row in routed
            ),
            "eager_atom_dispatch_rows": sum(
                row["eager_atom"]
                and row["fallback_path_map"].get("review_status") == "source_exhaustive_dispatch_path"
                for row in routed
            ),
            "eager_atom_runtime_error_rows_unreviewed": sum(
                row["eager_atom"]
                and row["fallback_path_map"].get("runtime_error_paths", {}).get("status") == "unreviewed"
                for row in routed
            ),
            "frame_constructor_dispatch_rows": sum(
                row["tag"] in FRAME_CONSTRUCTOR_ROWS
                and row["fallback_path_map"].get("review_status") == "source_exhaustive_dispatch_path"
                for row in routed
            ),
            "frame_constructor_runtime_error_rows_unreviewed": sum(
                row["tag"] in FRAME_CONSTRUCTOR_ROWS
                and row["fallback_path_map"].get("runtime_error_paths", {}).get("status") == "unreviewed"
                for row in routed
            ),
            "collection_construction_dispatch_rows": sum(
                row["tag"] in COLLECTION_CONSTRUCTION_ROWS
                and row["fallback_path_map"].get("review_status") == "source_exhaustive_dispatch_path"
                for row in routed
            ),
            "collection_construction_runtime_error_rows_unreviewed": sum(
                row["tag"] in COLLECTION_CONSTRUCTION_ROWS
                and row["fallback_path_map"].get("runtime_error_paths", {}).get("status") == "unreviewed"
                for row in routed
            ),
            "result_constructor_dispatch_rows": sum(
                row["tag"] in RESULT_CONSTRUCTOR_ROWS
                and row["fallback_path_map"].get("review_status") == "source_exhaustive_dispatch_path"
                for row in routed
            ),
            "result_constructor_runtime_error_rows_unreviewed": sum(
                row["tag"] in RESULT_CONSTRUCTOR_ROWS
                and row["fallback_path_map"].get("runtime_error_paths", {}).get("status") == "unreviewed"
                for row in routed
            ),
            "call_frame_dispatch_rows": sum(
                row["tag"] in CALL_FRAME_ROWS
                and row["fallback_path_map"].get("review_status") == "source_exhaustive_dispatch_path"
                for row in routed
            ),
            "call_frame_runtime_error_rows_unreviewed": sum(
                row["tag"] in CALL_FRAME_ROWS
                and row["fallback_path_map"].get("runtime_error_paths", {}).get("status") == "unreviewed"
                for row in routed
            ),
            "field_index_dispatch_rows": sum(
                row["tag"] in FIELD_INDEX_ROWS
                and row["fallback_path_map"].get("review_status") == "source_exhaustive_dispatch_path"
                for row in routed
            ),
            "field_index_runtime_error_rows_unreviewed": sum(
                row["tag"] in FIELD_INDEX_ROWS
                and row["fallback_path_map"].get("runtime_error_paths", {}).get("status") == "unreviewed"
                for row in routed
            ),
            "fallback_branch_inventory_gaps": sum(
                gap["path"] == "fallback_branch_inventory" for gap in route_gaps
            ),
            "runtime_error_path_inventory_gaps": sum(
                gap["path"] == "runtime_error_path_inventory" for gap in route_gaps
            ),
        },
        "typed_scalar_dispatch_inventory": {
            "status": "source_exhaustive_dispatch_path",
            "rows": len(scalar_inventory["schema_rows"]),
            "tags": [row["tag"] for row in scalar_inventory["schema_rows"]],
            "runtime_error_paths": "unreviewed",
            "source_paths": scalar_inventory["source_paths"],
            "limitation": (
                "tag classifiers, typed view arms, normal wrappers, and explicit-frame groups are exact; "
                "per-operation runtime error outcomes are not exhaustively reviewed"
            ),
        },
        "eager_atom_dispatch_inventory": {
            "status": "source_exhaustive_dispatch_path",
            "rows": len(eager_inventory["schema_rows"]),
            "tags": [row["tag"] for row in eager_inventory["schema_rows"]],
            "runtime_error_paths": "source_exhaustive_boundary_inventory",
            "helper_boundaries": eager_inventory["helper_boundaries"],
            "source_paths": eager_inventory["source_paths"],
            "limitation": (
                "schema classification, value provenance, normal/forced dispatch routes, decode and path error "
                "conversions, slot bounds, and ExprParam freeze ordering are source-exact; matching normal and "
                "forced runtime receipts are absent"
            ),
        },
        "frame_constructor_dispatch_inventory": {
            "status": "source_exhaustive_dispatch_path",
            "rows": len(frame_constructor_inventory["schema_rows"]),
            "tags": [row["tag"] for row in frame_constructor_inventory["schema_rows"]],
            "runtime_error_paths": "unreviewed",
            "helper_fallbacks": frame_constructor_inventory["helper_fallbacks"],
            "explicit_error_group": frame_constructor_inventory["explicit_error_group"],
            "source_paths": frame_constructor_inventory["source_paths"],
            "limitation": (
                "schema membership, FrameApply route selection, and typed child/value view arms are exact; "
                "ExprView decoding and row-specific value-construction outcomes remain unreviewed"
            ),
        },
        "collection_construction_dispatch_inventory": {
            "status": "source_exhaustive_dispatch_path",
            "rows": len(collection_construction_inventory["schema_rows"]),
            "tags": [row["tag"] for row in collection_construction_inventory["schema_rows"]],
            "runtime_error_paths": "unreviewed",
            "helper_fallbacks": collection_construction_inventory["helper_fallbacks"],
            "normal_view_fallbacks": collection_construction_inventory["normal_view_fallbacks"],
            "explicit_error_group": collection_construction_inventory["explicit_error_group"],
            "source_paths": collection_construction_inventory["source_paths"],
            "limitation": (
                "schema membership, normal collection arms, forced FrameApply route selection, and typed-view "
                "mismatch guards are exact; decode, child evaluation, and row-specific construction errors "
                "remain unreviewed"
            ),
        },
        "result_constructor_dispatch_inventory": {
            "status": "source_exhaustive_dispatch_path",
            "rows": len(result_constructor_inventory["schema_rows"]),
            "tags": [row["tag"] for row in result_constructor_inventory["schema_rows"]],
            "runtime_error_paths": "unreviewed",
            "normal_view_fallbacks": result_constructor_inventory["normal_view_fallbacks"],
            "forced_view_fallbacks": result_constructor_inventory["forced_view_fallbacks"],
            "explicit_error_group": result_constructor_inventory["explicit_error_group"],
            "source_paths": result_constructor_inventory["source_paths"],
            "limitation": (
                "schema membership, normal result-constructor arms, forced continuation selection, and typed-view "
                "mismatch guards are exact; decode, child evaluation, and row-specific result construction errors "
                "remain unreviewed"
            ),
        },
        "call_frame_dispatch_inventory": {
            "status": "source_exhaustive_dispatch_path",
            "rows": len(call_frame_inventory["schema_rows"]),
            "tags": [row["tag"] for row in call_frame_inventory["schema_rows"]],
            "runtime_error_paths": "unreviewed",
            "normal_route": call_frame_inventory["normal_route"],
            "forced_arms": call_frame_inventory["forced_arms"],
            "typed_view_fallbacks": call_frame_inventory["typed_view_fallbacks"],
            "explicit_error_group": call_frame_inventory["explicit_error_group"],
            "source_paths": call_frame_inventory["source_paths"],
            "limitation": (
                "call schema membership, normal scheduled FrameControl routing, dedicated forced call view groups, "
                "and typed-view mismatch guards are exact; decode, callee resolution, argument evaluation, and "
                "call outcome failures remain unreviewed"
            ),
        },
        "field_index_dispatch_inventory": {
            "status": "source_exhaustive_dispatch_path",
            "rows": len(field_index_inventory["schema_rows"]),
            "tags": [row["tag"] for row in field_index_inventory["schema_rows"]],
            "normal_route": field_index_inventory["normal_route"],
            "runtime_error_paths": "unreviewed",
            "normal_view_fallbacks": field_index_inventory["normal_view_fallbacks"],
            "forced_view_fallbacks": field_index_inventory["forced_view_fallbacks"],
            "explicit_error_group": field_index_inventory["explicit_error_group"],
            "source_paths": field_index_inventory["source_paths"],
            "limitation": (
                "schema membership, normal child order, explicit child continuations, and typed-view mismatch "
                "guards are exact; decode and runtime lookup errors remain unreviewed"
            ),
        },
        "operand_order_inventory": {
            "status": "syntax_schema_only",
            "manifest": str(OPERAND_MANIFEST.relative_to(root)),
            "manifest_sha256": sha256_file(OPERAND_MANIFEST),
            "syntax_source": str(ARENA_PATH),
            "syntax_source_sha256": sha256_bytes(arena_text.encode()),
            "pipeline_stage_source": str(PIPELINE_STAGE_SOURCE_PATH),
            "pipeline_stage_source_sha256": sha256_bytes(pipeline_stage_source.encode()),
            "payload_rows": operand_inventory["payload_rows"],
            "payload_fields": operand_inventory["payload_fields"],
            "evaluable_child_paths": operand_inventory["evaluable_child_paths"],
            "arena_shape_checks": operand_inventory["arena_shape_checks"],
            "pipeline_stage_shape_checks": operand_inventory["pipeline_stage_shape_checks"],
            "runtime_order_status": "unresolved",
            "limitation": (
                "schema and syntax-child structure enumerate possible children; neither establishes "
                "runtime evaluation order or whether lowered compound payloads preserve that order"
            ),
        },
        "route_gaps": route_gaps,
        "unmapped_paths": config["unmapped_paths"],
        "readiness": "red" if route_gaps or not expected_xsht_sha256 else "green",
        "verified_xsht_sha256": expected_xsht_sha256 or None,
        "rows": sorted(routed, key=lambda row: row["ordinal"]),
        "receipt_policy": (
            "A final-correctness receipt is reused only when its expected commit/tree match this "
            "clean source, its before/after xsht-verification hashes match the locally verified candidate "
            "binary, its successful step argv is the exact row oracle command, step IDs are unique, "
            "the force-frame environment matches the mode, and a log hash is present. The launcher plan is not a receipt."
        ),
    }


def initialize_static_map(root: pathlib.Path, config: dict[str, Any]) -> list[str]:
    if "static_or_rejection_tags" in config:
        raise EvidenceError("static_or_rejection_tags is already present; refusing to replace it")
    rows = parse_schema((root / SCHEMA_PATH).read_text())
    all_tags = {row["tag"] for row in rows}
    runtime_tags = set(config["runtime_oracles"])
    unknown = sorted(runtime_tags - all_tags)
    if unknown:
        raise EvidenceError(f"runtime mappings name rows outside the current schema: {unknown}")
    static_tags = sorted(all_tags - runtime_tags)
    config["static_or_rejection_tags"] = static_tags
    CONFIG.write_text(json.dumps(config, indent=2, sort_keys=True) + "\n")
    return static_tags


def write_source_lock(root: pathlib.Path, config: dict[str, Any]) -> dict[str, Any]:
    snapshot = source_snapshot(root, source_lock_paths(config))
    if not snapshot["clean"]:
        raise EvidenceError(f"refusing to lock dirty source:\n{snapshot['status']}")
    source_lock_path = root / SOURCE_LOCK_RELATIVE
    source_lock_path.parent.mkdir(parents=True, exist_ok=True)
    source_lock_path.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n")
    return snapshot


def source_lock_paths(config: dict[str, Any]) -> list[str]:
    return sorted(set(
        config["source_paths"]
        + config.get("decisive_sources", [])
        + [str(SCHEMA_PATH), str(ARENA_PATH), str(PIPELINE_STAGE_SOURCE_PATH)]
    ))


def duplicate_json_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise EvidenceError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=pathlib.Path, default=pathlib.Path.cwd())
    parser.add_argument("--lock-source", action="store_true", help="write a clean source identity lock")
    parser.add_argument("--initialize-static-map", action="store_true", help="explicitly record rows with static/rejection-only evidence")
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        config = json.loads(CONFIG.read_text(), object_pairs_hook=duplicate_json_object)
        if args.lock_source:
            snapshot = write_source_lock(root, config)
            print(f"locked {snapshot['head']} {snapshot['tree']}")
            return 0
        if args.initialize_static_map:
            tags = initialize_static_map(root, config)
            print(f"recorded {len(tags)} explicit static/rejection-only rows")
            return 0
        lock_path = root / SOURCE_LOCK_RELATIVE
        lock = json.loads(lock_path.read_text(), object_pairs_hook=duplicate_json_object)
        matrix = build_matrix(root, config, lock)
        output = args.output or root / OUTPUT_RELATIVE
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(matrix, indent=2, sort_keys=True) + "\n")
    except (EvidenceError, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"semantic-route-matrix: {error}", file=sys.stderr)
        return 2
    print(
        f"wrote {len(matrix['rows'])} rows; "
        f"{len(matrix['route_gaps'])} route/path gaps; readiness={matrix['readiness']}"
    )
    return 0 if not matrix["route_gaps"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
