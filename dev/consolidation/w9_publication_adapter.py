from __future__ import annotations

import re
from typing import Mapping


ADAPTER_SCHEMA = "w9-publication-adapter-v1"
SOURCE_SCHEMAS = {"legacy", "current"}
PATHS = {
    "legacy": (
        (("CheckOutput", "prepared_constants"), ("PreparedConstants", "global_bindings")),
        (("CheckOutput", "prepared_constants"), ("PreparedConstants", "tail_bindings")),
    ),
    "current": (
        (("CheckOutput", "selected_function_calls"),),
        (("CheckOutput", "checked_control_flow"),),
    ),
}
SPAN = re.compile(r"Span\{source_id:SourceId\((\d+)\),start:(\d+),len:(\d+)\}\Z")
OWNER_SPAN = re.compile(
    r"Span\s*\{\s*source_id:\s*SourceId\((\d+)\),\s*start:\s*(\d+),\s*len:\s*(\d+)\s*\}\Z"
)
CONTROL_FLOW_OWNERS = {
    "builtin_range_calls": {"expr:Call"},
    "cardinality_preserving_collection_calls": {"expr:Call"},
    "propagating_error_spans": {"expr"},
    "latent_stream_error_spans": {"expr"},
    "fallible_pipeline_stage_spans": {"stage:Range", "stage:Repeat"},
    "propagating_stage_callback_spans": {"stage"},
    "exhaustive_match_spans": {"expr:Match", "stmt:Match"},
}
class AdapterError(ValueError):
    pass


def _canonical_owner_span(value: str, line_number: int) -> str:
    match = OWNER_SPAN.fullmatch(value)
    if match is None:
        raise AdapterError(f"owner index line {line_number}: malformed span")
    source_id, start, length = match.groups()
    return f"Span{{source_id:SourceId({source_id}),start:{start},len:{length}}}"


def parse_owner_index(
    contents: str,
) -> tuple[
    dict[int, int],
    dict[str, set[str]],
    dict[str, tuple[str, str, str]],
    dict[str, tuple[str, str]],
]:
    source_sizes: dict[int, int] = {}
    source_owners: dict[str, set[str]] = {}
    tail_statement_owners: dict[str, tuple[str, str, str]] = {}
    statement_owners: dict[str, tuple[str, str]] = {}
    seen_owners: set[tuple[str, str]] = set()
    for line_number, line in enumerate(contents.splitlines(), 1):
        fields = line.split("\t")
        if fields[0] == "source" and len(fields) == 3:
            source = re.fullmatch(r"SourceId\((\d+)\)", fields[1])
            size = re.fullmatch(r"\d+", fields[2])
            if source is None or size is None:
                raise AdapterError(f"owner index line {line_number}: malformed source row")
            source_id, byte_length = int(source.group(1)), int(size.group())
            if source_id in source_sizes:
                raise AdapterError(f"owner index line {line_number}: duplicate source id")
            source_sizes[source_id] = byte_length
        elif fields[0] == "owner" and len(fields) == 3:
            span, kind = _canonical_owner_span(fields[1], line_number), fields[2]
            if re.fullmatch(r"(?:expr|stmt|stage):[A-Za-z][A-Za-z0-9_]*", kind) is None:
                raise AdapterError(f"owner index line {line_number}: malformed owner row")
            owner = (span, kind)
            if owner in seen_owners:
                raise AdapterError(f"owner index line {line_number}: duplicate owner row")
            seen_owners.add(owner)
            source_owners.setdefault(span, set()).add(kind)
        elif fields[0] == "stmt" and len(fields) == 4:
            statement_id, raw_span, kind = fields[1:]
            span = _canonical_owner_span(raw_span, line_number)
            if (
                re.fullmatch(r"StmtId\(\d+\)", statement_id) is None
                or re.fullmatch(r"stmt:[A-Za-z][A-Za-z0-9_]*", kind) is None
            ):
                raise AdapterError(f"owner index line {line_number}: malformed statement row")
            if statement_id in statement_owners:
                raise AdapterError(f"owner index line {line_number}: duplicate statement id")
            statement_owners[statement_id] = (span, kind)
        elif fields[0] == "tail" and len(fields) == 5:
            statement_id, raw_span, name, kind = fields[1:]
            span = _canonical_owner_span(raw_span, line_number)
            if (
                re.fullmatch(r"StmtId\(\d+\)", statement_id) is None
                or re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name) is None
                or kind != "stmt:TailBareIdent"
            ):
                raise AdapterError(f"owner index line {line_number}: malformed tail row")
            if statement_id in tail_statement_owners:
                raise AdapterError(f"owner index line {line_number}: duplicate tail statement")
            tail_statement_owners[statement_id] = (span, name, kind)
        else:
            raise AdapterError(f"owner index line {line_number}: unrecognized row")
    if not source_sizes or set(source_sizes) != set(range(max(source_sizes) + 1)):
        raise AdapterError("owner index source ids are empty or nonsequential")
    for span in source_owners:
        source_id, start, length = map(int, SPAN.fullmatch(span).groups())
        if source_id not in source_sizes or start + length > source_sizes[source_id]:
            raise AdapterError(f"owner index span is outside its source {span}")
    for statement_id, (span, _name, _kind) in tail_statement_owners.items():
        if "stmt:TailBareIdent" not in source_owners.get(span, set()):
            raise AdapterError(f"owner index tail has no TailBareIdent node: {statement_id}")
        if statement_owners.get(statement_id) != (span, "stmt:TailBareIdent"):
            raise AdapterError(f"owner index tail disagrees with its statement id: {statement_id}")
    for statement_id, (span, kind) in statement_owners.items():
        if kind not in source_owners.get(span, set()):
            raise AdapterError(f"owner index statement has no matching AST owner: {statement_id}")
    return source_sizes, source_owners, tail_statement_owners, statement_owners


def _quoted_end(value: str, start: int) -> int | None:
    quote = value[start]
    if quote not in {"\"", "'"}:
        return None
    index = start + 1
    while index < len(value):
        if value[index] == "\\":
            index += 2
        elif value[index] == quote:
            return index + 1
        elif quote == "'" and value[index].isspace():
            return None
        else:
            index += 1
    return None


def _group_end(value: str, start: int) -> int | None:
    closes = {"{": "}", "[": "]", "(": ")"}
    opening = value[start] if start < len(value) else ""
    if opening not in closes:
        return None
    stack = [closes[opening]]
    index = start + 1
    while index < len(value):
        quoted = _quoted_end(value, index)
        if quoted is not None:
            index = quoted
            continue
        character = value[index]
        if character in closes:
            stack.append(closes[character])
        elif character in "}])":
            if not stack or stack.pop() != character:
                raise AdapterError("unbalanced Rust Debug value")
            if not stack:
                return index
        index += 1
    raise AdapterError("unterminated Rust Debug value")


def _struct_parts(value: str, expected_type: str) -> tuple[str, list[str]]:
    index = 0
    opening = None
    while index < len(value):
        quoted = _quoted_end(value, index)
        if quoted is not None:
            index = quoted
            continue
        if value[index] == "{":
            opening = index
            break
        index += 1
    if opening is None:
        raise AdapterError(f"expected named {expected_type} Debug struct")
    closing = _group_end(value, opening)
    if value[closing + 1 :].strip():
        raise AdapterError(f"trailing content after {expected_type} Debug struct")
    type_name = value[:opening].strip().rsplit("::", 1)[-1]
    if type_name != expected_type:
        raise AdapterError(f"expected {expected_type} Debug struct, found {type_name!r}")
    body = value[opening + 1 : closing]
    entries: list[str] = []
    stack: list[str] = []
    closes = {"{": "}", "[": "]", "(": ")"}
    start = 0
    index = 0
    while index < len(body):
        quoted = _quoted_end(body, index)
        if quoted is not None:
            index = quoted
            continue
        character = body[index]
        if character in closes:
            stack.append(closes[character])
        elif character in "}])":
            if not stack or stack.pop() != character:
                raise AdapterError("unbalanced Rust Debug field value")
        elif character == "," and not stack:
            entry = body[start:index].strip()
            if entry:
                entries.append(entry)
            start = index + 1
        index += 1
    tail = body[start:].strip()
    if tail:
        entries.append(tail)
    return value[:opening].strip(), entries


def _field(entry: str) -> tuple[str, str]:
    name, separator, value = entry.partition(":")
    name = name.strip()
    if not separator or not name.isidentifier() or not value.strip():
        raise AdapterError(f"expected Rust Debug struct field, found {entry!r}")
    return name, value.strip()


def _fields(value: str, expected_type: str) -> dict[str, str]:
    _, entries = _struct_parts(value, expected_type)
    fields = dict(_field(entry) for entry in entries)
    if len(fields) != len(entries):
        raise AdapterError(f"duplicate {expected_type} Debug field")
    return fields


def _split_top_level(value: str, separator: str) -> list[str]:
    parts = []
    stack: list[str] = []
    closes = {"{": "}", "[": "]", "(": ")"}
    start = 0
    index = 0
    while index < len(value):
        quoted = _quoted_end(value, index)
        if quoted is not None:
            index = quoted
            continue
        character = value[index]
        if character in closes:
            stack.append(closes[character])
        elif character in "}])":
            if not stack or stack.pop() != character:
                raise AdapterError("unbalanced Rust Debug collection")
        elif character == separator and not stack:
            parts.append(value[start:index].strip())
            start = index + 1
        index += 1
    if stack:
        raise AdapterError("unterminated Rust Debug collection")
    parts.append(value[start:].strip())
    return [part for part in parts if part]


def _split_first_top_level(value: str, separator: str) -> tuple[str, str] | None:
    stack: list[str] = []
    closes = {"{": "}", "[": "]", "(": ")"}
    index = 0
    while index < len(value):
        quoted = _quoted_end(value, index)
        if quoted is not None:
            index = quoted
            continue
        character = value[index]
        if character in closes:
            stack.append(closes[character])
        elif character in "}])":
            if not stack or stack.pop() != character:
                raise AdapterError("unbalanced Rust Debug collection")
        elif character == separator and not stack:
            return value[:index].strip(), value[index + 1 :].strip()
        index += 1
    if stack:
        raise AdapterError("unterminated Rust Debug collection")
    return None


def _map_entries(value: str) -> dict[str, str]:
    value = value.strip()
    if not value.startswith("{") or _group_end(value, 0) != len(value) - 1:
        raise AdapterError("expected Rust Debug map")
    entries = {}
    for entry in _split_top_level(value[1:-1], ","):
        pair = _split_first_top_level(entry, ":")
        if pair is None or pair[0] in entries:
            raise AdapterError("malformed or duplicate Rust Debug map entry")
        entries[pair[0]] = pair[1]
    return entries


def _set_items(value: str) -> set[str]:
    value = value.strip()
    if not value.startswith("{") or _group_end(value, 0) != len(value) - 1:
        raise AdapterError("expected Rust Debug set")
    items = _split_top_level(value[1:-1], ",")
    if len(set(items)) != len(items):
        raise AdapterError("duplicate Rust Debug set entry")
    return set(items)


def _pair_key(value: str) -> tuple[str, str]:
    value = value.strip()
    if not value.startswith("(") or _group_end(value, 0) != len(value) - 1:
        raise AdapterError("expected namespace and name pair")
    parts = _split_top_level(value[1:-1], ",")
    if len(parts) != 2:
        raise AdapterError("expected namespace and name pair")
    return parts[0], parts[1]


def _legacy_redundancy(
    fields: Mapping[str, str],
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
    tail_statement_owners: Mapping[str, tuple[str, str, str]],
    statement_owners: Mapping[str, tuple[str, str]],
) -> None:
    prepared = _fields(fields["prepared_constants"], "PreparedConstants")
    bindings = _fields(fields["resolved_bindings"], "ResolvedBindings")
    values = _map_entries(prepared["binding_values"])
    declarations = _map_entries(bindings["declarations"])
    global_bindings = _map_entries(prepared["global_bindings"])
    derived_globals = {}
    constant_bindings = {}
    for binding_id, declaration in declarations.items():
        declaration_fields = _fields(declaration, "BindingDeclaration")
        constant = re.fullmatch(r"Constant\((StmtId\(\d+\))\)", declaration_fields["kind"])
        if constant:
            declaration_statement = constant.group(1)
            if statement_owners.get(declaration_statement) != (
                declaration_fields["span"], "stmt:Const"
            ):
                raise AdapterError(f"legacy constant has no matching AST declaration: {declaration_statement}")
            constant_bindings[binding_id] = declaration_statement
        if declaration_fields["global"] == "true" and constant:
            if binding_id not in values:
                continue
            key = (declaration_fields["namespace"], declaration_fields["name"])
            if key in derived_globals:
                raise AdapterError("duplicate legacy global declaration")
            derived_globals[key] = values[binding_id]
    actual_globals = {_pair_key(key): value for key, value in global_bindings.items()}
    if actual_globals != derived_globals:
        raise AdapterError("legacy global bindings differ from declaration and binding-value facts")
    tail_values = _map_entries(prepared["values"])
    tails = _map_entries(bindings["tails"])
    tail_bindings = _map_entries(prepared["tail_bindings"])
    derived_tails = {}
    for statement_id, binding_id in tails.items():
        owner = tail_statement_owners.get(statement_id)
        declaration = declarations.get(binding_id)
        if owner is None or declaration is None:
            raise AdapterError(f"legacy tail has no statement owner or declaration: {statement_id}")
        span, name, owner_kind = owner
        if name == "" or owner_kind != "stmt:TailBareIdent" or not SPAN.fullmatch(span):
            raise AdapterError(f"malformed legacy tail owner for {statement_id}")
        _validate_source_span(
            span, "legacy tail statement", source_sizes, source_owners, {"stmt:TailBareIdent"}
        )
        declaration_fields = _fields(declaration, "BindingDeclaration")
        declaration_name = re.fullmatch(r'Name\("(.*)"\)', declaration_fields["name"])
        if declaration_name is None or name != declaration_name.group(1):
            raise AdapterError(f"legacy tail owner disagrees with its binding declaration: {statement_id}")
        if binding_id not in constant_bindings:
            continue
        if binding_id not in values:
            continue
        expression = values[binding_id]
        if expression not in tail_values:
            continue
        if span in derived_tails:
            raise AdapterError(f"duplicate legacy tail owner span: {span}")
        derived_tails[span] = expression
    if tail_bindings != derived_tails:
        raise AdapterError("legacy tail bindings differ from tail, declaration, and prepared-value facts")


def _validate_source_span(
    span: str,
    label: str,
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
    allowed_owners: set[str],
) -> None:
    span_match = SPAN.fullmatch(span)
    if span_match is None:
        raise AdapterError(f"malformed {label} span {span}")
    source_id, start, length = map(int, span_match.groups())
    if source_id not in source_sizes or start + length > source_sizes[source_id]:
        raise AdapterError(f"{label} span is outside its source {span}")
    owners = source_owners.get(span, set())
    if not any(owner in allowed_owners or owner.split(":", 1)[0] in allowed_owners for owner in owners):
        raise AdapterError(f"{label} span has no matching AST owner: {span}")


def _current_fact_owners(
    fields: Mapping[str, str],
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
) -> None:
    declarations = _fields(fields["checked_declarations"], "CheckedDeclarations")
    functions = _map_entries(declarations["functions"])
    function_owners = {}
    for definition, declaration in functions.items():
        declaration_fields = _fields(declaration, "CheckedFunctionDeclaration")
        function_owners[definition] = (
            declaration_fields["namespace"],
            declaration_fields["name"],
        )
    selected = _map_entries(fields["selected_function_calls"])
    static = _set_items(fields["statically_resolved_call_spans"])
    other_static_calls = set()
    for name in (
        "standard_call_spans",
        "api_calls",
        "module_commands",
        "record_constructor_instances",
        "message_payload_constructors",
        "error_constructors",
    ):
        other_static_calls.update(_map_entries(fields[name]))
    if set(selected) != static - other_static_calls:
        raise AdapterError("selected function-call sites differ from static call-site facts")
    for span, target in selected.items():
        _validate_source_span(span, "selected function-call", source_sizes, source_owners, {"expr:Call"})
        if span not in _map_entries(fields["expr_types"]):
            raise AdapterError(f"selected function-call span has no checked expression type: {span}")
        target_fields = _fields(target, "CheckedFunctionTarget")
        if set(target_fields) != {"definition", "namespace", "name"}:
            raise AdapterError(f"selected function-call target has unexpected fields at {span}")
        owner = function_owners.get(target_fields["definition"])
        if owner != (target_fields["namespace"], target_fields["name"]):
            raise AdapterError(f"selected function-call target disagrees with its declaration at {span}")
    control_flow = _fields(fields["checked_control_flow"], "CheckedControlFlowFacts")
    expected_fields = {"complete", *CONTROL_FLOW_OWNERS}
    if set(control_flow) != expected_fields:
        missing = sorted(expected_fields - set(control_flow))
        added = sorted(set(control_flow) - expected_fields)
        raise AdapterError(f"checked control-flow fields differ: missing={missing} added={added}")
    if control_flow.get("complete") != "true":
        raise AdapterError("checked control-flow facts are incomplete")
    for field, allowed_owners in CONTROL_FLOW_OWNERS.items():
        for span in _set_items(control_flow[field]):
            _validate_source_span(span, field.replace("_", " "), source_sizes, source_owners, allowed_owners)
            if field in {"builtin_range_calls", "propagating_error_spans", "latent_stream_error_spans"}:
                if span not in _map_entries(fields["expr_types"]):
                    raise AdapterError(f"{field.replace('_', ' ')} span has no checked expression type: {span}")
            elif field == "cardinality_preserving_collection_calls":
                if span not in _map_entries(fields["api_calls"]):
                    raise AdapterError(f"{field.replace('_', ' ')} span has no checked API call: {span}")
            elif field in {"fallible_pipeline_stage_spans", "propagating_stage_callback_spans"}:
                stage_spans = {_pair_key(key)[1] for key in _map_entries(fields["stream_stage_types"])}
                if span not in stage_spans:
                    raise AdapterError(f"{field.replace('_', ' ')} span has no checked stream stage: {span}")
            elif field == "exhaustive_match_spans":
                owner_kinds = source_owners.get(span, set())
                if "expr:Match" in owner_kinds and span not in _map_entries(fields["expr_types"]):
                    raise AdapterError(f"exhaustive match expression has no checked expression type: {span}")
                if "stmt:Match" in owner_kinds and span not in _map_entries(fields["statement_positions"]):
                    raise AdapterError(f"exhaustive match statement has no checked statement position: {span}")


def _remove_field_path(value: str, path: tuple[tuple[str, str], ...]) -> tuple[str, str]:
    expected_type, target_field = path[0]
    prefix, entries = _struct_parts(value, expected_type)
    rebuilt: list[str] = []
    removed_value = None
    matches = 0
    for entry in entries:
        name, field_value = _field(entry)
        if name != target_field:
            rebuilt.append(entry)
            continue
        matches += 1
        if len(path) == 1:
            removed_value = field_value
        else:
            nested_value, nested_removed = _remove_field_path(field_value, path[1:])
            removed_value = nested_removed
            rebuilt.append(f"{name}:{nested_value}")
    if matches != 1 or removed_value is None:
        raise AdapterError(f"field path {path!r} matched {matches} times")
    return f"{prefix}{{{','.join(rebuilt)}}}", removed_value


def project_publication(
    source_schema: str,
    rendered: str,
    source_sizes: Mapping[int, int] | None = None,
    source_owners: Mapping[str, set[str]] | None = None,
    tail_statement_owners: Mapping[str, tuple[str, str, str]] | None = None,
    statement_owners: Mapping[str, tuple[str, str]] | None = None,
) -> str:
    if source_schema not in SOURCE_SCHEMAS:
        raise AdapterError(f"unsupported source schema {source_schema!r}")
    fields = _fields(rendered, "CheckOutput")
    common = rendered
    for path in PATHS[source_schema]:
        common, _ = _remove_field_path(common, path)
    if source_schema == "legacy":
        if source_sizes is None or source_owners is None or tail_statement_owners is None or statement_owners is None:
            raise AdapterError("legacy publication validation requires source and statement owners")
        _legacy_redundancy(
            fields, source_sizes, source_owners, tail_statement_owners, statement_owners
        )
    elif source_sizes is None or source_owners is None:
        raise AdapterError("current publication validation requires source sizes and AST owners")
    else:
        _current_fact_owners(fields, source_sizes, source_owners)
    return common


def compare_publications(
    legacy_diagnostics: str,
    legacy_rendered: str,
    legacy_source_sizes: Mapping[int, int],
    legacy_source_owners: Mapping[str, set[str]],
    legacy_tail_statement_owners: Mapping[str, tuple[str, str, str]],
    legacy_statement_owners: Mapping[str, tuple[str, str]],
    current_diagnostics: str,
    current_rendered: str,
    current_source_sizes: Mapping[int, int],
    current_source_owners: Mapping[str, set[str]],
) -> None:
    if legacy_diagnostics != current_diagnostics:
        raise AdapterError("diagnostic mismatch")
    if project_publication(
        "legacy", legacy_rendered, legacy_source_sizes, legacy_source_owners,
        legacy_tail_statement_owners, legacy_statement_owners
    ) != project_publication(
        "current", current_rendered, current_source_sizes, current_source_owners
    ):
        raise AdapterError("common checked facts mismatch")
