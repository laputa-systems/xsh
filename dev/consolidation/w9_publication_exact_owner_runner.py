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
import time
from collections.abc import Mapping

import w9_publication_adapter as adapter
import w9_publication_adapter_runner as adapter_runner


EXACT_OWNER_TEST = "sema::w9_exact_repository_owners_receipt"
EXACT_OWNER_HASH_LIBTEST_PREFIX = f"test {EXACT_OWNER_TEST} ... "
EXACT_OWNER_HELPER = pathlib.Path(__file__).with_name("w9-publication-exact-owners.rs")
EXACT_OWNER_HELPER_SHA256 = "33c77273d9f9096d3e6478622d0b29a3f0f1f79dee772cdea12b06f2ac8200d8"
EXACT_PROOF_SCHEMA = "w9-exact-owner-proof\t1\n"
SOURCE_EXACT_PROOF_SCHEMA = "w9-exact-owner-proof\t2\n"
EXACT_PROOF_FIELDS = (
    "schema",
    "role",
    "source_root",
    "source_commit",
    "source_tree",
    "build_receipt",
    "build_receipt_sha256",
    "binary",
    "binary_sha256",
    "helper_sha256",
    "exact_helper_path",
    "exact_helper_sha256",
    "owner_helper_sha256",
    "input_root",
    "input_manifest",
    "input_manifest_sha256",
    "corpus_blake3",
    "legacy_capture_receipt",
    "legacy_capture_receipt_sha256",
    "capture_receipt",
    "capture_receipt_sha256",
    "capture_root",
    "raw_root",
    "receipt_directory",
    "range_count",
    "owner_count",
    "ledger_sha256",
    "owner_hashes_sha256",
)
EXACT_RECEIPT_FIELDS = (
    "corpus",
    "shard",
    "range",
    "shard_size",
    "input_digest",
)
EXACT_CAPTURE_FIELDS = (
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
    "exact_helper_sha256",
    "input_root",
    "input_manifest",
    "input_manifest_sha256",
    "corpus_blake3",
    "legacy_capture_receipt",
    "legacy_capture_receipt_sha256",
    "capture_root",
    "raw_root",
    "receipt_directory",
    "range_count",
    "owner_count",
    "ledger_sha256",
)
SOURCE_EXACT_CAPTURE_FIELDS = (
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
    "helper_sha256",
    "exact_helper_sha256",
    "input_root",
    "input_manifest",
    "input_manifest_sha256",
    "corpus_blake3",
    "adapter_capture_receipt",
    "adapter_capture_receipt_sha256",
    "capture_root",
    "raw_root",
    "receipt_directory",
    "range_count",
    "owner_count",
    "ledger_sha256",
)
SOURCE_EXACT_PROOF_FIELDS = (
    "schema",
    "role",
    "source_receipt",
    "source_receipt_sha256",
    "source_root",
    "source_commit",
    "source_tree",
    "build_receipt",
    "build_receipt_sha256",
    "binary",
    "binary_sha256",
    "helper_sha256",
    "exact_helper_path",
    "exact_helper_sha256",
    "owner_helper_sha256",
    "input_root",
    "input_manifest",
    "input_manifest_sha256",
    "corpus_blake3",
    "adapter_capture_receipt",
    "adapter_capture_receipt_sha256",
    "capture_receipt",
    "capture_receipt_sha256",
    "capture_root",
    "raw_root",
    "receipt_directory",
    "range_count",
    "owner_count",
    "ledger_sha256",
    "owner_hashes_sha256",
)
EXPR_ID = re.compile(r"ExprId\(([1-9][0-9]*)\)\Z")
RUN_FORM_ID = re.compile(r"RunFormId\(([1-9][0-9]*)\)\Z")
STMT_ID = re.compile(r"StmtId\(([1-9][0-9]*)\)\Z")
DECIMAL = re.compile(r"(0|[1-9][0-9]*)\Z")
HEX256 = re.compile(r"[0-9a-f]{64}\Z")
IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")
BINDING_NAME = re.compile(r"(?:[A-Za-z_][A-Za-z0-9_]*|%(?:dest|delay|cap))\Z")
STREAM_STAGE_TAGS = frozenset(
    {
        "Where",
        "Map",
        "ParMap",
        "Each",
        "Batch",
        "Sort",
        "SortBy",
        "Take",
        "Drop",
        "First",
        "Last",
        "UniqueBy",
        "Enumerate",
        "Zip",
        "Range",
        "Repeat",
        "Tee",
        "Sum",
        "Min",
        "Max",
        "GroupBy",
        "Fold",
        "Reduce",
        "FlatMap",
        "Any",
        "All",
        "Shuffle",
        "TablePrint",
        "TextStreamLines",
        "BytesChunks",
        "JsonLines",
        "JsonStream",
        "Count",
        "Collect",
        "ReduceBy",
    }
)
LEGACY_OWNER_ROWS = {"source", "stmt", "tail", "owner"}
EXACT_OWNER_ROWS = {
    "binding-initializer",
    "binding-initializer-count",
    "exact-counts",
    "expr",
    "run",
    "run-form-count",
    "record",
    "record-field",
    "record-shorthand-alias",
    "call",
    "field-access",
    "structured-stage-count",
    "structured-stage-callback-count",
    "structured-stage",
    "structured-stage-arg",
    "structured-stage-callback",
}


class ExactOwnerError(ValueError):
    pass


def _decimal(value: str, label: str) -> int:
    match = DECIMAL.fullmatch(value)
    if match is None:
        raise ExactOwnerError(f"malformed {label}")
    return int(match.group(1))


def _expr_id(value: str, label: str) -> int:
    match = EXPR_ID.fullmatch(value)
    if match is None:
        raise ExactOwnerError(f"malformed {label}")
    return int(match.group(1)) - 1


def _run_id(value: str, label: str) -> int:
    match = RUN_FORM_ID.fullmatch(value)
    if match is None:
        raise ExactOwnerError(f"malformed {label}")
    return int(match.group(1)) - 1


def _stmt_id(value: str) -> int:
    match = STMT_ID.fullmatch(value)
    if match is None:
        raise ExactOwnerError("malformed statement id")
    return int(match.group(1)) - 1


def _span(value: str, source_sizes: Mapping[int, int], label: str) -> str:
    try:
        canonical, source_id, start, length = adapter._canonicalize_span(value)
    except adapter.AdapterError as error:
        raise ExactOwnerError(f"{label}: malformed Span: {error}") from None
    if source_id not in source_sizes or start + length > source_sizes[source_id]:
        raise ExactOwnerError(f"{label}: Span is outside its source")
    return canonical


def _decode_rust_string(value: str, label: str) -> str:
    if len(value) < 2 or value[0] != '"' or value[-1] != '"':
        raise ExactOwnerError(f"{label}: malformed Rust Debug string")
    decoded: list[str] = []
    index = 1
    limit = len(value) - 1
    escapes = {"n": "\n", "r": "\r", "t": "\t", "0": "\0", "\\": "\\", '"': '"'}
    while index < limit:
        character = value[index]
        if character == "\\":
            index += 1
            if index >= limit:
                raise ExactOwnerError(f"{label}: incomplete Rust Debug escape")
            escape = value[index]
            if escape in escapes:
                decoded.append(escapes[escape])
                index += 1
                continue
            if escape != "u" or index + 1 >= limit or value[index + 1] != "{":
                raise ExactOwnerError(f"{label}: unsupported Rust Debug escape")
            close = value.find("}", index + 2, limit)
            digits = value[index + 2 : close] if close >= 0 else ""
            if not 1 <= len(digits) <= 6 or any(
                digit not in "0123456789abcdefABCDEF" for digit in digits
            ):
                raise ExactOwnerError(f"{label}: malformed Rust Unicode escape")
            codepoint = int(digits, 16)
            if codepoint > 0x10FFFF or 0xD800 <= codepoint <= 0xDFFF:
                raise ExactOwnerError(f"{label}: invalid Rust Unicode escape")
            decoded.append(chr(codepoint))
            index = close + 1
            continue
        if character == '"' or not character.isprintable():
            raise ExactOwnerError(f"{label}: unescaped character in Rust Debug string")
        decoded.append(character)
        index += 1
    return "".join(decoded)


def _decode_rust_string_vector(value: str, label: str) -> tuple[str, ...]:
    if not value.startswith("["):
        raise ExactOwnerError(f"{label}: malformed Rust Debug string vector")
    if value == "[]":
        return ()
    values: list[str] = []
    index = 1
    while index < len(value):
        if index >= len(value) or value[index] != '"':
            raise ExactOwnerError(f"{label}: malformed Rust Debug string vector")
        start = index
        index += 1
        closed = False
        while index < len(value):
            if value[index] == "\\":
                if index + 1 >= len(value):
                    break
                index += 2
                continue
            if value[index] == '"':
                index += 1
                closed = True
                break
            index += 1
        if not closed:
            raise ExactOwnerError(f"{label}: unterminated Rust Debug string vector item")
        values.append(_decode_rust_string(value[start:index], label))
        if index == len(value) - 1 and value[index] == "]":
            return tuple(values)
        if not value.startswith(", ", index):
            raise ExactOwnerError(f"{label}: malformed Rust Debug string vector separator")
        index += 2
    raise ExactOwnerError(f"{label}: unterminated Rust Debug string vector")


def _field_name(kind: str, value: str, label: str) -> str | tuple[str, ...]:
    if kind == "path":
        components = _decode_rust_string_vector(value, label)
        if not components:
            raise ExactOwnerError(f"{label}: malformed field path")
        return components
    decoded = _decode_rust_string(value, label)
    if kind in {"computed", "spread"}:
        if decoded:
            raise ExactOwnerError(f"{label}: {kind} field has an unexpected name")
    return decoded


def parse_exact_owner_index(
    contents: str,
) -> tuple[
    tuple[
        dict[int, int],
        dict[str, set[str]],
        dict[str, tuple[str, str, str]],
        dict[str, tuple[str, str]],
    ],
    dict[str, object],
]:
    legacy_lines: list[str] = []
    run_owner_spans: list[str] = []
    exact_count: tuple[int, int] | None = None
    binding_initializer_count: int | None = None
    binding_initializers: dict[tuple[int, str], int] = {}
    binding_initializer_statements: set[int] = set()
    structured_stage_count: int | None = None
    structured_stage_callback_count: int | None = None
    run_form_count: int | None = None
    expressions: dict[int, tuple[str, str, str]] = {}
    runs: dict[int, tuple[str, bool, str]] = {}
    record_headers: dict[int, tuple[str, int]] = {}
    record_fields: dict[
        int,
        dict[int, tuple[str, str | tuple[str, ...], str, int | None, int | None]],
    ] = {}
    aliases: dict[tuple[int, int], tuple[int, str, str]] = {}
    calls: dict[int, int] = {}
    field_accesses: dict[int, tuple[int, str]] = {}
    structured_stages: dict[int, tuple[str, str, bool, bool, int]] = {}
    structured_stage_arguments: dict[int, dict[int, tuple[str, str | None, int]]] = {}
    structured_stage_callbacks: dict[int, int] = {}
    for line_number, line in enumerate(contents.splitlines(), 1):
        fields = line.split("\t")
        if fields[0] in {"source", "stmt", "tail"}:
            legacy_lines.append(line)
        elif fields[0] == "owner" and len(fields) == 3 and fields[2] == "run:RunForm":
            run_owner_spans.append(fields[1])
        elif fields[0] == "owner":
            legacy_lines.append(line)
        elif fields[0] not in EXACT_OWNER_ROWS:
            raise ExactOwnerError(f"owner index line {line_number}: unrecognized row {fields[0]!r}")
        elif fields[0] == "exact-counts":
            if len(fields) != 3 or exact_count is not None:
                raise ExactOwnerError(f"owner index line {line_number}: malformed or duplicate exact-counts row")
            exact_count = (
                _decimal(fields[1], "expression count"),
                _decimal(fields[2], "record field count"),
            )
        elif fields[0] == "structured-stage-count":
            if len(fields) != 2 or structured_stage_count is not None:
                raise ExactOwnerError(
                    f"owner index line {line_number}: malformed or duplicate structured-stage-count row"
                )
            structured_stage_count = _decimal(fields[1], "structured-stage count")
        elif fields[0] == "structured-stage-callback-count":
            if len(fields) != 2 or structured_stage_callback_count is not None:
                raise ExactOwnerError(
                    f"owner index line {line_number}: malformed or duplicate structured-stage-callback-count row"
                )
            structured_stage_callback_count = _decimal(fields[1], "structured-stage callback count")
        elif fields[0] == "run-form-count":
            if len(fields) != 2 or run_form_count is not None:
                raise ExactOwnerError(
                    f"owner index line {line_number}: malformed or duplicate run-form-count row"
                )
            run_form_count = _decimal(fields[1], "run form count")
        elif fields[0] == "binding-initializer-count":
            if len(fields) != 2 or binding_initializer_count is not None:
                raise ExactOwnerError(
                    f"owner index line {line_number}: malformed or duplicate binding-initializer-count row"
                )
            binding_initializer_count = _decimal(fields[1], "binding initializer count")
        elif fields[0] == "binding-initializer":
            if len(fields) != 4:
                raise ExactOwnerError(f"owner index line {line_number}: malformed binding-initializer row")
            statement_id = _stmt_id(fields[1])
            name = _decode_rust_string(fields[2], f"owner index line {line_number} binding name")
            if BINDING_NAME.fullmatch(name) is None:
                raise ExactOwnerError(f"owner index line {line_number}: malformed binding initializer name")
            initializer_id = _expr_id(fields[3], "binding initializer expression id")
            key = (statement_id, name)
            if key in binding_initializers or statement_id in binding_initializer_statements:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate binding initializer")
            binding_initializers[key] = initializer_id
            binding_initializer_statements.add(statement_id)
        elif fields[0] == "structured-stage":
            if len(fields) != 7:
                raise ExactOwnerError(f"owner index line {line_number}: malformed structured-stage row")
            stage_index = _decimal(fields[1], "structured-stage index")
            if stage_index in structured_stages:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate structured-stage index")
            tag = fields[3]
            if tag not in STREAM_STAGE_TAGS:
                raise ExactOwnerError(f"owner index line {line_number}: unknown structured-stage kind")
            if fields[4] not in {"true", "false"} or fields[5] not in {"true", "false"}:
                raise ExactOwnerError(f"owner index line {line_number}: malformed structured-stage flag")
            structured_stages[stage_index] = (
                fields[2], tag, fields[4] == "true", fields[5] == "true",
                _decimal(fields[6], "structured-stage argument count"),
            )
        elif fields[0] == "structured-stage-arg":
            if len(fields) != 6:
                raise ExactOwnerError(f"owner index line {line_number}: malformed structured-stage-arg row")
            stage_index = _decimal(fields[1], "structured-stage argument owner")
            argument_index = _decimal(fields[2], "structured-stage argument index")
            kind = fields[3]
            if kind not in {"Positional", "Named", "NamedSpread", "Splice"}:
                raise ExactOwnerError(f"owner index line {line_number}: unknown structured-stage argument kind")
            if kind == "Named":
                name = _decode_rust_string(fields[4], f"owner index line {line_number} argument name")
                if IDENTIFIER.fullmatch(name) is None:
                    raise ExactOwnerError(f"owner index line {line_number}: malformed structured-stage argument name")
            else:
                if fields[4] != "-":
                    raise ExactOwnerError(f"owner index line {line_number}: non-named argument has a name")
                name = None
            expression_id = _expr_id(fields[5], "structured-stage argument expression id")
            arguments = structured_stage_arguments.setdefault(stage_index, {})
            if argument_index in arguments:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate structured-stage argument index")
            arguments[argument_index] = (kind, name, expression_id)
        elif fields[0] == "structured-stage-callback":
            if len(fields) != 3:
                raise ExactOwnerError(f"owner index line {line_number}: malformed structured-stage-callback row")
            stage_index = _decimal(fields[1], "structured-stage callback owner")
            expression_id = _expr_id(fields[2], "structured-stage callback expression id")
            if stage_index in structured_stage_callbacks:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate structured-stage callback owner")
            structured_stage_callbacks[stage_index] = expression_id
        elif fields[0] == "expr":
            if len(fields) != 5:
                raise ExactOwnerError(f"owner index line {line_number}: malformed expr row")
            expression_id = _expr_id(fields[1], "expression id")
            if expression_id in expressions:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate expression id")
            tag, raw_name = fields[3], fields[4]
            if IDENTIFIER.fullmatch(tag) is None:
                raise ExactOwnerError(f"owner index line {line_number}: malformed expression tag")
            name = _decode_rust_string(raw_name, f"owner index line {line_number} expression name")
            if tag != "Ident" and name:
                raise ExactOwnerError(f"owner index line {line_number}: non-identifier expression has a spelling")
            expressions[expression_id] = (fields[2], tag, name)
        elif fields[0] == "run":
            if len(fields) != 5:
                raise ExactOwnerError(f"owner index line {line_number}: malformed run row")
            run_id = _run_id(fields[1], "run form id")
            if run_id in runs:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate run form id")
            if fields[3] not in {"true", "false"}:
                raise ExactOwnerError(f"owner index line {line_number}: malformed run propagation flag")
            if fields[4] not in adapter.RUN_FORM_KINDS:
                raise ExactOwnerError(f"owner index line {line_number}: unknown run form head kind")
            runs[run_id] = (fields[2], fields[3] == "true", fields[4])
        elif fields[0] == "record":
            if len(fields) != 4:
                raise ExactOwnerError(f"owner index line {line_number}: malformed record row")
            parent = _expr_id(fields[1], "record parent id")
            if parent in record_headers:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate record parent")
            record_headers[parent] = (fields[2], _decimal(fields[3], "record field count"))
            record_fields[parent] = {}
        elif fields[0] == "record-field":
            if len(fields) != 8:
                raise ExactOwnerError(f"owner index line {line_number}: malformed record-field row")
            parent = _expr_id(fields[1], "record field parent id")
            field_index = _decimal(fields[2], "record field index")
            kind = fields[3]
            if kind not in {"computed", "path", "named", "shorthand", "spread"}:
                raise ExactOwnerError(f"owner index line {line_number}: unknown record field kind")
            fields_by_index = record_fields.setdefault(parent, {})
            if field_index in fields_by_index or any(
                field_index in other for other_parent, other in record_fields.items() if other_parent != parent
            ):
                raise ExactOwnerError(f"owner index line {line_number}: duplicate global record field index")
            name = _field_name(kind, fields[4], f"owner index line {line_number}")
            key_expr = None if not fields[6] else _expr_id(fields[6], "record field key expression")
            value_expr = None if not fields[7] else _expr_id(fields[7], "record field value expression")
            fields_by_index[field_index] = (kind, name, fields[5], key_expr, value_expr)
        elif fields[0] == "record-shorthand-alias":
            if len(fields) != 6:
                raise ExactOwnerError(f"owner index line {line_number}: malformed record shorthand alias row")
            parent = _expr_id(fields[1], "record alias parent id")
            field_index = _decimal(fields[2], "record alias field index")
            alias_id = _expr_id(fields[3], "record alias expression id")
            name = _decode_rust_string(fields[5], f"owner index line {line_number} alias name")
            if (parent, field_index) in aliases:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate record shorthand alias")
            if IDENTIFIER.fullmatch(name) is None:
                raise ExactOwnerError(
                    f"owner index line {line_number}: malformed record shorthand alias name"
                )
            aliases[(parent, field_index)] = (alias_id, fields[4], name)
        elif fields[0] == "call":
            if len(fields) != 3:
                raise ExactOwnerError(f"owner index line {line_number}: malformed call row")
            expression_id = _expr_id(fields[1], "call expression id")
            callee_id = _expr_id(fields[2], "call callee expression id")
            if expression_id in calls:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate call expression id")
            calls[expression_id] = callee_id
        elif fields[0] == "field-access":
            if len(fields) != 4:
                raise ExactOwnerError(f"owner index line {line_number}: malformed field-access row")
            expression_id = _expr_id(fields[1], "field-access expression id")
            base_id = _expr_id(fields[2], "field-access base expression id")
            name = _decode_rust_string(fields[3], f"owner index line {line_number} field name")
            if expression_id in field_accesses:
                raise ExactOwnerError(f"owner index line {line_number}: duplicate field-access expression id")
            field_accesses[expression_id] = (base_id, name)

    try:
        legacy = adapter.parse_owner_index("\n".join(legacy_lines) + "\n")
    except adapter.AdapterError as error:
        raise ExactOwnerError(f"legacy owner rows are invalid: {error}") from None
    source_sizes = legacy[0]
    statement_owners = legacy[3]
    if exact_count is None:
        raise ExactOwnerError("exact owner index is missing its exact-counts row")
    if binding_initializer_count is None:
        raise ExactOwnerError("exact owner index is missing its binding-initializer-count row")
    if binding_initializer_count != len(binding_initializers):
        raise ExactOwnerError("binding initializer rows do not cover the declared table")
    expr_count, record_field_count = exact_count
    if structured_stage_count is None:
        raise ExactOwnerError("exact owner index is missing its structured-stage-count row")
    if structured_stage_count != len(structured_stages) or set(structured_stages) != set(
        range(len(structured_stages))
    ):
        raise ExactOwnerError("structured-stage rows do not cover the declared table")
    if structured_stage_callback_count is None:
        raise ExactOwnerError("exact owner index is missing its structured-stage-callback-count row")
    if structured_stage_callback_count != len(structured_stage_callbacks):
        raise ExactOwnerError("structured-stage callback rows do not cover the declared table")
    if set(structured_stage_arguments) - set(structured_stages):
        raise ExactOwnerError("structured-stage arguments reference an unknown stage")
    if set(structured_stage_callbacks) - set(structured_stages):
        raise ExactOwnerError("structured-stage callbacks reference an unknown stage")
    if any(stage_index not in structured_stage_arguments for stage_index in structured_stages):
        structured_stage_arguments.update(
            (stage_index, {})
            for stage_index in structured_stages
            if stage_index not in structured_stage_arguments
        )
    for stage_index, (_raw_span, _tag, _inline_block, _accepts_callable, argument_count) in structured_stages.items():
        arguments = structured_stage_arguments[stage_index]
        if len(arguments) != argument_count or set(arguments) != set(range(argument_count)):
            raise ExactOwnerError("structured-stage argument rows do not cover the declared argument table")
    if run_form_count is None:
        raise ExactOwnerError("exact owner index is missing its run-form-count row")
    if len(runs) != run_form_count:
        raise ExactOwnerError("run form rows do not cover the declared table")
    if set(runs) != set(range(run_form_count)):
        raise ExactOwnerError("exact run form ids are not contiguous")
    if set(expressions) != set(range(expr_count)):
        raise ExactOwnerError("exact expression ids do not cover the declared arena table")
    if set(expressions) != set(range(len(expressions))):
        raise ExactOwnerError("exact expression ids are not contiguous")

    normalized_stage_arguments: dict[int, tuple[tuple[int, str, str | None, int], ...]] = {}
    for stage_index, arguments in structured_stage_arguments.items():
        normalized_rows = []
        for argument_index in range(len(arguments)):
            kind, name, expression_id = arguments[argument_index]
            if expression_id not in expressions:
                raise ExactOwnerError(
                    f"structured-stage argument {stage_index}:{argument_index} references a missing expression"
                )
            normalized_rows.append((argument_index, kind, name, expression_id))
        normalized_stage_arguments[stage_index] = tuple(normalized_rows)

    for stage_index, expression_id in structured_stage_callbacks.items():
        raw_span, tag, inline_block, accepts_callable, _argument_count = structured_stages[stage_index]
        if inline_block:
            raise ExactOwnerError(f"structured-stage callback {stage_index} has an inline block")
        if not accepts_callable:
            raise ExactOwnerError(f"structured-stage callback {stage_index} is on a stage that does not accept callable")
        if expression_id not in expressions:
            raise ExactOwnerError(f"structured-stage callback {stage_index} references a missing expression")
        expression_tag = expressions[expression_id][1]
        if expression_tag not in {"Ident", "Field"}:
            raise ExactOwnerError(f"structured-stage callback {stage_index} is not a direct identifier or field")
        arguments = normalized_stage_arguments[stage_index]
        if any(kind in {"NamedSpread", "Splice"} for _index, kind, _name, _expr in arguments):
            raise ExactOwnerError(f"structured-stage callback {stage_index} has ambiguous spread arguments")
        named_blocks = [row for row in arguments if row[1] == "Named" and row[2] == "block"]
        positionals = [row for row in arguments if row[1] == "Positional"]
        if len(named_blocks) == 1 and all(
            index > named_blocks[0][0] for index, _kind, _name, _expression_id in positionals
        ):
            candidate_id = named_blocks[0][3]
        elif not named_blocks and positionals and positionals[0][0] == 0:
            candidate_id = positionals[0][3]
        else:
            raise ExactOwnerError(f"structured-stage callback {stage_index} does not identify its callback argument")
        if candidate_id != expression_id:
            raise ExactOwnerError(f"structured-stage callback {stage_index} does not match its argument graph")

    call_ids = {
        expression_id
        for expression_id, (_span_value, tag, _name) in expressions.items()
        if tag == "Call"
    }
    if set(calls) != call_ids:
        raise ExactOwnerError("call links do not cover the Call expression table")
    field_ids = {
        expression_id
        for expression_id, (_span_value, tag, _name) in expressions.items()
        if tag == "Field"
    }
    if set(field_accesses) != field_ids:
        raise ExactOwnerError("field-access links do not cover the Field expression table")
    for expression_id, callee_id in calls.items():
        if callee_id not in expressions or callee_id == expression_id:
            raise ExactOwnerError(f"call {expression_id} references an invalid callee expression")
    for expression_id, (base_id, _name) in field_accesses.items():
        if base_id not in expressions or base_id == expression_id:
            raise ExactOwnerError(f"field access {expression_id} references an invalid base expression")
    structured_stages = {
        stage_index: (
            _span(raw_span, source_sizes, f"structured stage {stage_index}"),
            tag,
            inline_block,
            accepts_callable,
        )
        for stage_index, (raw_span, tag, inline_block, accepts_callable, _argument_count) in structured_stages.items()
    }

    for expression_id, (raw_span, tag, _name) in expressions.items():
        span = _span(raw_span, source_sizes, f"expression {expression_id}")
        if f"expr:{tag}" not in legacy[1].get(span, set()):
            raise ExactOwnerError(f"expression {expression_id} has no matching generic AST owner")
        expressions[expression_id] = (span, tag, _name)
    validated_binding_initializers: dict[tuple[int, str], tuple[str, int]] = {}
    for (statement_id, name), initializer_id in binding_initializers.items():
        statement_key = f"StmtId({statement_id + 1})"
        statement = statement_owners.get(statement_key)
        if statement is None:
            raise ExactOwnerError(f"binding initializer references missing statement {statement_id}")
        statement_span, statement_kind = statement
        if statement_kind not in {
            "stmt:Const",
            "stmt:Let",
            "stmt:LetExprNoTy",
            "stmt:Var",
            "stmt:VarExprNoTy",
        }:
            raise ExactOwnerError(f"binding initializer {statement_id} belongs to a non-binding statement")
        if initializer_id not in expressions:
            raise ExactOwnerError(f"binding initializer {statement_id} references a missing expression")
        initializer_span = expressions[initializer_id][0]
        _, statement_source, _, _ = adapter._canonicalize_span(statement_span)
        _, initializer_source, _, _ = adapter._canonicalize_span(initializer_span)
        if statement_source != initializer_source:
            raise ExactOwnerError(f"binding initializer {statement_id} crosses source ownership")
        validated_binding_initializers[(statement_id, name)] = (statement_span, initializer_id)
    for run_id, (raw_span, propagate, head_kind) in runs.items():
        runs[run_id] = (_span(raw_span, source_sizes, f"run form {run_id}"), propagate, head_kind)
    normalized_run_owners = {
        _span(raw_span, source_sizes, "run AST owner") for raw_span in run_owner_spans
    }
    if len(normalized_run_owners) != len(run_owner_spans) or normalized_run_owners != {
        span for span, _propagate, _head_kind in runs.values()
    }:
        raise ExactOwnerError("run AST owners do not match exact run forms")

    record_tags = {expression_id for expression_id, (_span_value, tag, _name) in expressions.items() if tag == "Record"}
    if set(record_fields) - set(record_headers):
        raise ExactOwnerError("exact record field rows have no parent record row")
    if set(record_headers) != record_tags:
        raise ExactOwnerError("exact record rows do not match the Record expression table")
    normalized_records: dict[int, dict[str, object]] = {}
    all_field_indexes: set[int] = set()
    for parent, (raw_span, declared_count) in record_headers.items():
        span = _span(raw_span, source_sizes, f"record {parent}")
        expression_span = expressions[parent][0]
        if span != expression_span:
            raise ExactOwnerError(f"record {parent} span differs from its Record expression")
        fields_by_index = record_fields[parent]
        indexes = sorted(fields_by_index)
        if len(indexes) != declared_count:
            raise ExactOwnerError(f"record {parent} field count differs from its row count")
        if indexes and indexes != list(range(indexes[0], indexes[0] + declared_count)):
            raise ExactOwnerError(f"record {parent} field indexes are not contiguous")
        normalized_fields = {}
        for field_index, (kind, name, raw_field_span, key_expr, value_expr) in fields_by_index.items():
            if field_index in all_field_indexes:
                raise ExactOwnerError("duplicate global record field index")
            all_field_indexes.add(field_index)
            field_span = _span(raw_field_span, source_sizes, f"record field {parent}:{field_index}")
            for expression_ref in (key_expr, value_expr):
                if expression_ref is not None and expression_ref not in expressions:
                    raise ExactOwnerError(f"record field {parent}:{field_index} references a missing expression")
            if kind == "computed" and (not name == "" or key_expr is None or value_expr is None):
                raise ExactOwnerError(f"record field {parent}:{field_index} has malformed computed data")
            if kind == "path" and (not name or key_expr is not None or value_expr is None):
                raise ExactOwnerError(f"record field {parent}:{field_index} has malformed path data")
            if kind == "named" and (key_expr is not None or value_expr is None):
                raise ExactOwnerError(f"record field {parent}:{field_index} has malformed named data")
            if kind == "shorthand" and (not name or key_expr is not None or value_expr is not None):
                raise ExactOwnerError(f"record field {parent}:{field_index} has malformed shorthand data")
            if kind == "spread" and (name or key_expr is not None or value_expr is None):
                raise ExactOwnerError(f"record field {parent}:{field_index} has malformed spread data")
            normalized_fields[field_index] = (kind, name, field_span, key_expr, value_expr)
        normalized_records[parent] = {"span": span, "count": declared_count, "fields": normalized_fields}
    if any(field_index >= record_field_count for field_index in all_field_indexes):
        raise ExactOwnerError("exact record field id is outside the declared arena table")
    if all_field_indexes != set(range(record_field_count)):
        raise ExactOwnerError("exact record field ids do not cover the declared arena table")

    expected_aliases: dict[tuple[int, int], tuple[int, str, str]] = {}
    for parent, record in normalized_records.items():
        fields_by_index = record["fields"]
        for field_index, (kind, name, field_span, _key_expr, _value_expr) in fields_by_index.items():
            if kind != "shorthand":
                continue
            candidates = [
                expression_id
                for expression_id, (span, tag, spelling) in expressions.items()
                if tag == "Ident" and span == field_span and spelling == name
            ]
            if len(candidates) == 1:
                expected_aliases[(parent, field_index)] = (candidates[0], field_span, name)
    normalized_aliases: dict[tuple[int, int], tuple[int, str, str]] = {}
    for (parent, field_index), (alias_id, raw_span, name) in aliases.items():
        if parent not in normalized_records or field_index not in normalized_records[parent]["fields"]:
            raise ExactOwnerError("record shorthand alias references an unknown field")
        field = normalized_records[parent]["fields"][field_index]
        span = _span(raw_span, source_sizes, f"record shorthand alias {parent}:{field_index}")
        if field[0] != "shorthand" or field[1] != name or field[2] != span:
            raise ExactOwnerError("record shorthand alias disagrees with its shorthand field")
        if alias_id not in expressions or expressions[alias_id] != (span, "Ident", name):
            raise ExactOwnerError("record shorthand alias does not identify the exact matching expression")
        normalized_aliases[(parent, field_index)] = (alias_id, span, name)
    if normalized_aliases != expected_aliases:
        raise ExactOwnerError("record shorthand aliases do not match unique Ident expressions")

    exact_owners: dict[str, object] = {
        "exprs": expressions,
        "runs": runs,
        "records": normalized_records,
        "aliases": normalized_aliases,
        "expr_count": expr_count,
        "record_field_count": record_field_count,
        "calls": calls,
        "field_accesses": field_accesses,
        "structured_stages": structured_stages,
        "stage_arguments": normalized_stage_arguments,
        "stage_callbacks": structured_stage_callbacks,
        "binding_initializers": validated_binding_initializers,
        "tails": {
            _stmt_id(statement_id): (span, name, kind)
            for statement_id, (span, name, kind) in legacy[2].items()
        },
    }
    return legacy, exact_owners


def parse_exact_owner_hash_output(stdout: str, count: int) -> dict[int, str]:
    hashes: dict[int, str] = {}
    for line in stdout.splitlines():
        if line.startswith("w9exacthash"):
            row = line
        elif "w9exacthash" in line:
            if not line.startswith(EXACT_OWNER_HASH_LIBTEST_PREFIX):
                raise ExactOwnerError("native exact-owner hash output has an unknown prefix")
            row = line[len(EXACT_OWNER_HASH_LIBTEST_PREFIX) :]
        else:
            continue
        fields = row.split("\t")
        if len(fields) != 3 or fields[0] != "w9exacthash":
            raise ExactOwnerError("native exact-owner hash output has a malformed row")
        index = _decimal(fields[1], "native exact-owner hash index")
        if index >= count or not HEX256.fullmatch(fields[2]):
            raise ExactOwnerError("native exact-owner hash output contains an unexpected key or digest")
        if index in hashes:
            raise ExactOwnerError(f"native exact-owner hash output repeats input {index}")
        hashes[index] = fields[2]
    if set(hashes) != set(range(count)):
        raise ExactOwnerError(
            f"native exact-owner hash output coverage differs: missing={count - len(hashes)}"
        )
    return hashes


def _read_exact_owner_receipt(
    path: pathlib.Path,
    start: int,
    end: int,
    reference,
    runner,
) -> dict[int, str]:
    return parse_exact_owner_range_receipt(
        runner.read_regular_text(path), start, end, reference, runner.CORPUS_BLAKE3, path
    )


def parse_exact_owner_range_receipt(
    contents: str,
    start: int,
    end: int,
    reference,
    corpus_digest: str,
    path: pathlib.Path | None = None,
) -> dict[int, str]:
    label = str(path or "exact-owner range receipt")
    lines = contents.splitlines()
    inputs = reference.inputs if hasattr(reference, "inputs") else reference
    if len(lines) < len(EXACT_RECEIPT_FIELDS) + 1:
        raise ExactOwnerError(f"{label}: incomplete exact-owner range receipt")
    expected_shard_count = (len(inputs) + 99) // 100
    expected_header = {
        "corpus": str(len(inputs)),
        "shard": f"{start // 100}/{expected_shard_count}",
        "range": f"{start}..{end}",
        "shard_size": "100",
        "input_digest": corpus_digest,
    }
    for line, field in zip(lines, EXACT_RECEIPT_FIELDS):
        parts = line.split("=", 1)
        if len(parts) != 2 or parts[0] != field or parts[1] != expected_header[field]:
            raise ExactOwnerError(f"{label}: malformed or stale exact-owner {field}")
    rows = lines[len(EXACT_RECEIPT_FIELDS):-1]
    terminal = lines[-1].split("=", 1)
    if len(terminal) != 2 or terminal[0] != "complete" or terminal[1] != str(end - start):
        raise ExactOwnerError(f"{label}: exact-owner completion count differs")
    if len(rows) != end - start:
        raise ExactOwnerError(f"{label}: exact-owner row count differs")
    hashes: dict[int, str] = {}
    for row, index in zip(rows, range(start, end)):
        fields = row.split("\t")
        if len(fields) != 5 or fields[0] != "owner":
            raise ExactOwnerError(f"{label}: malformed exact-owner row for input {index}")
        if _decimal(fields[1], "exact-owner input index") != index:
            raise ExactOwnerError(f"{label}: exact-owner inputs are out of order")
        source = inputs[index]
        if fields[2] != source.relative_path or fields[3] != source.blake3:
            raise ExactOwnerError(f"{label}: exact-owner source differs at input {index}")
        if not HEX256.fullmatch(fields[4]):
            raise ExactOwnerError(f"{label}: malformed owner digest at input {index}")
        hashes[index] = fields[4]
    return hashes


def _exact_range_receipt_path(directory: pathlib.Path, start: int, end: int) -> pathlib.Path:
    return directory / f"range-{start}-{end}.tsv"


def _exact_capture_record(
    role: str,
    build_receipt: pathlib.Path,
    build: dict[str, str],
    adapter_capture_receipt: pathlib.Path,
    capture_root: pathlib.Path,
    raw_root: pathlib.Path,
    receipt_directory: pathlib.Path,
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    range_receipts: dict[tuple[int, int], pathlib.Path],
    owner_hashes: dict[int, str],
    exact_helper_sha256: str,
    runner,
) -> dict[str, str]:
    range_rows = [
        f"range\t{start}\t{end}\t{runner.sha256_file(path)}\n"
        for (start, end), path in sorted(range_receipts.items())
    ]
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
            "helper_sha256": build["helper_sha256"],
            "exact_helper_sha256": exact_helper_sha256,
            "input_root": str(input_root),
            "input_manifest": str(input_manifest),
            "input_manifest_sha256": build["input_manifest_sha256"],
            "corpus_blake3": build["corpus_blake3"],
            "adapter_capture_receipt": str(adapter_capture_receipt),
            "adapter_capture_receipt_sha256": runner.sha256_file(adapter_capture_receipt),
            "capture_root": str(capture_root),
            "raw_root": str(raw_root),
            "receipt_directory": str(receipt_directory),
            "range_count": str(len(range_receipts)),
            "owner_count": str(len(owner_hashes)),
            "ledger_sha256": hashlib.sha256("".join(range_rows).encode()).hexdigest(),
        }
        fields = SOURCE_EXACT_CAPTURE_FIELDS
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
            "exact_helper_sha256": exact_helper_sha256,
            "input_root": str(input_root),
            "input_manifest": str(input_manifest),
            "input_manifest_sha256": runner.INPUT_MANIFEST_SHA256,
            "corpus_blake3": runner.CORPUS_BLAKE3,
            "legacy_capture_receipt": str(adapter_capture_receipt),
            "legacy_capture_receipt_sha256": runner.sha256_file(adapter_capture_receipt),
            "capture_root": str(capture_root),
            "raw_root": str(raw_root),
            "receipt_directory": str(receipt_directory),
            "range_count": str(len(range_receipts)),
            "owner_count": str(len(owner_hashes)),
            "ledger_sha256": hashlib.sha256("".join(range_rows).encode()).hexdigest(),
        }
        fields = EXACT_CAPTURE_FIELDS
    else:
        raise ExactOwnerError("exact-owner build receipt has an unsupported schema")
    if set(record) != set(fields):
        raise ExactOwnerError("exact-owner capture receipt fields differ")
    return record


def parse_exact_owner_ledger(
    range_lines: list[str],
    expected_ranges: list[tuple[int, int]],
    label: str = "exact-owner capture receipt",
    field_count: int = len(EXACT_CAPTURE_FIELDS),
) -> tuple[list[tuple[int, int, str]], str]:
    rows: list[tuple[int, int, str]] = []
    for row_number, line in enumerate(range_lines, field_count + 1):
        fields = line.split("\t")
        if len(fields) != 4 or fields[0] != "range":
            raise ExactOwnerError(f"{label}: malformed range row {row_number}")
        start = _decimal(fields[1], "exact-owner range start")
        end = _decimal(fields[2], "exact-owner range end")
        if not HEX256.fullmatch(fields[3]):
            raise ExactOwnerError(f"{label}: malformed range receipt hash at row {row_number}")
        rows.append((start, end, fields[3]))
    if [(start, end) for start, end, _digest in rows] != expected_ranges:
        raise ExactOwnerError(f"{label}: range set differs from all source shards")
    canonical = "".join(f"range\t{start}\t{end}\t{digest}\n" for start, end, digest in rows)
    return rows, hashlib.sha256(canonical.encode()).hexdigest()


def _write_exact_capture_receipt(
    path: pathlib.Path,
    record: dict[str, str],
    range_receipts: dict[tuple[int, int], pathlib.Path],
    runner,
) -> None:
    schema = record.get("schema")
    if schema == "2":
        fields = SOURCE_EXACT_CAPTURE_FIELDS
    elif schema == "1":
        fields = EXACT_CAPTURE_FIELDS
    else:
        raise ExactOwnerError("exact-owner capture receipt has an unsupported schema")
    if set(record) != set(fields):
        raise ExactOwnerError("exact-owner capture receipt fields differ")
    lines = [f"{field}\t{record[field]}\n" for field in fields]
    lines.extend(
        f"range\t{start}\t{end}\t{runner.sha256_file(receipt)}\n"
        for (start, end), receipt in sorted(range_receipts.items())
    )
    adapter_runner._write_new_file(path, "".join(lines).encode("utf-8"), runner)


def _parse_exact_capture_receipt(
    path: pathlib.Path,
    role: str,
    expected_build_receipt: pathlib.Path,
    expected_build: dict[str, str],
    expected_inputs: dict[int, object],
    reference,
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    exact_helper_sha256: str,
    runner,
) -> tuple[dict[str, str], dict[tuple[int, int], pathlib.Path], dict[int, str]]:
    receipt = runner.regular_file(path, f"{role} exact-owner capture receipt")
    lines = runner.read_regular_text(receipt).splitlines()
    if not lines or lines[0] not in {"schema\t1", "schema\t2"}:
        raise ExactOwnerError(f"{receipt}: unsupported exact-owner capture receipt schema")
    capture_schema = lines[0].split("\t", 1)[1]
    if expected_build.get("schema") != capture_schema:
        raise ExactOwnerError(f"{receipt}: exact-owner capture and build receipt schemas differ")
    if lines[0] == "schema\t2":
        record, range_lines = adapter_runner._parse_receipt_header(
            receipt, lines, SOURCE_EXACT_CAPTURE_FIELDS, "source-qualified exact-owner capture"
        )
        if record["schema"] != "2" or record["role"] != role:
            raise ExactOwnerError(f"{receipt}: source-qualified exact-owner schema or role differs")
        for field in (
            "source_root", "source_commit", "source_tree", "binary", "binary_sha256",
            "helper_sha256",
        ):
            if record[field] != expected_build[field]:
                raise ExactOwnerError(f"{receipt}: {field} differs from exact-owner build receipt")
        source_receipt = runner.regular_file(
            expected_build["source_receipt"], "exact-owner source-cohort receipt"
        )
        if (
            record["source_receipt"] != str(source_receipt)
            or record["source_receipt_sha256"] != runner.sha256_file(source_receipt)
            or expected_build["source_receipt_sha256"] != runner.sha256_file(source_receipt)
        ):
            raise ExactOwnerError(f"{receipt}: source-cohort receipt identity differs")
        source_record = runner.parse_candidate_receipt(source_receipt)
        if source_record.get("schema") != runner.SOURCE_COHORT_SCHEMA:
            raise ExactOwnerError(f"{receipt}: source-cohort receipt is not source-qualified")
        if (
            record["input_root"] != str(input_root)
            or record["input_manifest"] != str(input_manifest)
            or record["input_manifest_sha256"] != source_record["input_manifest_sha256"]
            or record["corpus_blake3"] != source_record["corpus_blake3"]
        ):
            raise ExactOwnerError(f"{receipt}: source-qualified input cohort differs")
        expected_build_receipt = runner.regular_file(
            expected_build_receipt, f"{role} exact-owner build receipt"
        )
        if record["build_receipt"] != str(expected_build_receipt):
            raise ExactOwnerError(f"{receipt}: exact-owner build receipt path differs")
        if runner.sha256_file(expected_build_receipt) != record["build_receipt_sha256"]:
            raise ExactOwnerError(f"{receipt}: exact-owner build receipt hash differs")
        if record["exact_helper_sha256"] != exact_helper_sha256:
            raise ExactOwnerError(f"{receipt}: exact helper hash differs")
        adapter_capture_receipt = runner.regular_file(
            record["adapter_capture_receipt"], f"{role} source-qualified adapter capture"
        )
        if (
            str(adapter_capture_receipt) != record["adapter_capture_receipt"]
            or runner.sha256_file(adapter_capture_receipt) != record["adapter_capture_receipt_sha256"]
        ):
            raise ExactOwnerError(f"{receipt}: adapter capture receipt identity differs")
        adapter_build, adapter_raw_root, _adapter_receipt_directory, adapter_capture, adapter_inputs = _source_adapter_capture(
            adapter_capture_receipt,
            role,
            source_record,
            pathlib.Path(expected_build["source_root"]),
            expected_inputs,
            input_root,
            input_manifest,
            source_receipt,
            runner,
        )
        if (
            adapter_capture["source_commit"] != expected_build["source_commit"]
            or adapter_capture["source_tree"] != expected_build["source_tree"]
            or adapter_build["source_receipt_sha256"] != record["source_receipt_sha256"]
        ):
            raise ExactOwnerError(f"{receipt}: adapter capture source identity differs")
        if record["range_count"] != str(len(runner.expected_ranges(len(expected_inputs)))):
            raise ExactOwnerError(f"{receipt}: complete exact-owner range coverage differs")
        if record["owner_count"] != str(len(expected_inputs)):
            raise ExactOwnerError(f"{receipt}: complete exact-owner count differs")
        capture_root = runner.real_directory(record["capture_root"], f"{role} exact-owner capture root")
        raw_root = runner.real_directory(record["raw_root"], f"{role} exact-owner sidecars")
        receipt_directory = runner.real_directory(
            record["receipt_directory"], f"{role} exact-owner range receipts"
        )
        if (
            receipt.parent != capture_root
            or raw_root != capture_root / "owners"
            or receipt_directory != capture_root / "receipts"
            or pathlib.Path(adapter_capture["raw_root"]) != adapter_raw_root
        ):
            raise ExactOwnerError(f"{receipt}: exact-owner output paths differ")
        range_rows, ledger_sha256 = parse_exact_owner_ledger(
            range_lines, runner.expected_ranges(len(expected_inputs)), str(receipt),
            len(SOURCE_EXACT_CAPTURE_FIELDS),
        )
        if ledger_sha256 != record["ledger_sha256"]:
            raise ExactOwnerError(f"{receipt}: range ledger hash differs")
        range_receipts: dict[tuple[int, int], pathlib.Path] = {}
        owner_hashes: dict[int, str] = {}
        for (start, end), (_row_start, _row_end, digest) in zip(
            runner.expected_ranges(len(expected_inputs)), range_rows
        ):
            range_path = _exact_range_receipt_path(receipt_directory, start, end)
            if runner.sha256_file(range_path) != digest:
                raise ExactOwnerError(f"{receipt}: range receipt hash differs for {start}..{end}")
            range_receipts[(start, end)] = range_path
            observed = _read_exact_owner_receipt(
                range_path,
                start,
                end,
                adapter_inputs if record["schema"] == "2" else reference,
                runner,
            )
            if set(owner_hashes) & set(observed):
                raise ExactOwnerError(f"{receipt}: duplicated owner index")
            owner_hashes.update(observed)
        if set(owner_hashes) != set(range(len(expected_inputs))):
            raise ExactOwnerError(f"{receipt}: owner index coverage differs")
        return record, range_receipts, owner_hashes
    record, range_lines = adapter_runner._parse_receipt_header(
        receipt, lines, EXACT_CAPTURE_FIELDS, "exact-owner capture"
    )
    if record["schema"] != "1" or record["role"] != role:
        raise ExactOwnerError(f"{receipt}: exact-owner capture schema or role differs")
    for field in (
        "source_root", "source_commit", "source_tree", "binary", "binary_sha256", "helper_sha256",
    ):
        if record[field] != expected_build[field]:
            raise ExactOwnerError(f"{receipt}: {field} differs from exact-owner build receipt")
    expected_build_receipt = runner.regular_file(
        expected_build_receipt, f"{role} exact-owner build receipt"
    )
    if record["build_receipt"] != str(expected_build_receipt):
        raise ExactOwnerError(f"{receipt}: build receipt path differs")
    if runner.sha256_file(expected_build_receipt) != record["build_receipt_sha256"]:
        raise ExactOwnerError(f"{receipt}: build receipt hash differs")
    if record["exact_helper_sha256"] != exact_helper_sha256:
        raise ExactOwnerError(f"{receipt}: exact helper hash differs")
    if record["input_root"] != str(input_root) or record["input_manifest"] != str(input_manifest):
        raise ExactOwnerError(f"{receipt}: frozen input paths differ")
    if (
        record["input_manifest_sha256"] != runner.INPUT_MANIFEST_SHA256
        or record["corpus_blake3"] != runner.CORPUS_BLAKE3
    ):
        raise ExactOwnerError(f"{receipt}: frozen corpus pins differ")
    count = len(expected_inputs)
    expected_ranges = runner.expected_ranges(count)
    if record["range_count"] != str(len(expected_ranges)) or record["owner_count"] != str(count):
        raise ExactOwnerError(f"{receipt}: complete owner coverage differs")
    capture_root = runner.real_directory(record["capture_root"], f"{role} exact-owner capture root")
    raw_root = runner.real_directory(record["raw_root"], f"{role} exact-owner sidecars")
    receipt_directory = runner.real_directory(record["receipt_directory"], f"{role} exact-owner range receipts")
    if (
        receipt.parent != capture_root
        or capture_root != receipt.parent
        or raw_root != capture_root / "owners"
        or receipt_directory != capture_root / "receipts"
    ):
        raise ExactOwnerError(f"{receipt}: exact-owner output paths differ")
    range_rows, ledger_sha256 = parse_exact_owner_ledger(
        range_lines, expected_ranges, str(receipt)
    )
    if ledger_sha256 != record["ledger_sha256"]:
        raise ExactOwnerError(f"{receipt}: range ledger hash differs")
    range_receipts: dict[tuple[int, int], pathlib.Path] = {}
    owner_hashes: dict[int, str] = {}
    for (start, end), (_row_start, _row_end, digest) in zip(expected_ranges, range_rows):
        range_path = _exact_range_receipt_path(receipt_directory, start, end)
        if runner.sha256_file(range_path) != digest:
            raise ExactOwnerError(f"{receipt}: range receipt hash differs for {start}..{end}")
        range_receipts[(start, end)] = range_path
        observed = _read_exact_owner_receipt(range_path, start, end, reference, runner)
        if set(owner_hashes) & set(observed):
            raise ExactOwnerError(f"{receipt}: duplicated owner index")
        owner_hashes.update(observed)
    if set(owner_hashes) != set(range(count)):
        raise ExactOwnerError(f"{receipt}: owner index coverage differs")
    return record, range_receipts, owner_hashes


def _open_append_only(path: pathlib.Path, contents: bytes, runner) -> None:
    parent = runner.open_absolute_nofollow(path.parent, directory=True)
    descriptor = -1
    try:
        descriptor = os.open(
            path.name,
            os.O_WRONLY | os.O_APPEND | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0),
            dir_fd=parent,
        )
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ExactOwnerError(f"derived helper is not a regular file: {path}")
        with os.fdopen(descriptor, "ab") as output:
            descriptor = -1
            output.write(contents)
            output.flush()
            os.fsync(output.fileno())
        os.fsync(parent)
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        os.close(parent)


def _load_exact_helper(runner) -> tuple[pathlib.Path, bytes, str]:
    helper = runner.regular_file(EXACT_OWNER_HELPER, "approved exact-owner Rust helper")
    contents = runner.read_regular_bytes(helper)
    digest = hashlib.sha256(contents).hexdigest()
    if not EXACT_OWNER_HELPER_SHA256 or digest != EXACT_OWNER_HELPER_SHA256:
        raise ExactOwnerError("approved exact-owner Rust helper SHA-256 differs from its pin")
    return helper, contents, digest


def _project_exact_owner_helper(contents: bytes) -> bytes:
    projections = (
        (
            "repository receipt Read import",
            b"fn w9_exact_repository_owners_receipt() {\n"
            b"    use std::io::Read as _;\n",
            b"fn w9_exact_repository_owners_receipt() {\n",
        ),
        (
            "directory hash OFlags import",
            b"    use rustix::fs::{Dir, OFlags};\n",
            b"    use rustix::fs::Dir;\n",
        ),
        (
            "directory hash OsStrExt import",
            b"    use std::os::unix::ffi::OsStrExt as _;\n",
            b"",
        ),
    )
    projected = contents
    for name, source, replacement in projections:
        count = projected.count(source)
        if count != 1:
            raise ExactOwnerError(
                "source-qualified exact-owner helper projection drift: "
                f"expected one {name}, found {count}"
            )
        projected = projected.replace(source, replacement, 1)
    return projected


def _exact_helper_payload(
    source_record: Mapping[str, str], exact_helper_bytes: bytes, runner
) -> bytes:
    schema = source_record.get("schema")
    if schema == runner.SOURCE_COHORT_SCHEMA:
        return _project_exact_owner_helper(exact_helper_bytes)
    if schema == "2":
        return exact_helper_bytes
    raise ExactOwnerError(f"unsupported source receipt schema for exact-owner helper: {schema!r}")


def _compose_exact_helper(
    base_helper_bytes: bytes,
    source_record: Mapping[str, str],
    exact_helper_bytes: bytes,
    runner,
) -> bytes:
    payload = _exact_helper_payload(source_record, exact_helper_bytes, runner)
    separator = b"" if base_helper_bytes.endswith(b"\n") else b"\n"
    return base_helper_bytes + separator + payload


def _validate_exact_helper_artifact(
    helper_bytes: bytes,
    expected_sha256: str,
    source_record: Mapping[str, str],
    exact_helper_bytes: bytes,
    runner,
    artifact_name: str,
) -> None:
    if hashlib.sha256(helper_bytes).hexdigest() != expected_sha256:
        raise ExactOwnerError(f"{artifact_name} hash differs from its build receipt")
    payload = _exact_helper_payload(source_record, exact_helper_bytes, runner)
    if not helper_bytes.endswith(payload):
        raise ExactOwnerError(f"{artifact_name} does not end with its approved helper projection")


def parse_harness_test_names(stdout: str) -> list[str]:
    suffix = ": test"
    return [line[: -len(suffix)] for line in stdout.splitlines() if line.endswith(suffix)]


def require_exact_owner_test_listing(stdout: str) -> None:
    test_names = parse_harness_test_names(stdout)
    count = test_names.count(EXACT_OWNER_TEST)
    if count != 1:
        raise ExactOwnerError(f"harness listing contains {count} exact proof tests")


def _reuse_reference_build(
    receipt_path: pathlib.Path,
    reference_root: pathlib.Path,
    owner_helper_source: pathlib.Path,
    expected_helper_sha256: str,
    expected_owner_helper_sha256: str,
    expected_owner_helper_source_sha256: str,
    derived_helper: pathlib.Path,
    runner,
) -> tuple[pathlib.Path, pathlib.Path, dict[str, str]]:
    receipt = runner.regular_file(receipt_path, "retained reference adapter build receipt")
    build = adapter_runner._parse_adapter_build_receipt(
        receipt, "reference", runner, owner_helper_source
    )
    if pathlib.Path(build["source_root"]) != reference_root:
        raise ExactOwnerError("retained reference build source root differs")
    if (
        build["helper_sha256"] != expected_helper_sha256
        or build["owner_helper_sha256"] != expected_owner_helper_sha256
        or build["owner_helper_source_sha256"] != expected_owner_helper_source_sha256
    ):
        raise ExactOwnerError("retained reference build helper identity differs")
    retained_helper = runner.read_regular_bytes(
        runner.regular_file(build["helper_path"], "retained reference derived helper")
    )
    if retained_helper != runner.read_regular_bytes(derived_helper):
        raise ExactOwnerError("retained reference derived helper differs from the fresh helper")
    return pathlib.Path(build["binary"]), receipt, build


def _check_exact_harness_listing(
    builder: pathlib.Path,
    source_root: pathlib.Path,
    binary: pathlib.Path,
    docker_environment: dict[str, str],
) -> None:
    try:
        result = subprocess.run(
            [str(builder), str(source_root), "run", str(binary), "--list"],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=docker_environment,
        )
    except OSError as error:
        raise ExactOwnerError(f"could not list exact-owner harness tests: {error}") from error
    if result.returncode != 0:
        raise ExactOwnerError(
            f"exact-owner harness list failed with status {result.returncode}: "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    require_exact_owner_test_listing(result.stdout)


def _validate_source_inputs(source_record: dict[str, str], runner):
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA:
        expected_inputs, paths_by_name = runner.validate_source_cohort_inputs(source_record)
        return expected_inputs, paths_by_name, None
    return runner.validate_all_inputs(source_record)


def build_exact_owner_harnesses(args: argparse.Namespace, bootstrap_runner) -> None:
    candidate_root = bootstrap_runner.real_directory(args.candidate_root, "candidate source")
    source_receipt = bootstrap_runner.regular_file(args.candidate_source_receipt, "candidate source receipt")
    runner = adapter_runner._load_candidate_publication_runner(
        candidate_root, source_receipt, bootstrap_runner
    )
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise ExactOwnerError("candidate source receipt names a different checkout")
    if runner.source_identity(candidate_root) != (
        source_record["candidate_commit"], source_record["candidate_tree"]
    ):
        raise ExactOwnerError("candidate source identity differs from its source receipt")
    expected_inputs, _paths_by_name, _reference = _validate_source_inputs(source_record, runner)
    if len(expected_inputs) != 1690:
        raise ExactOwnerError(f"candidate input count differs: {len(expected_inputs)}")
    reference_root = adapter_runner._prepare_reference_source(
        candidate_root, pathlib.Path(args.reference_source_root), runner
    )
    runner.candidate_scratch_parent(candidate_root)
    owner_helper_source = candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
    owner_helper_path, exact_helper_bytes, exact_helper_sha256 = _load_exact_helper(runner)
    helper_attempt = adapter_runner._new_derived_helper_directory(candidate_root, runner)
    previous_root = adapter_runner.ROOT
    try:
        adapter_runner.ROOT = candidate_root
        helper, helper_sha256, owner_helper_sha256, owner_helper_source_sha256 = (
            adapter_runner.derive_helper(
                approved_helper=adapter_runner.SOURCE_APPROVED_HELPER,
                owner_helper=owner_helper_source,
                output_directory=helper_attempt,
                output_root=candidate_root,
            )
        )
    finally:
        adapter_runner.ROOT = previous_root
    base_helper = runner.read_regular_bytes(helper)
    composed_helper = _compose_exact_helper(
        base_helper, source_record, exact_helper_bytes, runner
    )
    _open_append_only(helper, composed_helper[len(base_helper):], runner)
    helper_bytes = runner.read_regular_bytes(helper)
    if helper_bytes != composed_helper:
        raise ExactOwnerError("derived exact-owner helper differs from its approved composition")
    helper_sha256 = hashlib.sha256(helper_bytes).hexdigest()
    docker_environment = runner.local_docker_environment()
    builds = {}
    for role, source_root in (("reference", reference_root), ("candidate", candidate_root)):
        if role == "reference" and args.reference_build_receipt is not None:
            binary, build_receipt, build = _reuse_reference_build(
                args.reference_build_receipt,
                reference_root,
                owner_helper_source,
                helper_sha256,
                owner_helper_sha256,
                owner_helper_source_sha256,
                helper,
                runner,
            )
        else:
            binary, build_receipt = adapter_runner.build_adapter_harness(
                source_root,
                candidate_root,
                source_receipt,
                source_record,
                role,
                helper,
                helper_sha256,
                owner_helper_sha256,
                owner_helper_source_sha256,
                runner,
            )
            build = adapter_runner._parse_adapter_build_receipt(
                build_receipt, role, runner, owner_helper_source
            )
        if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and build["schema"] != "2":
            raise ExactOwnerError("source-qualified exact-owner build requires source-qualified adapter builds")
        build_schema = build.get("schema")
        if build_schema == "2":
            listing_builder = pathlib.Path(source_record["builder_path"])
            listing_environment = {
                **docker_environment,
                "W9_SOURCE_COMMIT": build["source_commit"],
                "W9_SOURCE_TREE": build["source_tree"],
            }
        elif build_schema == "1":
            listing_builder = runner.regular_file(
                runner.APPROVED_BUILD_RUNNER, "pinned build runner", executable=True
            )
            listing_environment = docker_environment
        else:
            raise ExactOwnerError(f"{role} build receipt has an unsupported schema")
        runner.run_harness_test_listing(
            listing_builder,
            source_root,
            binary,
            listing_environment,
        )
        _check_exact_harness_listing(
            listing_builder,
            source_root,
            binary,
            listing_environment,
        )
        if build["helper_sha256"] != helper_sha256:
            raise ExactOwnerError(f"{role} build receipt helper hash differs")
        _validate_exact_helper_artifact(
            runner.read_regular_bytes(pathlib.Path(build["helper_path"])),
            build["helper_sha256"],
            source_record,
            exact_helper_bytes,
            runner,
            f"{role} build helper",
        )
        builds[role] = (binary, build_receipt)
    print(
        f"exact_helper={owner_helper_path} sha256={exact_helper_sha256}\n"
        f"reference_binary={builds['reference'][0]}\nreference_build_receipt={builds['reference'][1]}\n"
        f"candidate_binary={builds['candidate'][0]}\ncandidate_build_receipt={builds['candidate'][1]}"
    )


def _legacy_capture(
    capture_receipt: pathlib.Path,
    role: str,
    source_root: pathlib.Path,
    candidate_root: pathlib.Path,
    expected_inputs: dict[int, object],
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    reference,
    runner,
) -> tuple[dict[str, str], pathlib.Path, pathlib.Path]:
    capture_receipt = runner.regular_file(capture_receipt, f"{role} original capture receipt")
    capture_root = runner.real_directory(
        adapter_runner._capture_field(capture_receipt, "capture_root", runner),
        f"{role} original capture root",
    )
    raw_root = runner.real_directory(
        adapter_runner._capture_field(capture_receipt, "raw_root", runner),
        f"{role} original owner sidecars",
    )
    receipt_directory = runner.real_directory(
        adapter_runner._capture_field(capture_receipt, "receipt_directory", runner),
        f"{role} original range receipts",
    )
    if capture_receipt.parent != capture_root or capture_receipt.name != "capture.receipt":
        raise ExactOwnerError(f"{role} original capture receipt path differs")
    raw_descriptor = runner.open_absolute_nofollow(raw_root, directory=True)
    try:
        raw_names = set(os.listdir(raw_descriptor))
    finally:
        os.close(raw_descriptor)
    if raw_names != (
        {"schema"}
        | {f"owners-{index}.tsv" for index in range(len(expected_inputs))}
        | {
            f"record-{index}-{profile}"
            for index in range(len(expected_inputs))
            for profile in adapter_runner.PROFILES
        }
    ):
        raise ExactOwnerError(f"{role} original raw sidecar set differs")
    raw_descriptor = runner.open_absolute_nofollow(raw_root, directory=True)
    try:
        if adapter_runner._read_regular_at(raw_descriptor, "schema") != adapter_runner.RAW_SCHEMA:
            raise ExactOwnerError(f"{role} original raw schema differs")
    finally:
        os.close(raw_descriptor)
    lines = runner.read_regular_text(capture_receipt).splitlines()
    original_record, _range_lines = adapter_runner._parse_receipt_header(
        capture_receipt, lines, adapter_runner.CAPTURE_FIELDS, "original adapter capture"
    )
    if original_record["role"] != role:
        raise ExactOwnerError(f"{role} original capture receipt has the wrong role")
    original_build_receipt = runner.regular_file(
        original_record["build_receipt"], f"{role} original adapter build receipt"
    )
    original_build = adapter_runner._parse_adapter_build_receipt(
        original_build_receipt,
        role,
        runner,
        candidate_root / "dev/consolidation/w9-publication-owner-helper.rs",
    )
    if pathlib.Path(original_record["build_receipt"]) != original_build_receipt:
        raise ExactOwnerError(f"{role} original capture names a noncanonical build receipt")
    parsed_capture = adapter_runner._parse_capture_receipt(
        capture_receipt,
        role,
        receipt_directory,
        capture_root,
        raw_root,
        original_build,
        expected_inputs,
        input_root,
        input_manifest,
        runner,
    )
    if (
        pathlib.Path(parsed_capture["source_root"]) != source_root
        or parsed_capture["source_commit"] != runner.source_identity(source_root)[0]
        or parsed_capture["source_tree"] != runner.source_identity(source_root)[1]
        or parsed_capture["input_manifest_sha256"] != runner.INPUT_MANIFEST_SHA256
        or parsed_capture["corpus_blake3"] != runner.CORPUS_BLAKE3
    ):
        raise ExactOwnerError(f"{role} original capture source or corpus identity differs")
    if set(reference.inputs) != set(range(len(expected_inputs))):
        raise ExactOwnerError("pinned reference input coverage is incomplete")
    return original_build, raw_root, receipt_directory


def _source_adapter_capture(
    capture_receipt: pathlib.Path,
    role: str,
    source_record: dict[str, str],
    source_root: pathlib.Path,
    expected_inputs: dict[int, object],
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    source_receipt: pathlib.Path,
    runner,
) -> tuple[dict[str, str], pathlib.Path, pathlib.Path, dict[str, str], dict[int, object]]:
    capture_receipt = runner.regular_file(capture_receipt, f"{role} source-qualified adapter capture")
    lines = runner.read_regular_text(capture_receipt).splitlines()
    if not lines or lines[0] != "schema\t2":
        raise ExactOwnerError(f"{role} adapter capture is not source-qualified")
    build_receipt = runner.regular_file(
        adapter_runner._capture_field(capture_receipt, "build_receipt", runner),
        f"{role} source-qualified adapter build receipt",
    )
    owner_helper_source = pathlib.Path(source_record["candidate_root"]) / "dev/consolidation/w9-publication-owner-helper.rs"
    build = adapter_runner._parse_adapter_build_receipt(
        build_receipt, role, runner, owner_helper_source
    )
    if pathlib.Path(build["source_root"]) != source_root:
        raise ExactOwnerError(f"{role} adapter capture source root differs")
    if build["source_receipt"] != str(source_receipt) or build["source_receipt_sha256"] != runner.sha256_file(source_receipt):
        raise ExactOwnerError(f"{role} adapter build uses a different source receipt")
    parsed = adapter_runner._parse_capture_receipt(
        capture_receipt,
        role,
        pathlib.Path(adapter_runner._capture_field(capture_receipt, "receipt_directory", runner)),
        pathlib.Path(adapter_runner._capture_field(capture_receipt, "capture_root", runner)),
        pathlib.Path(adapter_runner._capture_field(capture_receipt, "raw_root", runner)),
        build,
        expected_inputs,
        input_root,
        input_manifest,
        runner,
    )
    if (
        parsed["source_receipt"] != str(source_receipt)
        or parsed["source_receipt_sha256"] != runner.sha256_file(source_receipt)
        or pathlib.Path(parsed["source_root"]) != source_root
        or parsed["source_commit"] != build["source_commit"]
        or parsed["source_tree"] != build["source_tree"]
        or parsed["input_root"] != str(input_root)
        or parsed["input_manifest"] != str(input_manifest)
        or parsed["input_manifest_sha256"] != source_record["input_manifest_sha256"]
        or parsed["corpus_blake3"] != source_record["corpus_blake3"]
    ):
        raise ExactOwnerError(f"{role} adapter capture source or cohort identity differs")
    raw_root = runner.real_directory(parsed["raw_root"], f"{role} adapter owner sidecars")
    descriptor = runner.open_absolute_nofollow(raw_root, directory=True)
    try:
        names = set(os.listdir(descriptor))
    finally:
        os.close(descriptor)
    expected_names = (
        {"schema"}
        | {f"owners-{index}.tsv" for index in range(len(expected_inputs))}
        | {
            f"record-{index}-{profile}"
            for index in range(len(expected_inputs))
            for profile in adapter_runner.PROFILES
        }
    )
    if names != expected_names:
        raise ExactOwnerError(f"{role} adapter capture raw owner coverage differs")
    descriptor = runner.open_absolute_nofollow(raw_root, directory=True)
    try:
        if adapter_runner._read_regular_at(descriptor, "schema") != adapter_runner.RAW_SCHEMA:
            raise ExactOwnerError(f"{role} adapter capture raw schema differs")
    finally:
        os.close(descriptor)
    receipt_directory = pathlib.Path(parsed["receipt_directory"])
    try:
        adapter_receipt = adapter_runner.aggregate_candidate_receipts(
            runner, receipt_directory, expected_inputs
        )
    except (adapter_runner.PublicationError, runner.PublicationError) as error:
        raise ExactOwnerError(f"{role} adapter capture input identities differ: {error}") from error
    return build, raw_root, receipt_directory, parsed, adapter_receipt.inputs


def _exact_owner_hash_job_directory(
    capture_output: pathlib.Path,
    role: str,
    pass_number: int,
    runner,
) -> pathlib.Path:
    hash_jobs = capture_output / "hashjobs"
    if pass_number == 1:
        adapter_runner._mkdir_new_directory(hash_jobs, runner)
    elif pass_number == 2:
        os.close(runner.open_absolute_nofollow(hash_jobs, directory=True))
    else:
        raise ExactOwnerError(f"unsupported exact-owner hash pass {pass_number}")
    job_dir = hash_jobs / f"{role}-{pass_number}"
    adapter_runner._mkdir_new_directory(job_dir, runner)
    return job_dir


def _run_exact_owner_hashes(
    binary: pathlib.Path,
    raw_root: pathlib.Path,
    source_record: dict[str, str],
    capture_output: pathlib.Path,
    role: str,
    pass_number: int,
    docker_environment: dict[str, str],
    runner,
) -> dict[int, str]:
    if not runner.under(raw_root, pathlib.Path(source_record["candidate_root"]) / ".work"):
        raise ExactOwnerError(f"exact-owner sidecars are outside candidate .work/: {raw_root}")
    job_dir = _exact_owner_hash_job_directory(capture_output, role, pass_number, runner)
    container = f"w9exact-{os.getpid()}-{role}-hash-{pass_number}"
    if runner.existing_container(container, docker_environment):
        raise ExactOwnerError(f"exact-owner hash Docker container already exists: {container}")
    source_qualified = source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA
    if source_qualified:
        command = [
            "timeout", "900s", source_record["builder_path"], source_record["candidate_root"], "run",
            f"W9_INPUT_ROOT={source_record['input_root']}",
            "W9_CPU_SET=0",
            f"W9_CONTAINER_NAME={container}",
            "env",
            f"W9_EXACT_OWNER_HASH_ROOT={raw_root}",
            "taskset", "-c", "0", str(binary), "--exact", EXACT_OWNER_TEST, "--nocapture",
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
            job_dir, container, 0, pathlib.Path(source_record["input_root"])
        )
        environment = dict(docker_environment)
        environment["PATH"] = f"{shim_dir}:/usr/bin:/bin"
        command = [
            "timeout", "900s", str(wrapper), str(wrapper_root), "run", "env",
            f"W9_EXACT_OWNER_HASH_ROOT={raw_root}",
            "taskset", "-c", "0", str(binary), "--exact", EXACT_OWNER_TEST, "--nocapture",
        ]
        job_root = wrapper_root
    log = job_dir / "worker.log"
    descriptor = os.open(
        log,
        os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0),
        0o600,
    )
    output = os.fdopen(descriptor, "wb")
    output.write(
        f"hash_role={role} cpu=0 memory_limit=1GiB input_count=1690 owners={raw_root} "
        f"binary_sha256={runner.sha256_file(binary)}\n".encode()
    )
    output.flush()
    job = {
        "worker": role, "cpu": 0, "start": 0, "end": 1690, "container": container,
        "receipt": log, "timing": log, "log": log, "process": None, "output": output,
    }
    active: list[dict[str, object]] = []
    try:
        runner.start_and_register_job(job, active, command, job_root, environment)
        process = job["process"]
        while process.poll() is None:
            time.sleep(1)
        if not output.closed:
            output.close()
        if process.returncode != 0:
            raise ExactOwnerError(
                f"native {role} exact-owner sidecar hashing failed with status {process.returncode}; log={log}"
            )
        leftovers = runner.inspect_owned_containers(
            f"^w9exact-{os.getpid()}-{role}-hash-{pass_number}$", docker_environment
        )
        if leftovers:
            runner.stop_jobs(active, docker_environment)
            raise ExactOwnerError(f"exact-owner hash container remained: {', '.join(leftovers)}")
        active.remove(job)
        return parse_exact_owner_hash_output(runner.read_regular_text(log), 1690)
    except BaseException as primary_error:
        if not output.closed:
            output.close()
        if active:
            try:
                runner.stop_jobs(active, docker_environment)
            except runner.CleanupFailure as cleanup_error:
                raise runner.CleanupFailure(
                    f"{role} exact-owner hash failed: {primary_error}; {cleanup_error}"
                ) from primary_error
        raise


def _launch_exact_owner_job(
    runner,
    source_record: dict[str, str],
    role: str,
    output: pathlib.Path,
    build: dict[str, str],
    raw_root: pathlib.Path,
    receipt_directory: pathlib.Path,
    start: int,
    end: int,
    worker: str,
    active: list[dict[str, object]],
    docker_environment: dict[str, str],
) -> dict[str, object]:
    cpu = next((cpu for slot, cpu in adapter_runner.CAPTURE_SLOTS if slot == worker), None)
    if cpu is None:
        raise ExactOwnerError(f"unknown exact-owner worker slot {worker}")
    job_dir = output / "jobs" / worker / f"{start}-{end}"
    job_dir.mkdir(mode=0o700, exist_ok=False)
    receipt = _exact_range_receipt_path(receipt_directory, start, end)
    log = job_dir / "worker.log"
    if any(path.exists() or path.is_symlink() for path in (receipt, log)):
        raise ExactOwnerError(f"refusing to overwrite exact-owner shard artifact {worker}/{start}..{end}")
    container = f"w9exact-{os.getpid()}-{role}-{worker}-{start}"
    if runner.existing_container(container, docker_environment):
        raise ExactOwnerError(f"exact-owner Docker container already exists: {container}")
    source_qualified = source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA
    if source_qualified:
        command = [
            "timeout", "900s", source_record["builder_path"], source_record["candidate_root"], "run",
            f"W9_INPUT_ROOT={source_record['input_root']}",
            f"W9_CPU_SET={cpu}",
            f"W9_CONTAINER_NAME={container}",
            "env",
            f"W9_EXACT_OWNER_OUTPUT_DIR={raw_root}",
            f"W9_EXACT_OWNER_RECEIPT={receipt}",
            f"W9_SHARD_INDEX={start // 100}",
            "W9_SHARD_SIZE=100",
            f"W9_START={start}",
            f"W9_END={end}",
            "taskset", "-c", str(cpu), build["binary"], "--exact", EXACT_OWNER_TEST, "--nocapture",
        ]
        environment = {
            **docker_environment,
            "W9_SOURCE_COMMIT": source_record["candidate_commit"],
            "W9_SOURCE_TREE": source_record["candidate_tree"],
        }
        job_root = pathlib.Path(source_record["candidate_root"])
    else:
        shim_dir = runner.docker_shim(
            job_dir, container, cpu, pathlib.Path(source_record["input_root"])
        )
        command = [
            "timeout", "900s", source_record["wrapper"], source_record["wrapper_root"], "run", "env",
            f"W9_INPUT_ROOT={source_record['input_root']}",
            f"W9_EXACT_OWNER_OUTPUT_DIR={raw_root}",
            f"W9_EXACT_OWNER_RECEIPT={receipt}",
            f"W9_SHARD_INDEX={start // 100}",
            "W9_SHARD_SIZE=100",
            f"W9_START={start}",
            f"W9_END={end}",
            "taskset", "-c", str(cpu), build["binary"], "--exact", EXACT_OWNER_TEST, "--nocapture",
        ]
        environment = dict(docker_environment)
        environment["PATH"] = f"{shim_dir}:/usr/bin:/bin"
    descriptor = os.open(
        log,
        os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0),
        0o600,
    )
    output_handle = os.fdopen(descriptor, "wb")
    output_handle.write(
        f"role={role} cpu={cpu} memory_limit=1GiB range={start}..{end} "
        f"source={build['source_commit']}:{build['source_tree']} "
        f"binary_sha256={build['binary_sha256']} corpus={runner.CORPUS_BLAKE3}\n".encode()
    )
    output_handle.flush()
    job = {
        "worker": worker,
        "cpu": cpu,
        "start": start,
        "end": end,
        "container": container,
        "receipt": receipt,
        "timing": log,
        "log": log,
        "process": None,
        "output": output_handle,
    }
    try:
        runner.start_and_register_job(
            job, active, command, job_root, environment
        )
    except BaseException:
        if not output_handle.closed:
            output_handle.close()
        raise
    return job


def _capture_exact_owner_role(
    runner,
    source_record: dict[str, str],
    role: str,
    build_receipt: pathlib.Path,
    build: dict[str, str],
    adapter_capture_receipt: pathlib.Path,
    exact_helper_sha256: str,
    expected_inputs: dict[int, object],
    reference,
    input_root: pathlib.Path,
    input_manifest: pathlib.Path,
    output: pathlib.Path,
    completed: list[tuple[int, int]],
    docker_environment: dict[str, str],
) -> dict[str, str]:
    raw_root = output / "owners"
    receipt_inputs = reference if reference is not None else expected_inputs
    receipt_directory = output / "receipts"
    jobs_root = output / "jobs"
    for directory in (raw_root, receipt_directory, jobs_root):
        adapter_runner._mkdir_new_directory(directory, runner)
    for worker, _cpu in adapter_runner.CAPTURE_SLOTS:
        adapter_runner._mkdir_new_directory(jobs_root / worker, runner)
    expected_ranges, pending_ranges = adapter_runner._capture_plan(len(expected_inputs), runner)
    active: list[dict[str, object]] = []
    range_receipts: dict[tuple[int, int], pathlib.Path] = {}
    owner_hashes: dict[int, str] = {}
    try:
        for worker, _cpu in adapter_runner.CAPTURE_SLOTS:
            if not pending_ranges:
                break
            start, end = pending_ranges.pop(0)
            _launch_exact_owner_job(
                runner, source_record, role, output, build, raw_root,
                receipt_directory, start, end, worker, active, docker_environment,
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
                start, end = int(job["start"]), int(job["end"])
                if status != 0:
                    raise ExactOwnerError(
                        f"{role} exact-owner worker {job['worker']} range {start}..{end} "
                        f"failed with status {status}; log={job['log']}"
                    )
                receipt = runner.regular_file(job["receipt"], f"{role} exact-owner range receipt")
                observed = _read_exact_owner_receipt(receipt, start, end, receipt_inputs, runner)
                if set(owner_hashes) & set(observed):
                    raise ExactOwnerError(f"{role} exact-owner ranges duplicate an input index")
                owner_hashes.update(observed)
                range_receipts[(start, end)] = receipt
                completed.append((start, end))
                runner.write_status(
                    output,
                    "incomplete",
                    completed,
                    f"completed_owner_range={start}..{end}",
                )
                active.remove(job)
                worker = str(job["worker"])
                if pending_ranges:
                    next_start, next_end = pending_ranges.pop(0)
                    _launch_exact_owner_job(
                        runner, source_record, role, output, build, raw_root,
                        receipt_directory, next_start, next_end, worker, active, docker_environment,
                    )
            if not changed:
                time.sleep(1)
        leftovers = runner.inspect_owned_containers(f"^w9exact-{os.getpid()}-{role}-", docker_environment)
        if leftovers:
            stranded = [{"container": name, "process": None, "output": None} for name in leftovers]
            runner.stop_jobs(stranded, docker_environment)
            raise ExactOwnerError(f"exact-owner containers remained after {role} capture: {', '.join(leftovers)}")
        if sorted(range_receipts) != expected_ranges or set(owner_hashes) != set(range(len(expected_inputs))):
            raise ExactOwnerError(f"{role} exact-owner capture coverage differs")
        _validate_source_inputs(source_record, runner)
        if runner.source_identity(pathlib.Path(build["source_root"])) != (
            build["source_commit"], build["source_tree"]
        ):
            raise ExactOwnerError(f"{role} source identity changed during exact-owner capture")
        record = _exact_capture_record(
            role, build_receipt, build, adapter_capture_receipt, output, raw_root,
            receipt_directory, input_root, input_manifest, range_receipts, owner_hashes,
            exact_helper_sha256, runner,
        )
        capture_receipt = output / "capture.receipt"
        _write_exact_capture_receipt(capture_receipt, record, range_receipts, runner)
        return record
    except BaseException as primary_error:
        if active:
            try:
                runner.stop_jobs(active, docker_environment)
            except runner.CleanupFailure as cleanup_error:
                raise runner.CleanupFailure(
                    f"{role} exact-owner capture failed: {primary_error}; {cleanup_error}"
                ) from primary_error
        raise


def capture_exact_owner_role(args: argparse.Namespace, bootstrap_runner) -> int:
    candidate_root = bootstrap_runner.real_directory(args.candidate_root, "candidate source")
    source_receipt = bootstrap_runner.regular_file(args.candidate_source_receipt, "candidate source receipt")
    runner = adapter_runner._load_candidate_publication_runner(
        candidate_root, source_receipt, bootstrap_runner
    )
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise ExactOwnerError("candidate source receipt names a different checkout")
    if runner.source_identity(candidate_root) != (
        source_record["candidate_commit"], source_record["candidate_tree"]
    ):
        raise ExactOwnerError("candidate source identity differs from its source receipt")
    expected_inputs, _paths_by_name, reference = _validate_source_inputs(source_record, runner)
    if len(expected_inputs) != 1690:
        raise ExactOwnerError(f"candidate input count differs: {len(expected_inputs)}")
    reference_root = adapter_runner._prepare_reference_source(
        candidate_root, pathlib.Path(args.reference_source_root), runner
    )
    role = args.role
    source_root = reference_root if role == "reference" else candidate_root
    build_receipt = runner.regular_file(args.build_receipt, f"{role} exact-owner build receipt")
    build = adapter_runner._parse_adapter_build_receipt(
        build_receipt,
        role,
        runner,
        candidate_root / "dev/consolidation/w9-publication-owner-helper.rs",
    )
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and build["schema"] != "2":
        raise ExactOwnerError(f"{role} source-qualified exact-owner capture requires a schema-2 build")
    if pathlib.Path(build["source_root"]) != source_root:
        raise ExactOwnerError(f"{role} exact-owner build source differs")
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and (
        build["source_receipt"] != str(source_receipt)
        or build["source_receipt_sha256"] != runner.sha256_file(source_receipt)
    ):
        raise ExactOwnerError(f"{role} exact-owner build uses a different source receipt")
    exact_helper_path, exact_helper_bytes, exact_helper_sha256 = _load_exact_helper(runner)
    helper_path = runner.regular_file(build["helper_path"], f"{role} exact-owner derived helper")
    if not runner.under(helper_path, candidate_root / ".work"):
        raise ExactOwnerError(f"{role} exact-owner helper is outside candidate .work/: {helper_path}")
    helper_bytes = runner.read_regular_bytes(helper_path)
    _validate_exact_helper_artifact(
        helper_bytes,
        build["helper_sha256"],
        source_record,
        exact_helper_bytes,
        runner,
        f"{role} exact-owner helper",
    )
    runner.require_sha256(pathlib.Path(build["binary"]), build["binary_sha256"], f"{role} exact-owner harness")
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA:
        if getattr(args, "adapter_capture_receipt", None) is None:
            raise ExactOwnerError(f"{role} exact-owner capture requires a fresh source-qualified adapter capture")
        adapter_capture_receipt = runner.regular_file(
            args.adapter_capture_receipt, f"{role} source-qualified adapter capture receipt"
        )
        adapter_build, _adapter_raw, _adapter_receipts, _adapter_capture, adapter_inputs = _source_adapter_capture(
            adapter_capture_receipt,
            role,
            source_record,
            source_root,
            expected_inputs,
            pathlib.Path(source_record["input_root"]),
            pathlib.Path(source_record["input_manifest"]),
            source_receipt,
            runner,
        )
        if (adapter_build["source_commit"], adapter_build["source_tree"]) != (
            build["source_commit"], build["source_tree"]
        ):
            raise ExactOwnerError(f"{role} adapter capture and exact-owner build source identities differ")
        receipt_reference = adapter_inputs
    else:
        adapter_capture_receipt = runner.regular_file(
            args.legacy_capture_receipt, f"{role} original capture receipt"
        )
        legacy_build, _legacy_raw, _legacy_receipts = _legacy_capture(
            adapter_capture_receipt,
            role,
            source_root,
            candidate_root,
            expected_inputs,
            pathlib.Path(source_record["input_root"]),
            pathlib.Path(source_record["input_manifest"]),
            reference,
            runner,
        )
        if (legacy_build["source_commit"], legacy_build["source_tree"]) != (
            build["source_commit"], build["source_tree"]
        ):
            raise ExactOwnerError(f"{role} original capture and exact-owner build source identities differ")
        receipt_reference = reference
    output = runner.require_output_path(args.output, candidate_root)
    if args.dry_run:
        print(
            f"preflight ok: role={role} source={build['source_commit']} tree={build['source_tree']} "
            f"corpus=1690 ranges={len(runner.expected_ranges(len(expected_inputs)))} "
            f"workers=4 cpus=0,1,2,3 memory_per_worker=1GiB output={output}"
        )
        return 0
    completed: list[tuple[int, int]] = []
    docker_environment = runner.local_docker_environment()
    with runner.claim_run_resources(candidate_root, output, completed):
        runner.write_status(output, "incomplete", completed)
        record = _capture_exact_owner_role(
            runner,
            source_record,
            role,
            build_receipt,
            build,
            adapter_capture_receipt,
            exact_helper_sha256,
            expected_inputs,
            receipt_reference,
            pathlib.Path(source_record["input_root"]),
            pathlib.Path(source_record["input_manifest"]),
            output,
            completed,
            docker_environment,
        )
        runner.write_status(output, "complete", completed, f"role={role} owners={record['owner_count']}")
    print(f"role={role} owners=1690 capture_receipt={output / 'capture.receipt'}")
    return 0


def _validate_owner_projections(
    exact_raw_root: pathlib.Path,
    legacy_raw_root: pathlib.Path,
    expected_inputs: dict[int, object],
    exact_hashes: dict[int, str],
    runner,
) -> dict[int, tuple[object, dict[str, object]]]:
    exact_descriptor = runner.open_absolute_nofollow(exact_raw_root, directory=True)
    legacy_descriptor = runner.open_absolute_nofollow(legacy_raw_root, directory=True)
    try:
        exact_names = set(os.listdir(exact_descriptor))
        legacy_names = set(os.listdir(legacy_descriptor))
        expected_exact_names = {f"owners-{index}.tsv" for index in range(len(expected_inputs))}
        if exact_names != expected_exact_names:
            raise ExactOwnerError("exact-owner sidecar set differs from complete native hash input")
        if not expected_exact_names <= legacy_names:
            raise ExactOwnerError("original owner sidecar set is incomplete")
        validated: dict[int, tuple[object, dict[str, object]]] = {}
        for index in range(len(expected_inputs)):
            exact_bytes = adapter_runner._read_regular_bytes_at(
                exact_descriptor, f"owners-{index}.tsv"
            )
            legacy_text = adapter_runner._read_regular_at(
                legacy_descriptor, f"owners-{index}.tsv"
            )
            try:
                exact_text = exact_bytes.decode("utf-8")
                legacy_projection = adapter.parse_owner_index(legacy_text)
                exact_projection, exact_owners = parse_exact_owner_index(exact_text)
            except (UnicodeError, adapter.AdapterError, ExactOwnerError) as error:
                raise ExactOwnerError(f"owner {index}: invalid owner projection: {error}") from error
            if exact_projection != legacy_projection:
                raise ExactOwnerError(
                    f"owner {index}: original source, generic, tail, or statement rows differ"
                )
            if index not in exact_hashes:
                raise ExactOwnerError(f"owner {index}: native exact-owner digest is missing")
            if not HEX256.fullmatch(exact_hashes[index]):
                raise ExactOwnerError(f"owner {index}: native exact-owner digest is malformed")
            validated[index] = (exact_projection, exact_owners)
        return validated
    finally:
        os.close(exact_descriptor)
        os.close(legacy_descriptor)


def _proof_record(
    role: str,
    build_receipt: pathlib.Path,
    build: dict[str, str],
    exact_helper_path: pathlib.Path,
    exact_helper_sha256: str,
    adapter_capture_receipt: pathlib.Path,
    exact_capture_receipt: pathlib.Path,
    capture: dict[str, str],
    owner_hashes: dict[int, str],
    runner,
) -> dict[str, str]:
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
            "build_receipt": str(build_receipt),
            "build_receipt_sha256": runner.sha256_file(build_receipt),
            "binary": build["binary"],
            "binary_sha256": build["binary_sha256"],
            "helper_sha256": build["helper_sha256"],
            "exact_helper_path": str(exact_helper_path),
            "exact_helper_sha256": exact_helper_sha256,
            "owner_helper_sha256": build["owner_helper_sha256"],
            "input_root": capture["input_root"],
            "input_manifest": capture["input_manifest"],
            "input_manifest_sha256": capture["input_manifest_sha256"],
            "corpus_blake3": capture["corpus_blake3"],
            "adapter_capture_receipt": str(adapter_capture_receipt),
            "adapter_capture_receipt_sha256": runner.sha256_file(adapter_capture_receipt),
            "capture_receipt": str(exact_capture_receipt),
            "capture_receipt_sha256": runner.sha256_file(exact_capture_receipt),
            "capture_root": capture["capture_root"],
            "raw_root": capture["raw_root"],
            "receipt_directory": capture["receipt_directory"],
            "range_count": capture["range_count"],
            "owner_count": capture["owner_count"],
            "ledger_sha256": capture["ledger_sha256"],
            "owner_hashes_sha256": hashlib.sha256(
                "".join(f"{index}\t{owner_hashes[index]}\n" for index in sorted(owner_hashes)).encode()
            ).hexdigest(),
        }
        fields = SOURCE_EXACT_PROOF_FIELDS
    elif build_schema == "1":
        record = {
        "schema": "1",
        "role": role,
        "source_root": build["source_root"],
        "source_commit": build["source_commit"],
        "source_tree": build["source_tree"],
        "build_receipt": str(build_receipt),
        "build_receipt_sha256": runner.sha256_file(build_receipt),
        "binary": build["binary"],
        "binary_sha256": build["binary_sha256"],
        "helper_sha256": build["helper_sha256"],
        "exact_helper_path": str(exact_helper_path),
        "exact_helper_sha256": exact_helper_sha256,
        "owner_helper_sha256": build["owner_helper_sha256"],
        "input_root": capture["input_root"],
        "input_manifest": capture["input_manifest"],
        "input_manifest_sha256": capture["input_manifest_sha256"],
        "corpus_blake3": capture["corpus_blake3"],
        "legacy_capture_receipt": str(adapter_capture_receipt),
        "legacy_capture_receipt_sha256": runner.sha256_file(adapter_capture_receipt),
        "capture_receipt": str(exact_capture_receipt),
        "capture_receipt_sha256": runner.sha256_file(exact_capture_receipt),
        "capture_root": capture["capture_root"],
        "raw_root": capture["raw_root"],
        "receipt_directory": capture["receipt_directory"],
        "range_count": capture["range_count"],
        "owner_count": capture["owner_count"],
        "ledger_sha256": capture["ledger_sha256"],
        "owner_hashes_sha256": hashlib.sha256(
            "".join(f"{index}\t{owner_hashes[index]}\n" for index in sorted(owner_hashes)).encode()
        ).hexdigest(),
        }
        fields = EXACT_PROOF_FIELDS
    else:
        raise ExactOwnerError("exact-owner build receipt has an unsupported schema")
    if set(record) != set(fields):
        raise ExactOwnerError("exact-owner proof receipt fields differ")
    return record


def _write_proof_receipt(path: pathlib.Path, record: dict[str, str], runner) -> None:
    schema = record.get("schema")
    if schema == "2":
        source_qualified = True
        fields = SOURCE_EXACT_PROOF_FIELDS
    elif schema == "1":
        source_qualified = False
        fields = EXACT_PROOF_FIELDS
    else:
        raise ExactOwnerError("exact-owner proof receipt has an unsupported schema")
    if set(record) != set(fields):
        raise ExactOwnerError("exact-owner proof receipt fields differ")
    adapter_runner._write_new_file(
        path,
        ((SOURCE_EXACT_PROOF_SCHEMA if source_qualified else EXACT_PROOF_SCHEMA)
         + "".join(f"{field}\t{record[field]}\n" for field in fields)).encode(),
        runner,
    )


def validate_proof(
    candidate_root: pathlib.Path,
    source_receipt: pathlib.Path,
    reference_source_root: pathlib.Path,
    role: str,
    build_receipt: pathlib.Path,
    exact_capture_receipt: pathlib.Path,
    output: pathlib.Path,
    bootstrap_runner,
    dry_run: bool = False,
) -> dict[str, object]:
    candidate_root = bootstrap_runner.real_directory(candidate_root, "candidate source")
    source_receipt = bootstrap_runner.regular_file(source_receipt, "candidate source receipt")
    runner = adapter_runner._load_candidate_publication_runner(
        candidate_root, source_receipt, bootstrap_runner
    )
    source_record = runner.parse_candidate_receipt(source_receipt)
    if pathlib.Path(source_record["candidate_root"]) != candidate_root:
        raise ExactOwnerError("candidate source receipt names a different checkout")
    if runner.source_identity(candidate_root) != (
        source_record["candidate_commit"], source_record["candidate_tree"]
    ):
        raise ExactOwnerError("candidate source identity differs from its source receipt")
    expected_inputs, _paths_by_name, reference = _validate_source_inputs(source_record, runner)
    if len(expected_inputs) != 1690:
        raise ExactOwnerError(f"candidate input count differs: {len(expected_inputs)}")
    reference_root = adapter_runner._prepare_reference_source(
        candidate_root, reference_source_root, runner
    )
    selected_source_root = reference_root if role == "reference" else candidate_root
    build_receipt = runner.regular_file(build_receipt, f"{role} exact-owner build receipt")
    build = adapter_runner._parse_adapter_build_receipt(
        build_receipt,
        role,
        runner,
        candidate_root / "dev/consolidation/w9-publication-owner-helper.rs",
    )
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and build["schema"] != "2":
        raise ExactOwnerError(f"{role} source-qualified exact-owner proof requires a schema-2 build")
    if pathlib.Path(build["source_root"]) != selected_source_root:
        raise ExactOwnerError(f"{role} exact-owner build source differs")
    if source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA and (
        build["source_receipt"] != str(source_receipt)
        or build["source_receipt_sha256"] != runner.sha256_file(source_receipt)
    ):
        raise ExactOwnerError(f"{role} exact-owner build uses a different source receipt")
    exact_helper_path, exact_helper_bytes, exact_helper_sha256 = _load_exact_helper(runner)
    helper_path = runner.regular_file(build["helper_path"], f"{role} exact-owner derived helper")
    if not runner.under(helper_path, candidate_root / ".work"):
        raise ExactOwnerError(f"{role} exact-owner helper is outside candidate .work/: {helper_path}")
    helper_bytes = runner.read_regular_bytes(helper_path)
    _validate_exact_helper_artifact(
        helper_bytes,
        build["helper_sha256"],
        source_record,
        exact_helper_bytes,
        runner,
        f"{role} exact-owner helper",
    )
    binary = runner.regular_file(build["binary"], f"{role} exact-owner harness", executable=True)
    runner.require_sha256(binary, build["binary_sha256"], f"{role} exact-owner harness")
    exact_capture_receipt = runner.regular_file(
        exact_capture_receipt, f"{role} exact-owner capture receipt"
    )
    capture_record_lines = runner.read_regular_text(exact_capture_receipt).splitlines()
    if not capture_record_lines or capture_record_lines[0] not in {"schema\t1", "schema\t2"}:
        raise ExactOwnerError(f"{role} exact-owner capture has an unsupported receipt schema")
    if (
        source_record.get("schema") == runner.SOURCE_COHORT_SCHEMA
        and capture_record_lines[0] != "schema\t2"
    ):
        raise ExactOwnerError(f"{role} source-qualified exact-owner proof requires a schema-2 capture")
    if capture_record_lines[0] == "schema\t2":
        capture, _range_rows = adapter_runner._parse_receipt_header(
            exact_capture_receipt,
            capture_record_lines,
            SOURCE_EXACT_CAPTURE_FIELDS,
            "source-qualified exact-owner capture",
        )
        adapter_capture_receipt = runner.regular_file(
            capture["adapter_capture_receipt"], f"{role} source-qualified adapter capture"
        )
        if (
            runner.sha256_file(adapter_capture_receipt) != capture["adapter_capture_receipt_sha256"]
            or pathlib.Path(capture["source_receipt"]) != source_receipt
            or capture["source_receipt_sha256"] != runner.sha256_file(source_receipt)
        ):
            raise ExactOwnerError(f"{role} source or adapter capture receipt identity differs")
        adapter_build, adapter_raw_root, _adapter_receipt_directory, _adapter_capture, _adapter_inputs = _source_adapter_capture(
            adapter_capture_receipt,
            role,
            source_record,
            selected_source_root,
            expected_inputs,
            pathlib.Path(source_record["input_root"]),
            pathlib.Path(source_record["input_manifest"]),
            source_receipt,
            runner,
        )
        if (adapter_build["source_commit"], adapter_build["source_tree"]) != (
            build["source_commit"], build["source_tree"]
        ):
            raise ExactOwnerError(f"{role} adapter capture and exact-owner build source identities differ")
    else:
        capture, _range_rows = adapter_runner._parse_receipt_header(
            exact_capture_receipt, capture_record_lines, EXACT_CAPTURE_FIELDS, "exact-owner capture"
        )
        adapter_capture_receipt = runner.regular_file(
            capture["legacy_capture_receipt"], f"{role} original capture receipt"
        )
        if runner.sha256_file(adapter_capture_receipt) != capture["legacy_capture_receipt_sha256"]:
            raise ExactOwnerError(f"{role} original capture receipt hash differs")
        original_build, adapter_raw_root, _adapter_receipt_directory = _legacy_capture(
            adapter_capture_receipt,
            role,
            selected_source_root,
            candidate_root,
            expected_inputs,
            pathlib.Path(source_record["input_root"]),
            pathlib.Path(source_record["input_manifest"]),
            reference,
            runner,
        )
        if (original_build["source_commit"], original_build["source_tree"]) != (
            build["source_commit"], build["source_tree"]
        ):
            raise ExactOwnerError(f"{role} original capture and exact-owner build source identities differ")
    _, range_receipts, receipt_hashes = _parse_exact_capture_receipt(
        exact_capture_receipt,
        role,
        build_receipt,
        build,
        expected_inputs,
        reference,
        pathlib.Path(source_record["input_root"]),
        pathlib.Path(source_record["input_manifest"]),
        exact_helper_sha256,
        runner,
    )
    output = runner.require_output_path(output, candidate_root)
    if dry_run:
        print(
            f"preflight ok: role={role} source={build['source_commit']} tree={build['source_tree']} "
            f"owners={len(receipt_hashes)} ranges={len(range_receipts)} output={output}"
        )
        return {"capture": capture, "range_receipts": range_receipts, "owner_hashes": receipt_hashes}
    completed: list[tuple[int, int]] = []
    docker_environment = runner.local_docker_environment()
    with runner.claim_run_resources(candidate_root, output, completed):
        runner.write_status(output, "incomplete", completed)
        native_hashes = _run_exact_owner_hashes(
            binary, pathlib.Path(capture["raw_root"]), source_record, output, role, 1,
            docker_environment, runner,
        )
        if native_hashes != receipt_hashes:
            differing = [
                index for index in range(len(expected_inputs))
                if native_hashes.get(index) != receipt_hashes.get(index)
            ][:10]
            raise ExactOwnerError(f"{role} native and range-receipt owner hashes differ: {differing}")
        _validate_source_inputs(source_record, runner)
        if runner.source_identity(selected_source_root) != (
            build["source_commit"], build["source_tree"]
        ):
            raise ExactOwnerError(f"{role} source identity changed before exact-owner sidecar validation")
        validated = _validate_owner_projections(
            pathlib.Path(capture["raw_root"]), adapter_raw_root, expected_inputs, native_hashes, runner
        )
        confirmed_hashes = _run_exact_owner_hashes(
            binary, pathlib.Path(capture["raw_root"]), source_record, output, role, 2,
            docker_environment, runner,
        )
        if confirmed_hashes != native_hashes:
            raise ExactOwnerError(f"{role} exact-owner sidecars changed during validation")
        _validate_source_inputs(source_record, runner)
        if runner.source_identity(selected_source_root) != (
            build["source_commit"], build["source_tree"]
        ):
            raise ExactOwnerError(f"{role} source identity changed during exact-owner proof")
        record = _proof_record(
            role,
            build_receipt,
            build,
            exact_helper_path,
            exact_helper_sha256,
            adapter_capture_receipt,
            exact_capture_receipt,
            capture,
            confirmed_hashes,
            runner,
        )
        proof_receipt = output / "proof.receipt"
        _write_proof_receipt(proof_receipt, record, runner)
        runner.write_status(output, "complete", completed, f"role={role} owners={len(validated)}")
    print(f"role={role} owners={len(validated)} proof_receipt={proof_receipt}")
    return {
        "proof_receipt": proof_receipt,
        "capture": capture,
        "range_receipts": range_receipts,
        "owner_hashes": confirmed_hashes,
        "sidecar_root": pathlib.Path(capture["raw_root"]),
        "owners": validated,
    }


def main(argv: list[str] | None = None) -> int:
    bootstrap_runner = adapter_runner._load_publication_runner(
        adapter_runner.ROOT / "dev/consolidation/w9-publication-run.py"
    )
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("build-harnesses")
    build.add_argument("--candidate-root", type=pathlib.Path, required=True)
    build.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    build.add_argument("--reference-source-root", type=pathlib.Path, required=True)
    build.add_argument("--reference-build-receipt", type=pathlib.Path)
    build.add_argument("--dry-run", action="store_true")
    capture = commands.add_parser("capture")
    capture.add_argument("--candidate-root", type=pathlib.Path, required=True)
    capture.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    capture.add_argument("--reference-source-root", type=pathlib.Path, required=True)
    capture.add_argument("--role", choices=("reference", "candidate"), required=True)
    capture.add_argument("--build-receipt", type=pathlib.Path, required=True)
    capture_receipt_group = capture.add_mutually_exclusive_group(required=True)
    capture_receipt_group.add_argument("--legacy-capture-receipt", type=pathlib.Path)
    capture_receipt_group.add_argument("--adapter-capture-receipt", type=pathlib.Path)
    capture.add_argument("--output", required=True)
    capture.add_argument("--dry-run", action="store_true")
    validate = commands.add_parser("validate")
    validate.add_argument("--candidate-root", type=pathlib.Path, required=True)
    validate.add_argument("--candidate-source-receipt", type=pathlib.Path, required=True)
    validate.add_argument("--reference-source-root", type=pathlib.Path, required=True)
    validate.add_argument("--role", choices=("reference", "candidate"), required=True)
    validate.add_argument("--build-receipt", type=pathlib.Path, required=True)
    validate.add_argument("--capture-receipt", type=pathlib.Path, required=True)
    validate.add_argument("--output", required=True)
    validate.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.command == "build-harnesses":
            if args.dry_run:
                candidate_root = bootstrap_runner.real_directory(args.candidate_root, "candidate source")
                source_receipt = bootstrap_runner.regular_file(
                    args.candidate_source_receipt, "candidate source receipt"
                )
                runner = adapter_runner._load_candidate_publication_runner(
                    candidate_root, source_receipt, bootstrap_runner
                )
                record = runner.parse_candidate_receipt(source_receipt)
                expected_inputs, _paths_by_name, _reference = _validate_source_inputs(record, runner)
                if len(expected_inputs) != 1690:
                    raise ExactOwnerError(f"candidate input count differs: {len(expected_inputs)}")
                _helper, _bytes, digest = _load_exact_helper(runner)
                print(
                    f"preflight ok: candidate={record['candidate_commit']}:{record['candidate_tree']} "
                    f"corpus=1690 exact_helper_sha256={digest} reference={args.reference_source_root}"
                )
                return 0
            build_exact_owner_harnesses(args, bootstrap_runner)
            return 0
        if args.command == "capture":
            return capture_exact_owner_role(args, bootstrap_runner)
        result = validate_proof(
            args.candidate_root,
            args.candidate_source_receipt,
            args.reference_source_root,
            args.role,
            args.build_receipt,
            args.capture_receipt,
            pathlib.Path(args.output),
            bootstrap_runner,
            args.dry_run,
        )
        return 0 if result else 1
    except (
        ExactOwnerError,
        adapter.AdapterError,
        adapter_runner.PublicationError,
        bootstrap_runner.PublicationError,
        bootstrap_runner.CleanupFailure,
        OSError,
        subprocess.SubprocessError,
    ) as error:
        print(f"W9 exact-owner proof failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
