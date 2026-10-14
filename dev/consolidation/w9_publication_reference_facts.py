from __future__ import annotations

import re
from collections.abc import Mapping
from dataclasses import dataclass
from types import MappingProxyType

from w9_publication_adapter import (
    AdapterError,
    _canonical_span_map,
    _canonical_span_set,
    _canonicalize_span,
    _checked_stream_stage_types as _stream_stage_types,
    _fields,
    _group_end,
    _map_entries,
    _parse_resolved_bindings,
    _split_top_level,
    _validate_source_span,
    _validate_selected_module_binding,
)


_FUNCTION_ID = re.compile(r"FunctionDefId\((\d+)\)\Z")
_TYPE_EXPR_ID = re.compile(r"TypeExprId\((\d+)\)\Z")
_IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")
_EFFECTS = {"Fs", "Net", "Process", "Env", "Time", "Error", "Io"}
_RECORD_FIELD_KINDS = {"computed", "path", "named", "shorthand", "spread"}


@dataclass(frozen=True)
class StageCallbackEdge:
    expression_id: int
    expression_span: str
    stage_id: int
    stage_span: str
    expression_kind: str
    stage_kind: str
    arguments: tuple[tuple[int, str, str | None, int], ...]


@dataclass(frozen=True)
class StageCallbackValidation:
    implicit_function_spans: frozenset[str]
    edges: Mapping[str, StageCallbackEdge]


@dataclass(frozen=True)
class _StructuredStreamStage:
    span: str
    kind: str
    inline_block: bool
    accepts_callable: bool


def _tuple_payload(value: str, expected: str, label: str) -> str:
    value = value.strip()
    opening = value.find("(")
    if opening < 0 or value[:opening].strip().rsplit("::", 1)[-1] != expected:
        raise AdapterError(f"{label} is not a {expected} Rust Debug tuple")
    closing = _group_end(value, opening)
    if closing != len(value) - 1:
        raise AdapterError(f"{label} has trailing content after {expected}")
    return value[opening + 1 : closing].strip()


def _name(value: str, label: str) -> str:
    payload = _tuple_payload(value, "Name", label)
    if len(payload) < 2 or payload[0] != '"' or payload[-1] != '"':
        raise AdapterError(f"{label} is not a quoted Name")
    spelling = payload[1:-1]
    if "\\" in spelling or not spelling:
        raise AdapterError(f"{label} has an unsupported escaped or empty name")
    return spelling


def _option_name(value: str, label: str) -> str | None:
    value = value.strip()
    if value == "None":
        return None
    return _name(_tuple_payload(value, "Some", label), label)


def _check_span_bounds(
    span: str, label: str, source_sizes: Mapping[int, int]
) -> tuple[int, int, int]:
    _, source_id, start, length = _canonicalize_span(span)
    if source_id not in source_sizes or start + length > source_sizes[source_id]:
        raise AdapterError(f"{label} span is outside its source: {span}")
    return source_id, start, length


def _vector_items(value: str, label: str) -> list[str]:
    value = value.strip()
    if not value.startswith("[") or _group_end(value, 0) != len(value) - 1:
        raise AdapterError(f"{label} is not a Rust Debug vector")
    return _split_top_level(value[1:-1], ",")


def _balanced_debug(value: str, label: str) -> None:
    value = value.strip()
    if not value:
        raise AdapterError(f"{label} is empty")
    _split_top_level(value, "\x00")


def _option_payload(value: str, label: str) -> str | None:
    value = value.strip()
    if value == "None":
        return None
    return _tuple_payload(value, "Some", label)


def _effect_list(value: str, label: str) -> None:
    payload = _option_payload(value, label)
    if payload is None:
        return
    effects = _vector_items(payload, label)
    if any(effect not in _EFFECTS for effect in effects) or len(set(effects)) != len(effects):
        raise AdapterError(f"{label} contains an invalid or duplicate effect")


def _parameter(value: str, label: str, *, typed: bool) -> str:
    fields = _fields(value, "CallableParamType")
    expected = {"name", "ty", "defaulted", "rest"}
    if set(fields) != expected:
        raise AdapterError(f"{label} has unexpected callable-parameter fields")
    name = _name(fields["name"], f"{label} name")
    if not _IDENTIFIER.fullmatch(name):
        raise AdapterError(f"{label} has an invalid parameter name")
    _balanced_debug(fields["ty"], f"{label} type")
    if fields["defaulted"] not in {"true", "false"} or fields["rest"] not in {"true", "false"}:
        raise AdapterError(f"{label} has a malformed parameter flag")
    if typed and (fields["defaulted"] != "false" or fields["rest"] != "false"):
        raise AdapterError(f"{label} typed-callable parameter has a default or rest flag")
    return name


def _callable_signature(value: str, label: str) -> tuple[bool, int]:
    typed = _fields(value, "TypedCallable")
    if set(typed) != {"pure", "sig"} or typed["pure"] not in {"true", "false"}:
        raise AdapterError(f"{label} has malformed TypedCallable fields")
    signature = _fields(typed["sig"], "CallableType")
    if set(signature) != {"params", "return_ty", "effects"}:
        raise AdapterError(f"{label} has unexpected CallableType fields")
    params = _vector_items(signature["params"], f"{label} parameters")
    names = [
        _parameter(parameter, f"{label} parameter {index}", typed=True)
        for index, parameter in enumerate(params)
    ]
    if len(set(names)) != len(names):
        raise AdapterError(f"{label} has duplicate callable parameter names")
    _balanced_debug(signature["return_ty"], f"{label} return type")
    _effect_list(signature["effects"], f"{label} effects")
    if typed["pure"] == "true" and signature["effects"] != "None":
        raise AdapterError(f"{label} pure callable has an effect bound")
    return typed["pure"] == "true", len(params)


def _checked_value_type(value: str, label: str) -> str:
    value = value.strip()
    if value in {"Pure", "Proc"}:
        return value
    payload = _tuple_payload(value, "Callable", label)
    _callable_signature(payload, label)
    return "Callable"


def _function_signature(value: str, label: str) -> dict[str, str]:
    payload = _option_payload(value, f"{label} signature")
    if payload is None:
        raise AdapterError(f"{label} has no checked signature")
    fields = _fields(payload, "CompactFunctionSig")
    expected = {
        "params",
        "parameter_schemas",
        "return_ty",
        "return_type_expr",
        "inferred_effects",
        "effects",
    }
    if set(fields) != expected:
        raise AdapterError(f"{label} signature has unexpected fields")
    params = _vector_items(fields["params"], f"{label} signature parameters")
    names = [
        _parameter(parameter, f"{label} signature parameter {index}", typed=False)
        for index, parameter in enumerate(params)
    ]
    if len(set(names)) != len(names):
        raise AdapterError(f"{label} signature has duplicate parameter names")
    schemas = _vector_items(fields["parameter_schemas"], f"{label} parameter schemas")
    if len(schemas) != len(params):
        raise AdapterError(f"{label} signature schema and parameter counts differ")
    for schema in schemas:
        _balanced_debug(schema, f"{label} parameter schema")
    _balanced_debug(fields["return_ty"], f"{label} signature return type")
    if _TYPE_EXPR_ID.fullmatch(fields["return_type_expr"]) is None:
        raise AdapterError(f"{label} signature has a malformed return type expression id")
    if fields["inferred_effects"] not in {"true", "false"}:
        raise AdapterError(f"{label} signature has a malformed inferred-effects flag")
    _effect_list(fields["effects"], f"{label} signature effects")
    return fields


def _target_declaration(
    target: str,
    functions: Mapping[str, str],
    source_sizes: Mapping[int, int],
    label: str,
) -> tuple[str, dict[str, str], dict[str, str]]:
    target_fields = _fields(target, "CheckedFunctionTarget")
    if set(target_fields) != {"definition", "namespace", "name"}:
        raise AdapterError(f"{label} target has unexpected fields")
    if _FUNCTION_ID.fullmatch(target_fields["definition"]) is None:
        raise AdapterError(f"{label} target has a malformed definition id")
    target_name = _name(target_fields["name"], f"{label} target name")
    if not _IDENTIFIER.fullmatch(target_name):
        raise AdapterError(f"{label} target name is not an identifier")
    target_namespace = _option_name(target_fields["namespace"], f"{label} target namespace")
    declaration = functions.get(target_fields["definition"])
    if declaration is None:
        raise AdapterError(f"{label} target has no checked declaration")
    declaration_fields = _fields(declaration, "CheckedFunctionDeclaration")
    expected = {
        "name",
        "namespace",
        "internal",
        "kind",
        "signature",
        "body",
        "parameter_spans",
    }
    if set(declaration_fields) != expected:
        raise AdapterError(f"{label} declaration has unexpected fields")
    declaration_name = _name(declaration_fields["name"], f"{label} declaration name")
    declaration_namespace = _option_name(
        declaration_fields["namespace"], f"{label} declaration namespace"
    )
    if target_name != declaration_name or target_namespace != declaration_namespace:
        raise AdapterError(f"{label} target disagrees with its checked declaration")
    if declaration_fields["internal"] not in {"true", "false"}:
        raise AdapterError(f"{label} declaration has a malformed internal flag")
    if declaration_fields["kind"] not in {"Pure", "Proc"}:
        raise AdapterError(f"{label} target is not a checked pure function or proc")
    signature = _function_signature(declaration_fields["signature"], label)
    body = declaration_fields["body"]
    _check_span_bounds(body, f"{label} declaration body", source_sizes)
    parameter_spans = _vector_items(declaration_fields["parameter_spans"], f"{label} parameter spans")
    if len(parameter_spans) != len(_vector_items(signature["params"], f"{label} signature parameters")):
        raise AdapterError(f"{label} declaration parameter span and signature counts differ")
    for span in parameter_spans:
        _check_span_bounds(span, f"{label} declaration parameter span", source_sizes)
    return declaration_fields["kind"], declaration_fields, signature


def _resolved_bindings(fields: Mapping[str, str]) -> dict[str, str]:
    resolved = _fields(fields["resolved_bindings"], "ResolvedBindings")
    expected = {
        "declarations",
        "declaration_sites",
        "expressions",
        "tails",
        "value_sites",
        "expression_namespaces",
    }
    if set(resolved) != expected:
        raise AdapterError("resolved binding fields are incomplete")
    return resolved


def _tuple_items(value: str, label: str) -> list[str]:
    value = value.strip()
    if not value.startswith("(") or _group_end(value, 0) != len(value) - 1:
        raise AdapterError(f"{label} is not a Rust Debug tuple")
    return _split_top_level(value[1:-1], ",")


def _value_site_keys(value: str) -> set[tuple[str, str]]:
    result = set()
    for key in _map_entries(value):
        pair = _tuple_items(key, "resolved value-site key")
        if len(pair) != 2:
            raise AdapterError("resolved value-site key has an unexpected tuple length")
        span = _canonicalize_span(pair[0])[0]
        name = _name(pair[1], "resolved value-site name")
        result.add((span, name))
    return result


def _validate_reference_namespace(
    expression_id: int,
    declaration: Mapping[str, str],
    expression_namespaces: Mapping[str, str],
    label: str,
) -> None:
    expression_key = f"ExprId({expression_id + 1})"
    if expression_key not in expression_namespaces:
        raise AdapterError(f"{label} has no checked expression namespace")
    expression_namespace = _option_name(
        expression_namespaces[expression_key], f"{label} expression namespace"
    )
    target_namespace = _option_name(declaration["namespace"], f"{label} target namespace")
    if expression_namespace != target_namespace:
        raise AdapterError(f"{label} target is outside its checked namespace")


def _exact_owner_tables(
    exact_owners: Mapping[str, object], source_sizes: Mapping[int, int]
) -> tuple[
    dict[int, tuple[str, str, str]],
    dict[int, dict[str, object]],
    dict[tuple[int, int], tuple[int, str, str]],
    dict[int, tuple[str, str, str]],
]:
    required = {"exprs", "records", "aliases", "expr_count", "record_field_count", "tails"}
    if not required <= set(exact_owners):
        raise AdapterError("exact owner facts are incomplete")
    expr_count = exact_owners["expr_count"]
    field_count = exact_owners["record_field_count"]
    if not isinstance(expr_count, int) or expr_count < 0:
        raise AdapterError("exact owner expression count is malformed")
    if not isinstance(field_count, int) or field_count < 0:
        raise AdapterError("exact owner record-field count is malformed")
    raw_exprs = exact_owners["exprs"]
    raw_records = exact_owners["records"]
    raw_aliases = exact_owners["aliases"]
    raw_tails = exact_owners["tails"]
    if not all(isinstance(value, Mapping) for value in (raw_exprs, raw_records, raw_aliases, raw_tails)):
        raise AdapterError("exact owner tables are malformed")
    exprs = raw_exprs
    records = raw_records
    aliases = raw_aliases
    tails = raw_tails
    if set(exprs) != set(range(expr_count)):
        raise AdapterError("exact owner expression ids do not cover the expression table")
    normalized_exprs: dict[int, tuple[str, str, str]] = {}
    for index, row in exprs.items():
        if type(index) is not int or not isinstance(row, tuple) or len(row) != 3:
            raise AdapterError("exact owner expression row is malformed")
        span, tag, name = row
        if not all(isinstance(part, str) for part in row) or not tag:
            raise AdapterError(f"exact owner expression row {index} is malformed")
        if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", tag):
            raise AdapterError(f"exact owner expression tag is malformed at {index}")
        if tag != "Ident" and name:
            raise AdapterError(f"non-identifier expression has an identifier spelling at {index}")
        _check_span_bounds(span, f"exact owner expression {index}", source_sizes)
        normalized_exprs[index] = (span, tag, name)
    if not isinstance(records, Mapping):
        raise AdapterError("exact owner record table is malformed")
    if set(records) != {index for index, (_, tag, _) in normalized_exprs.items() if tag == "Record"}:
        raise AdapterError("exact owner record rows do not cover the record expressions")
    normalized_records: dict[int, dict[str, object]] = {}
    global_field_indices: set[int] = set()
    shorthand_keys: set[tuple[int, int]] = set()
    for parent, record in records.items():
        if type(parent) is not int or parent not in normalized_exprs or not isinstance(record, Mapping):
            raise AdapterError("exact owner record parent is malformed")
        if set(record) != {"span", "count", "fields"}:
            raise AdapterError(f"exact owner record {parent} has unexpected fields")
        span, tag, _ = normalized_exprs[parent]
        if tag != "Record" or record["span"] != span:
            raise AdapterError(f"exact owner record {parent} disagrees with its expression")
        count, fields = record["count"], record["fields"]
        if type(count) is not int or count < 0 or not isinstance(fields, Mapping):
            raise AdapterError(f"exact owner record {parent} has malformed fields")
        if len(fields) != count:
            raise AdapterError(f"exact owner record {parent} field count differs")
        indexes = sorted(fields)
        if indexes and indexes != list(range(indexes[0], indexes[0] + count)):
            raise AdapterError(f"exact owner record {parent} field indexes are not contiguous")
        if not indexes and count:
            raise AdapterError(f"exact owner record {parent} has no field rows")
        normalized_fields: dict[
            int, tuple[str, str | tuple[str, ...], str, int | None, int | None]
        ] = {}
        for field_index, field in fields.items():
            if type(field_index) is not int or not isinstance(field, tuple) or len(field) != 5:
                raise AdapterError(f"exact owner record field {parent}:{field_index} is malformed")
            kind, name, field_span, key_expr, value_expr = field
            if (
                not isinstance(kind, str)
                or kind not in _RECORD_FIELD_KINDS
                or not isinstance(field_span, str)
            ):
                raise AdapterError(f"exact owner record field {parent}:{field_index} has malformed values")
            if field_index in global_field_indices or not 0 <= field_index < field_count:
                raise AdapterError(f"exact owner record field index is duplicated or out of bounds: {field_index}")
            global_field_indices.add(field_index)
            for child in (key_expr, value_expr):
                if child is not None and (not isinstance(child, int) or child not in normalized_exprs):
                    raise AdapterError(f"exact owner record field {parent}:{field_index} has an invalid child id")
            _check_span_bounds(field_span, f"exact owner record field {parent}:{field_index}", source_sizes)
            if kind == "computed":
                if not isinstance(name, str) or name or key_expr is None or value_expr is None:
                    raise AdapterError(f"exact owner computed field {parent}:{field_index} is malformed")
            elif kind == "path":
                if (
                    not isinstance(name, tuple)
                    or not name
                    or any(not isinstance(component, str) for component in name)
                    or key_expr is not None
                    or value_expr is None
                ):
                    raise AdapterError(f"exact owner path field {parent}:{field_index} is malformed")
            elif kind == "named":
                if not isinstance(name, str) or key_expr is not None or value_expr is None:
                    raise AdapterError(f"exact owner named field {parent}:{field_index} is malformed")
            elif kind == "shorthand":
                if (
                    not isinstance(name, str)
                    or not _IDENTIFIER.fullmatch(name)
                    or key_expr is not None
                    or value_expr is not None
                ):
                    raise AdapterError(f"exact owner shorthand field {parent}:{field_index} is malformed")
                shorthand_keys.add((parent, field_index))
            elif kind == "spread" and (
                not isinstance(name, str) or name or key_expr is not None or value_expr is None
            ):
                raise AdapterError(f"exact owner spread field {parent}:{field_index} is malformed")
            normalized_fields[field_index] = field
        normalized_records[parent] = {"span": span, "count": count, "fields": normalized_fields}
    if global_field_indices != set(range(field_count)):
        raise AdapterError("exact owner record-field ids do not cover the field table")
    if not isinstance(aliases, Mapping):
        raise AdapterError("exact owner alias table is malformed")
    normalized_aliases: dict[tuple[int, int], tuple[int, str, str]] = {}
    for key, alias in aliases.items():
        if (
            not isinstance(key, tuple)
            or len(key) != 2
            or not all(isinstance(index, int) for index in key)
            or not isinstance(alias, tuple)
            or len(alias) != 3
        ):
            raise AdapterError("exact owner shorthand alias is malformed")
        parent, field_index = key
        expr_id, span, name = alias
        record = normalized_records.get(parent)
        field = record["fields"].get(field_index) if record is not None else None
        if field is None or field[0] != "shorthand":
            raise AdapterError(f"exact owner alias has no shorthand field: {key}")
        if not isinstance(expr_id, int) or expr_id not in normalized_exprs or not isinstance(span, str) or not isinstance(name, str):
            raise AdapterError(f"exact owner alias values are malformed: {key}")
        field_name, field_span = field[1], field[2]
        expr_span, expr_tag, expr_name = normalized_exprs[expr_id]
        if (span, name) != (field_span, field_name) or (expr_span, expr_tag, expr_name) != (span, "Ident", name):
            raise AdapterError(f"exact owner alias disagrees with its shorthand field or identifier: {key}")
        matching = [
            index for index, (candidate_span, candidate_tag, candidate_name) in normalized_exprs.items()
            if (candidate_span, candidate_tag, candidate_name) == (span, "Ident", name)
        ]
        if matching != [expr_id]:
            raise AdapterError(f"exact owner alias does not uniquely identify its shorthand expression: {key}")
        normalized_aliases[key] = alias
    if set(normalized_aliases) != shorthand_keys:
        raise AdapterError("exact owner shorthand aliases do not cover all shorthand fields")
    normalized_tails: dict[int, tuple[str, str, str]] = {}
    for statement, tail in tails.items():
        if (
            type(statement) is not int
            or statement < 0
            or not isinstance(tail, tuple)
            or len(tail) != 3
        ):
            raise AdapterError("exact owner tail row is malformed")
        span, name, kind = tail
        if not all(isinstance(value, str) for value in tail) or kind != "stmt:TailBareIdent" or not _IDENTIFIER.fullmatch(name):
            raise AdapterError(f"exact owner tail row {statement} is malformed")
        _check_span_bounds(span, f"exact owner tail {statement}", source_sizes)
        normalized_tails[statement] = tail
    return normalized_exprs, normalized_records, normalized_aliases, normalized_tails


def _selected_call_expression_spans(
    selected_spans: set[str],
    expressions: Mapping[int, tuple[str, str, str]],
    source_owners: Mapping[str, set[str]],
) -> set[str]:
    calls_by_span: dict[str, list[int]] = {}
    for expression_id, (span, tag, _name) in expressions.items():
        if tag == "Call":
            calls_by_span.setdefault(span, []).append(expression_id)
    call_spans: set[str] = set()
    for span in selected_spans:
        call_ids = calls_by_span.get(span, [])
        has_call_owner = "expr:Call" in source_owners.get(span, set())
        if call_ids and not has_call_owner:
            raise AdapterError(f"selected Call AST site has no exact expr:Call owner: {span}")
        if has_call_owner:
            if len(call_ids) != 1:
                raise AdapterError(f"selected expr:Call owner has no unique exact AST identity: {span}")
            call_spans.add(span)
    return call_spans


def _record_field_type(parent_type: str, field_name: str, label: str) -> str:
    if parent_type.startswith("Record("):
        fields = _map_entries(_tuple_payload(parent_type, "Record", label))
        name_debug = f'Name("{field_name}")'
        if name_debug not in fields:
            raise AdapterError(f"{label} is absent from its checked Record type")
        return fields[name_debug]
    if parent_type.startswith("Map("):
        parts = _split_top_level(_tuple_payload(parent_type, "Map", label), ",")
        if len(parts) != 2:
            raise AdapterError(f"{label} has a malformed checked Map type")
        _balanced_debug(parts[0], f"{label} map key type")
        _balanced_debug(parts[1], f"{label} map value type")
        return parts[1]
    if parent_type.startswith("Set("):
        parts = _split_top_level(_tuple_payload(parent_type, "Set", label), ",")
        if len(parts) != 1:
            raise AdapterError(f"{label} has a malformed checked Set type")
        _balanced_debug(parts[0], f"{label} set element type")
        return parts[0]
    raise AdapterError(f"{label} has no checked Record, Map, or Set parent type")


def _validate_reference_type(
    value: str,
    kind: str,
    label: str,
) -> None:
    value_kind = _checked_value_type(value, label)
    if value_kind == "Callable":
        pure, _ = _callable_signature(_tuple_payload(value, "Callable", label), label)
        expected_kind = "Pure" if pure else "Proc"
    else:
        expected_kind = value_kind
    if expected_kind != kind:
        raise AdapterError(f"{label} checked type disagrees with its declaration kind")


def expected_stage_callback_position(stage_kind: str) -> str:
    return "Statement" if stage_kind in {"Each", "Tee"} else "Value"


def _stage_callback_references(
    exact_owners: Mapping[str, object],
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
    expressions: Mapping[int, tuple[str, str, str]],
) -> dict[str, StageCallbackEdge]:
    if "stage_callbacks" not in exact_owners:
        return {}
    raw_stages = exact_owners.get("structured_stages")
    raw_arguments = exact_owners.get("stage_arguments")
    raw_callbacks = exact_owners["stage_callbacks"]
    if not all(isinstance(value, Mapping) for value in (raw_stages, raw_arguments, raw_callbacks)):
        raise AdapterError("exact stage-callback proof tables are incomplete")
    stages = raw_stages
    arguments = raw_arguments
    callbacks = raw_callbacks
    if set(stages) != set(range(len(stages))) or set(arguments) != set(stages):
        raise AdapterError("exact stage-callback argument rows do not cover the stage table")
    normalized_stages: dict[int, _StructuredStreamStage] = {}
    stages_by_span: dict[str, int] = {}
    normalized_arguments: dict[int, tuple[tuple[int, str, str | None, int], ...]] = {}
    for stage_id, row in stages.items():
        if type(stage_id) is not int or not isinstance(row, tuple) or len(row) != 4:
            raise AdapterError("exact structured stream-stage row is malformed")
        raw_span, kind, inline_block, accepts_callable = row
        if (
            not isinstance(raw_span, str)
            or not isinstance(kind, str)
            or re.fullmatch(r"[A-Z][A-Za-z0-9]*", kind) is None
            or type(inline_block) is not bool
            or type(accepts_callable) is not bool
        ):
            raise AdapterError(f"exact structured stream-stage row is malformed: {stage_id}")
        _check_span_bounds(raw_span, f"exact structured stream stage {stage_id}", source_sizes)
        span = _canonicalize_span(raw_span)[0]
        if span in stages_by_span:
            raise AdapterError(f"exact structured stream-stage span is ambiguous: {span}")
        stages_by_span[span] = stage_id
        normalized_stages[stage_id] = _StructuredStreamStage(
            span, kind, inline_block, accepts_callable
        )
        raw_stage_arguments = arguments[stage_id]
        if not isinstance(raw_stage_arguments, tuple):
            raise AdapterError(f"exact stream-stage arguments are malformed: {stage_id}")
        normalized_stage_arguments = []
        for expected_index, argument in enumerate(raw_stage_arguments):
            if not isinstance(argument, tuple) or len(argument) != 4:
                raise AdapterError(f"exact stream-stage argument is malformed: {stage_id}")
            index, argument_kind, name, expression_id = argument
            if (
                type(index) is not int
                or index != expected_index
                or not isinstance(argument_kind, str)
                or argument_kind not in {"Positional", "Named", "NamedSpread", "Splice"}
                or (name is not None and not isinstance(name, str))
                or type(expression_id) is not int
                or expression_id not in expressions
            ):
                raise AdapterError(f"exact stream-stage argument is malformed: {stage_id}:{expected_index}")
            if argument_kind == "Named" and (
                name is None or _IDENTIFIER.fullmatch(name) is None
            ):
                raise AdapterError(f"exact named stream-stage argument is malformed: {stage_id}:{index}")
            if argument_kind != "Named" and name is not None:
                raise AdapterError(f"non-named stream-stage argument has a name: {stage_id}:{index}")
            normalized_stage_arguments.append((index, argument_kind, name, expression_id))
        normalized_arguments[stage_id] = tuple(normalized_stage_arguments)
    if any(type(stage_id) is not int or stage_id not in stages for stage_id in callbacks):
        raise AdapterError("exact stage-callback row refers to an unknown stage")
    result: dict[str, StageCallbackEdge] = {}
    callback_expr_ids: set[int] = set()
    for stage_id, expression_id in callbacks.items():
        stage = normalized_stages[stage_id]
        stage_arguments = normalized_arguments[stage_id]
        if type(expression_id) is not int or expression_id not in expressions:
            raise AdapterError(f"exact stage-callback row has an unknown expression: {stage_id}")
        if expression_id in callback_expr_ids:
            raise AdapterError("exact stage-callback expression is linked to multiple stages")
        callback_expr_ids.add(expression_id)
        if not stage.accepts_callable or stage.inline_block:
            raise AdapterError(f"exact stage does not accept this named callback: {stage.span}")
        if any(kind in {"NamedSpread", "Splice"} for _, kind, _, _ in stage_arguments):
            raise AdapterError(f"exact stage callback has ambiguous spread arguments: {stage.span}")
        matches = [argument for argument in stage_arguments if argument[3] == expression_id]
        if len(matches) != 1:
            raise AdapterError(f"exact stage callback has no unique direct argument link: {stage.span}")
        argument_index, argument_kind, argument_name, _ = matches[0]
        if argument_kind == "Positional":
            if argument_index != 0 or any(
                kind == "Named" and name == "block"
                for _, kind, name, _ in stage_arguments
            ):
                raise AdapterError(f"exact positional stage callback is ambiguous: {stage.span}")
        elif argument_kind == "Named":
            if argument_name != "block" or sum(
                kind == "Named" and name == "block"
                for _, kind, name, _ in stage_arguments
            ) != 1 or any(
                index < argument_index and kind == "Positional"
                for index, kind, _, _ in stage_arguments
            ):
                raise AdapterError(f"exact named stage callback is ambiguous: {stage.span}")
        else:
            raise AdapterError(f"exact stage callback is not a direct callable argument: {stage.span}")
        callback_span, callback_kind, _ = expressions[expression_id]
        if callback_kind not in {"Ident", "Field"}:
            raise AdapterError(f"exact stage callback is not a named function expression: {stage.span}")
        canonical_callback_span = _canonicalize_span(callback_span)[0]
        _validate_source_span(
            canonical_callback_span,
            "exact stage callback expression",
            source_sizes,
            source_owners,
            {f"expr:{callback_kind}"},
        )
        if canonical_callback_span in result:
            raise AdapterError(f"exact stage callback identifier is ambiguous: {canonical_callback_span}")
        result[canonical_callback_span] = StageCallbackEdge(
            expression_id=expression_id,
            expression_span=canonical_callback_span,
            stage_id=stage_id,
            stage_span=stage.span,
            expression_kind=callback_kind,
            stage_kind=stage.kind,
            arguments=stage_arguments,
        )
    return result


def validate_stage_callback_edges(
    *,
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
    exact_owners: Mapping[str, object],
) -> Mapping[str, StageCallbackEdge]:
    expressions, _, _, _ = _exact_owner_tables(exact_owners, source_sizes)
    edges = _stage_callback_references(
        exact_owners, source_sizes, source_owners, expressions
    )
    return MappingProxyType(edges)


def validate_function_reference_targets(
    *,
    fields: Mapping[str, str],
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
    exact_owners: Mapping[str, object],
) -> StageCallbackValidation:
    selected = _canonical_span_map(fields["selected_function_calls"], "selected function reference")
    exprs, records, aliases, tails = _exact_owner_tables(exact_owners, source_sizes)
    stage_callbacks = _stage_callback_references(
        exact_owners, source_sizes, source_owners, exprs
    )
    if not selected:
        return StageCallbackValidation(frozenset(), MappingProxyType(stage_callbacks))
    call_spans = _selected_call_expression_spans(set(selected), exprs, source_owners)
    reference_spans = set(selected) - call_spans
    if not reference_spans:
        return StageCallbackValidation(frozenset(), MappingProxyType(stage_callbacks))
    checked_stage_types: dict[str, tuple[str | None, str, str]] | None = None
    checked_declarations = _fields(fields["checked_declarations"], "CheckedDeclarations")
    functions = _map_entries(checked_declarations["functions"])
    expression_types = _canonical_span_map(fields["expr_types"], "checked expression type")
    resolved_bindings = _resolved_bindings(fields)
    resolved_expressions = _map_entries(resolved_bindings["expressions"])
    expression_namespaces = _map_entries(resolved_bindings["expression_namespaces"])
    value_site_keys = _value_site_keys(resolved_bindings["value_sites"])
    statement_positions = _canonical_span_map(
        fields.get("statement_positions", "{}"), "checked statement position"
    )
    static_calls = _canonical_span_set(
        fields.get("statically_resolved_call_spans", "{}"), "statically resolved call"
    )
    validated_stage_callbacks: set[str] = set()
    ident_rows: dict[str, list[tuple[int, str]]] = {}
    for expr_id, (span, tag, name) in exprs.items():
        if tag == "Ident":
            ident_rows.setdefault(span, []).append((expr_id, name))
    aliases_by_span: dict[str, list[tuple[tuple[int, int], tuple[int, str, str]]]] = {}
    for key, alias in aliases.items():
        aliases_by_span.setdefault(alias[1], []).append((key, alias))
    tails_by_span: dict[str, list[tuple[int, str, str]]] = {}
    for statement, (span, name, _kind) in tails.items():
        tails_by_span.setdefault(span, []).append((statement, name, _kind))
    exact_field_accesses = exact_owners.get("field_accesses", {})
    if not isinstance(exact_field_accesses, Mapping):
        raise AdapterError("exact field-access owner facts are malformed")
    parsed_resolved_bindings = None
    for span in sorted(reference_spans):
        target_kind, target_declaration, signature = _target_declaration(
            selected[span], functions, source_sizes, f"selected function reference at {span}"
        )
        shorthand_matches = aliases_by_span.get(span, [])
        tail_matches = tails_by_span.get(span, [])
        identifiers = ident_rows.get(span, [])
        if shorthand_matches and tail_matches:
            raise AdapterError(f"selected function reference has ambiguous AST site: {span}")
        stage_callback = stage_callbacks.get(span)
        if stage_callback is not None and (shorthand_matches or tail_matches):
            raise AdapterError(f"selected stage callback has an ambiguous AST site: {span}")
        if stage_callback is not None:
            expression_id = stage_callback.expression_id
            expression_span, expression_kind, expression_name = exprs[expression_id]
            target_fields = _fields(selected[span], "CheckedFunctionTarget")
            target_name = _name(
                target_fields["name"], f"selected stage callback target at {span}"
            )
            if expression_span != span:
                raise AdapterError(f"selected stage callback span disagrees with its exact AST site: {span}")
            if expression_kind == "Ident":
                if expression_name != target_name or ident_rows.get(span) != [(expression_id, expression_name)]:
                    raise AdapterError(f"selected stage callback name disagrees with its target: {span}")
                expression_key = f"ExprId({expression_id + 1})"
                if expression_key in resolved_expressions:
                    raise AdapterError(f"selected stage callback resolves to a value binding: {span}")
                _validate_reference_namespace(
                    expression_id,
                    target_declaration,
                    expression_namespaces,
                    f"selected stage callback at {span}",
                )
                callback_owners = {"expr:Ident"}
            else:
                field_access = exact_field_accesses.get(expression_id)
                if (
                    not isinstance(field_access, tuple)
                    or len(field_access) != 2
                    or type(field_access[0]) is not int
                    or field_access[0] not in exprs
                    or field_access[1] != target_name
                ):
                    raise AdapterError(f"selected stage callback has no exact target field: {span}")
                base_id, member_name = field_access
                base_span, base_kind, base_name = exprs[base_id]
                if base_kind != "Ident" or not base_name or member_name != target_name:
                    raise AdapterError(f"selected stage callback field has no exact module identifier: {span}")
                if parsed_resolved_bindings is None:
                    parsed_resolved_bindings = _parse_resolved_bindings(fields["resolved_bindings"])
                _validate_selected_module_binding(
                    span,
                    target_fields,
                    base_id,
                    base_span,
                    base_name,
                    parsed_resolved_bindings,
                    source_sizes,
                    source_owners,
                    target_may_be_reexported=True,
                )
                callback_owners = {"expr:Field"}
            _validate_source_span(
                span, "selected stage callback", source_sizes, source_owners, callback_owners
            )
            if checked_stage_types is None:
                checked_stage_types = _stream_stage_types(
                    fields.get("stream_stage_types", "{}")
                )
                for checked_stage_span in checked_stage_types:
                    _check_span_bounds(
                        checked_stage_span,
                        "checked stream-stage",
                        source_sizes,
                    )
            if stage_callback.stage_span not in checked_stage_types:
                raise AdapterError(f"selected stage callback has no checked stream-stage type: {span}")
            if span not in static_calls:
                raise AdapterError(f"selected stage callback has no static call fact: {span}")
            expected_position = expected_stage_callback_position(stage_callback.stage_kind)
            if statement_positions.get(span) != expected_position:
                raise AdapterError(
                    f"selected stage callback has no {expected_position} statement position: {span}"
                )
            checked_type = expression_types.get(span)
            if checked_type is None:
                raise AdapterError(f"selected stage callback has no checked result type: {span}")
            if checked_type != signature["return_ty"]:
                raise AdapterError(
                    f"selected stage callback result type disagrees with its checked declaration: {span}"
                )
            validated_stage_callbacks.add(span)
            continue
        if shorthand_matches:
            if len(shorthand_matches) != 1 or len(identifiers) != 1:
                raise AdapterError(f"selected Record shorthand has ambiguous expression identity: {span}")
            (parent, field_index), (expr_id, alias_span, name) = shorthand_matches[0]
            if identifiers[0] != (expr_id, name):
                raise AdapterError(f"selected Record shorthand identity disagrees at {span}")
            record = records[parent]
            field = record["fields"][field_index]
            if field[1] != name or field[2] != alias_span or name != _name(
                _fields(selected[span], "CheckedFunctionTarget")["name"],
                f"selected Record shorthand target at {span}",
            ):
                raise AdapterError(f"selected Record shorthand name disagrees with its target at {span}")
            if (alias_span, name) in value_site_keys:
                raise AdapterError(f"selected Record shorthand resolves to a value binding: {span}")
            _validate_reference_namespace(
                parent,
                target_declaration,
                expression_namespaces,
                f"selected Record shorthand at {span}",
            )
            _validate_source_span(
                span, "selected Record shorthand", source_sizes, source_owners, {"expr:Ident"}
            )
            parent_span = record["span"]
            _validate_source_span(
                parent_span, "selected Record shorthand parent", source_sizes, source_owners, {"expr:Record"}
            )
            parent_type = expression_types.get(parent_span)
            if parent_type is None:
                raise AdapterError(f"selected Record shorthand has no checked parent type: {span}")
            field_type = _record_field_type(
                parent_type, name, f"selected Record shorthand at {span}"
            )
            direct_field_type = expression_types.get(span)
            if direct_field_type is not None and direct_field_type != field_type:
                raise AdapterError(f"selected Record shorthand type disagrees with its parent at {span}")
            _validate_reference_type(
                field_type, target_kind, f"selected Record shorthand at {span}"
            )
            continue
        if tail_matches:
            if len(tail_matches) != 1 or identifiers:
                raise AdapterError(f"selected TailBareIdent has ambiguous expression identity: {span}")
            _, name, _ = tail_matches[0]
            target_name = _name(
                _fields(selected[span], "CheckedFunctionTarget")["name"],
                f"selected tail target at {span}",
            )
            if name != target_name:
                raise AdapterError(f"selected TailBareIdent name disagrees with its target at {span}")
            _validate_source_span(
                span, "selected TailBareIdent", source_sizes, source_owners, {"stmt:TailBareIdent"}
            )
            _validate_reference_type(
                expression_types.get(span, ""),
                target_kind,
                f"selected TailBareIdent at {span}",
            )
            continue
        if not identifiers:
            raise AdapterError(f"selected function reference has no exact AST site: {span}")
        if len(identifiers) != 1:
            raise AdapterError(f"selected ExprIdent has ambiguous expression identity: {span}")
        _, name = identifiers[0]
        target_name = _name(
            _fields(selected[span], "CheckedFunctionTarget")["name"],
            f"selected ExprIdent target at {span}",
        )
        if name != target_name:
            raise AdapterError(f"selected ExprIdent name disagrees with its target at {span}")
        expression_id = identifiers[0][0]
        expression_key = f"ExprId({expression_id + 1})"
        if expression_key in resolved_expressions:
            raise AdapterError(f"selected ExprIdent resolves to a value binding: {span}")
        _validate_reference_namespace(
            expression_id,
            target_declaration,
            expression_namespaces,
            f"selected ExprIdent at {span}",
        )
        _validate_source_span(span, "selected ExprIdent", source_sizes, source_owners, {"expr:Ident"})
        checked_type = expression_types.get(span, target_kind)
        _validate_reference_type(checked_type, target_kind, f"selected ExprIdent at {span}")
    return StageCallbackValidation(
        frozenset(validated_stage_callbacks), MappingProxyType(stage_callbacks)
    )
