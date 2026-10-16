#!/usr/bin/env python3
"""Synthetic checks for row exhaustiveness, duplicate rejection, and receipt reuse."""

from __future__ import annotations

import csv
import copy
import io
import json
import pathlib
import re
import subprocess
import tempfile
import unittest
from unittest import mock

import generate


SCHEMA = """instruction_schema! {
    BuildExprRow => ExprView [Expr] {
        Int(value: i64) => ExprInt = 0 [0; Operands; FrameApply; EagerAtom];
        Binary { left: BuildExprId } => ExprBinary = 1 [0; Operands; FrameControl];
    }
    BuildStmtRow => StmtView [Stmt] {
        Print(args: Vec<BuildExprId>, flush: bool) => StmtPrint = 2 [0; Scheduled; FrameControl];
    }
}

pattern_schema! {}
"""


def expression_dispatch_fixture(rows: list[dict]) -> dict:
    expression_rows = [row for row in rows if row["kind"] == "Expr"]
    paths = {
        "schema": str(generate.SCHEMA_PATH),
        "normal_dispatch": generate._INDEXED_DISPATCH,
        "forced_dispatch": generate._EXPLICIT_DISPATCH,
    }

    def source(path: str, line: int) -> dict:
        return {
            "path": path,
            "start_line": line,
            "start_column": 0,
            "end_line": line,
            "end_column": 1,
            "snippet": "x",
        }

    selector_contract = {
        "forced_eager_atom": ("ExplicitFrames::eval_expr", "tag.is_eager_atom()"),
        "forced_expr_view_kind": ("ExplicitFrames::eval_expr", "tag.expr_view_kind()"),
        "forced_frame_apply": ("ExplicitFrames::eval_expr", "super::indexed_frame_apply_tag(tag)"),
        "forced_frame_entry": (
            "ExplicitFrames::eval_expr",
            generate.EXPRESSION_FORCED_FRAME_ENTRY_PREDICATE,
        ),
        "forced_in_place_condition": (
            "eval_expr.in_place",
            "!self.evaluator.force_frames\n            && execution.is_in_place(instruction)",
        ),
        "forced_in_place_bypass": ("ExplicitFrames::eval_expr", "in_place"),
        "forced_typed_scalar": ("ExplicitFrames::eval_expr", "is_typed_scalar_tag(tag)"),
        "normal_entry": (
            "eval_indexed_expr",
            "self.force_frames || !execution.is_in_place(instruction)",
        ),
        "normal_frame_dispatch": (
            "eval_indexed_expr_with_frames",
            "self.force_frames || !execution.is_in_place(instruction)",
        ),
        "normal_recursive_entry": (
            "eval_indexed_expr_inner",
            "else branch of self.force_frames || !execution.is_in_place(instruction)",
        ),
        "prepared_operand_value": ("indexed_frame_apply_value", "indexed_frame_apply_prepares_operands(tag)"),
        "recursive_eager_atom": ("eval_indexed_expr_inner", "tag.is_eager_atom()"),
        "recursive_expr_view_kind": ("eval_indexed_expr_inner", "ExprViewKind"),
        "recursive_selection": ("eval_indexed_expr_inner", "matches!(tag, FullTag::ExprIf | FullTag::ExprMatch)"),
        "recursive_tag_to_view_kind": ("FullTag::expr_view_kind", "Self::$tag => Some(ExprViewKind::$variant)"),
    }
    forced_order = {
        "forced_in_place_condition": 1,
        "forced_in_place_bypass": 2,
        "forced_frame_apply": 3,
        "forced_expr_view_kind": 4,
    }
    selectors = {
        name: {
            "path": path,
            "predicate": predicate,
            "source": source(
                paths["forced_dispatch"] if path.startswith("ExplicitFrames") or path == "eval_expr.in_place"
                else paths["normal_dispatch"],
                forced_order.get(name, line + 10),
            ),
        }
        for line, (name, (path, predicate)) in enumerate(selector_contract.items(), start=1)
    }
    forced_selector = {
        "path": selectors["forced_frame_entry"]["path"],
        "predicate": selectors["forced_frame_entry"]["predicate"],
        "source": selectors["forced_in_place_bypass"]["source"],
    }
    normal_selector = dict(selectors["normal_entry"])
    schema_rows = []
    dispatch_rows = []
    frame_apply_tags = []
    eager_tags = []
    for index, row in enumerate(expression_rows, start=10):
        eager = row["eager_atom"]
        apply = row["frame_strategy"] == "FrameApply" and not eager
        if eager:
            eager_tags.append(row["tag"])
        if apply:
            frame_apply_tags.append(row["tag"])
        schema = {
            "tag": row["tag"],
            "variant": row["variant"],
            "ordinal": row["ordinal"],
            "effects": "|".join(effect.strip() for effect in row["effects"].split("|")),
            "class": row["in_place"],
            "strategy": row["frame_strategy"],
            "classification": "EagerAtom" if eager else None,
            "line": index,
            "source": source(paths["schema"], index),
        }
        schema_rows.append(schema)
        child_dispatch = [{
            "function": "indexed_frame_apply_child",
            "variant": row["variant"],
            "predicate": f"ExprView::{row['variant']}",
            "behavior": "handled",
            "arm_index": 0,
            "defensive_unreachable": False,
            "source": source(paths["normal_dispatch"], index),
        }] if apply else []
        value_dispatch = [{
            "function": "indexed_frame_apply_value",
            "variant": row["variant"],
            "predicate": f"ExprView::{row['variant']}",
            "behavior": "handled",
            "arm_index": 0,
            "defensive_unreachable": False,
            "source": source(paths["normal_dispatch"], index),
        }] if apply else []
        dispatch_rows.append({
            "schema": schema,
            "recursive_dispatch": {
                "function": "eval_indexed_expr_inner",
                "variant": row["variant"],
                "predicate": f"ExprViewKind::{row['variant']}",
                "behavior": "eager_atom_value" if eager else "handled",
                "arm_index": None if eager else 0,
                "defensive_unreachable": False,
                "source": source(paths["normal_dispatch"], index),
            },
            "explicit_view_dispatch": {
                "function": "eval_expr_view",
                "variant": row["variant"],
                "predicate": f"ExprViewKind::{row['variant']}",
                "behavior": "frame_strategy_error" if apply or eager else "handled",
                "arm_index": 0,
                "defensive_unreachable": False,
                "source": source(paths["forced_dispatch"], index),
            },
            "forced_dispatch": {
                "function": "eval_expr",
                "variant": row["variant"],
                "predicate": "indexed_frame_apply_tag(tag)" if apply else "expr_view_kind",
                "behavior": "schedule_frame_apply_operands" if apply else "eager_atom_value" if eager else "handled",
                "arm_index": None if eager else 0,
                "defensive_unreachable": False,
                "source": source(paths["forced_dispatch"], index),
            },
            "indexed_frame_apply_selected": apply,
            "frame_strategy_overridden": False,
            "prepared_operands_selected": False,
            "normal_executor": [
                {
                    "path": "eval_indexed_expr_with_frames",
                    "predicate": selectors["normal_frame_dispatch"]["predicate"],
                    "source": source(paths["normal_dispatch"], 1),
                },
                {
                    "path": "eval_indexed_expr_inner",
                    "predicate": selectors["normal_recursive_entry"]["predicate"],
                    "source": source(paths["normal_dispatch"], 2),
                },
            ],
            "child_dispatch": child_dispatch,
            "value_dispatch": value_dispatch,
            "host_value_dispatch": [],
        })
    classifier = lambda tags, line: {
        "function": "classifier",
        "target": "tag",
        "pattern": "tag pattern",
        "tags": tags,
        "source": source(paths["normal_dispatch"], line),
        "predicate_source": source(paths["normal_dispatch"], line),
    }
    boundaries = [
        {
            "function": function,
            "kind": kind,
            "outcome": outcome,
            "predicate": (
                generate.EXPRESSION_DISPATCH_NORMAL_BYPASS_PREDICATE
                if (function, kind, outcome)
                == generate.EXPRESSION_DISPATCH_NORMAL_BYPASS_IDENTITY
                else "boundary predicate"
            ),
            "source": source(paths["normal_dispatch"], index),
        }
        for index, (function, kind, outcome) in enumerate(
            sorted(generate.EXPRESSION_DISPATCH_BOUNDARY_IDENTITIES),
            start=1,
        )
    ]
    return {
        "source_paths": paths,
        "source_classification_only": True,
        "runtime_behavior_proven": False,
        "schema_rows": schema_rows,
        "rows": dispatch_rows,
        "normal_selector": normal_selector,
        "forced_selector": forced_selector,
        "selectors": selectors,
        "classifiers": {
            "Bool": classifier([row["tag"] for row in rows if row["kind"] == "Bool"], 1),
            "EagerAtom": classifier(eager_tags, 2),
            "ExprViewKind": classifier([row["tag"] for row in expression_rows], 3),
            "FrameApply": classifier(frame_apply_tags, 4),
            "Int": classifier([row["tag"] for row in rows if row["kind"] == "Int"], 5),
            "prepared_operands": classifier([], 6),
        },
        "boundaries": boundaries,
    }


def operand_manifest(rows: list[dict]) -> str:
    headers = [
        "record", "tag", "ordinal", "kind", "variant", "field", "type", "policy", "child_paths"
    ]
    output = io.StringIO()
    writer = csv.DictWriter(output, fieldnames=headers, delimiter="\t", lineterminator="\n")
    writer.writeheader()
    for row in rows:
        writer.writerow({
            "record": "row", "tag": row["tag"], "ordinal": row["ordinal"],
            "kind": row["kind"], "variant": row["variant"],
        })
        for field in row["schema_fields"]:
            paths = generate.schema_child_paths(field["type"], field["name"])
            writer.writerow({
                "record": "field", "tag": row["tag"], "ordinal": row["ordinal"],
                "kind": row["kind"], "variant": row["variant"],
                "field": field["name"], "type": field["type"],
                "policy": generate.operand_policy(row["tag"], field["name"], field["type"], paths),
                "child_paths": ";".join(paths),
            })
    return output.getvalue()


def arena_shape_source() -> str:
    functions: dict[str, list[str]] = {}
    for shape, function_name in generate.ARENA_SHAPE_CONTRACTS.values():
        functions.setdefault(function_name, []).append(shape)
    return "\n".join(
        f"fn {function_name}() {{\n" + "\n".join(shapes) + "\n}"
        for function_name, shapes in functions.items()
    )


def typed_scalar_fixture() -> tuple[list[dict], dict]:
    rows_by_family = {
        "Int": [
            ("Int", "IntInt"), ("Slot", "IntSlot"), ("Binary", "IntBinary"),
            ("StrByteLenSlot", "IntStrByteLenSlot"),
            ("StrCountLinesSlot", "IntStrCountLinesSlot"),
            ("StrByteAtSlot", "IntStrByteAtSlot"),
        ],
        "Bool": [
            ("Bool", "BoolBool"), ("Slot", "BoolSlot"), ("Not", "BoolNot"),
            ("And", "BoolAnd"), ("Or", "BoolOr"), ("IntCompare", "BoolIntCompare"),
            ("StrPredicateSlot", "BoolStrPredicateSlot"),
            ("ContainsSlot", "BoolContainsSlot"),
            ("StrContainsSlot", "BoolStrContainsSlot"),
            ("TrimEmptySlot", "BoolTrimEmptySlot"),
            ("TrimStrPredicateSlot", "BoolTrimStrPredicateSlot"),
            ("LiteralCompareSlot", "BoolLiteralCompareSlot"),
        ],
    }
    rows = []
    schema_rows = []
    classifiers = {"Int": [], "Bool": []}
    view_arms = []
    ordinal = 0
    for family, variants in rows_by_family.items():
        for variant, tag in variants:
            row = {
                "kind": family,
                "variant": variant,
                "tag": tag,
                "ordinal": ordinal,
                "schema_fields": [],
            }
            rows.append(row)
            schema_rows.append({
                "family": family,
                "kind": family,
                "view": f"{family}View",
                "variant": variant,
                "tag": tag,
                "ordinal": ordinal,
                "line": ordinal + 1,
            })
            classifiers[family].append({"tag": tag, "line": ordinal + 1})
            view_arms.append({
                "family": family,
                "variant": variant,
                "line": ordinal + 1,
                "arm_index": len(view_arms),
            })
            ordinal += 1

    operation_tags = ["IntBinary", "IntStrByteAtSlot", "BoolNot", "BoolAnd", "BoolOr", "BoolIntCompare"]
    delegated_tags = [row["tag"] for row in rows if row["tag"] not in operation_tags]
    inventory = {
        "schema_rows": schema_rows,
        "classifiers": classifiers,
        "normal_checks": [
            {"family": "Int", "line": 100, "wrapper": "eval_indexed_typed_int", "result_variant": "Int"},
            {"family": "Bool", "line": 101, "wrapper": "eval_indexed_typed_bool", "result_variant": "Bool"},
        ],
        "forced_entry": {"family": "FrameApply", "line": 200, "wrapper": "eval_typed_scalar_expr"},
        "view_arms": view_arms,
        "frame_groups": [
            {"tags": operation_tags, "behavior": "frame_operation", "line": 201},
            {"tags": delegated_tags, "behavior": "verified_in_place_typed_delegation", "line": 202},
        ],
        "fallbacks": [
            {"applies_to": ["IntInt", "IntSlot", "IntBinary", "IntStrByteLenSlot", "IntStrCountLinesSlot", "IntStrByteAtSlot"], "scope": "malformed IntView payload", "outcome": "indexed_error: IntView decode failed", "source": "explicit_dispatch", "line": 203, "arm_index": 0},
            {"applies_to": ["BoolBool", "BoolSlot", "BoolNot", "BoolAnd", "BoolOr", "BoolIntCompare", "BoolStrPredicateSlot", "BoolContainsSlot", "BoolStrContainsSlot", "BoolTrimEmptySlot", "BoolTrimStrPredicateSlot", "BoolLiteralCompareSlot"], "scope": "malformed BoolView payload", "outcome": "indexed_error: BoolView decode failed", "source": "explicit_dispatch", "line": 204, "arm_index": 1},
            {"applies_to": [], "scope": "tag outside the typed scalar cohort", "outcome": "frame_strategy_error: unmatched tag", "source": "explicit_dispatch", "line": 205, "arm_index": 2},
            {"applies_to": delegated_tags, "scope": "delegated typed leaf without verifier in-place proof", "outcome": "frame_strategy_error: verifier in-place proof absent", "source": "explicit_dispatch", "line": 206, "arm_index": 3},
        ],
        "wrong_union_errors": [
            {"family": "Int", "outcome": "typed integer evaluator produced Bool", "line": 300},
            {"family": "Bool", "outcome": "typed boolean evaluator produced Int", "line": 301},
        ],
        "source_paths": {
            "schema": "src/runtime/eval/indexed/full/schema.rs",
            "normal_dispatch": "src/runtime/eval/lowered_run/indexed_run.rs",
            "typed_values": "src/runtime/eval/lowered_run/indexed_run/values.rs",
            "forced_dispatch": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
        },
        "source_semantics": {
            "status": "source_cues_checked_runtime_receipts_pending",
            "view_arm_refs": {
                f"{arm['family']}{arm['variant']}": f"src/runtime/eval/lowered_run/indexed_run/values.rs:{arm['line']}"
                for arm in view_arms
            },
            "runtime_semantic_refs": {
                tag: ["src/runtime/eval/lowered_run/indexed_run/values.rs:700"]
                for tag in operation_tags
            },
        },
    }
    return rows, inventory


def typed_scalar_source_fixture() -> tuple[str, list[dict]]:
    repo = pathlib.Path(__file__).resolve().parents[5]
    source_path = repo / "src/runtime/eval/lowered_run/indexed_run/values.rs"
    source = source_path.read_text()
    lines = source.splitlines()
    view_arms = []
    for (family, variant), cue in generate.TYPED_SCALAR_VIEW_CUES.items():
        matches = [index + 1 for index, line in enumerate(lines) if line.strip() == cue]
        if len(matches) != 1:
            raise AssertionError(f"expected one source cue for {family}{variant}, found {matches}")
        view_arms.append({"family": family, "variant": variant, "line": matches[0]})
    return source, view_arms


def eager_atom_fixture() -> tuple[list[dict], dict]:
    rows_by_variant = [
        ("Null", "ExprNull", 18),
        ("Unit", "ExprUnit", 19),
        ("Int", "ExprInt", 20),
        ("Float", "ExprFloat", 21),
        ("Duration", "ExprDuration", 22),
        ("Bool", "ExprBool", 23),
        ("Str", "ExprStr", 24),
        ("Bytes", "ExprBytes", 25),
        ("PreparedRegex", "ExprPreparedRegex", 26),
        ("PreparedConstant", "ExprPreparedConstant", 27),
        ("Path", "ExprPath", 28),
        ("Param", "ExprParam", 31),
    ]
    fields_by_variant = {
        "Null": [],
        "Unit": [],
        "Int": [{"name": "value", "type": "i64"}],
        "Float": [{"name": "value", "type": "FloatValue"}],
        "Duration": [{"name": "value", "type": "DurationValue"}],
        "Bool": [{"name": "value", "type": "bool"}],
        "Str": [{"name": "value", "type": "Arc<str>"}],
        "Bytes": [{"name": "value", "type": "Arc<[u8]>"}],
        "PreparedRegex": [{"name": "value", "type": "RegexValue"}],
        "PreparedConstant": [{"name": "value", "type": "PreparedConstantValue"}],
        "Path": [{"name": "value", "type": "PathValue"}],
        "Param": [{"name": "slot", "type": "usize"}],
    }
    rows = [
        {
            "kind": "Expr",
            "variant": variant,
            "tag": tag,
            "ordinal": ordinal,
            "effects": "0",
            "in_place": "Operands",
            "frame_strategy": "FrameApply",
            "eager_atom": True,
            "schema_fields": fields_by_variant[variant],
        }
        for variant, tag, ordinal in rows_by_variant
    ]
    schema_rows = [
        {
            "variant": variant,
            "tag": tag,
            "ordinal": ordinal,
            "effects": "0",
            "class": "Operands",
            "strategy": "FrameApply",
            "classification": "EagerAtom",
            "fields": fields_by_variant[variant],
            "line": ordinal + 1,
        }
        for variant, tag, ordinal in rows_by_variant
    ]
    variants = [variant for variant, _, _ in rows_by_variant]
    tags = [tag for _, tag, _ in rows_by_variant]
    inventory = {
        "schema_rows": schema_rows,
        "classification_rules": {
            "default_false_line": 442,
            "eager_true_line": 445,
            "expansion_line": 506,
        },
        "normal_route": {
            "mode": "normal", "line": 1460, "helper_line": 1461,
            "statement_index": 4, "fallback_kind": "ExprViewKind",
            "fallback_line": 1470, "fallback_statement_index": 5,
        },
        "forced_route": {
            "mode": "forced", "line": 6369, "helper_line": 6371,
            "statement_index": 5, "fallback_kind": "FrameApply",
            "fallback_line": 6394, "fallback_statement_index": 7,
        },
        "helper_arms": [
            {
                "variant": variant,
                "tag": tag,
                "input": generate.EAGER_ATOM_PROVENANCE[variant][1],
                "transformation": generate.EAGER_ATOM_PROVENANCE[variant][2],
                "output": generate.EAGER_ATOM_PROVENANCE[variant][3],
                "line": 300 + index,
                "arm_index": index,
            }
            for index, (variant, tag, _) in enumerate(rows_by_variant)
        ],
        "helper_fallback": {
            "scope": "non-eager ExprView reaches the eager atom helper",
            "outcome": "unreachable!: only eager atom tags reach this value helper",
            "source": "normal_dispatch", "line": 327, "arm_index": 12,
        },
        "explicit_error_group": {
            "scope": "eager atoms misrouted to generic explicit expression-view dispatch",
            "outcome": "frame_strategy_error(tag, span)",
            "source": "explicit_dispatch", "line": 7326, "arm_index": 38,
        },
        "helper_boundaries": [
            {
                "kind": "ExprView decode", "tags": tags,
                "outcome": "ExprView decode errors become indexed-ir RuntimeError values at the expression span",
                "references": [{"source": "normal_dispatch", "line": 306}],
            },
            {
                "kind": "PathValue conversion", "tags": ["ExprPath"],
                "outcome": "PathValue validation errors become indexed-ir RuntimeError values at the expression span",
                "references": [{"source": "normal_dispatch", "line": 320}],
            },
            {
                "kind": "ExprParam slot access and freeze", "tags": ["ExprParam"],
                "outcome": "usize decoding checks slot_count; the selected slot is frozen in place before it is cloned",
                "references": [
                    {"source": "normal_dispatch", "line": 324},
                    {"source": "normal_dispatch", "line": 325},
                    {"source": "normal_dispatch", "line": 326},
                    {"source": "slot_codec", "line": 210},
                    {"source": "slot_codec", "line": 212},
                    {"source": "freeze_helper", "line": 20},
                    {"source": "freeze_helper", "line": 21},
                    {"source": "freeze_helper", "line": 22},
                    {"source": "freeze_helper", "line": 23},
                ],
            },
        ],
        "slot_decode_line": 210,
        "slot_decode_bounds_line": 212,
        "freeze_contract": {
            "line": 20,
            "shared_threshold": "LOWERED_SHARED_LIST_THRESHOLD",
            "threshold_line": 21,
            "list_take_line": 22,
            "shared_list_line": 23,
        },
        "value_contract": {
            "decode_error": "map_err(indexed_error(error, span)) propagates ExprView decode errors",
            "path_error": "map_err(indexed_error(IrVerifyError { message: error.message }, span)) propagates PathValue validation errors",
            "slot_bounds": "FullCodec<usize>::decode rejects slot >= decoder.slot_count before ExprParam reads slots[slot]",
            "freeze": "lowered_freeze_large_slot_list is infallible and runs before slots[slot].clone()",
        },
        "runtime_error_paths": {
            "status": "source_exhaustive_boundary_inventory",
            "note": "runtime conformance remains unproven",
        },
        "source_paths": {
            "schema": "src/runtime/eval/indexed/full/schema.rs",
            "normal_dispatch": "src/runtime/eval/lowered_run/indexed_run.rs",
            "forced_dispatch": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
            "slot_codec": "src/runtime/eval/indexed/full.rs",
            "freeze_helper": "src/runtime/eval/lowered_run.rs",
        },
    }
    return rows, inventory


def eager_atom_operand_fixture(rows: list[dict]) -> dict:
    return {
        "rows": {
            row["tag"]: {
                "child_paths": [],
                "fields": [
                    {
                        "name": field["name"],
                        "type": field["type"],
                        "policy": "not_applicable",
                        "child_paths": [],
                    }
                    for field in row["schema_fields"]
                ],
            }
            for row in rows
        }
    }


def frame_constructor_fixture() -> tuple[list[dict], dict]:
    rows_by_variant = [
        ("FunctionRef", "ExprFunctionRef", 29),
        ("PathFrom", "ExprPathFrom", 30),
        ("EmptyMap", "ExprEmptyMap", 49),
    ]
    rows = [
        {
            "kind": "Expr",
            "variant": variant,
            "tag": tag,
            "ordinal": ordinal,
            "effects": "0",
            "in_place": "Scheduled",
            "frame_strategy": "FrameApply",
            "eager_atom": False,
        }
        for variant, tag, ordinal in rows_by_variant
    ]
    rows.append({
        "kind": "Expr",
        "variant": "FsFiles",
        "tag": "ExprFsFiles",
        "ordinal": 65,
        "effects": "0",
        "in_place": "Scheduled",
        "frame_strategy": "FrameApply",
        "eager_atom": False,
    })
    schema_rows = [
        {
            "variant": variant,
            "tag": tag,
            "ordinal": ordinal,
            "effects": "0",
            "class": "Scheduled",
            "strategy": "FrameApply",
            "line": ordinal + 1,
        }
        for variant, tag, ordinal in rows_by_variant
    ]
    frame_tags = [tag for _, tag, _ in rows_by_variant] + ["ExprFsFiles"]
    child_behavior = {"FunctionRef": "no_child", "PathFrom": "one_child", "EmptyMap": "no_child"}
    value_behavior = {
        "FunctionRef": "indexed_function_ref_value",
        "PathFrom": "indexed_path_from_value",
        "EmptyMap": "indexed_empty_map_value",
    }
    inventory = {
        "schema_rows": schema_rows,
        "classifiers": {
            "FrameApply": [
                {"tag": tag, "pattern": f"FullTag::{tag}", "line": 40 + index, "arm_index": index}
                for index, tag in enumerate(frame_tags)
            ],
            "prepared_operands": [
                {"tag": "ExprFsFiles", "pattern": "FullTag::ExprFsFiles", "line": 50, "arm_index": 0}
            ],
        },
        "normal_route": {
            "mode": "normal", "line": 200, "helper_line": 201,
            "statement_index": 4, "fallback_line": 210, "fallback_statement_index": 5,
        },
        "forced_route": {
            "mode": "forced", "line": 200, "helper_line": 201,
            "statement_index": 4, "fallback_line": 210, "fallback_statement_index": 5,
        },
        "child_arms": [
            {
                "helper": "indexed_frame_apply_child",
                "variant": variant,
                "behavior": child_behavior[variant],
                "line": 300 + index,
                "arm_index": index,
            }
            for index, (variant, _, _) in enumerate(rows_by_variant)
        ],
        "value_arms": [
            {
                "helper": "indexed_frame_apply_value",
                "variant": variant,
                "behavior": value_behavior[variant],
                "line": 400 + index,
                "arm_index": index,
            }
            for index, (variant, _, _) in enumerate(rows_by_variant)
        ],
        "helper_fallbacks": [
            {
                "helper": helper,
                "outcome": "unreachable!: frame apply tag has a matching typed view",
                "scope": "non-matching ExprView reaches a frame-apply helper",
                "line": line,
                "arm_index": 3,
            }
            for helper, line in (
                ("indexed_frame_apply_child", 350),
                ("indexed_frame_apply_value", 450),
            )
        ],
        "explicit_error_group": {
            "helper": "eval_expr_view",
            "outcome": "frame_strategy_error(tag, span)",
            "scope": "FrameApply constructor reaches generic ExprView dispatch",
            "line": 250,
            "arm_index": 30,
        },
        "value_decode_line": 390,
        "value_boundaries": {variant: 400 + index for index, (variant, _, _) in enumerate(rows_by_variant)},
        "runtime_error_paths": {"status": "unreviewed"},
        "source_paths": {
            "schema": "src/runtime/eval/indexed/full/schema.rs",
            "normal_dispatch": "src/runtime/eval/lowered_run/indexed_run.rs",
            "forced_dispatch": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
        },
    }
    return rows, inventory


def collection_construction_fixture() -> tuple[list[dict], dict]:
    explicit_source = (
        generate.HERE.parents[4]
        / "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs"
    ).read_text().splitlines()

    def helper_error_line(message: str) -> int:
        matches = [
            index + 1
            for index, line in enumerate(explicit_source)
            if message in line
        ]
        if len(matches) != 1:
            raise AssertionError(f"expected one source line for {message!r}, found {matches}")
        return matches[0]

    helper_error_lines = {
        helper: helper_error_line(message)
        for helper, message in [
            ("indexed_list_literal_view", "list literal view is invalid"),
            ("indexed_map_literal_view", "map literal view is invalid"),
            ("indexed_record_literal_view", "record literal view is invalid"),
        ]
    }
    field_types = {
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
    normal_behaviors = {
        "ExprRecord": "evaluate_each_entry_then_append_record_field_or_spread",
        "ExprMapLiteral": "evaluate_optional_key_before_value_then_append_entry",
        "ExprRecordUpdate": "evaluate_base_then_update_values_before_batch_replacement",
        "ExprList": "evaluate_each_item_then_append_without_splicing",
        "ExprListBuild": "evaluate_each_item_then_append_with_encoded_splice_policy",
    }
    forced_behaviors = {
        "ExprRecord": "record_items_frame_step",
        "ExprMapLiteral": "map_literal_frame_step",
        "ExprRecordUpdate": "record_update_base_continuation",
        "ExprList": "indexed_list_literal_frame_continuation",
        "ExprListBuild": "indexed_list_build_frame_continuation",
    }
    rows = []
    schema_rows = []
    normal_arms = []
    forced_arms = []
    for index, (tag, (variant, ordinal, strategy)) in enumerate(
        generate.COLLECTION_CONSTRUCTION_ROWS.items()
    ):
        rows.append({
            "kind": "Expr",
            "variant": variant,
            "tag": tag,
            "ordinal": ordinal,
            "effects": "0",
            "in_place": "Operands",
            "frame_strategy": strategy,
            "eager_atom": False,
            "schema_fields": [
                {"name": name, "type": type_name}
                for name, type_name in field_types[tag]
            ],
        })
        schema_rows.append({
            "tag": tag,
            "variant": variant,
            "ordinal": ordinal,
            "effects": "0",
            "class": "Operands",
            "strategy": strategy,
            "line": 850 + ordinal,
        })
        normal_arms.append({
            "tag": tag,
            "variant": variant,
            "behavior": normal_behaviors[tag],
            "line": 1500 + index,
            "arm_index": index,
        })
        forced_arms.append({
            "tag": tag,
            "variant": variant,
            "behavior": forced_behaviors[tag],
            "line": 6700 + index,
            "arm_index": 17 + index,
        })
    inventory = {
        "schema_rows": schema_rows,
        "frame_apply_classifier": {"line": 400, "tags": []},
        "normal_arms": normal_arms,
        "forced_arms": forced_arms,
        "forced_route": {
            "frame_apply_line": 6394,
            "frame_apply_statement_index": 7,
            "view_kind_line": 6402,
            "view_kind_statement_index": 8,
            "eval_expr_view_line": 6405,
        },
        "normal_fallback": {
            "helper": "eval_indexed_expr_inner",
            "outcome": "RuntimeError(indexed-ir: direct indexed evaluator does not support tag)",
            "line": 1933,
            "arm_index": 22,
        },
        "normal_view_fallbacks": [
            {
                "helper": "eval_indexed_expr_inner",
                "outcome": outcome,
                "scope": f"{variant} tag reaches the normal evaluator with a different typed ExprView",
                "line": 1600 + index,
                "arm_index": index,
            }
            for index, (variant, outcome) in enumerate([
                ("Record", "unreachable!: record literal tag has a typed record view"),
                ("MapLiteral", "unreachable!: map literal tag has a typed map view"),
                ("List", "unreachable!: list literal tag has a typed list view"),
                ("ListBuild", "unreachable!: list build tag has a typed list build view"),
            ])
        ],
        "helper_fallbacks": [
            {
                "helper": helper,
                "outcome": outcome,
                "scope": "a tag reaches the helper with a different typed ExprView",
                "line": line,
                "arm_index": index,
            }
            for index, (helper, outcome, line) in enumerate([
                ("indexed_list_literal_view", "Err(indexed-ir: list literal view is invalid)", helper_error_lines["indexed_list_literal_view"]),
                ("indexed_map_literal_view", "Err(indexed-ir: map literal view is invalid)", helper_error_lines["indexed_map_literal_view"]),
                ("indexed_record_literal_view", "Err(indexed-ir: record literal view is invalid)", helper_error_lines["indexed_record_literal_view"]),
                ("decode_record_updates", "unreachable!: record-update tag has a matching borrowed expression view", 44),
            ])
        ],
        "explicit_error_group": {
            "helper": "eval_expr_view",
            "outcome": "frame_strategy_error(tag, span)",
            "scope": "valid ExprViewKind values without dedicated frame handlers",
            "line": 7326,
            "arm_index": 38,
        },
        "runtime_error_paths": {"status": "unreviewed"},
        "source_paths": {
            "schema": "src/runtime/eval/indexed/full/schema.rs",
            "normal_dispatch": "src/runtime/eval/lowered_run/indexed_run.rs",
            "forced_dispatch": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
            "operand_helpers": "src/runtime/eval/lowered_run/indexed_run/operands.rs",
        },
    }
    return rows, inventory


def result_constructor_fixture() -> tuple[list[dict], dict]:
    fields = {
        "ExprOk": [("value", "BuildExprId")],
        "ExprErr": [("value", "BuildExprId"), ("cause", "Option<BuildExprId>")],
    }
    normal = {
        "ExprOk": "evaluate_child_then_wrap_result_ok",
        "ExprErr": "evaluate_error_then_optional_cause_before_constructing_error",
    }
    forced = {
        "ExprOk": "push_child_with_wrap_ok_continuation",
        "ExprErr": "push_value_with_optional_cause_wrap_err_continuation",
    }
    rows = []
    schema_rows = []
    normal_arms = []
    forced_arms = []
    normal_fallbacks = []
    forced_fallbacks = []
    for index, (tag, (variant, ordinal)) in enumerate(generate.RESULT_CONSTRUCTOR_ROWS.items()):
        rows.append({
            "kind": "Expr",
            "variant": variant,
            "tag": tag,
            "ordinal": ordinal,
            "effects": "0",
            "in_place": "Operands",
            "frame_strategy": "FrameControl",
            "eager_atom": False,
            "schema_fields": [
                {"name": name, "type": type_name}
                for name, type_name in fields[tag]
            ],
        })
        schema_rows.append({
            "tag": tag,
            "variant": variant,
            "ordinal": ordinal,
            "effects": "0",
            "class": "Operands",
            "strategy": "FrameControl",
            "line": 1100 + index,
        })
        normal_arms.append({
            "tag": tag,
            "variant": variant,
            "behavior": normal[tag],
            "line": 1800 + index,
            "arm_index": index + 1,
        })
        forced_arms.append({
            "tag": tag,
            "variant": variant,
            "behavior": forced[tag],
            "line": 6900 + index,
            "arm_index": index + 1,
        })
        lower = variant.lower()
        normal_fallbacks.append({
            "helper": "eval_indexed_expr_inner",
            "outcome": f"unreachable!: {lower} tag has an {lower} view",
            "scope": f"{variant} tag reaches the normal evaluator with a different typed ExprView",
            "line": 1850 + index,
            "arm_index": index + 1,
        })
        forced_fallbacks.append({
            "helper": "eval_expr_view",
            "outcome": f"unreachable!: {lower} tag has an {lower} view",
            "scope": f"{variant} view-kind arm decodes a different ExprView",
            "line": 6950 + index,
            "arm_index": index + 1,
        })
    inventory = {
        "schema_rows": schema_rows,
        "normal_arms": normal_arms,
        "normal_view_fallbacks": normal_fallbacks,
        "normal_fallback": {
            "helper": "eval_indexed_expr_inner",
            "outcome": "RuntimeError(indexed-ir: direct indexed evaluator does not support tag)",
            "scope": "FullTag values without a direct indexed expression arm",
            "line": 1933,
            "arm_index": 22,
        },
        "forced_arms": forced_arms,
        "forced_view_fallbacks": forced_fallbacks,
        "forced_route": {
            "frame_apply_line": 6394,
            "frame_apply_statement_index": 7,
            "view_kind_line": 6402,
            "view_kind_statement_index": 8,
            "eval_expr_view_line": 6405,
        },
        "explicit_error_group": {
            "helper": "eval_expr_view",
            "outcome": "frame_strategy_error(tag, span)",
            "scope": "valid ExprViewKind values without dedicated frame handlers",
            "line": 7326,
            "arm_index": 38,
        },
        "runtime_error_paths": {"status": "unreviewed"},
        "source_paths": {
            "schema": "src/runtime/eval/indexed/full/schema.rs",
            "normal_dispatch": "src/runtime/eval/lowered_run/indexed_run.rs",
            "forced_dispatch": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
        },
    }
    return rows, inventory


def call_frame_fixture() -> tuple[list[dict], dict]:
    fields = {
        "ExprCall": [("function", "LoweredFunctionKey"), ("args", "Vec<LoweredCallArg>"), ("span", "Span")],
        "ExprDirectPureCall": [("function", "LoweredFunctionKey"), ("args", "Vec<LoweredCallArg>"), ("span", "Span")],
        "ExprExternalCall": [("function", "QualifiedName"), ("args", "Vec<LoweredCallArg>"), ("span", "Span")],
        "ExprDynamicCall": [("callee", "BuildExprId"), ("args", "Vec<LoweredCallArg>"), ("span", "Span")],
    }
    rows = [
        {
            "kind": "Expr",
            "variant": variant,
            "tag": tag,
            "ordinal": ordinal,
            "effects": effects,
            "in_place": "Scheduled",
            "frame_strategy": "FrameControl",
            "eager_atom": False,
            "schema_fields": [{"name": name, "type": type_name} for name, type_name in fields[tag]],
        }
        for tag, (variant, ordinal, effects) in generate.CALL_FRAME_ROWS.items()
    ]
    inventory = {
        "schema_rows": [
            {
                "tag": row["tag"],
                "variant": row["variant"],
                "ordinal": row["ordinal"],
                "effects": row["effects"].replace(" | ", "|"),
                "class": "Scheduled",
                "strategy": "FrameControl",
                "line": 1127 + index * 5,
            }
            for index, row in enumerate(rows)
        ],
        "normal_route": {
            "scheduled_predicate_line": 1295,
            "frame_schedule_line": 1307,
            "frame_helper_line": 1308,
            "inner_helper_line": 1310,
            "view_kind_line": 6403,
            "eval_expr_view_line": 6405,
        },
        "normal_fallback": {
            "helper": "eval_indexed_expr_inner",
            "outcome": "RuntimeError(indexed-ir: direct indexed evaluator does not support tag)",
            "scope": "FullTag values without a direct indexed expression arm",
            "line": 1933,
            "arm_index": 22,
        },
        "forced_arms": [
            {
                "tag": tag,
                "variant": variant,
                "view_group": group,
                "behavior": behavior,
                "line": line,
                "arm_index": arm_index,
            }
            for tag, variant, group, behavior, line, arm_index in [
                ("ExprCall", "Call", ["Call", "SelfCall", "DirectPureCall"], "decode_static_call_and_schedule_arguments", 7279, 37),
                ("ExprDirectPureCall", "DirectPureCall", ["Call", "SelfCall", "DirectPureCall"], "decode_static_call_and_schedule_arguments", 7279, 37),
                ("ExprExternalCall", "ExternalCall", ["ExternalCall"], "resolve_external_kind_then_schedule_arguments", 7252, 36),
                ("ExprDynamicCall", "DynamicCall", ["DynamicCall", "TypedCall"], "evaluate_callee_before_scheduling_dynamic_arguments", 7113, 31),
            ]
        ],
        "typed_view_fallbacks": [
            {
                "helper": "eval_expr_view",
                "outcome": outcome,
                "scope": f"{variant} view-kind arm decodes a different ExprView",
                "line": line,
                "arm_index": arm_index,
            }
            for variant, outcome, line, arm_index in [
                ("Call", "unreachable!: static call tag has a typed call view", 7308, 37),
                ("DirectPureCall", "unreachable!: static call tag has a typed call view", 7308, 37),
                ("ExternalCall", "unreachable!: external call tag has a typed call view", 7260, 36),
                ("DynamicCall", "unreachable!: dynamic call tag has a typed call view", 7129, 31),
            ]
        ],
        "explicit_error_group": {
            "helper": "eval_expr_view",
            "outcome": "frame_strategy_error(tag, span)",
            "scope": "valid ExprViewKind values without dedicated frame handlers",
            "line": 7326,
            "arm_index": 38,
        },
        "runtime_error_paths": {"status": "unreviewed", "note": "call decode and execution errors remain unreviewed"},
        "source_paths": {
            "schema": "src/runtime/eval/indexed/full/schema.rs",
            "normal_dispatch": "src/runtime/eval/lowered_run/indexed_run.rs",
            "forced_dispatch": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
        },
    }
    return rows, inventory


def field_index_fixture() -> tuple[list[dict], dict]:
    fields = {
        "ExprField": [("base", "BuildExprId"), ("name", "NameText"), ("span", "Span")],
        "ExprIndex": [("base", "BuildExprId"), ("index", "BuildExprId"), ("span", "Span")],
        "ExprIndexFromEnd": [
            ("base", "BuildExprId"),
            ("distance", "crate::runtime::eval::EndDistance"),
            ("span", "Span"),
        ],
    }
    rows = [
        {
            "kind": "Expr",
            "variant": variant,
            "tag": tag,
            "ordinal": ordinal,
            "effects": "0",
            "in_place": "Operands",
            "frame_strategy": "FrameApply",
            "eager_atom": False,
            "schema_fields": [{"name": name, "type": type_name} for name, type_name in fields[tag]],
        }
        for tag, (variant, ordinal) in generate.FIELD_INDEX_ROWS.items()
    ]
    inventory = {
        "schema_rows": [
            {
                "tag": row["tag"], "variant": row["variant"], "ordinal": row["ordinal"],
                "effects": "0", "class": "Operands", "strategy": "FrameApply",
                "line": line,
            }
            for row, line in zip(rows, (895, 902, 909))
        ],
        "frame_apply_classifier_line": 335,
        "normal_route": {
            "scheduled_predicate_line": 1298,
            "frame_schedule_line": 1307,
            "frame_helper_line": 1308,
            "inner_helper_line": 1309,
        },
        "normal_arms": [
            {"tag": tag, "variant": variant, "behavior": behavior, "line": line, "arm_index": arm_index}
            for tag, variant, behavior, line, arm_index in [
                ("ExprField", "Field", "try_record_field_chain_then_evaluate_base_and_lookup_field", 1672, 10),
                ("ExprIndex", "Index", "evaluate_base_then_index_and_lookup", 1692, 11),
                ("ExprIndexFromEnd", "IndexFromEnd", "evaluate_base_then_lookup_from_end_distance", 1711, 12),
            ]
        ],
        "normal_view_fallbacks": [
            {"helper": "eval_indexed_expr_inner", "outcome": outcome, "scope": f"{variant} tag reaches the normal evaluator with a different typed ExprView", "line": line, "arm_index": arm_index}
            for variant, outcome, line, arm_index in [
                ("Field", "unreachable!: field tag has a field view", 1676, 10),
                ("Index", "unreachable!: index tag has an index view", 1696, 11),
                ("IndexFromEnd", "unreachable!: index-from-end tag has an index-from-end view", 1718, 12),
            ]
        ],
        "normal_fallback": {
            "helper": "eval_indexed_expr_inner",
            "outcome": "RuntimeError(indexed-ir: direct indexed evaluator does not support tag)",
            "scope": "FullTag values without a direct indexed expression arm",
            "line": 1933,
            "arm_index": 22,
        },
        "forced_arms": [
            {"tag": tag, "variant": variant, "behavior": behavior, "line": line, "arm_index": arm_index}
            for tag, variant, behavior, line, arm_index in [
                ("ExprField", "Field", "try_record_field_chain_then_schedule_base", 7149, 32),
                ("ExprIndex", "Index", "schedule_base_then_index_continuation", 7184, 33),
                ("ExprIndexFromEnd", "IndexFromEnd", "schedule_base_then_from_end_continuation", 7205, 34),
            ]
        ],
        "forced_view_fallbacks": [
            {"helper": "eval_expr_view", "outcome": outcome, "scope": f"{variant} view-kind arm decodes a different ExprView", "line": line, "arm_index": arm_index}
            for variant, outcome, line, arm_index in [
                ("Field", "unreachable!: field tag has a field view", 7156, 32),
                ("Index", "unreachable!: index tag has an index view", 7191, 33),
                ("IndexFromEnd", "unreachable!: index-from-end tag has an index-from-end view", 7212, 34),
            ]
        ],
        "forced_route": {
            "frame_apply_line": 6394,
            "frame_apply_statement_index": 7,
            "view_kind_line": 6402,
            "view_kind_statement_index": 8,
            "eval_expr_view_line": 6405,
        },
        "explicit_error_group": {
            "helper": "eval_expr_view",
            "outcome": "frame_strategy_error(tag, span)",
            "scope": "valid ExprViewKind values without dedicated frame handlers",
            "line": 7326,
            "arm_index": 38,
        },
        "runtime_error_paths": {
            "status": "unreviewed",
            "note": "field/index decode, child evaluation outcomes, and runtime field/index lookup errors remain unreviewed",
        },
        "source_paths": {
            "schema": "src/runtime/eval/indexed/full/schema.rs",
            "normal_dispatch": "src/runtime/eval/lowered_run/indexed_run.rs",
            "forced_dispatch": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
        },
    }
    return rows, inventory


ARENA_SHAPES = arena_shape_source()


class SemanticRouteMatrixTests(unittest.TestCase):
    def test_ast_extractor_uses_worktree_and_compiler_qualified_target(self) -> None:
        root = generate.HERE.parents[4]
        scratch = root / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch, prefix="route-ast-target-") as temporary:
            fixture = pathlib.Path(temporary)
            cargo = fixture / "cargo"
            cargo.write_bytes(b"cargo fixture")
            cargo.chmod(0o755)
            kache = fixture / "kache"
            kache.write_bytes(b"kache fixture")
            kache.chmod(0o755)
            identity = {
                "head": "a" * 40,
                "tree": "b" * 40,
                "cargo": "cargo 1.0",
                "rustc": "rustc 1.0",
            }
            target_dirs: list[pathlib.Path] = []
            cargo_environments: list[dict[str, str]] = []

            def fake_run(argv, **kwargs):
                if argv[0] == "git":
                    value = identity["head"] if argv[-1] == "HEAD" else identity["tree"]
                    return subprocess.CompletedProcess(argv, 0, stdout=value, stderr="")
                if argv == [str(cargo), "--version"]:
                    return subprocess.CompletedProcess(argv, 0, stdout=identity["cargo"], stderr="")
                if argv == ["rustc", "--version", "--verbose"]:
                    return subprocess.CompletedProcess(argv, 0, stdout=identity["rustc"], stderr="")
                if argv[0] == str(cargo) and "run" in argv:
                    target_dirs.append(pathlib.Path(kwargs["env"]["CARGO_TARGET_DIR"]))
                    cargo_environments.append(kwargs["env"])
                    return subprocess.CompletedProcess(argv, 0, stdout="{}", stderr="")
                self.fail(f"unexpected subprocess: {argv}")

            with (
                mock.patch.object(generate, "CARGO_EXE", cargo),
                mock.patch.object(generate, "KACHE_EXE", kache, create=True),
                mock.patch.object(generate.subprocess, "run", side_effect=fake_run),
            ):
                generate.run_ast_extractor(root, ["fixture.rs"], "test subject")
                identity["tree"] = "c" * 40
                generate.run_ast_extractor(root, ["fixture.rs"], "test subject")
                identity["rustc"] = "rustc 2.0"
                generate.run_ast_extractor(root, ["fixture.rs"], "test subject")

            self.assertEqual(len(target_dirs), 3)
            self.assertTrue(all(path.is_relative_to(root / ".work") for path in target_dirs))
            self.assertIn("b" * 12, str(target_dirs[0]))
            self.assertIn("c" * 12, str(target_dirs[1]))
            self.assertEqual(len(set(target_dirs)), 3)
            self.assertTrue(all(env["RUSTC_WRAPPER"] == str(kache) for env in cargo_environments))

    def test_receipt_collector_uses_canonical_xsht_and_rejects_symlinks(self) -> None:
        root = generate.HERE.parents[4]
        scratch = root / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch, prefix="route-witness-product-") as temporary:
            fixture_root = pathlib.Path(temporary)
            canonical = fixture_root / generate.CANONICAL_VERIFICATION_XSHT_RELATIVE
            canonical.parent.mkdir(parents=True)
            canonical.write_bytes(b"canonical product")
            canonical.chmod(0o755)
            target_copy = fixture_root / "target/x86_64-unknown-linux-musl/verification/xsht"
            target_copy.parent.mkdir(parents=True)
            target_copy.write_bytes(b"stale target product")
            target_copy.chmod(0o755)

            self.assertEqual(generate.canonical_verification_xsht_path(fixture_root), canonical)
            self.assertEqual(
                generate.sha256_file(generate.canonical_verification_xsht_path(fixture_root)),
                generate.sha256_bytes(b"canonical product"),
            )

            linked_root = fixture_root / "linked"
            linked_root.mkdir()
            (linked_root / ".work").symlink_to(fixture_root / ".work", target_is_directory=True)
            with self.assertRaisesRegex(generate.EvidenceError, "contains a symlink"):
                generate.canonical_verification_xsht_path(linked_root)

    def test_runtime_oracle_commands_select_native_or_rust_runner(self) -> None:
        xsh_id = "tests/xsh/expression-route-oracles.xsh::test_expr_match"
        rust_id = "runtime::eval::indexed::full::tests::verifier_rejects_a_pipeline_input_that_names_a_statement"
        self.assertEqual(
            generate.runtime_oracle_commands(xsh_id, {rust_id}),
            generate.native_commands(xsh_id),
        )
        self.assertEqual(
            generate.runtime_oracle_commands(rust_id, {rust_id}),
            {"verification": generate.rust_test_command(rust_id)},
        )

    def test_pipeline_verifier_rejections_are_separate_from_route_oracle(self) -> None:
        root = generate.HERE.parents[4]
        config = json.loads(generate.CONFIG.read_text())
        pipeline = config["runtime_oracles"]["ExprPipeline"]
        xsh_id = "tests/xsh/expression-route-oracles.xsh::test_expr_pipeline"
        verifier_ids = {
            "runtime::eval::indexed::full::tests::verifier_rejects_a_pipeline_input_that_names_a_statement",
            "runtime::eval::indexed::full::tests::verifier_rejects_a_pipeline_with_no_input_payload",
            "runtime::eval::indexed::full::tests::verifier_rejects_a_pipeline_with_a_truncated_payload_after_input",
        }
        rust_ids = generate.rust_test_ids(
            root,
            config["rust_oracle_files"],
            config["rust_test_modules"],
        )
        self.assertTrue(verifier_ids <= rust_ids)
        self.assertEqual(set(pipeline["ids"]), verifier_ids | {xsh_id})
        self.assertEqual(pipeline["proof_kind"], "runtime")
        self.assertIn("route candidate remains uncredited", pipeline["scope"])
        self.assertFalse(
            any(path["tag"] == "ExprPipeline" for path in config["unmapped_paths"])
        )
        route_mapping = next(
            mapping for mapping in config["route_witness_mappings"]
            if mapping["tag"] == "ExprPipeline"
        )
        self.assertEqual(route_mapping["test_id"], xsh_id)
        for verifier_id in verifier_ids:
            self.assertEqual(
                generate.runtime_oracle_commands(verifier_id, rust_ids),
                {"verification": generate.rust_test_command(verifier_id)},
            )

    def route_witness_receipt_fixture(
        self,
        root: pathlib.Path,
        run_dir: pathlib.Path,
        mode: str,
        *,
        test_id: str = "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map",
        extra_env: dict[str, str] | None = None,
    ) -> tuple[dict, dict, pathlib.Path, dict, bytes]:
        config = json.loads(generate.CONFIG.read_text())
        rows = generate.parse_schema((root / generate.SCHEMA_PATH).read_text())
        mappings = generate.validate_route_witness_mappings(root, config, rows)
        mapping = mappings[test_id]
        row = next(row for row in rows if row["tag"] == mapping["tag"])
        source_bytes = generate.route_witness_source_bytes(
            root, mapping, config["route_witness_source_commit"]
        )
        self.assertIsNotNone(source_bytes)
        source_hash = generate.sha256_bytes(source_bytes)
        expectation = mapping["mode_expectations"][mode]
        artifact = {
            "version": 1,
            "test_id": mapping["test_id"],
            "source_hash": source_hash,
            "records": [{
                "tag_name": mapping["tag"],
                "tag_ordinal": row["ordinal"],
                "route": expectation["route_id"],
                "mode": mode,
            }],
        }
        artifact_bytes = (json.dumps(artifact, separators=(",", ":")) + "\n").encode()
        run_dir.mkdir(parents=True)
        (run_dir / "route-witness.json").write_bytes(artifact_bytes)
        log_path = run_dir / "step.log"
        log_path.write_text("native test passed\n")
        step_id = f"{mapping['tag']}-{mode}-{run_dir.name}"
        step = {
            "id": step_id,
            "exit_code": 0,
            "argv": [
                "bash", "run-native-final.sh", "run",
                "target/x86_64-unknown-linux-musl/verification/xsht",
                "test", "--exact", mapping["test_id"],
            ],
            "env": {
                **({"XSH_INTERNAL_TEST_FORCE_FRAMES": "1"} if mode == "forced" else {}),
                **(extra_env or {}),
            },
            "log": str(log_path.relative_to(root)),
            "log_sha256": generate.sha256_file(log_path),
            "route_witness_artifact": {
                "path": "route-witness.json",
                "sha256": generate.sha256_bytes(artifact_bytes),
                "source_hash": source_hash,
            },
        }
        receipt = {
            "source_expected": {"commit": "head", "tree": "tree"},
            "hashes": {
                "binaries_before_tests": {"xsht-verification": "xsht-hash"},
                "binaries_after_tests": {"xsht-verification": "xsht-hash"},
            },
            "source_checks": [
                {"where": f"before:{step_id}", "head": "head", "tree": "tree", "clean": True},
                {"where": f"after:{step_id}", "head": "head", "tree": "tree", "clean": True},
            ],
            "steps": [step],
        }
        receipt_path = run_dir / "receipt.json"
        return config, mapping, receipt_path, receipt, artifact_bytes

    def test_schema_parser_enumerates_rows_and_metadata(self) -> None:
        rows = generate.parse_schema(SCHEMA)
        self.assertEqual([row["tag"] for row in rows], ["ExprInt", "ExprBinary", "StmtPrint"])
        self.assertEqual(rows[0]["kind"], "Expr")
        self.assertTrue(rows[0]["eager_atom"])
        self.assertEqual(rows[0]["schema_fields"], [{"name": "value", "type": "i64"}])
        self.assertEqual(rows[1]["schema_fields"], [{"name": "left", "type": "BuildExprId"}])
        self.assertEqual(rows[2]["schema_fields"], [
            {"name": "args", "type": "Vec<BuildExprId>"},
            {"name": "flush", "type": "bool"},
        ])
        self.assertEqual(rows[1]["frame_strategy"], "FrameControl")

    def test_schema_parser_rejects_duplicate_rows(self) -> None:
        duplicate = SCHEMA.replace(
            "        Binary { left: BuildExprId } => ExprBinary = 1 [0; Operands; FrameControl];",
            "        Binary { left: BuildExprId } => ExprInt = 1 [0; Operands; FrameControl];",
        )
        with self.assertRaisesRegex(generate.EvidenceError, "duplicate instruction tags"):
            generate.parse_schema(duplicate)

    def test_operand_inventory_rejects_added_and_missing_rows(self) -> None:
        rows = generate.parse_schema(SCHEMA)
        manifest = operand_manifest(rows)
        added = rows + [{
            "tag": "ExprAdded", "ordinal": 3, "kind": "Expr", "variant": "Added",
            "schema_fields": [],
        }]
        with self.assertRaisesRegex(generate.EvidenceError, r"operand row inventory mismatch: added=\['ExprAdded'\]"):
            generate.validate_operand_inventory(added, manifest, ARENA_SHAPES)
        with self.assertRaisesRegex(generate.EvidenceError, r"operand row inventory mismatch: added=\[\]; missing=\['StmtPrint'\]"):
            generate.validate_operand_inventory(rows[:-1], manifest, ARENA_SHAPES)

    def test_operand_inventory_rejects_duplicate_rows(self) -> None:
        rows = generate.parse_schema(SCHEMA)
        manifest = operand_manifest(rows)
        marker = "row\tExprInt\t0\tExpr\tInt\t\t\t\t\n"
        with self.assertRaisesRegex(generate.EvidenceError, "duplicate operand manifest row: ExprInt"):
            generate.validate_operand_inventory(rows, manifest + marker, ARENA_SHAPES)

    def test_operand_inventory_rejects_unknown_policy(self) -> None:
        rows = generate.parse_schema(SCHEMA)
        manifest = operand_manifest(rows).replace("\tstrict\tleft\n", "\teager\tleft\n")
        with self.assertRaisesRegex(generate.EvidenceError, "unknown operand policy"):
            generate.validate_operand_inventory(rows, manifest, ARENA_SHAPES)

    def test_operand_inventory_rejects_unclassified_payload_field(self) -> None:
        rows = generate.parse_schema(SCHEMA)
        manifest = operand_manifest(rows)
        manifest = manifest.replace(
            "field\tExprBinary\t1\tExpr\tBinary\tleft\tBuildExprId\tstrict\tleft\n", ""
        )
        with self.assertRaisesRegex(generate.EvidenceError, "operand fields are unclassified for ExprBinary"):
            generate.validate_operand_inventory(rows, manifest, ARENA_SHAPES)

    def test_run_operand_paths_cover_closed_source_variants(self) -> None:
        expected = [
            ".options[*].Timeout.value",
            ".options[*].CpuMax.value",
            ".options[*].Accept.value",
            ".env[*].value.kind.Single.value",
            ".env[*].value.kind.SingleOrSplice.value",
            ".env[*].value.kind.Splice.value",
            ".target.kind.Single.value",
            ".target.kind.SingleOrSplice.value",
            ".target.kind.Splice.value",
            ".tail[*].Argument.value.kind.Single.value",
            ".tail[*].Argument.value.kind.SingleOrSplice.value",
            ".tail[*].Argument.value.kind.Splice.value",
            ".tail[*].Redirection.value.target.kind.Single.value",
            ".tail[*].Redirection.value.target.kind.SingleOrSplice.value",
            ".tail[*].Redirection.value.target.kind.Splice.value",
        ]
        self.assertEqual(generate.RUN_OPERANDS_CHILD_PATHS, expected)
        self.assertEqual(generate.CUSTOM_CHILD_PATHS["LoweredRunEnv"], [
            ".value.kind.Single.value",
            ".value.kind.SingleOrSplice.value",
            ".value.kind.Splice.value",
        ])
        for name in ("LoweredRunCapture", "LoweredRunPipelineSegment", "LoweredSpawnRun"):
            self.assertEqual(
                generate.CUSTOM_CHILD_PATHS[name],
                [f".operands{path}" for path in expected],
            )
        self.assertEqual(
            generate.CUSTOM_CHILD_PATHS["LoweredProcessCommandBuilderEntry"],
            [".Field.value", *[f".Run.operands{path}" for path in expected]],
        )
        inventory = {
            "source_path": generate.RUN_OPERAND_DECODE_PATH.as_posix(),
            "enums": [
                {"name": name, "variants": copy.deepcopy(variants)}
                for name, variants in generate.RUN_OPERAND_ENUM_SCHEMA.items()
            ],
            "structs": [
                {"name": name, "fields": copy.deepcopy(fields)}
                for name, fields in generate.RUN_OPERAND_STRUCT_SCHEMA.items()
            ],
        }
        self.assertEqual(generate.validate_run_operand_variant_inventory(inventory), inventory)
        for enum_name in ("LoweredRunOption", "LoweredRunTailOperand"):
            changed = copy.deepcopy(inventory)
            row = next(item for item in changed["enums"] if item["name"] == enum_name)
            row["variants"].append({"name": "Unmapped", "fields": []})
            with self.assertRaisesRegex(generate.EvidenceError, "enum payloads differ from the child-path contract"):
                generate.validate_run_operand_variant_inventory(changed)
        changed = copy.deepcopy(inventory)
        option = next(item for item in changed["enums"] if item["name"] == "LoweredRunOption")
        option["variants"][0]["fields"][0]["type"] = "DurationValue"
        with self.assertRaisesRegex(generate.EvidenceError, "enum payloads differ from the child-path contract"):
            generate.validate_run_operand_variant_inventory(changed)
        changed = copy.deepcopy(inventory)
        operands = next(item for item in changed["structs"] if item["name"] == "LoweredRunOperands")
        operands["fields"].append({"name": "retry", "type": "BuildExprId"})
        with self.assertRaisesRegex(generate.EvidenceError, "struct fields differ from the child-path contract"):
            generate.validate_run_operand_variant_inventory(changed)

    def test_runtime_operand_order_is_not_applicable_only_for_exactly_empty_children(self) -> None:
        row = {
            "schema_fields": [
                {"name": "value", "type": "i64"},
                {"name": "span", "type": "Span"},
            ],
        }
        no_child_inventory = {
            "fields": [
                {"name": "value", "type": "i64", "policy": "not_applicable", "child_paths": []},
                {"name": "span", "type": "Span", "policy": "not_applicable", "child_paths": []},
            ],
            "child_paths": [],
        }
        evidence = generate.runtime_operand_order_evidence(row, no_child_inventory)
        self.assertEqual(evidence["status"], "not_applicable")
        self.assertEqual(evidence["fields"], [])
        self.assertEqual(evidence["child_paths"], [])

        wrong_policy = copy.deepcopy(no_child_inventory)
        wrong_policy["fields"][0]["policy"] = "strict"
        self.assertEqual(
            generate.runtime_operand_order_evidence(row, wrong_policy)["status"],
            "unresolved",
        )

        wrong_shape = copy.deepcopy(no_child_inventory)
        wrong_shape["fields"].pop()
        self.assertEqual(
            generate.runtime_operand_order_evidence(row, wrong_shape)["status"],
            "unresolved",
        )

        possible_child = copy.deepcopy(no_child_inventory)
        possible_child["fields"][0].update(policy="strict", child_paths=["value"])
        possible_child["child_paths"] = ["value"]
        self.assertEqual(
            generate.runtime_operand_order_evidence(row, possible_child)["status"],
            "unresolved",
        )

    def test_operand_inventory_rejects_recognized_arena_shape_drift(self) -> None:
        binary_shape = generate.ARENA_SHAPE_CONTRACTS["expr_binary"][0]
        drifted = ARENA_SHAPES.replace(
            binary_shape, binary_shape.replace("Child::Expr(left)", "Child::Expr(right)")
        )
        with self.assertRaisesRegex(generate.EvidenceError, "recognized syntax child shape drift: for_each_expr_child/expr_binary"):
            generate.validate_arena_child_shapes(drifted)

    def test_arena_shape_decoys_in_comments_and_strings_do_not_count(self) -> None:
        binary_shape = generate.ARENA_SHAPE_CONTRACTS["expr_binary"][0]
        without_binary = ARENA_SHAPES.replace(binary_shape, "", 1)
        for decoy in (f"// {binary_shape}", f'const SHAPE: &str = "{binary_shape}";'):
            with self.subTest(decoy=decoy[:12]):
                with self.assertRaisesRegex(
                    generate.EvidenceError,
                    "recognized syntax child shape drift: for_each_expr_child/expr_binary",
                ):
                    generate.validate_arena_child_shapes(without_binary + "\n" + decoy)

    def test_arena_shapes_are_scoped_to_unique_parseable_owner_functions(self) -> None:
        binary_shape = generate.ARENA_SHAPE_CONTRACTS["expr_binary"][0]
        drifted_owner = ARENA_SHAPES.replace(
            binary_shape, binary_shape.replace("Child::Expr(left)", "Child::Expr(right)"), 1
        )
        with self.assertRaisesRegex(
            generate.EvidenceError,
            "recognized syntax child shape drift: for_each_expr_child/expr_binary",
        ):
            generate.validate_arena_child_shapes(
                drifted_owner + f"\nfn unrelated() {{ {binary_shape} }}"
            )

        missing = ARENA_SHAPES.replace("fn for_each_expr_child", "fn other_function", 1)
        duplicate = ARENA_SHAPES + "\nfn for_each_expr_child() {}"
        unparseable = ARENA_SHAPES.replace(
            "fn for_each_expr_child() {", "fn for_each_expr_child {", 1
        )
        unbalanced = ARENA_SHAPES.replace("\n}\nfn for_each_stmt_child", "\nfn for_each_stmt_child", 1)
        for source in (missing, duplicate, unparseable, unbalanced):
            with self.subTest(source=source[-30:]):
                with self.assertRaises(generate.EvidenceError):
                    generate.validate_arena_child_shapes(source)

    def test_pipeline_stage_inventory_projects_nested_children(self) -> None:
        root = generate.HERE.parents[4]
        source = (root / generate.PIPELINE_STAGE_SOURCE_PATH).read_text()
        inventory = generate.pipeline_stage_child_inventory(source)
        paths = set(inventory["child_paths"])
        self.assertEqual(inventory["variant_count"], len(generate.PIPELINE_STAGE_VARIANTS))
        for path in (
            ".Where.predicate",
            ".WhereBlock.body[*]",
            ".WhereBlock.value",
            ".FlatMap.value",
            ".FlatMapBlock.body[*]",
            ".ParMap.jobs?",
            ".ParMapBlock.body[*]",
            ".ParMapFlatMapReduceBy.body?[*]",
            ".ParMapFlatMapReduceBy.reduce_body[*]",
            ".ParMapFlatMapReduceBy.reduce_value",
            ".ReduceBy.jobs?",
            ".ReduceBy.body[*]",
            ".ReduceByConfigured.configuration",
        ):
            self.assertIn(path, paths)
        projected = generate.schema_child_paths(
            "Vec<LoweredPipelineStage>", "stages", inventory["child_paths"]
        )
        self.assertEqual(len(projected), inventory["child_path_count"])
        self.assertEqual(
            generate.operand_policy("ExprPipeline", "stages", "Vec<LoweredPipelineStage>", projected),
            "iterative",
        )

    def test_pipeline_stage_inventory_rejects_added_variant(self) -> None:
        root = generate.HERE.parents[4]
        source = (root / generate.PIPELINE_STAGE_SOURCE_PATH).read_text()
        start = source.rfind("    Range struct {")
        end = source.find("};", start)
        self.assertGreaterEqual(start, 0)
        self.assertGreaterEqual(end, 0)
        drifted = source[:end + 2] + "\n    AddedStage unit {};" + source[end + 2:]
        with self.assertRaisesRegex(generate.EvidenceError, r"LoweredPipelineStage variant set changed: added=\['AddedStage'\]"):
            generate.pipeline_stage_child_inventory(drifted)

    def test_mapping_rejects_unmapped_duplicate_and_conflicting_rows(self) -> None:
        with self.assertRaisesRegex(generate.EvidenceError, r"unmapped=\['ExprBinary'\]"):
            generate.validate_mapping_coverage(["ExprInt", "ExprBinary"], {"ExprInt"}, [])
        with self.assertRaisesRegex(generate.EvidenceError, "duplicate schema row tags"):
            generate.validate_mapping_coverage(["ExprInt", "ExprInt"], {"ExprInt"}, [])
        with self.assertRaisesRegex(generate.EvidenceError, "duplicate static-or-rejection"):
            generate.validate_mapping_coverage(["ExprInt"], set(), ["ExprInt", "ExprInt"])
        with self.assertRaisesRegex(generate.EvidenceError, r"overlap=\['ExprInt'\]"):
            generate.validate_mapping_coverage(["ExprInt"], {"ExprInt"}, ["ExprInt"])

    def test_scalar_route_oracle_ids_are_one_to_one_and_exact(self) -> None:
        config = json.loads(generate.CONFIG.read_text())
        expected = generate.scalar_route_oracle_ids()
        generate.validate_scalar_route_oracle_mappings(config)
        self.assertEqual(len(expected), 30)
        self.assertEqual(len(set(expected.values())), 30)

        missing = copy.deepcopy(config)
        del missing["runtime_oracles"]["IntBinary"]
        with self.assertRaisesRegex(generate.EvidenceError, "missing reviewed scalar route oracle mapping"):
            generate.validate_scalar_route_oracle_mappings(missing)

        duplicate = copy.deepcopy(config)
        duplicate["runtime_oracles"]["IntBinary"]["ids"] = [expected["IntInt"]]
        with self.assertRaisesRegex(generate.EvidenceError, "duplicate scalar route oracle ID mapping"):
            generate.validate_scalar_route_oracle_mappings(duplicate)

        wrong_tag = copy.deepcopy(config)
        wrong_tag["runtime_oracles"]["IntBinary"]["ids"] = [expected["BoolAnd"]]
        with self.assertRaisesRegex(generate.EvidenceError, "reviewed scalar route oracle ID differs"):
            generate.validate_scalar_route_oracle_mappings(wrong_tag)

    def test_scalar_route_oracles_are_pinned_without_current_checkout_source(self) -> None:
        config = json.loads(generate.CONFIG.read_text())
        generate.validate_scalar_route_oracle_mappings(config)
        config["route_witness_mappings"] = [
            mapping for mapping in config["route_witness_mappings"]
            if mapping["test_id"] != generate.scalar_route_oracle_ids()["IntBinary"]
        ]
        with self.assertRaisesRegex(generate.EvidenceError, "absent from the pinned source map"):
            generate.validate_scalar_route_oracle_mappings(config)

    def test_route_witness_mappings_cover_the_schema_and_leave_non_expr_uncredited(self) -> None:
        root = generate.HERE.parents[4]
        config = json.loads(generate.CONFIG.read_text())
        rows = generate.parse_schema((root / generate.SCHEMA_PATH).read_text())
        mappings = generate.validate_route_witness_mappings(root, config, rows)
        expected = generate.expected_route_witness_mappings(
            root, config["route_witness_source_commit"], rows
        )
        self.assertEqual(set(mappings), set(expected))
        candidates = [
            mapping for mapping in mappings.values()
            if mapping["mapping_kind"] != "artifact_capability"
        ]
        capabilities = [
            mapping for mapping in mappings.values()
            if mapping["mapping_kind"] == "artifact_capability"
        ]
        expression_children = [
            mapping for mapping in mappings.values() if "child_source" in mapping
        ]
        unsupported = [mapping for mapping in mappings.values() if "child_source" not in mapping]
        self.assertEqual(len(rows), 152)
        self.assertEqual(len(mappings), 154)
        self.assertEqual({mapping["tag"] for mapping in mappings.values()}, {row["tag"] for row in rows})
        self.assertEqual(len(expression_children), 94)
        self.assertEqual(len(unsupported), 60)
        self.assertEqual(len(capabilities), 1)
        if_mappings = [mapping for mapping in mappings.values() if mapping["tag"] == "ExprIf"]
        self.assertEqual(
            {mapping["test_id"] for mapping in if_mappings},
            {
                "tests/xsh/expression-route-oracles.xsh::test_expr_if",
                "tests/xsh/expression-route-oracles.xsh::test_expr_if_pure_selection",
            },
        )
        self.assertEqual(
            {mapping["mode_expectations"]["normal"]["route_id"] for mapping in if_mappings},
            {"explicit_frame_control_path", "in_place_evaluator_path"},
        )
        self.assertEqual(
            {mapping["mode_expectations"]["forced"]["route_id"] for mapping in if_mappings},
            {"explicit_frame_control_path"},
        )
        match_pure = mappings[
            "tests/xsh/expression-route-oracles.xsh::test_expr_match_pure_selection"
        ]
        self.assertEqual(match_pure["tag"], "ExprMatch")
        self.assertEqual(
            match_pure["mode_expectations"]["normal"]["route_id"],
            "in_place_evaluator_path",
        )
        self.assertNotIn(
            "tests/xsh/expression-route-oracles.xsh::test_expr_match_pure_selection",
            generate.ROUTE_WITNESS_BEHAVIOR_ONLY_IDS,
        )
        for mapping in candidates:
            for expectation in mapping["mode_expectations"].values():
                if "child_source" in mapping:
                    self.assertIsNotNone(expectation["route_id"])
                    self.assertEqual(expectation["status"], "supported_by_artifact_format")
                else:
                    self.assertIsNone(expectation["route_id"])
                    self.assertEqual(expectation["status"], "unsupported_by_artifact_format")
        self.assertEqual(
            sum(
                expectation["route_id"] is not None
                for mapping in mappings.values()
                for expectation in mapping["mode_expectations"].values()
            ),
            188,
        )

        empty_map = capabilities[0]
        self.assertEqual(empty_map["tag"], "ExprEmptyMap")
        self.assertEqual(
            empty_map["mode_expectations"]["normal"]["route_id"],
            "explicit_frame_apply_path",
        )
        self.assertEqual(
            empty_map["mode_expectations"]["forced"]["route_id"],
            "generic_operand_apply_path",
        )

        missing = copy.deepcopy(config)
        missing["route_witness_mappings"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "route-witness source map differs"):
            generate.validate_route_witness_mappings(root, missing, rows)

        wrong_id = copy.deepcopy(config)
        wrong_id["route_witness_mappings"][0]["test_id"] = "tests/xsh/statement-route-oracles.xsh::test_missing"
        with self.assertRaisesRegex(generate.EvidenceError, "absent from the pinned source commit"):
            generate.validate_route_witness_mappings(root, wrong_id, rows)

        duplicate = copy.deepcopy(config)
        duplicate["route_witness_mappings"].append(copy.deepcopy(duplicate["route_witness_mappings"][0]))
        with self.assertRaisesRegex(generate.EvidenceError, "duplicate route-witness test ID mapping"):
            generate.validate_route_witness_mappings(root, duplicate, rows)

        contradictory = copy.deepcopy(config)
        frame_if = next(
            mapping for mapping in contradictory["route_witness_mappings"]
            if mapping["test_id"] == "tests/xsh/expression-route-oracles.xsh::test_expr_if"
        )
        conflicting_if = copy.deepcopy(frame_if)
        conflicting_if["mode_expectations"]["normal"]["route_id"] = "in_place_evaluator_path"
        contradictory["route_witness_mappings"].append(conflicting_if)
        with self.assertRaisesRegex(generate.EvidenceError, "duplicate route-witness test ID mapping"):
            generate.validate_route_witness_mappings(root, contradictory, rows)

        wrong_tag = copy.deepcopy(config)
        wrong_tag["route_witness_mappings"][0]["tag"] = "StmtAssign"
        with self.assertRaisesRegex(generate.EvidenceError, "tag/source mapping differs"):
            generate.validate_route_witness_mappings(root, wrong_tag, rows)

        wrong_route = copy.deepcopy(config)
        wrong_route["route_witness_mappings"][0]["mode_expectations"]["normal"]["route_id"] = "recursive_bridge_to_explicit_frames"
        with self.assertRaisesRegex(generate.EvidenceError, "without a reviewed child source"):
            generate.validate_route_witness_mappings(root, wrong_route, rows)

        wrong_mode = copy.deepcopy(config)
        capability = next(
            mapping for mapping in wrong_mode["route_witness_mappings"]
            if mapping["mapping_kind"] == "artifact_capability"
        )
        capability["mode_expectations"]["forced"]["route_id"] = "recursive_bridge_to_explicit_frames"
        with self.assertRaisesRegex(generate.EvidenceError, "unsupported route-witness capability claim"):
            generate.validate_route_witness_mappings(root, wrong_mode, rows)

        stale_source = copy.deepcopy(config)
        stale_source["route_witness_source_commit"] = "0" * 40
        with self.assertRaisesRegex(generate.EvidenceError, "candidate oracle source is absent"):
            generate.validate_route_witness_mappings(root, stale_source, rows)

    def test_expression_route_witness_mappings_cover_the_schema_and_reject_mutations(self) -> None:
        root = generate.HERE.parents[4]
        rows = generate.parse_schema((root / generate.SCHEMA_PATH).read_text())
        expected: dict[str, dict[str, str]] = {}
        mappings: list[dict] = []
        eager_tags = {item[0] for item in generate.EAGER_ATOM_PROVENANCE.values()}
        for row in rows:
            if row["kind"] != "Expr":
                continue
            if row["tag"] == "ExprEmptyMap":
                test_id = "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map"
                source_file = "tests/xsh/frame-apply-oracles.xsh"
                mapping_kind = "artifact_capability"
                normal_route = "explicit_frame_apply_path"
                forced_route = "generic_operand_apply_path"
            else:
                source_file = (
                    generate.SCALAR_ROUTE_ORACLE_FILE
                    if row["tag"] in eager_tags
                    else "tests/xsh/expression-route-oracles.xsh"
                )
                mapping_kind = "scalar" if row["tag"] in eager_tags else "statement_expression"
                suffix = re.sub(r"(?<!^)(?=[A-Z])", "_", row["tag"]).lower().removeprefix("expr_")
                test_id = f"{source_file}::test_expr_{suffix}"
                normal_route = (
                    "in_place_evaluator_path"
                    if row["in_place"] == "Operands"
                    else "explicit_frame_apply_path"
                    if row["frame_strategy"] == "FrameApply"
                    else "explicit_frame_control_path"
                )
                forced_route = (
                    "explicit_frame_control_path"
                    if row["frame_strategy"] == "FrameControl"
                    else "eager_frame_apply_path"
                    if row["eager_atom"]
                    else "generic_operand_apply_path"
                )
            item = {
                "tag": row["tag"],
                "test_id": test_id,
                "source_file": source_file,
                "mapping_kind": mapping_kind,
                "mode_expectations": {
                    "normal": {
                        "route_id": normal_route,
                        "status": "supported_by_artifact_format",
                    },
                    "forced": {
                        "route_id": forced_route,
                        "status": "supported_by_artifact_format",
                    },
                },
                "child_source": f"assert route_child_{row['ordinal']}\n",
            }
            expected[test_id] = {
                "tag": row["tag"],
                "source_file": source_file,
                "mapping_kind": mapping_kind,
            }
            mappings.append(item)

        scalar_oracle_ids = generate.scalar_route_oracle_ids()
        for row in rows:
            if row["kind"] == "Expr":
                continue
            if row["kind"] == "Stmt":
                source_file = "tests/xsh/statement-route-oracles.xsh"
                mapping_kind = "statement_expression"
                suffix = re.sub(r"(?<!^)(?=[A-Z])", "_", row["tag"]).lower().removeprefix("stmt_")
                test_id = f"{source_file}::test_stmt_{suffix}"
            else:
                source_file = generate.SCALAR_ROUTE_ORACLE_FILE
                mapping_kind = "scalar"
                test_id = scalar_oracle_ids[row["tag"]]
            expected[test_id] = {
                "tag": row["tag"],
                "source_file": source_file,
                "mapping_kind": mapping_kind,
            }
            mappings.append({
                **expected[test_id],
                "test_id": test_id,
                "mode_expectations": {
                    mode: {"route_id": None, "status": "unsupported_by_artifact_format"}
                    for mode in generate.ROUTE_WITNESS_EXPECTATION_MODES
                },
            })
        config = {
            "route_witness_source_commit": "a" * 40,
            "route_witness_mappings": mappings,
        }
        with mock.patch.object(generate, "expected_route_witness_mappings", return_value=expected):
            validated = generate.validate_route_witness_mappings(root, config, rows)
            self.assertEqual(set(validated), set(expected))
            self.assertEqual(
                {mapping["tag"] for mapping in mappings if mapping.get("child_source")},
                {row["tag"] for row in rows if row["kind"] == "Expr"},
            )

            for row in rows:
                if row["kind"] != "Expr":
                    continue
                frame_route = (
                    "explicit_frame_control_path"
                    if row["frame_strategy"] == "FrameControl"
                    else "explicit_frame_apply_path"
                )
                forced_route = (
                    frame_route
                    if row["frame_strategy"] == "FrameControl"
                    else "eager_frame_apply_path"
                    if row["eager_atom"]
                    else "generic_operand_apply_path"
                )
                self.assertTrue(generate.route_witness_route_allowed(row, "normal", frame_route))
                self.assertTrue(generate.route_witness_route_allowed(row, "forced", forced_route))
                for recursive_route in (
                    "in_place_evaluator_path",
                    "recursive_evaluator_path",
                ):
                    self.assertEqual(
                        generate.route_witness_route_allowed(row, "normal", recursive_route),
                        row["in_place"] == "Operands",
                    )
                    self.assertFalse(
                        generate.route_witness_route_allowed(row, "forced", recursive_route)
                    )

            recursive_row = next(
                row for row in rows
                if row["kind"] == "Expr" and row["in_place"] == "Operands"
            )
            recursive_config = copy.deepcopy(config)
            recursive_mapping = next(
                item for item in recursive_config["route_witness_mappings"]
                if item["tag"] == recursive_row["tag"]
            )
            recursive_mapping["mode_expectations"]["normal"]["route_id"] = (
                "recursive_evaluator_path"
            )
            with mock.patch.object(
                generate,
                "expected_route_witness_mappings",
                return_value=expected,
            ):
                generate.validate_route_witness_mappings(root, recursive_config, rows)

            for strategy in ("FrameApply", "FrameControl"):
                scheduled_row = next(
                    row for row in rows
                    if row["kind"] == "Expr"
                    and row["in_place"] == "Scheduled"
                    and row["frame_strategy"] == strategy
                )
                scheduled_config = copy.deepcopy(config)
                scheduled_mapping = next(
                    item for item in scheduled_config["route_witness_mappings"]
                    if item["tag"] == scheduled_row["tag"]
                )
                scheduled_mapping["mode_expectations"]["normal"]["route_id"] = (
                    "recursive_evaluator_path"
                )
                with mock.patch.object(
                    generate,
                    "expected_route_witness_mappings",
                    return_value=expected,
                ):
                    with self.assertRaisesRegex(
                        generate.EvidenceError,
                        "unsupported route-witness capability claim",
                    ):
                        generate.validate_route_witness_mappings(root, scheduled_config, rows)

            for mutation, message in (
                ("wrong_id", "absent from the pinned source commit"),
                ("wrong_source", "tag/source mapping differs"),
                ("wrong_tag", "tag/source mapping differs"),
                ("wrong_mode_route", "unsupported route-witness capability claim"),
                ("missing", "route-witness source map differs"),
                ("duplicate", "duplicate route-witness test ID mapping"),
            ):
                changed_config = copy.deepcopy(config)
                changed_expected = copy.deepcopy(expected)
                binary = next(
                    item for item in changed_config["route_witness_mappings"]
                    if item["tag"] == "ExprBinary"
                )
                if mutation == "wrong_id":
                    binary["test_id"] = "tests/xsh/expression-route-oracles.xsh::test_missing"
                elif mutation == "wrong_source":
                    binary["source_file"] = "tests/xsh/typed-scalar-route-oracles.xsh"
                elif mutation == "wrong_tag":
                    binary["tag"] = "ExprInt"
                elif mutation == "wrong_mode_route":
                    binary["mode_expectations"]["forced"]["route_id"] = "generic_operand_apply_path"
                elif mutation == "missing":
                    changed_config["route_witness_mappings"].remove(binary)
                elif mutation == "duplicate":
                    changed_config["route_witness_mappings"].append(copy.deepcopy(binary))
                with mock.patch.object(
                    generate,
                    "expected_route_witness_mappings",
                    return_value=changed_expected,
                ):
                    with self.assertRaisesRegex(generate.EvidenceError, message):
                        generate.validate_route_witness_mappings(root, changed_config, rows)

            missing_tag_config = copy.deepcopy(config)
            missing_tag_expected = copy.deepcopy(expected)
            missing_tag = next(
                item for item in missing_tag_config["route_witness_mappings"]
                if item["tag"] == "ExprBinary"
            )
            missing_tag_config["route_witness_mappings"].remove(missing_tag)
            missing_tag_expected.pop(missing_tag["test_id"])
            with mock.patch.object(
                generate,
                "expected_route_witness_mappings",
                return_value=missing_tag_expected,
            ):
                with self.assertRaisesRegex(generate.EvidenceError, "route-witness tag map differs from schema"):
                    generate.validate_route_witness_mappings(root, missing_tag_config, rows)

            missing_schema_config = copy.deepcopy(config)
            missing_schema_expected = copy.deepcopy(expected)
            missing_schema_tags = [
                next(row for row in rows if row["kind"] == "Stmt"),
                next(row for row in rows if row["kind"] == "Int"),
            ]
            for row in missing_schema_tags:
                mapping = next(
                    item for item in missing_schema_config["route_witness_mappings"]
                    if item["tag"] == row["tag"]
                )
                missing_schema_config["route_witness_mappings"].remove(mapping)
                missing_schema_expected.pop(mapping["test_id"])
            with mock.patch.object(
                generate,
                "expected_route_witness_mappings",
                return_value=missing_schema_expected,
            ):
                with self.assertRaisesRegex(generate.EvidenceError, "route-witness tag map differs from schema"):
                    generate.validate_route_witness_mappings(root, missing_schema_config, rows)

    def test_scalar_route_candidates_stay_pending_for_each_mode_and_receipt(self) -> None:
        config = json.loads(generate.CONFIG.read_text())
        expected = generate.scalar_route_oracle_ids()
        tag = "IntBinary"
        rows, inventory = typed_scalar_fixture()
        row = next(item for item in rows if item["tag"] == tag)
        row["oracles"] = {"runtime": config["runtime_oracles"][tag]["ids"]}
        row["normal_routes"] = [{"name": "typed_recursive_scalar", "test_ids": []}]
        row["forced_routes"] = [{"name": "forced_typed_scalar", "test_ids": []}]
        mapped = generate.build_fallback_path_map(
            row,
            {
                "schema_row": ["schema.rs:1"],
                "normal_dispatch": ["indexed_run.rs:1"],
                "forced_dispatch": ["explicit_run.rs:1"],
            },
            typed_scalar_inventory=generate.validate_typed_scalar_inventory(rows, inventory),
        )["branches"][0]["runtime_witness_mapping"]
        for mode in ("normal", "forced"):
            self.assertEqual(mapped[mode][0]["candidate_ids"], [expected[tag]])
            self.assertEqual(
                mapped[mode][0]["status"],
                "candidate_requires_exact_source_witness_and_receipt",
            )

        eager_row = {"oracles": {"runtime": config["runtime_oracles"]["ExprInt"]["ids"]}}
        for mode in ("normal", "forced"):
            mapped_route = generate.route_oracle_mapping(
                eager_row,
                {"name": f"{mode}_eager_atom", "test_ids": []},
            )
            self.assertEqual(mapped_route["oracle_ids"], [expected["ExprInt"]])
            self.assertEqual(
                mapped_route["mapping_status"],
                "row_candidate_requires_route_witness_and_receipt",
            )
            self.assertNotIn("receipt", mapped_route)

        oracle_id = expected[tag]
        receipt = {
            "source_expected": {"commit": "head", "tree": "tree"},
            "hashes": {
                "binaries_before_tests": {"xsht-verification": "xsht-hash"},
                "binaries_after_tests": {"xsht-verification": "xsht-hash"},
            },
            "source_checks": [
                {"where": "before:scalar", "head": "head", "tree": "tree", "clean": True},
                {"where": "after:scalar", "head": "head", "tree": "tree", "clean": True},
            ],
        }
        normal_step = {
            "id": "scalar",
            "exit_code": 0,
            "argv": ["bash", "run-native-final.sh", "run", "target/x86_64-unknown-linux-musl/verification/xsht", "test", "--exact", oracle_id],
            "env": {},
            "log_sha256": "a" * 64,
        }
        self.assertTrue(generate.receipt_step_matches(
            receipt,
            normal_step,
            expected_head="head",
            expected_tree="tree",
            oracle_id=oracle_id,
            mode="normal",
            expected_xsht_sha256="xsht-hash",
        ))
        self.assertFalse(generate.receipt_step_matches(
            receipt,
            normal_step,
            expected_head="head",
            expected_tree="tree",
            oracle_id=oracle_id,
            mode="forced",
            expected_xsht_sha256="xsht-hash",
        ))

    def test_expression_dispatch_requires_and_consumes_ast_inventory(self) -> None:
        rows = generate.parse_schema(SCHEMA)
        indexed = """fn indexed_frame_apply_tag(tag: FullTag) -> bool {
    matches!(tag, FullTag::ExprInt)
}
fn eval_indexed_expr() {
    matches!(tag, FullTag::ExprBinary)
}
"""
        explicit = """fn eval_expr_view() {
    match view_kind {
        ExprViewKind::Binary => {}
        ExprViewKind::Null | ExprViewKind::Unit => {
            return Err(frame_strategy_error(tag, span));
        }
    }
}
"""
        routed = generate.route_rows(rows, indexed, explicit)
        by_tag = {row["tag"]: row for row in routed}
        self.assertEqual(by_tag["ExprInt"]["normal_route"], "recursive_eager_atom_read")
        self.assertEqual(by_tag["ExprInt"]["forced_route"], "eager_atom_frame_fast_path")
        self.assertEqual(by_tag["ExprBinary"]["normal_route"], "direct_explicit_frames")
        self.assertEqual(by_tag["ExprBinary"]["forced_route"], "explicit_expression_arm")
        self.assertEqual(by_tag["StmtPrint"]["normal_route"], "eval_statement")
        self.assertEqual(by_tag["StmtPrint"]["forced_route"], "eval_statement")
        self.assertEqual(by_tag["ExprBinary"]["runtime_operand_order"]["status"], "unverified")
        with self.assertRaisesRegex(generate.EvidenceError, "missing AST expression dispatch inventory"):
            generate.build_fallback_path_map(
                by_tag["ExprBinary"],
                {"schema_row": ["schema.rs:3"], "normal_dispatch": ["indexed.rs:8"], "forced_dispatch": ["explicit.rs:5"]},
            )
        inventory = generate.validate_expression_dispatch_inventory(
            rows,
            expression_dispatch_fixture(rows),
        )
        row = by_tag["ExprBinary"]
        source_refs = generate.expression_dispatch_source_refs(row, inventory)
        self.assertEqual(len(source_refs["valid_branches"]), 1)
        self.assertEqual(len(source_refs["defensive_boundaries"]), 7)
        self.assertTrue(set(source_refs["valid_branches"]).isdisjoint(source_refs["defensive_boundaries"]))
        fallback = generate.build_fallback_path_map(
            row,
            source_refs,
            expression_dispatch_inventory=inventory,
        )
        self.assertEqual(fallback["review_status"], "source_exhaustive_dispatch_path")
        self.assertEqual(len(fallback["branches"]), 1)
        self.assertTrue(fallback["branches_exhaustive"])
        self.assertIsNone(fallback["no_fallback"])
        self.assertEqual(fallback["branches"][0]["normal_dispatch"]["kind"], "in_place_recursive_dispatch")
        self.assertEqual(fallback["branches"][0]["forced_dispatch"]["kind"], "explicit_frame_view_dispatch")
        normal_dispatch = fallback["branches"][0]["normal_dispatch"]
        self.assertTrue(normal_dispatch["in_place_eligible"])
        self.assertEqual(normal_dispatch["selected_when"]["path"], "eval_indexed_expr_inner")
        self.assertEqual(
            normal_dispatch["selected_when"]["predicate"],
            inventory["selectors"]["normal_recursive_entry"]["predicate"],
        )
        self.assertEqual(
            {branch["path"] for branch in normal_dispatch["executor_branches"]},
            {"eval_indexed_expr_with_frames", "eval_indexed_expr_inner"},
        )
        bypass = fallback["branches"][0]["normal_dispatch"]["in_place_bypass"]
        self.assertEqual(bypass["kind"], "normal_in_place_bypass")
        self.assertEqual(bypass["mode"], "normal")
        self.assertEqual(bypass["predicate"], "in_place")
        self.assertEqual(
            bypass["mode_selector_predicate"],
            "!self.evaluator.force_frames\n            && execution.is_in_place(instruction)",
        )
        self.assertTrue(bypass["reachable_for_schema_row"])
        self.assertEqual(len(fallback["defensive_boundaries"]), 7)
        self.assertEqual(
            sorted(boundary["source_ref"] for boundary in fallback["defensive_boundaries"]),
            source_refs["defensive_boundaries"],
        )
        self.assertNotIn(
            "normal_in_place_bypass",
            {boundary["kind"] for boundary in fallback["defensive_boundaries"]},
        )
        self.assertEqual(
            [bypass["source_ref"]],
            source_refs["valid_branches"],
        )
        self.assertTrue(all(
            boundary["reachable_for_valid_schema_row"] is False
            for boundary in fallback["defensive_boundaries"]
        ))
        self.assertFalse(fallback["runtime_behavior_proven"])

        malformed_inventory = expression_dispatch_fixture(rows)
        next(
            boundary
            for boundary in malformed_inventory["boundaries"]
            if (boundary["function"], boundary["kind"], boundary["outcome"])
            == generate.EXPRESSION_DISPATCH_NORMAL_BYPASS_IDENTITY
        )["predicate"] = "always"
        with self.assertRaisesRegex(generate.EvidenceError, "normal in-place bypass predicate changed"):
            generate.validate_expression_dispatch_inventory(rows, malformed_inventory)

    def test_expression_fallback_uses_schema_class_and_frame_apply_classifier(self) -> None:
        schema = """instruction_schema! {
    BuildExprRow => ExprView [Expr] {
        ProcCallResult { value: BuildExprId } => ExprProcCallResult = 0 [0; Scheduled; FrameControl];
        ProcessCommandArgv { value: BuildExprId } => ExprProcessCommandArgv = 1 [0; Scheduled; FrameApply];
    }
}

pattern_schema! {}
"""
        rows = generate.parse_schema(schema)
        inventory = generate.validate_expression_dispatch_inventory(
            rows,
            expression_dispatch_fixture(rows),
        )
        by_tag = {row["tag"]: row for row in rows}
        result_call = generate.expression_dispatch_fallback_path_map(
            by_tag["ExprProcCallResult"],
            generate.expression_dispatch_source_refs(by_tag["ExprProcCallResult"], inventory),
            inventory,
        )
        process_argv = generate.expression_dispatch_fallback_path_map(
            by_tag["ExprProcessCommandArgv"],
            generate.expression_dispatch_source_refs(by_tag["ExprProcessCommandArgv"], inventory),
            inventory,
        )
        normal_dispatch = result_call["branches"][0]["normal_dispatch"]
        self.assertFalse(normal_dispatch["in_place_eligible"])
        self.assertFalse(result_call["branches"][0]["normal_dispatch"]["recursive_match_reachable"])
        self.assertEqual(normal_dispatch["selected_when"]["path"], "eval_indexed_expr_with_frames")
        self.assertEqual(
            normal_dispatch["selected_when"]["predicate"],
            inventory["selectors"]["normal_frame_dispatch"]["predicate"],
        )
        self.assertEqual(
            {branch["path"] for branch in normal_dispatch["executor_branches"]},
            {"eval_indexed_expr_with_frames", "eval_indexed_expr_inner"},
        )
        bypass = result_call["branches"][0]["normal_dispatch"]["in_place_bypass"]
        self.assertFalse(bypass["reachable_for_schema_row"])
        self.assertEqual(bypass["required_schema_class"], "Operands")
        self.assertEqual(bypass["mode"], "normal")
        self.assertEqual(len(result_call["defensive_boundaries"]), 7)
        self.assertTrue(all(
            boundary["reachable_for_valid_schema_row"] is False
            for boundary in result_call["defensive_boundaries"]
        ))
        self.assertTrue(process_argv["branches"][0]["forced_dispatch"]["indexed_frame_apply_selected"])
        self.assertFalse(process_argv["branches"][0]["forced_dispatch"]["frame_strategy_overridden"])
        self.assertEqual(process_argv["branches"][0]["normal_dispatch"]["kind"], "scheduled_frame_apply_dispatch")
        self.assertEqual(len(process_argv["branches"][0]["forced_dispatch"]["child_arms"]), 1)
        self.assertEqual(len(process_argv["branches"][0]["forced_dispatch"]["value_arms"]), 1)

    def test_if_and_match_keep_distinct_normal_routes(self) -> None:
        schema = """instruction_schema! {
    BuildExprRow => ExprView [Expr] {
        IfExpr { branches: Vec<BuildExprId> } => ExprIf = 0 [0; Operands; FrameControl];
        MatchExpr { value: BuildExprId } => ExprMatch = 1 [0; Operands; FrameControl];
    }
}

pattern_schema! {}
"""
        rows = generate.parse_schema(schema)
        indexed = """fn indexed_frame_apply_tag(tag: FullTag) -> bool { false }
fn eval_indexed_expr() { FullTag::ExprIf; FullTag::ExprMatch; }
"""
        explicit = """fn eval_expr_view() {
    match view_kind {
        ExprViewKind::IfExpr => {}
        ExprViewKind::MatchExpr => {}
        ExprViewKind::Null | ExprViewKind::Unit => {
            return Err(frame_strategy_error(tag, span));
        }
    }
}
"""
        routed = {row["tag"]: row for row in generate.route_rows(rows, indexed, explicit)}
        for tag in ("ExprIf", "ExprMatch"):
            self.assertEqual(len(routed[tag]["normal_routes"]), 2)
            self.assertEqual(len(routed[tag]["forced_routes"]), 1)
            self.assertEqual(
                routed[tag]["normal_routes"][0]["name"],
                "verifier_approved_in_place_recursive_selection",
            )
            self.assertEqual(
                routed[tag]["normal_routes"][1]["name"],
                "non_in_place_explicit_frame_selection",
            )
            self.assertEqual(
                routed[tag]["forced_routes"][0]["name"],
                "forced_explicit_frame_selection",
            )
        self.assertEqual(routed["ExprMatch"]["normal_routes"][1]["test_ids"], [])

    def test_typed_scalar_inventory_classifies_errors_without_claiming_receipts(self) -> None:
        rows, inventory = typed_scalar_fixture()
        checked = generate.validate_typed_scalar_inventory(rows, inventory)
        row = next(item for item in rows if item["tag"] == "IntBinary")
        refs = {
            "schema_row": ["src/runtime/eval/indexed/full/schema.rs:3"],
            "normal_dispatch": ["src/runtime/eval/lowered_run/indexed_run.rs:10"],
            "forced_dispatch": ["src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:20"],
        }
        fallback = generate.build_fallback_path_map(
            row,
            refs,
            typed_scalar_inventory=checked,
        )
        self.assertEqual(fallback["review_status"], "source_exhaustive_dispatch_path")
        self.assertTrue(fallback["branches_exhaustive"])
        self.assertIsNone(fallback["no_fallback"])
        self.assertEqual(
            fallback["runtime_error_paths"]["status"],
            "source_exhaustive_boundary_inventory",
        )
        self.assertEqual(fallback["runtime_error_paths"]["runtime_receipts"], "pending")
        self.assertIn(
            "checked integer operation error",
            [item["outcome"] for item in fallback["runtime_error_paths"]["classifications"]],
        )
        self.assertEqual(fallback["branches"][0]["normal_wrapper"], "eval_indexed_typed_int")
        self.assertEqual(fallback["branches"][0]["forced_behavior"], "frame_operation")
        self.assertIn(
            {
                "scope": "malformed IntView payload",
                "outcome": "indexed_error: IntView decode failed",
                "source_ref": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:203",
                "arm_index": 0,
                "applies_to_row": True,
            },
            fallback["fallbacks"],
        )
        self.assertEqual(fallback["defensive_wrong_union"][0]["outcome"], "typed integer evaluator produced Bool")
        self.assertIn("checked_int_binary", fallback["branches"][0]["result_provenance"])
        self.assertEqual(
            fallback["branches"][0]["operand_order_and_liveness"],
            {
                "children": ["left", "right"],
                "evaluation": "left then right; LIFO work stack pushes right before left",
                "liveness": "both children are strict",
                "status": "source_cue_checked_pending_runtime_receipt",
            },
        )
        self.assertEqual(
            fallback["malformed_view_rejection"]["status"],
            "decoder_test_only_full_verifier_rejection_unproven",
        )
        for mode in ("normal", "forced"):
            self.assertEqual(
                fallback["branches"][0]["runtime_witness_mapping"][mode][0]["status"],
                "no_exact_row_mode_test_mapping",
            )

        row["normal_routes"] = [{"name": "typed_recursive_scalar", "test_ids": ["candidate::test"]}]
        candidate_map = generate.build_fallback_path_map(
            row,
            refs,
            typed_scalar_inventory=checked,
        )["branches"][0]["runtime_witness_mapping"]
        self.assertEqual(candidate_map["normal"][0]["candidate_ids"], ["candidate::test"])
        self.assertEqual(
            candidate_map["normal"][0]["status"],
            "candidate_requires_exact_source_witness_and_receipt",
        )

    def test_typed_scalar_semantics_keep_short_circuit_and_byte_default_liveness_open(self) -> None:
        semantics = generate.TYPED_SCALAR_SEMANTICS
        self.assertEqual(len(semantics), 18)
        self.assertEqual(semantics["BoolAnd"]["children"], ["left", "right"])
        self.assertEqual(semantics["BoolAnd"]["liveness"], "right child is lazy when left is false")
        self.assertEqual(semantics["BoolOr"]["liveness"], "right child is lazy when left is true")
        self.assertEqual(semantics["IntStrByteAtSlot"]["children"], ["index", "default?"])
        self.assertIn("only on absent byte", semantics["IntStrByteAtSlot"]["evaluation"])
        self.assertIn("default child decode/type error only on absent byte", semantics["IntStrByteAtSlot"]["errors"])
        self.assertEqual(semantics["BoolIntCompare"]["evaluation"], "left then right; LIFO work stack pushes right before left")
        self.assertEqual(semantics["BoolLiteralCompareSlot"]["children"], [])

    def test_typed_scalar_source_cues_cover_rows_and_critical_liveness(self) -> None:
        source, view_arms = typed_scalar_source_fixture()
        checked = generate.validate_typed_scalar_source_semantics(
            source,
            view_arms,
            "src/runtime/eval/lowered_run/indexed_run/values.rs",
        )
        self.assertEqual(checked["status"], "source_cues_checked_runtime_receipts_pending")
        self.assertEqual(len(checked["view_arm_refs"]), 18)
        self.assertIn("IntBinary", checked["runtime_semantic_refs"])
        self.assertIn("BoolAnd", checked["runtime_semantic_refs"])
        self.assertIn("BoolOr", checked["runtime_semantic_refs"])
        self.assertEqual(checked["verifier_rejection"], "no FullVerifier malformed typed scalar mutation receipt")

    def test_typed_scalar_source_mutations_reject_wrong_operand_order_and_liveness(self) -> None:
        source, view_arms = typed_scalar_source_fixture()
        swapped = source.replace(
            "work.push(TypedScalarWork::Int(right.raw()));\n                        work.push(TypedScalarWork::Int(left.raw()));",
            "work.push(TypedScalarWork::Int(left.raw()));\n                        work.push(TypedScalarWork::Int(right.raw()));",
            1,
        )
        with self.assertRaisesRegex(generate.EvidenceError, "work-stack order"):
            generate.validate_typed_scalar_source_semantics(swapped, view_arms, "values.rs")

        compare_pushes = (
            "work.push(TypedScalarWork::Int(right.raw()));\n"
            "                        work.push(TypedScalarWork::Int(left.raw()));"
        )
        compare_start = source.rfind(compare_pushes)
        self.assertGreater(compare_start, 0)
        swapped_compare = (
            source[:compare_start]
            + compare_pushes.replace("right.raw()", "TEMP.raw()").replace("left.raw()", "right.raw()").replace("TEMP.raw()", "left.raw()")
            + source[compare_start + len(compare_pushes):]
        )
        with self.assertRaisesRegex(generate.EvidenceError, "work-stack order"):
            generate.validate_typed_scalar_source_semantics(swapped_compare, view_arms, "values.rs")

        wrong_slot = source.replace(
            "let LoweredValue::Int(value) = slots[slot] else",
            "let LoweredValue::Int(value) = slots[slot + 1] else",
            1,
        )
        with self.assertRaisesRegex(generate.EvidenceError, "value provenance"):
            generate.validate_typed_scalar_source_semantics(wrong_slot, view_arms, "values.rs")

        wrong_boolean_tag = source.replace(
            "op: FullTag::BoolAnd,",
            "op: FullTag::BoolOr,",
            1,
        )
        with self.assertRaisesRegex(generate.EvidenceError, "value provenance"):
            generate.validate_typed_scalar_source_semantics(wrong_boolean_tag, view_arms, "values.rs")

        eager_bool_right = source.replace(
            "if (op == FullTag::BoolAnd && !left) || (op == FullTag::BoolOr && left)",
            "if (op == FullTag::BoolAnd && left) || (op == FullTag::BoolOr && !left)",
            1,
        )
        with self.assertRaisesRegex(generate.EvidenceError, "right-child liveness"):
            generate.validate_typed_scalar_source_semantics(eager_bool_right, view_arms, "values.rs")

        wrong_byte_miss = source.replace("if value == -1 {", "if value != -1 {", 1)
        with self.assertRaisesRegex(generate.EvidenceError, "work-stack order"):
            generate.validate_typed_scalar_source_semantics(wrong_byte_miss, view_arms, "values.rs")

    def test_typed_scalar_inventory_rejects_schema_and_classifier_drift(self) -> None:
        rows, inventory = typed_scalar_fixture()
        with self.assertRaisesRegex(generate.EvidenceError, "must contain 18 rows"):
            generate.validate_typed_scalar_inventory(rows + [{"kind": "Int"}], inventory)

        missing = copy.deepcopy(inventory)
        missing["classifiers"]["Int"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "classifier coverage"):
            generate.validate_typed_scalar_inventory(rows, missing)

        unknown = copy.deepcopy(inventory)
        unknown["classifiers"]["Bool"][0]["tag"] = "BoolUnknown"
        with self.assertRaisesRegex(generate.EvidenceError, "classifier coverage"):
            generate.validate_typed_scalar_inventory(rows, unknown)

        duplicate = copy.deepcopy(inventory)
        duplicate["classifiers"]["Bool"][1]["tag"] = duplicate["classifiers"]["Bool"][0]["tag"]
        with self.assertRaisesRegex(generate.EvidenceError, "classifier coverage"):
            generate.validate_typed_scalar_inventory(rows, duplicate)

    def test_typed_scalar_inventory_rejects_view_frame_and_fallback_drift(self) -> None:
        rows, inventory = typed_scalar_fixture()
        missing_view = copy.deepcopy(inventory)
        missing_view["view_arms"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "view arms differ"):
            generate.validate_typed_scalar_inventory(rows, missing_view)

        duplicate_view = copy.deepcopy(inventory)
        duplicate_view["view_arms"].append(dict(duplicate_view["view_arms"][0]))
        with self.assertRaisesRegex(generate.EvidenceError, "view arms differ"):
            generate.validate_typed_scalar_inventory(rows, duplicate_view)

        missing_group = copy.deepcopy(inventory)
        missing_group["frame_groups"][0]["tags"].remove("IntBinary")
        with self.assertRaisesRegex(generate.EvidenceError, "frame_operation group"):
            generate.validate_typed_scalar_inventory(rows, missing_group)

        missing_fallback = copy.deepcopy(inventory)
        missing_fallback["fallbacks"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "fallbacks differ"):
            generate.validate_typed_scalar_inventory(rows, missing_fallback)

        wrong_decoder_tags = copy.deepcopy(inventory)
        wrong_decoder_tags["fallbacks"][0]["applies_to"].remove("IntInt")
        with self.assertRaisesRegex(generate.EvidenceError, "fallback differs from its exact source branch"):
            generate.validate_typed_scalar_inventory(rows, wrong_decoder_tags)

        wrong_decoder_source = copy.deepcopy(inventory)
        wrong_decoder_source["fallbacks"][0]["source"] = "normal_dispatch"
        with self.assertRaisesRegex(generate.EvidenceError, "fallback differs from its exact source branch"):
            generate.validate_typed_scalar_inventory(rows, wrong_decoder_source)

        wrong_kind_arm = copy.deepcopy(inventory)
        wrong_kind_arm["fallbacks"][2]["arm_index"] = 3
        with self.assertRaisesRegex(generate.EvidenceError, "fallback differs from its exact source branch"):
            generate.validate_typed_scalar_inventory(rows, wrong_kind_arm)

        wrong_decoder_outcome = copy.deepcopy(inventory)
        wrong_decoder_outcome["fallbacks"][0]["outcome"] = "frame_strategy_error: decoded IntView does not match tag"
        with self.assertRaisesRegex(generate.EvidenceError, "fallbacks differ"):
            generate.validate_typed_scalar_inventory(rows, wrong_decoder_outcome)

        wrong_union = copy.deepcopy(inventory)
        wrong_union["wrong_union_errors"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "wrong-union errors"):
            generate.validate_typed_scalar_inventory(rows, wrong_union)

    def test_typed_scalar_fallback_map_requires_ast_inventory(self) -> None:
        rows, inventory = typed_scalar_fixture()
        row = rows[0]
        refs = {"schema_row": [], "normal_dispatch": [], "forced_dispatch": []}
        with self.assertRaisesRegex(generate.EvidenceError, "missing AST typed scalar inventory"):
            generate.build_fallback_path_map(row, refs)
        self.assertEqual(len(generate.validate_typed_scalar_inventory(rows, inventory)["schema_rows"]), 18)

    def test_eager_atom_inventory_maps_only_dispatch_and_preserves_helper_errors(self) -> None:
        rows, inventory = eager_atom_fixture()
        checked = generate.validate_eager_atom_inventory(rows, inventory)
        operand_proof = generate.validate_eager_atom_operand_structure(
            rows, checked, eager_atom_operand_fixture(rows)
        )
        self.assertEqual(set(operand_proof), {row["tag"] for row in rows})
        self.assertTrue(all(item["status"] == "not_applicable" for item in operand_proof.values()))
        self.assertEqual(operand_proof["ExprParam"]["schema_fields"], [{"name": "slot", "type": "usize"}])
        refs = {
            "schema_row": ["src/runtime/eval/indexed/full/schema.rs:780"],
            "normal_dispatch": ["src/runtime/eval/lowered_run/indexed_run.rs:1460"],
            "forced_dispatch": ["src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:6369"],
        }
        eager_path = next(row for row in rows if row["tag"] == "ExprPath")
        path_map = generate.build_fallback_path_map(
            eager_path,
            refs,
            eager_atom_inventory=checked,
        )
        self.assertEqual(path_map["review_status"], "source_exhaustive_dispatch_path")
        self.assertTrue(path_map["branches_exhaustive"])
        self.assertIsNone(path_map["no_fallback"])
        self.assertEqual(
            path_map["runtime_error_paths"]["status"],
            "source_exhaustive_boundary_inventory",
        )
        self.assertGreater(len(path_map["runtime_error_paths"]["source_refs"]), 2)
        self.assertEqual(len(path_map["runtime_error_paths"]["ast_helper_boundary_refs"]), 2)
        self.assertEqual(path_map["value_provenance"]["output"], "LoweredValue::Path")
        self.assertEqual(path_map["operand_liveness"]["status"], "not_applicable")
        self.assertTrue(any("frame_strategy_error" in item["outcome"] for item in path_map["fallbacks"]))
        self.assertFalse(next(
            item for item in path_map["fallbacks"]
            if "frame_strategy_error" in item["outcome"]
        )["applies_to_row"])
        self.assertTrue(any("unreachable!" in item["outcome"] for item in path_map["fallbacks"]))
        param_map = generate.build_fallback_path_map(
            next(row for row in rows if row["tag"] == "ExprParam"),
            refs,
            eager_atom_inventory=checked,
        )
        self.assertEqual(param_map["value_provenance"]["transformation"], "freeze_slot_then_clone")
        self.assertGreater(len(param_map["runtime_error_paths"]["source_refs"]), 2)
        self.assertEqual(len(param_map["runtime_error_paths"]["ast_helper_boundary_refs"]), 2)

    def test_eager_atom_inventory_rejects_schema_metadata_and_route_drift(self) -> None:
        rows, inventory = eager_atom_fixture()
        with self.assertRaisesRegex(generate.EvidenceError, "must contain 12 rows"):
            generate.validate_eager_atom_inventory(rows + [dict(rows[0])], inventory)

        missing = copy.deepcopy(inventory)
        missing["schema_rows"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "schema rows differ"):
            generate.validate_eager_atom_inventory(rows, missing)

        duplicate = copy.deepcopy(inventory)
        duplicate["schema_rows"].append(dict(duplicate["schema_rows"][0]))
        with self.assertRaisesRegex(generate.EvidenceError, "schema rows differ"):
            generate.validate_eager_atom_inventory(rows, duplicate)

        retagged = copy.deepcopy(inventory)
        retagged["schema_rows"][0]["tag"] = "ExprOther"
        with self.assertRaisesRegex(generate.EvidenceError, "schema rows differ"):
            generate.validate_eager_atom_inventory(rows, retagged)

        wrong_metadata = copy.deepcopy(inventory)
        wrong_metadata["schema_rows"][0]["strategy"] = "FrameControl"
        with self.assertRaisesRegex(generate.EvidenceError, "schema rows differ"):
            generate.validate_eager_atom_inventory(rows, wrong_metadata)

        missing_route = copy.deepcopy(inventory)
        missing_route["normal_route"] = None
        with self.assertRaisesRegex(generate.EvidenceError, "normal and forced"):
            generate.validate_eager_atom_inventory(rows, missing_route)

        reordered = copy.deepcopy(inventory)
        reordered["forced_route"]["fallback_statement_index"] = 3
        with self.assertRaisesRegex(generate.EvidenceError, "after its dispatch fallback"):
            generate.validate_eager_atom_inventory(rows, reordered)

        wrong_fallback = copy.deepcopy(inventory)
        wrong_fallback["normal_route"]["fallback_kind"] = "FrameApply"
        with self.assertRaisesRegex(generate.EvidenceError, "unexpected dispatch fallback"):
            generate.validate_eager_atom_inventory(rows, wrong_fallback)

    def test_eager_atom_inventory_rejects_view_fallback_and_boundary_drift(self) -> None:
        rows, inventory = eager_atom_fixture()
        missing_arm = copy.deepcopy(inventory)
        missing_arm["helper_arms"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "helper view arms differ"):
            generate.validate_eager_atom_inventory(rows, missing_arm)

        duplicate_arm = copy.deepcopy(inventory)
        duplicate_arm["helper_arms"].append(dict(duplicate_arm["helper_arms"][0]))
        with self.assertRaisesRegex(generate.EvidenceError, "helper view arms differ"):
            generate.validate_eager_atom_inventory(rows, duplicate_arm)

        unknown_arm = copy.deepcopy(inventory)
        unknown_arm["helper_arms"][0]["variant"] = "Mystery"
        with self.assertRaisesRegex(generate.EvidenceError, "helper view arms differ"):
            generate.validate_eager_atom_inventory(rows, unknown_arm)

        changed_wildcard = copy.deepcopy(inventory)
        changed_wildcard["helper_fallback"]["outcome"] = "Ok(())"
        with self.assertRaisesRegex(generate.EvidenceError, "wildcard outcome changed"):
            generate.validate_eager_atom_inventory(rows, changed_wildcard)

        changed_error_group = copy.deepcopy(inventory)
        changed_error_group["explicit_error_group"]["outcome"] = "Ok(())"
        with self.assertRaisesRegex(generate.EvidenceError, "defensive error group changed"):
            generate.validate_eager_atom_inventory(rows, changed_error_group)

        missing_boundary = copy.deepcopy(inventory)
        missing_boundary["helper_boundaries"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "helper boundary inventory"):
            generate.validate_eager_atom_inventory(rows, missing_boundary)

        reviewed = copy.deepcopy(inventory)
        reviewed["runtime_error_paths"]["status"] = "reviewed"
        with self.assertRaisesRegex(generate.EvidenceError, "not source-exhaustive"):
            generate.validate_eager_atom_inventory(rows, reviewed)

    def test_eager_atom_inventory_rejects_value_slot_and_boundary_mutations(self) -> None:
        rows, inventory = eager_atom_fixture()

        wrong_value = copy.deepcopy(inventory)
        next(arm for arm in wrong_value["helper_arms"] if arm["variant"] == "Int")["output"] = "LoweredValue::Bool"
        with self.assertRaisesRegex(generate.EvidenceError, "value provenance differs for Int"):
            generate.validate_eager_atom_inventory(rows, wrong_value)

        wrong_slot = copy.deepcopy(inventory)
        param = next(arm for arm in wrong_slot["helper_arms"] if arm["variant"] == "Param")
        param["input"] = "value"
        with self.assertRaisesRegex(generate.EvidenceError, "value provenance differs for Param"):
            generate.validate_eager_atom_inventory(rows, wrong_slot)

        missing_bounds = copy.deepcopy(inventory)
        missing_bounds["slot_decode_bounds_line"] = 0
        with self.assertRaisesRegex(generate.EvidenceError, "slot decoder"):
            generate.validate_eager_atom_inventory(rows, missing_bounds)

        bad_freeze = copy.deepcopy(inventory)
        bad_freeze["freeze_contract"]["shared_list_line"] = 0
        with self.assertRaisesRegex(generate.EvidenceError, "freeze contract"):
            generate.validate_eager_atom_inventory(rows, bad_freeze)

        operand_rows = eager_atom_operand_fixture(rows)
        operand_rows["rows"]["ExprParam"]["fields"][0]["policy"] = "strict"
        with self.assertRaisesRegex(generate.EvidenceError, "applicable operand-order policy"):
            generate.validate_eager_atom_operand_structure(rows, inventory, operand_rows)

    def test_eager_atom_fallback_map_requires_ast_inventory(self) -> None:
        rows, inventory = eager_atom_fixture()
        with self.assertRaisesRegex(generate.EvidenceError, "missing AST EagerAtom inventory"):
            generate.build_fallback_path_map(rows[0], {})
        checked = generate.validate_eager_atom_inventory(rows, inventory)
        self.assertEqual(len(checked["schema_rows"]), 12)
        path_map = generate.build_fallback_path_map(rows[0], {}, eager_atom_inventory=checked)
        branch = path_map["branches"][0]
        self.assertEqual(branch["normal_evaluator_fallback_ref"], "src/runtime/eval/lowered_run/indexed_run.rs:1470")
        self.assertNotIn("normal_generic_frame_apply_ref", branch)

    def test_frame_constructor_inventory_maps_only_dispatch_and_preserves_value_errors(self) -> None:
        rows, inventory = frame_constructor_fixture()
        checked = generate.validate_frame_constructor_inventory(rows, inventory)
        refs = {
            "schema_row": ["src/runtime/eval/indexed/full/schema.rs:790"],
            "normal_dispatch": ["src/runtime/eval/lowered_run/indexed_run.rs:40"],
            "forced_dispatch": ["src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:200"],
        }
        path_from = next(row for row in rows if row["tag"] == "ExprPathFrom")
        path_map = generate.build_fallback_path_map(
            path_from,
            refs,
            frame_constructor_inventory=checked,
        )
        self.assertEqual(path_map["review_status"], "source_exhaustive_dispatch_path")
        self.assertTrue(path_map["branches_exhaustive"])
        self.assertIsNone(path_map["no_fallback"])
        self.assertEqual(
            path_map["runtime_error_paths"]["status"],
            "source_exhaustive_boundary_inventory",
        )
        self.assertEqual(path_map["runtime_error_paths"]["runtime_receipts"], "pending")
        self.assertGreater(len(path_map["runtime_error_paths"]["source_refs"]), 2)
        self.assertEqual(path_map["branches"][0]["child_behavior"], "one_child")
        self.assertEqual(path_map["branches"][0]["value_behavior"], "indexed_path_from_value")
        self.assertEqual(
            path_map["branches"][0]["normal_route_ref"],
            "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:200",
        )
        self.assertEqual(len(path_map["fallbacks"]), 3)
        self.assertTrue(any("frame_strategy_error" in item["outcome"] for item in path_map["fallbacks"]))
        self.assertFalse(next(
            item for item in path_map["fallbacks"]
            if "frame_strategy_error" in item["outcome"]
        )["applies_to_row"])
        self.assertTrue(all(not item["applies_to_row"] for item in path_map["fallbacks"][:-1]))

    def test_frame_constructor_inventory_rejects_schema_and_classifier_drift(self) -> None:
        rows, inventory = frame_constructor_fixture()
        with self.assertRaisesRegex(generate.EvidenceError, "must contain 3 rows"):
            generate.validate_frame_constructor_inventory(rows + [dict(rows[0])], inventory)

        missing = copy.deepcopy(inventory)
        missing["schema_rows"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "schema rows differ"):
            generate.validate_frame_constructor_inventory(rows, missing)

        duplicate = copy.deepcopy(inventory)
        duplicate["schema_rows"].append(dict(duplicate["schema_rows"][0]))
        with self.assertRaisesRegex(generate.EvidenceError, "schema rows differ"):
            generate.validate_frame_constructor_inventory(rows, duplicate)

        wrong_metadata = copy.deepcopy(inventory)
        wrong_metadata["schema_rows"][0]["strategy"] = "FrameControl"
        with self.assertRaisesRegex(generate.EvidenceError, "schema metadata differs"):
            generate.validate_frame_constructor_inventory(rows, wrong_metadata)

        missing_classifier = copy.deepcopy(inventory)
        missing_classifier["classifiers"]["FrameApply"].pop(1)
        with self.assertRaisesRegex(generate.EvidenceError, "absent or duplicated"):
            generate.validate_frame_constructor_inventory(rows, missing_classifier)

        duplicate_classifier = copy.deepcopy(inventory)
        duplicate_classifier["classifiers"]["FrameApply"].append(
            dict(duplicate_classifier["classifiers"]["FrameApply"][0])
        )
        with self.assertRaisesRegex(generate.EvidenceError, "absent or duplicated"):
            generate.validate_frame_constructor_inventory(rows, duplicate_classifier)

        unknown_classifier = copy.deepcopy(inventory)
        unknown_classifier["classifiers"]["FrameApply"][0]["tag"] = "ExprMystery"
        with self.assertRaisesRegex(generate.EvidenceError, "unknown schema tag"):
            generate.validate_frame_constructor_inventory(rows, unknown_classifier)

        prepared_constructor = copy.deepcopy(inventory)
        prepared_constructor["classifiers"]["prepared_operands"].append(
            {"tag": "ExprPathFrom"}
        )
        with self.assertRaisesRegex(generate.EvidenceError, "prepared host operands"):
            generate.validate_frame_constructor_inventory(rows, prepared_constructor)

    def test_frame_constructor_inventory_rejects_route_helper_and_fallback_drift(self) -> None:
        rows, inventory = frame_constructor_fixture()
        missing_route = copy.deepcopy(inventory)
        missing_route["normal_route"] = None
        with self.assertRaisesRegex(generate.EvidenceError, "missing normal route"):
            generate.validate_frame_constructor_inventory(rows, missing_route)

        late_route = copy.deepcopy(inventory)
        late_route["forced_route"]["statement_index"] = 5
        with self.assertRaisesRegex(generate.EvidenceError, "occurs after its fallback"):
            generate.validate_frame_constructor_inventory(rows, late_route)

        different_dispatch = copy.deepcopy(inventory)
        different_dispatch["normal_route"]["line"] -= 1
        with self.assertRaisesRegex(generate.EvidenceError, "do not share the explicit scheduled route"):
            generate.validate_frame_constructor_inventory(rows, different_dispatch)

        missing_arm = copy.deepcopy(inventory)
        missing_arm["child_arms"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "child_arms differ"):
            generate.validate_frame_constructor_inventory(rows, missing_arm)

        duplicate_arm = copy.deepcopy(inventory)
        duplicate_arm["value_arms"].append(dict(duplicate_arm["value_arms"][0]))
        with self.assertRaisesRegex(generate.EvidenceError, "value_arms differ"):
            generate.validate_frame_constructor_inventory(rows, duplicate_arm)

        changed_value = copy.deepcopy(inventory)
        changed_value["value_arms"][1]["behavior"] = "indexed_other_value"
        with self.assertRaisesRegex(generate.EvidenceError, "value_arms differ"):
            generate.validate_frame_constructor_inventory(rows, changed_value)

        changed_wildcard = copy.deepcopy(inventory)
        changed_wildcard["helper_fallbacks"][0]["outcome"] = "Ok(None)"
        with self.assertRaisesRegex(generate.EvidenceError, "wildcard outcome changed"):
            generate.validate_frame_constructor_inventory(rows, changed_wildcard)

        changed_error_group = copy.deepcopy(inventory)
        changed_error_group["explicit_error_group"]["outcome"] = "Ok(())"
        with self.assertRaisesRegex(generate.EvidenceError, "defensive error group changed"):
            generate.validate_frame_constructor_inventory(rows, changed_error_group)

        missing_boundary = copy.deepcopy(inventory)
        missing_boundary["value_boundaries"].pop("PathFrom")
        with self.assertRaisesRegex(generate.EvidenceError, "value boundaries differ"):
            generate.validate_frame_constructor_inventory(rows, missing_boundary)

        reviewed = copy.deepcopy(inventory)
        reviewed["runtime_error_paths"]["status"] = "reviewed"
        with self.assertRaisesRegex(generate.EvidenceError, "must remain unreviewed"):
            generate.validate_frame_constructor_inventory(rows, reviewed)

    def test_frame_constructor_fallback_map_requires_ast_inventory(self) -> None:
        rows, inventory = frame_constructor_fixture()
        with self.assertRaisesRegex(generate.EvidenceError, "missing AST FrameApply constructor inventory"):
            generate.build_fallback_path_map(rows[0], {})
        self.assertEqual(
            len(generate.validate_frame_constructor_inventory(rows, inventory)["schema_rows"]),
            3,
        )

    def test_collection_construction_inventory_maps_dispatch_and_preserves_errors(self) -> None:
        rows, inventory = collection_construction_fixture()
        checked = generate.validate_collection_construction_inventory(rows, inventory)
        refs = {
            "schema_row": ["src/runtime/eval/indexed/full/schema.rs:858"],
            "normal_dispatch": ["src/runtime/eval/lowered_run/indexed_run.rs:1562"],
            "forced_dispatch": ["src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:6812"],
        }
        update = next(row for row in rows if row["tag"] == "ExprRecordUpdate")
        path_map = generate.build_fallback_path_map(
            update,
            refs,
            collection_construction_inventory=checked,
        )
        self.assertEqual(path_map["review_status"], "source_exhaustive_dispatch_path")
        self.assertTrue(path_map["branches_exhaustive"])
        self.assertIsNone(path_map["no_fallback"])
        self.assertEqual(
            path_map["runtime_error_paths"]["status"],
            "source_exhaustive_boundary_inventory",
        )
        self.assertEqual(path_map["branches"][0]["normal_behavior"], "evaluate_base_then_update_values_before_batch_replacement")
        self.assertEqual(path_map["branches"][0]["forced_behavior"], "record_update_base_continuation")
        self.assertEqual(len(path_map["fallbacks"]), 10)
        self.assertFalse(path_map["fallbacks"][-1]["applies_to_row"])
        for constructor_row in rows:
            constructor_map = generate.build_fallback_path_map(
                constructor_row,
                refs,
                collection_construction_inventory=checked,
            )
            frame_strategy_fallback = next(
                item for item in constructor_map["fallbacks"]
                if item["outcome"] == "frame_strategy_error(tag, span)"
            )
            self.assertFalse(frame_strategy_fallback["applies_to_row"])
            normal_view = next(
                item for item in constructor_map["fallbacks"]
                if item["scope"].startswith(constructor_row["variant"] + " ")
            ) if constructor_row["variant"] != "RecordUpdate" else None
            if normal_view is not None:
                self.assertTrue(normal_view["applies_to_row"])
                self.assertIn("indexed_run.rs", normal_view["source_ref"])
            helper_outcomes = {
                "Err(indexed-ir: list literal view is invalid)": {"ExprList", "ExprListBuild"},
                "Err(indexed-ir: map literal view is invalid)": {"ExprMapLiteral"},
                "Err(indexed-ir: record literal view is invalid)": {"ExprRecord"},
                "unreachable!: record-update tag has a matching borrowed expression view": {"ExprRecordUpdate"},
            }
            helper_messages = {
                "Err(indexed-ir: list literal view is invalid)": "list literal view is invalid",
                "Err(indexed-ir: map literal view is invalid)": "map literal view is invalid",
                "Err(indexed-ir: record literal view is invalid)": "record literal view is invalid",
            }
            row_helper_fallbacks = [
                item for item in constructor_map["fallbacks"]
                if item["outcome"] in helper_messages
            ]
            self.assertEqual(len(row_helper_fallbacks), 3)
            self.assertTrue(all("explicit_run.rs" in item["source_ref"] for item in row_helper_fallbacks))
            for item in constructor_map["fallbacks"]:
                if item["outcome"] in helper_outcomes:
                    self.assertEqual(item["applies_to_row"], constructor_row["tag"] in helper_outcomes[item["outcome"]])
                    if item["outcome"].startswith("Err("):
                        self.assertIn("explicit_run.rs", item["source_ref"])
                        source_path, line = item["source_ref"].rsplit(":", 1)
                        cited_line = (generate.HERE.parents[4] / source_path).read_text().splitlines()[int(line) - 1]
                        self.assertIn(helper_messages[item["outcome"]], cited_line)
                    if "record-update" in item["outcome"]:
                        self.assertIn("operands.rs", item["source_ref"])
        decode_fallback = next(
            item for item in path_map["fallbacks"] if item["scope"] == "a tag reaches the helper with a different typed ExprView"
            and item["source_ref"] == "src/runtime/eval/lowered_run/indexed_run/operands.rs:44"
        )
        self.assertIn("indexed_run/operands.rs", decode_fallback["source_ref"])
        self.assertEqual(
            decode_fallback["source_ref"],
            "src/runtime/eval/lowered_run/indexed_run/operands.rs:44",
        )
        source_root = generate.HERE.parents[4]
        cited_line = (source_root / "src/runtime/eval/lowered_run/indexed_run/operands.rs").read_text().splitlines()[43]
        self.assertIn("unreachable!(\"record-update tag has a matching borrowed expression view\")", cited_line)

    def test_collection_construction_inventory_rejects_source_drift(self) -> None:
        rows, inventory = collection_construction_fixture()

        missing = copy.deepcopy(inventory)
        missing["schema_rows"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "schema rows differ"):
            generate.validate_collection_construction_inventory(rows, missing)

        changed_arm = copy.deepcopy(inventory)
        changed_arm["normal_arms"][0]["behavior"] = "skip_record_entries"
        with self.assertRaisesRegex(generate.EvidenceError, "normal_arms differ"):
            generate.validate_collection_construction_inventory(rows, changed_arm)

        classifier_drift = copy.deepcopy(inventory)
        classifier_drift["frame_apply_classifier"]["tags"].append("ExprList")
        with self.assertRaisesRegex(generate.EvidenceError, "classifier exclusion"):
            generate.validate_collection_construction_inventory(rows, classifier_drift)

        view_drift = copy.deepcopy(inventory)
        view_drift["normal_view_fallbacks"][0]["scope"] = "Unknown tag reaches the normal evaluator"
        with self.assertRaisesRegex(generate.EvidenceError, "normal typed-view fallbacks differ"):
            generate.validate_collection_construction_inventory(rows, view_drift)

        helper_drift = copy.deepcopy(inventory)
        helper_drift["helper_fallbacks"][0]["outcome"] = "Ok(None)"
        with self.assertRaisesRegex(generate.EvidenceError, "typed-view helper fallbacks differ"):
            generate.validate_collection_construction_inventory(rows, helper_drift)

        conditional_route = copy.deepcopy(inventory)
        conditional_route["forced_route"]["view_kind_statement_index"] = 6
        with self.assertRaisesRegex(generate.EvidenceError, "forced route selection order"):
            generate.validate_collection_construction_inventory(rows, conditional_route)

        reviewed = copy.deepcopy(inventory)
        reviewed["runtime_error_paths"]["status"] = "reviewed"
        with self.assertRaisesRegex(generate.EvidenceError, "runtime errors must remain unreviewed"):
            generate.validate_collection_construction_inventory(rows, reviewed)

    def test_collection_construction_fallback_map_requires_ast_inventory(self) -> None:
        rows, inventory = collection_construction_fixture()
        with self.assertRaisesRegex(generate.EvidenceError, "missing AST collection constructor inventory"):
            generate.build_fallback_path_map(rows[0], {})
        self.assertEqual(
            len(generate.validate_collection_construction_inventory(rows, inventory)["schema_rows"]),
            5,
        )

    def test_call_frame_inventory_maps_scheduled_dispatch_and_preserves_call_errors(self) -> None:
        rows, inventory = call_frame_fixture()
        checked = generate.validate_call_frame_inventory(rows, inventory)
        call = next(row for row in rows if row["tag"] == "ExprCall")
        path_map = generate.build_fallback_path_map(
            call,
            {},
            call_frame_inventory=checked,
        )
        self.assertEqual(path_map["review_status"], "source_exhaustive_dispatch_path")
        self.assertTrue(path_map["branches_exhaustive"])
        self.assertIsNone(path_map["no_fallback"])
        self.assertEqual(
            path_map["runtime_error_paths"]["status"],
            "source_exhaustive_boundary_inventory",
        )
        self.assertEqual(path_map["branches"][0]["normal_behavior"], "scheduled_frame_control_routes_to_explicit_machine")
        self.assertEqual(path_map["branches"][0]["forced_behavior"], "decode_static_call_and_schedule_arguments")
        self.assertEqual(path_map["branches"][0]["forced_view_group"], ["Call", "SelfCall", "DirectPureCall"])
        self.assertEqual(len(path_map["fallbacks"]), 6)
        self.assertEqual(
            [item["applies_to_row"] for item in path_map["fallbacks"]],
            [False, True, True, False, False, False],
        )
        self.assertIn("indexed_run.rs:1307", path_map["source_refs"]["call_frame_normal_route"][0])
        self.assertIn("explicit_run.rs:7279", path_map["source_refs"]["call_frame_forced_dispatch"][0])
        for row in rows:
            row_map = generate.build_fallback_path_map(
                row,
                {},
                call_frame_inventory=checked,
            )
            self.assertEqual(len(row_map["branches"]), 1)
            self.assertEqual(len(row_map["fallbacks"]), 6)

    def test_call_frame_inventory_rejects_schema_route_and_view_drift(self) -> None:
        rows, inventory = call_frame_fixture()

        missing = copy.deepcopy(inventory)
        missing["schema_rows"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "AST schema rows differ"):
            generate.validate_call_frame_inventory(rows, missing)

        wrong_route = copy.deepcopy(inventory)
        wrong_route["normal_route"]["frame_helper_line"] = 0
        with self.assertRaisesRegex(generate.EvidenceError, "routes are incomplete"):
            generate.validate_call_frame_inventory(rows, wrong_route)

        wrong_group = copy.deepcopy(inventory)
        wrong_group["forced_arms"][0]["view_group"] = ["Call"]
        with self.assertRaisesRegex(generate.EvidenceError, "forced handlers differ"):
            generate.validate_call_frame_inventory(rows, wrong_group)

        wrong_fallback = copy.deepcopy(inventory)
        wrong_fallback["typed_view_fallbacks"][2]["outcome"] = "Ok(())"
        with self.assertRaisesRegex(generate.EvidenceError, "typed-view fallbacks differ"):
            generate.validate_call_frame_inventory(rows, wrong_fallback)

    def test_call_frame_fallback_map_requires_ast_inventory(self) -> None:
        rows, inventory = call_frame_fixture()
        with self.assertRaisesRegex(generate.EvidenceError, "missing AST call frame inventory"):
            generate.build_fallback_path_map(rows[0], {})
        self.assertEqual(
            len(generate.validate_call_frame_inventory(rows, inventory)["schema_rows"]),
            4,
        )

    def test_field_index_inventory_maps_routes_and_keeps_lookup_errors_unreviewed(self) -> None:
        rows, inventory = field_index_fixture()
        checked = generate.validate_field_index_inventory(rows, inventory)
        field_row = next(row for row in rows if row["tag"] == "ExprField")
        path_map = generate.build_fallback_path_map(
            field_row,
            {},
            field_index_inventory=checked,
        )
        self.assertEqual(path_map["review_status"], "source_exhaustive_dispatch_path")
        self.assertTrue(path_map["branches_exhaustive"])
        self.assertEqual(
            path_map["runtime_error_paths"]["status"],
            "source_exhaustive_boundary_inventory",
        )
        self.assertEqual(path_map["branches"][0]["normal_behavior"], "try_record_field_chain_then_evaluate_base_and_lookup_field")
        self.assertEqual(path_map["branches"][0]["forced_behavior"], "try_record_field_chain_then_schedule_base")
        self.assertEqual(
            [item["applies_to_row"] for item in path_map["fallbacks"]],
            [False, True, True, False],
        )
        self.assertTrue(any("indexed_run.rs:1672" in ref for ref in path_map["source_refs"]["field_index_normal_dispatch"]))
        self.assertEqual(path_map["branches"][0]["normal_wrapper_route_ref"], "src/runtime/eval/lowered_run/indexed_run.rs:1307")
        self.assertIn("explicit_run.rs:7149", path_map["source_refs"]["field_index_forced_dispatch"][0])
        for row in rows:
            row_map = generate.build_fallback_path_map(
                row,
                {},
                field_index_inventory=checked,
            )
            self.assertEqual(len(row_map["branches"]), 1)
            self.assertEqual(len(row_map["fallbacks"]), 4)

    def test_field_index_inventory_rejects_schema_route_and_fallback_drift(self) -> None:
        rows, inventory = field_index_fixture()

        missing = copy.deepcopy(inventory)
        missing["schema_rows"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "AST schema rows differ"):
            generate.validate_field_index_inventory(rows, missing)

        wrong_arm = copy.deepcopy(inventory)
        wrong_arm["normal_arms"][1]["behavior"] = "base_only"
        with self.assertRaisesRegex(generate.EvidenceError, "normal_arms differ"):
            generate.validate_field_index_inventory(rows, wrong_arm)

        wrong_view = copy.deepcopy(inventory)
        wrong_view["forced_view_fallbacks"][2]["outcome"] = "Ok(())"
        with self.assertRaisesRegex(generate.EvidenceError, "forced_view_fallbacks differ"):
            generate.validate_field_index_inventory(rows, wrong_view)

        wrong_route = copy.deepcopy(inventory)
        wrong_route["forced_route"]["view_kind_statement_index"] = 0
        with self.assertRaisesRegex(generate.EvidenceError, "route selection order changed"):
            generate.validate_field_index_inventory(rows, wrong_route)

        wrong_normal_route = copy.deepcopy(inventory)
        wrong_normal_route["normal_route"]["inner_helper_line"] = 1299
        with self.assertRaisesRegex(generate.EvidenceError, "normal wrapper route order changed"):
            generate.validate_field_index_inventory(rows, wrong_normal_route)

    def test_field_index_fallback_map_requires_ast_inventory(self) -> None:
        rows, inventory = field_index_fixture()
        with self.assertRaisesRegex(generate.EvidenceError, "missing AST field/index inventory"):
            generate.build_fallback_path_map(rows[0], {})
        self.assertEqual(
            len(generate.validate_field_index_inventory(rows, inventory)["schema_rows"]),
            3,
        )

    def test_result_constructor_inventory_maps_dispatch_and_preserves_errors(self) -> None:
        rows, inventory = result_constructor_fixture()
        checked = generate.validate_result_constructor_inventory(rows, inventory)
        refs = {
            "schema_row": ["src/runtime/eval/indexed/full/schema.rs:1114"],
            "normal_dispatch": ["src/runtime/eval/lowered_run/indexed_run.rs:1850"],
            "forced_dispatch": ["src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:6947"],
        }
        err = next(row for row in rows if row["tag"] == "ExprErr")
        path_map = generate.build_fallback_path_map(
            err,
            refs,
            result_constructor_inventory=checked,
        )
        self.assertEqual(path_map["review_status"], "source_exhaustive_dispatch_path")
        self.assertTrue(path_map["branches_exhaustive"])
        self.assertIsNone(path_map["no_fallback"])
        self.assertEqual(
            path_map["runtime_error_paths"]["status"],
            "source_exhaustive_boundary_inventory",
        )
        self.assertEqual(path_map["branches"][0]["normal_behavior"], "evaluate_error_then_optional_cause_before_constructing_error")
        self.assertEqual(path_map["branches"][0]["forced_behavior"], "push_value_with_optional_cause_wrap_err_continuation")
        self.assertEqual([item["applies_to_row"] for item in path_map["fallbacks"]], [False, True, True, False])
        self.assertIn("explicit_run.rs", path_map["fallbacks"][2]["source_ref"])
        for constructor_row in rows:
            row_map = generate.build_fallback_path_map(
                constructor_row,
                refs,
                result_constructor_inventory=checked,
            )
            self.assertTrue(row_map["branches_exhaustive"])
            self.assertEqual(len(row_map["fallbacks"]), 4)

    def test_result_constructor_inventory_rejects_source_drift(self) -> None:
        rows, inventory = result_constructor_fixture()
        missing = copy.deepcopy(inventory)
        missing["schema_rows"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "schema rows differ"):
            generate.validate_result_constructor_inventory(rows, missing)

        wrong_arm = copy.deepcopy(inventory)
        wrong_arm["normal_arms"][0]["behavior"] = "discard_result_value"
        with self.assertRaisesRegex(generate.EvidenceError, "normal_arms differ"):
            generate.validate_result_constructor_inventory(rows, wrong_arm)

        changed_view = copy.deepcopy(inventory)
        changed_view["forced_view_fallbacks"][0]["outcome"] = "Ok(())"
        with self.assertRaisesRegex(generate.EvidenceError, "forced_view_fallbacks differ"):
            generate.validate_result_constructor_inventory(rows, changed_view)

        changed_route = copy.deepcopy(inventory)
        changed_route["forced_route"]["view_kind_statement_index"] = 6
        with self.assertRaisesRegex(generate.EvidenceError, "selection order"):
            generate.validate_result_constructor_inventory(rows, changed_route)

        reviewed = copy.deepcopy(inventory)
        reviewed["runtime_error_paths"]["status"] = "reviewed"
        with self.assertRaisesRegex(generate.EvidenceError, "must remain unreviewed"):
            generate.validate_result_constructor_inventory(rows, reviewed)

    def test_result_constructor_fallback_map_requires_ast_inventory(self) -> None:
        rows, inventory = result_constructor_fixture()
        with self.assertRaisesRegex(generate.EvidenceError, "missing AST result constructor inventory"):
            generate.build_fallback_path_map(rows[0], {})
        self.assertEqual(
            len(generate.validate_result_constructor_inventory(rows, inventory)["schema_rows"]),
            2,
        )

    def test_statement_fallback_map_requires_source_exhaustive_tag_inventory(self) -> None:
        row = {"kind": "Stmt", "tag": "StmtPrint", "fallback_path_map": []}
        inventory = {
            "owner": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
            "match_line": 5384,
            "branches": [
                {
                    "tag": "StmtPrint",
                    "pattern": "FullTag::StmtPrint",
                    "line": 5385,
                    "arm_index": 0,
                }
            ],
            "fallbacks": [
                {
                    "pattern": "_",
                    "outcome": "frame_strategy_error",
                    "line": 6269,
                    "arm_index": 42,
                }
            ],
        }
        refs = {
            "schema_row": ["schema.rs:12"],
            "normal_dispatch": ["explicit_run.rs:5385", "explicit_run.rs:6269"],
            "forced_dispatch": ["explicit_run.rs:5385", "explicit_run.rs:6269"],
            "statement_match": ["explicit_run.rs:5384"],
        }
        fallback = generate.build_fallback_path_map(row, refs, inventory)
        self.assertEqual(
            fallback["review_status"],
            "source_exhaustive_top_level_tag_dispatch",
        )
        self.assertEqual(
            fallback["scope"],
            "eval_statement top-level FullTag match; arm-body semantics remain in runtime evidence",
        )
        self.assertTrue(fallback["branches_exhaustive"])
        self.assertTrue(fallback["no_fallback"])
        self.assertEqual(fallback["branches"][0]["source_ref"], "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs:5385")
        self.assertFalse(fallback["fallbacks"][0]["reachable_for_valid_statement_tag"])
        self.assertEqual(fallback["fallbacks"][0]["arm_index"], 42)
        with self.assertRaisesRegex(generate.EvidenceError, "missing AST statement inventory"):
            generate.build_fallback_path_map(row, refs)

    def test_statement_fallback_map_identifies_typed_view_dispatch(self) -> None:
        row = {"kind": "Stmt", "tag": "StmtPrint", "fallback_path_map": []}
        inventory = {
            "owner": "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
            "match_line": 5585,
            "branches": [
                {
                    "tag": "StmtPrint",
                    "pattern": "StmtView::Print",
                    "line": 5586,
                    "arm_index": 0,
                }
            ],
            "fallbacks": [
                {
                    "pattern": "tag.kind() != InstructionKind::Stmt",
                    "outcome": "frame_strategy_error",
                    "line": 5580,
                    "statement_index": 0,
                }
            ],
        }
        refs = {
            "schema_row": ["schema.rs:12"],
            "normal_dispatch": ["explicit_run.rs:5586", "explicit_run.rs:5580"],
            "forced_dispatch": ["explicit_run.rs:5586", "explicit_run.rs:5580"],
            "statement_match": ["explicit_run.rs:5585"],
        }
        fallback = generate.build_fallback_path_map(row, refs, inventory)
        self.assertEqual(
            fallback["scope"],
            "eval_statement top-level StmtView match; arm-body semantics remain in runtime evidence",
        )
        self.assertEqual(
            fallback["fallbacks"][0]["pattern"],
            "tag.kind() != InstructionKind::Stmt",
        )
        self.assertEqual(fallback["fallbacks"][0]["statement_index"], 0)
        self.assertNotIn("arm_index", fallback["fallbacks"][0])
        self.assertFalse(fallback["fallbacks"][0]["reachable_for_valid_statement_tag"])

    def test_route_witness_artifact_rejects_wrong_tuple_and_stale_identity(self) -> None:
        root = generate.HERE.parents[4]
        scratch = root / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch, prefix="route-witness-artifact-") as temporary:
            run_dir = pathlib.Path(temporary) / "normal"
            config, mapping, receipt_path, receipt, artifact_bytes = self.route_witness_receipt_fixture(
                root, run_dir, "normal"
            )
            rows = generate.parse_schema((root / generate.SCHEMA_PATH).read_text())
            row = next(row for row in rows if row["tag"] == "ExprEmptyMap")
            ordinal = row["ordinal"]
            source_bytes = generate.route_witness_source_bytes(
                root, mapping, config["route_witness_source_commit"]
            )
            self.assertEqual(source_bytes, mapping["child_source"].encode("utf-8"))
            self.assertTrue(generate.route_witness_artifact_matches(
                artifact_bytes, mapping, row, "normal", ordinal, source_bytes
            ))

            artifact = json.loads(artifact_bytes)
            for field, wrong in (
                ("test_id", "tests/xsh/frame-apply-oracles.xsh::test_wrong"),
                ("source_hash", "0" * 64),
            ):
                changed = {**artifact, field: wrong}
                self.assertFalse(generate.route_witness_artifact_matches(
                    json.dumps(changed).encode(), mapping, row, "normal", ordinal, source_bytes
                ))
            for field, wrong in (
                ("tag_name", "ExprPath"),
                ("tag_ordinal", 48),
                ("route", "generic_operand_apply_path"),
                ("mode", "forced"),
            ):
                changed = copy.deepcopy(artifact)
                changed["records"][0][field] = wrong
                self.assertFalse(generate.route_witness_artifact_matches(
                    json.dumps(changed).encode(), mapping, row, "normal", ordinal, source_bytes
                ))
            duplicate_record = copy.deepcopy(artifact)
            duplicate_record["records"].append(copy.deepcopy(duplicate_record["records"][0]))
            self.assertFalse(generate.route_witness_artifact_matches(
                json.dumps(duplicate_record).encode(), mapping, row, "normal", ordinal, source_bytes
            ))

            candidate_mappings = [
                item for item in json.loads(generate.CONFIG.read_text())["route_witness_mappings"]
                if item["mapping_kind"] != "artifact_capability"
            ]
            for candidate in candidate_mappings:
                candidate_row = next(row for row in rows if row["tag"] == candidate["tag"])
                for mode in generate.ROUTE_WITNESS_EXPECTATION_MODES:
                    self.assertFalse(generate.route_witness_artifact_matches(
                        artifact_bytes, candidate, candidate_row, mode, ordinal, source_bytes
                    ))

            proof = generate.route_witness_step_matches(
                root, receipt_path, receipt, receipt["steps"][0], mapping, row,
                source_commit=config["route_witness_source_commit"],
                expected_head="head", expected_tree="tree", expected_ordinal=ordinal,
                mode="normal", expected_xsht_sha256="xsht-hash",
            )
            self.assertIsNotNone(proof)
            self.assertEqual(proof["route_id"], "explicit_frame_apply_path")

            missing_artifact = copy.deepcopy(receipt["steps"][0])
            del missing_artifact["route_witness_artifact"]
            self.assertIsNone(generate.route_witness_step_matches(
                root, receipt_path, receipt, missing_artifact, mapping, row,
                source_commit=config["route_witness_source_commit"],
                expected_head="head", expected_tree="tree", expected_ordinal=ordinal,
                mode="normal", expected_xsht_sha256="xsht-hash",
            ))
            wrong_artifact_hash = copy.deepcopy(receipt["steps"][0])
            wrong_artifact_hash["route_witness_artifact"]["sha256"] = "0" * 64
            self.assertIsNone(generate.route_witness_step_matches(
                root, receipt_path, receipt, wrong_artifact_hash, mapping, row,
                source_commit=config["route_witness_source_commit"],
                expected_head="head", expected_tree="tree", expected_ordinal=ordinal,
                mode="normal", expected_xsht_sha256="xsht-hash",
            ))
            escaped_artifact = copy.deepcopy(receipt["steps"][0])
            escaped_artifact["route_witness_artifact"]["path"] = "../outside.json"
            self.assertIsNone(generate.route_witness_step_matches(
                root, receipt_path, receipt, escaped_artifact, mapping, row,
                source_commit=config["route_witness_source_commit"],
                expected_head="head", expected_tree="tree", expected_ordinal=ordinal,
                mode="normal", expected_xsht_sha256="xsht-hash",
            ))
            self.assertIsNone(generate.route_witness_step_matches(
                root, receipt_path, receipt, receipt["steps"][0], mapping, row,
                source_commit=config["route_witness_source_commit"],
                expected_head="stale-head", expected_tree="tree", expected_ordinal=ordinal,
                mode="normal", expected_xsht_sha256="xsht-hash",
            ))

    def test_route_witness_artifacts_bind_reviewed_child_source_in_both_modes(self) -> None:
        root = generate.HERE.parents[4]
        scratch = root / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch, prefix="route-witness-child-source-") as temporary:
            base = pathlib.Path(temporary)
            artifacts = {}
            mapping = None
            config = None
            row = next(
                row
                for row in generate.parse_schema((root / generate.SCHEMA_PATH).read_text())
                if row["tag"] == "ExprEmptyMap"
            )
            ordinal = row["ordinal"]
            for mode in generate.ROUTE_WITNESS_EXPECTATION_MODES:
                config, mapping, _, _, artifact_bytes = self.route_witness_receipt_fixture(
                    root, base / mode, mode
                )
                source_bytes = generate.route_witness_source_bytes(
                    root, mapping, config["route_witness_source_commit"]
                )
                self.assertEqual(source_bytes, mapping["child_source"].encode("utf-8"))
                self.assertTrue(generate.route_witness_artifact_matches(
                    artifact_bytes, mapping, row, mode, ordinal, source_bytes
                ))
                artifacts[mode] = artifact_bytes

            self.assertIsNotNone(config)
            self.assertIsNotNone(mapping)
            normal_artifact = json.loads(artifacts["normal"])
            for field, wrong in (
                ("test_id", "tests/xsh/frame-apply-oracles.xsh::test_wrong"),
                ("source_hash", "0" * 64),
            ):
                changed = {**normal_artifact, field: wrong}
                self.assertFalse(generate.route_witness_artifact_matches(
                    json.dumps(changed).encode(), mapping, row, "normal", ordinal,
                    mapping["child_source"].encode("utf-8"),
                ))
            for field, wrong in (
                ("tag_name", "ExprPath"),
                ("tag_ordinal", 48),
                ("route", "generic_operand_apply_path"),
                ("mode", "forced"),
            ):
                changed = copy.deepcopy(normal_artifact)
                changed["records"][0][field] = wrong
                self.assertFalse(generate.route_witness_artifact_matches(
                    json.dumps(changed).encode(), mapping, row, "normal", ordinal,
                    mapping["child_source"].encode("utf-8"),
                ))

            changed_mapping = copy.deepcopy(mapping)
            changed_mapping["child_source"] += "assert true\n"
            changed_source = generate.route_witness_source_bytes(
                root, changed_mapping, config["route_witness_source_commit"]
            )
            self.assertEqual(changed_source, changed_mapping["child_source"].encode("utf-8"))
            self.assertFalse(generate.route_witness_artifact_matches(
                artifacts["normal"], changed_mapping, row, "normal", ordinal, changed_source
            ))

            with mock.patch.object(
                generate.subprocess,
                "run",
                return_value=mock.Mock(stdout=b"altered reviewed test file"),
            ):
                self.assertIsNone(generate.route_witness_source_bytes(
                    root, mapping, config["route_witness_source_commit"]
                ))

            expression_mapping = copy.deepcopy(next(
                item
                for item in config["route_witness_mappings"]
                if item["tag"] == "ExprBinary"
            ))
            expression_mapping["child_source"] = "assert 1 + 2 == 3\n"
            expression_mapping["mode_expectations"] = {
                mode: {
                    "route_id": "explicit_frame_control_path",
                    "status": "supported_by_artifact_format",
                }
                for mode in generate.ROUTE_WITNESS_EXPECTATION_MODES
            }
            expression_row = next(
                item
                for item in generate.parse_schema((root / generate.SCHEMA_PATH).read_text())
                if item["tag"] == "ExprBinary"
            )
            expression_source = generate.route_witness_source_bytes(
                root,
                expression_mapping,
                config["route_witness_source_commit"],
            )
            self.assertEqual(
                expression_source,
                expression_mapping["child_source"].encode("utf-8"),
            )
            expression_source_hash = generate.sha256_bytes(expression_source)
            expression_artifact = {
                "version": 1,
                "test_id": expression_mapping["test_id"],
                "source_hash": expression_source_hash,
                "records": [{
                    "tag_name": "ExprBinary",
                    "tag_ordinal": expression_row["ordinal"],
                    "route": "explicit_frame_control_path",
                    "mode": "normal",
                }],
            }
            expression_artifact_bytes = json.dumps(expression_artifact).encode()
            self.assertTrue(generate.route_witness_artifact_matches(
                expression_artifact_bytes,
                expression_mapping,
                expression_row,
                "normal",
                expression_row["ordinal"],
                expression_source,
            ))
            forced_expression_artifact = copy.deepcopy(expression_artifact)
            forced_expression_artifact["records"][0]["mode"] = "forced"
            self.assertTrue(generate.route_witness_artifact_matches(
                json.dumps(forced_expression_artifact).encode(),
                expression_mapping,
                expression_row,
                "forced",
                expression_row["ordinal"],
                expression_source,
            ))
            for field, wrong in (
                ("test_id", "tests/xsh/expression-route-oracles.xsh::test_expr_other"),
                ("source_hash", "0" * 64),
            ):
                changed = {**expression_artifact, field: wrong}
                self.assertFalse(generate.route_witness_artifact_matches(
                    json.dumps(changed).encode(),
                    expression_mapping,
                    expression_row,
                    "normal",
                    expression_row["ordinal"],
                    expression_source,
                ))
            for field, wrong in (
                ("tag_name", "ExprIf"),
                ("tag_ordinal", expression_row["ordinal"] + 1),
                ("route", "in_place_evaluator_path"),
                ("mode", "forced"),
            ):
                changed = copy.deepcopy(expression_artifact)
                changed["records"][0][field] = wrong
                self.assertFalse(generate.route_witness_artifact_matches(
                    json.dumps(changed).encode(),
                    expression_mapping,
                    expression_row,
                    "normal",
                    expression_row["ordinal"],
                    expression_source,
                ))
            changed_expression_mapping = copy.deepcopy(expression_mapping)
            changed_expression_mapping["child_source"] += "assert false\n"
            changed_expression_source = generate.route_witness_source_bytes(
                root,
                changed_expression_mapping,
                config["route_witness_source_commit"],
            )
            self.assertFalse(generate.route_witness_artifact_matches(
                expression_artifact_bytes,
                changed_expression_mapping,
                expression_row,
                "normal",
                expression_row["ordinal"],
                changed_expression_source,
            ))
            changed_expression_mapping["test_id"] = (
                "tests/xsh/expression-route-oracles.xsh::test_expr_missing"
            )
            self.assertIsNone(generate.route_witness_source_bytes(
                root,
                changed_expression_mapping,
                config["route_witness_source_commit"],
            ))
            with mock.patch.object(
                generate.subprocess,
                "run",
                return_value=mock.Mock(stdout=b"stale reviewed source file"),
            ):
                self.assertIsNone(generate.route_witness_source_bytes(
                    root,
                    expression_mapping,
                    config["route_witness_source_commit"],
                ))

    def test_route_witness_receipts_credit_only_unique_exact_tuples(self) -> None:
        root = generate.HERE.parents[4]
        scratch = root / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch, prefix="route-witness-receipt-") as temporary:
            base = pathlib.Path(temporary)
            config, mapping, normal_path, normal_receipt, _ = self.route_witness_receipt_fixture(
                root, base / "normal", "normal"
            )
            rows = generate.parse_schema((root / generate.SCHEMA_PATH).read_text())
            mappings = generate.validate_route_witness_mappings(root, config, rows)
            collected = generate.collect_route_witness_receipts(
                root, config, mappings, rows, "head", "tree", "xsht-hash",
                [(normal_path, normal_receipt)],
            )
            normal_identity = (
                "ExprEmptyMap",
                "normal",
                "explicit_frame_apply_path",
                "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map",
            )
            self.assertEqual(set(collected), {normal_identity})

            forced_config, _, forced_path, forced_receipt, _ = self.route_witness_receipt_fixture(
                root, base / "forced", "forced"
            )
            forced = generate.collect_route_witness_receipts(
                root, forced_config, mappings, rows, "head", "tree", "xsht-hash",
                [(forced_path, forced_receipt)],
            )
            forced_identity = (
                "ExprEmptyMap",
                "forced",
                "generic_operand_apply_path",
                "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map",
            )
            self.assertEqual(set(forced), {forced_identity})
            self.assertEqual(forced[forced_identity]["route_id"], "generic_operand_apply_path")

            _, _, duplicate_path, duplicate_receipt, _ = self.route_witness_receipt_fixture(
                root, base / "normal-duplicate", "normal", extra_env={"WITNESS_DUPLICATE": "1"}
            )
            duplicate_claim = generate.collect_route_witness_receipts(
                root, config, mappings, rows, "head", "tree", "xsht-hash",
                [(normal_path, normal_receipt), (duplicate_path, duplicate_receipt)],
            )
            self.assertEqual(duplicate_claim, {})

            frame_id = "tests/xsh/expression-route-oracles.xsh::test_expr_if"
            pure_id = "tests/xsh/expression-route-oracles.xsh::test_expr_if_pure_selection"
            if_receipts = {}
            for test_id in (frame_id, pure_id):
                for mode in ("normal", "forced"):
                    fixture = self.route_witness_receipt_fixture(
                        root,
                        base / f"{test_id.rsplit('::', 1)[1]}-{mode}",
                        mode,
                        test_id=test_id,
                    )
                    if_receipts[(test_id, mode)] = fixture

            frame_normal = if_receipts[(frame_id, "normal")]
            pure_normal = if_receipts[(pure_id, "normal")]
            normal_if = generate.collect_route_witness_receipts(
                root,
                config,
                mappings,
                rows,
                "head",
                "tree",
                "xsht-hash",
                [
                    (frame_normal[2], frame_normal[3]),
                    (pure_normal[2], pure_normal[3]),
                ],
            )
            self.assertEqual(
                set(normal_if),
                {
                    ("ExprIf", "normal", "explicit_frame_control_path", frame_id),
                    ("ExprIf", "normal", "in_place_evaluator_path", pure_id),
                },
            )
            frame_forced = if_receipts[(frame_id, "forced")]
            pure_forced = if_receipts[(pure_id, "forced")]
            forced_if = generate.collect_route_witness_receipts(
                root,
                config,
                mappings,
                rows,
                "head",
                "tree",
                "xsht-hash",
                [
                    (frame_forced[2], frame_forced[3]),
                    (pure_forced[2], pure_forced[3]),
                ],
            )
            self.assertEqual(
                set(forced_if),
                {
                    ("ExprIf", "forced", "explicit_frame_control_path", frame_id),
                    ("ExprIf", "forced", "explicit_frame_control_path", pure_id),
                },
            )
            path_evidence = generate.build_route_path_evidence(
                [{
                    "tag": "ExprIf",
                    "route_witness_expectations": {
                        "normal": {
                            "probes": [
                                {
                                    "test_id": frame_id,
                                    "route_id": "explicit_frame_control_path",
                                },
                                {
                                    "test_id": pure_id,
                                    "route_id": "in_place_evaluator_path",
                                },
                            ],
                            "status": "credited",
                        },
                        "forced": {
                            "probes": [
                                {
                                    "test_id": frame_id,
                                    "route_id": "explicit_frame_control_path",
                                },
                                {
                                    "test_id": pure_id,
                                    "route_id": "explicit_frame_control_path",
                                },
                            ],
                            "status": "credited",
                        },
                    },
                    "oracles": {"runtime": []},
                    "reused_final_correctness_receipts": [],
                }],
                {**normal_if, **forced_if},
            )["ExprIf"]
            self.assertEqual(path_evidence["normal"]["status"], "route_witness_credited")
            self.assertEqual(
                set(path_evidence["normal"]["routes"]),
                {"explicit_frame_control_path", "in_place_evaluator_path"},
            )
            self.assertEqual(len(path_evidence["normal"]["witnesses"]), 2)
            self.assertEqual(path_evidence["forced"]["status"], "route_witness_credited")
            self.assertEqual(
                set(path_evidence["forced"]["routes"]),
                {"explicit_frame_control_path"},
            )
            self.assertEqual(len(path_evidence["forced"]["witnesses"]), 2)

    def test_route_witness_uses_current_emitter_mapping_and_preserves_uncovered_branch(self) -> None:
        gaps = [
            {"tag": "ExprEmptyMap", "path": "normal", "route": "normal", "expected_runtime_route_id": "explicit_frame_apply_path"},
            {"tag": "ExprEmptyMap", "path": "forced", "route": "forced", "expected_runtime_route_id": "generic_operand_apply_path"},
            {"tag": "ExprIf", "path": "normal", "route": "verifier_approved_in_place_recursive_selection", "expected_runtime_route_id": "in_place_evaluator_path"},
            {"tag": "ExprIf", "path": "normal", "route": "non_in_place_explicit_frame_selection", "expected_runtime_route_id": "explicit_frame_control_path"},
            {"tag": "ExprInt", "path": "normal", "route": "normal", "expected_runtime_route_id": "in_place_evaluator_path"},
            {"tag": "ExprEmptyMap", "path": "fallback_branch_inventory"},
        ]
        rows = {
            "ExprEmptyMap": {
                "execution_paths": {
                    "normal": [{"name": "normal", "route": "recursive_bridge_to_explicit_frames"}],
                    "forced": [{"name": "forced", "route": "generic_operand_apply_path"}],
                },
            },
            "ExprIf": {
                "execution_paths": {
                    "normal": [
                        {
                            "name": "verifier_approved_in_place_recursive_selection",
                            "selected_when": "execution.is_in_place(instruction)",
                        },
                        {
                            "name": "non_in_place_explicit_frame_selection",
                            "selected_when": "execution.is_in_place(instruction) is false",
                        },
                    ],
                },
                "route_witness_expectations": {
                    "normal": {
                        "route_ids": ["in_place_evaluator_path", "explicit_frame_control_path"],
                    },
                },
            },
            "ExprInt": {
                "execution_paths": {"normal": [{"name": "normal", "route": "typed_recursive_scalar"}]},
            },
        }
        evidence = {
            "ExprEmptyMap": {
                "normal": {
                    "status": "route_witness_credited",
                    "routes": {"explicit_frame_apply_path": {"status": "credited"}},
                },
                "forced": {
                    "status": "route_witness_credited",
                    "routes": {"generic_operand_apply_path": {"status": "credited"}},
                },
            },
            "ExprIf": {
                "normal": {
                    "status": "pending",
                    "routes": {"explicit_frame_control_path": {"status": "credited"}},
                },
            },
            "ExprInt": {
                "normal": {"status": "behavior_only_receipt", "receipts": [{"mode": "normal"}]},
            },
        }
        self.assertEqual(
            generate.retain_unwitnessed_route_gaps(gaps, evidence, rows),
            [gaps[2], gaps[5]],
        )

    def test_route_witness_path_mapping_uses_selector_and_schema_strategy(self) -> None:
        row = {"frame_strategy": "FrameControl"}
        expectation = {
            "probes": [
                {"route_id": "explicit_frame_control_path"},
                {"route_id": "in_place_evaluator_path"},
            ],
        }
        self.assertEqual(
            generate.route_witness_route_id_for_path(
                row,
                "normal",
                {"selected_when": "execution.is_in_place(instruction)"},
                expectation,
            ),
            "in_place_evaluator_path",
        )
        self.assertEqual(
            generate.route_witness_route_id_for_path(
                row,
                "normal",
                {"selected_when": "execution.is_in_place(instruction) is false"},
                expectation,
            ),
            "explicit_frame_control_path",
        )
        self.assertEqual(
            generate.route_witness_route_id_for_path(
                {"frame_strategy": "FrameApply"},
                "forced",
                {},
                {"probes": [{"route_id": "generic_operand_apply_path"}]},
            ),
            "generic_operand_apply_path",
        )

    def test_receipt_reuse_requires_exact_source_id_command_and_mode(self) -> None:
        oracle_id = "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map"
        receipt = {
            "source_expected": {"commit": "head", "tree": "tree"},
            "hashes": {
                "binaries_before_tests": {"xsht-verification": "xsht-hash"},
                "binaries_after_tests": {"xsht-verification": "xsht-hash"},
            },
            "source_checks": [
                {"where": "before:empty-map", "head": "head", "tree": "tree", "clean": True},
                {"where": "after:empty-map", "head": "head", "tree": "tree", "clean": True},
            ],
        }
        normal = {
            "id": "empty-map",
            "exit_code": 0,
            "argv": ["bash", "run-native-final.sh", "run", "target/x86_64-unknown-linux-musl/verification/xsht", "test", "--exact", oracle_id],
            "env": {},
            "log_sha256": "a" * 64,
        }
        forced = {
            "id": "empty-map",
            "exit_code": 0,
            "argv": ["bash", "run-native-final.sh", "run", "target/x86_64-unknown-linux-musl/verification/xsht", "test", "--exact", oracle_id],
            "env": {"XSH_INTERNAL_TEST_FORCE_FRAMES": "1"},
            "log_sha256": "b" * 64,
        }
        self.assertTrue(
            generate.receipt_step_matches(
                receipt,
                normal,
                expected_head="head",
                expected_tree="tree",
                oracle_id=oracle_id,
                mode="normal",
                expected_xsht_sha256="xsht-hash",
            )
        )
        trailing_option = {**normal, "argv": [*normal["argv"], "--list"]}
        self.assertFalse(
            generate.receipt_step_matches(
                receipt,
                trailing_option,
                expected_head="head",
                expected_tree="tree",
                oracle_id=oracle_id,
                mode="normal",
                expected_xsht_sha256="xsht-hash",
            )
        )
        duplicate_source_check = {
            **receipt,
            "source_checks": [*receipt["source_checks"], dict(receipt["source_checks"][0])],
        }
        self.assertFalse(
            generate.receipt_step_matches(
                duplicate_source_check,
                normal,
                expected_head="head",
                expected_tree="tree",
                oracle_id=oracle_id,
                mode="normal",
                expected_xsht_sha256="xsht-hash",
            )
        )
        malformed_source_check = {
            **receipt,
            "source_checks": [
                *receipt["source_checks"],
                {"where": None, "head": "head", "tree": "tree", "clean": True},
            ],
        }
        self.assertFalse(
            generate.receipt_step_matches(
                malformed_source_check,
                normal,
                expected_head="head",
                expected_tree="tree",
                oracle_id=oracle_id,
                mode="normal",
                expected_xsht_sha256="xsht-hash",
            )
        )
        self.assertTrue(
            generate.receipt_step_matches(
                receipt,
                forced,
                expected_head="head",
                expected_tree="tree",
                oracle_id=oracle_id,
                mode="forced",
                expected_xsht_sha256="xsht-hash",
            )
        )
        self.assertFalse(
            generate.receipt_step_matches(
                receipt,
                normal,
                expected_head="other-head",
                expected_tree="tree",
                oracle_id=oracle_id,
                mode="normal",
                expected_xsht_sha256="xsht-hash",
            )
        )
        self.assertFalse(
            generate.receipt_step_matches(
                receipt,
                normal,
                expected_head="head",
                expected_tree="tree",
                oracle_id="tests/xsh/frame-apply-oracles.xsh::test_frame_apply_path_from_value",
                mode="normal",
                expected_xsht_sha256="xsht-hash",
            )
        )
        self.assertFalse(
            generate.receipt_step_matches(
                receipt,
                forced,
                expected_head="head",
                expected_tree="tree",
                oracle_id=oracle_id,
                mode="normal",
                expected_xsht_sha256="xsht-hash",
            )
        )
        self.assertFalse(
            generate.receipt_step_matches(
                receipt,
                normal,
                expected_head="head",
                expected_tree="tree",
                oracle_id=oracle_id,
                mode="normal",
                expected_xsht_sha256="different-xsht-hash",
            )
        )

    def test_source_order_receipts_require_both_modes_exact_ids_and_logs(self) -> None:
        root = generate.HERE.parents[4]
        scratch = root / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch, prefix="source-order-receipt-") as temporary:
            run_dir = pathlib.Path(temporary)
            oracle_id = "tests/xsh/operand-order-scalars.xsh::test_operand_order_int_binary_is_left_to_right"
            log_paths = {}
            steps = []
            source_checks = []
            for mode in ("normal", "forced"):
                step_id = f"int-binary-{mode}"
                log_path = run_dir / f"{step_id}.log"
                log_path.write_text("native test passed\n")
                log_paths[mode] = log_path
                step = {
                    "id": step_id,
                    "exit_code": 0,
                    "argv": [
                        "bash", "run-native-final.sh", "run",
                        "target/x86_64-unknown-linux-musl/verification/xsht",
                        "test", "--exact", oracle_id,
                    ],
                    "env": {"XSH_INTERNAL_TEST_FORCE_FRAMES": "1"} if mode == "forced" else {},
                    "log": str(log_path.relative_to(root)),
                    "log_sha256": generate.sha256_file(log_path),
                }
                steps.append(step)
                source_checks.extend([
                    {"where": f"before:{step_id}", "head": "head", "tree": "tree", "clean": True},
                    {"where": f"after:{step_id}", "head": "head", "tree": "tree", "clean": True},
                ])
            receipt = {
                "source_expected": {"commit": "head", "tree": "tree"},
                "hashes": {
                    "binaries_before_tests": {"xsht-verification": "xsht-hash"},
                    "binaries_after_tests": {"xsht-verification": "xsht-hash"},
                },
                "source_checks": source_checks,
                "steps": steps,
            }
            row = {
                "tag": "IntBinary",
                "source_order_evidence": {"oracle_ids": [oracle_id]},
            }
            collected = generate.collect_source_order_receipts(
                root, [row], "head", "tree", "xsht-hash", [(run_dir / "receipt.json", receipt)]
            )
            self.assertEqual(collected["IntBinary"]["status"], "credited")
            self.assertEqual(
                {execution["mode"] for execution in collected["IntBinary"]["executions"]},
                {"normal", "forced"},
            )

            missing_forced = {**receipt, "steps": steps[:1]}
            pending = generate.collect_source_order_receipts(
                root, [row], "head", "tree", "xsht-hash", [(run_dir / "receipt.json", missing_forced)]
            )
            self.assertEqual(pending["IntBinary"]["status"], "pending")

            log_paths["normal"].write_text("changed after receipt\n")
            changed_log = generate.collect_source_order_receipts(
                root, [row], "head", "tree", "xsht-hash", [(run_dir / "receipt.json", receipt)]
            )
            self.assertEqual(changed_log["IntBinary"]["status"], "pending")

    def test_receipts_reject_duplicate_steps_and_unverified_binaries(self) -> None:
        step = {"id": "same", "argv": ["xsht", "test"], "env": {}}
        self.assertTrue(generate.receipt_steps_are_unique([step]))
        self.assertFalse(generate.receipt_steps_are_unique([step, dict(step)]))
        self.assertFalse(generate.receipt_steps_are_unique([step, {**step, "id": "other"}]))
        self.assertTrue(generate.receipt_corpus_steps_are_unique([{"steps": [step]}]))
        self.assertFalse(generate.receipt_corpus_steps_are_unique([
            {"steps": [step]},
            {"steps": [dict(step)]},
        ]))
        self.assertFalse(generate.receipt_corpus_steps_are_unique([
            {"steps": [step]},
            {"steps": [{**step, "id": "different"}]},
        ]))

    def test_explicit_empty_defensive_path_inventory_has_no_gaps(self) -> None:
        self.assertEqual(generate.defensive_path_gaps([]), [])

    def test_unmapped_defensive_path_cannot_disappear_from_gap_report(self) -> None:
        path = {
            "tag": "ExprPipeline",
            "path": "absent_or_truncated_lowered_pipeline_input",
            "modes": ["normal", "forced"],
            "reason": "no corruption oracle",
            "source": ["serial_pipeline.rs:262"],
            "structural_note": "the verifier prevents reaching this state",
            "route_behaviors": {
                "normal": {"result_message": "frame-scheduler normal rejection"},
                "forced": {"result_message": "frame strategy error"},
            },
        }
        gaps = generate.defensive_path_gaps([path])
        self.assertEqual([gap["mode"] for gap in gaps], ["normal", "forced"])
        self.assertEqual(gaps[0]["route_behavior"]["result_message"], "frame-scheduler normal rejection")
        self.assertEqual(gaps[1]["route_behavior"]["result_message"], "frame strategy error")

    def test_defensive_path_inventory_requires_explicit_list(self) -> None:
        with self.assertRaisesRegex(generate.EvidenceError, "must be explicitly configured"):
            generate.defensive_path_inventory({})
        with self.assertRaisesRegex(generate.EvidenceError, "must be a list"):
            generate.defensive_path_inventory({"unmapped_paths": None})

    def test_stale_source_lock_is_reported_without_resealing(self) -> None:
        actual = {
            "head": "new-head",
            "tree": "new-tree",
            "files": {"old.rs": "new-hash", "decisive.rs": "decisive-hash"},
        }
        lock = {
            "head": "old-head",
            "tree": "old-tree",
            "files": {"old.rs": "old-hash"},
        }
        status = generate.source_lock_status(actual, lock)
        self.assertEqual(status["status"], "stale")
        self.assertEqual(status["file_mismatches"], ["old.rs"])
        self.assertEqual(status["unsealed_files"], ["decisive.rs"])

    def test_unsealed_decisive_source_keeps_source_lock_stale(self) -> None:
        actual = {
            "head": "same-head",
            "tree": "same-tree",
            "files": {"locked.rs": "same-hash", "decisive.rs": "decisive-hash"},
        }
        lock = {
            "head": "same-head",
            "tree": "same-tree",
            "files": {"locked.rs": "same-hash"},
        }
        status = generate.source_lock_status(actual, lock)
        self.assertEqual(status["unsealed_files"], ["decisive.rs"])
        self.assertEqual(status["status"], "stale")

    def test_runtime_error_classification_is_closed_and_source_referenced(self) -> None:
        expected_tags = {
            *generate.TYPED_SCALAR_SEMANTICS,
            *generate.EAGER_ATOM_RUNTIME_ERRORS,
            *generate.FRAME_CONSTRUCTOR_ROWS,
            *generate.COLLECTION_CONSTRUCTION_ROWS,
            *generate.RESULT_CONSTRUCTOR_ROWS,
            *generate.CALL_FRAME_ROWS,
            *generate.FIELD_INDEX_ROWS,
        }
        self.assertEqual(len(expected_tags), 47)
        self.assertEqual(set(generate.RUNTIME_ERROR_CLASSIFICATIONS), expected_tags)
        self.assertEqual(set(generate.RUNTIME_ERROR_SOURCE_FUNCTIONS), expected_tags)
        for tag, outcomes in generate.RUNTIME_ERROR_CLASSIFICATIONS.items():
            self.assertTrue(outcomes, tag)
            self.assertEqual(len(outcomes), len(set(outcomes)), tag)
        self.assertIn("NUL path validation", generate.RUNTIME_ERROR_CLASSIFICATIONS["ExprPath"][-1])
        self.assertIn("out-of-bounds slot", generate.RUNTIME_ERROR_CLASSIFICATIONS["ExprParam"][1])
        self.assertIn("non-list splice", generate.RUNTIME_ERROR_CLASSIFICATIONS["ExprListBuild"][-1])
        self.assertIn("missing-field", generate.RUNTIME_ERROR_CLASSIFICATIONS["ExprRecordUpdate"][-1])
        self.assertIn("map-missing", generate.RUNTIME_ERROR_CLASSIFICATIONS["ExprIndex"][-1])
        self.assertIn("nul-path", generate.RUNTIME_ERROR_CLASSIFICATIONS["ExprPathFrom"][-1])

        root = generate.HERE.parents[4]
        config = json.loads(generate.CONFIG.read_text())
        sealed_candidates = set(config["source_paths"] + config["decisive_sources"])
        for tag, functions in generate.RUNTIME_ERROR_SOURCE_FUNCTIONS.items():
            self.assertTrue(functions, tag)
            for relative_path, function_name in functions:
                self.assertIn(relative_path, sealed_candidates)
                self.assertRegex(
                    generate.rust_function_source_ref(root, relative_path, function_name),
                    rf"^{re.escape(relative_path)}:[1-9][0-9]*$",
                )

    def test_runtime_error_classification_rejects_missing_or_false_evidence(self) -> None:
        tag = "IntBinary"
        base = generate.runtime_error_boundary_inventory(
            tag,
            {
                "source_refs": {
                    "normal_dispatch": ["indexed_run.rs:1"],
                    "forced_dispatch": ["explicit_run.rs:2"],
                }
            },
            root=None,
        )
        self.assertEqual(base["status"], "source_exhaustive_boundary_inventory")
        self.assertEqual(base["runtime_receipts"], "pending")
        self.assertEqual(
            [item["outcome"] for item in base["classifications"]],
            generate.RUNTIME_ERROR_CLASSIFICATIONS[tag],
        )

        missing = copy.deepcopy(base)
        missing["classifications"].pop()
        with self.assertRaisesRegex(generate.EvidenceError, "classifications differ"):
            generate.validate_runtime_error_boundary_inventory(tag, missing)

        changed = copy.deepcopy(base)
        changed["classifications"][0]["outcome"] = "all errors are impossible"
        with self.assertRaisesRegex(generate.EvidenceError, "classifications differ"):
            generate.validate_runtime_error_boundary_inventory(tag, changed)

        no_source = copy.deepcopy(base)
        no_source["classifications"][0]["source_refs"] = []
        with self.assertRaisesRegex(generate.EvidenceError, "incomplete source refs"):
            generate.validate_runtime_error_boundary_inventory(tag, no_source)

        false_receipt = copy.deepcopy(base)
        false_receipt["runtime_receipts"] = "passed"
        with self.assertRaisesRegex(generate.EvidenceError, "incorrectly claimed"):
            generate.validate_runtime_error_boundary_inventory(tag, false_receipt)

        with self.assertRaisesRegex(generate.EvidenceError, "unaudited row"):
            generate.validate_runtime_error_boundary_inventory("ExprUnknown", base)


if __name__ == "__main__":
    unittest.main()
