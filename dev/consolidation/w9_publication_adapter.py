from __future__ import annotations

import json
import re
from typing import Callable, Mapping, NamedTuple


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
CONTROL_FLOW_OWNERS = {
    "builtin_range_calls": {"expr:Call"},
    "cardinality_preserving_collection_calls": {"expr:Call"},
    "propagating_error_spans": {
        "expr", "stmt:Assert", "stmt:AssertBare", "stmt:Command", "stmt:DeferExpr",
        "stmt:Expr", "run:RunForm",
    },
    "latent_stream_error_spans": {"expr"},
    "fallible_pipeline_stage_spans": {"stage:Range", "stage:Repeat"},
    "propagating_stage_callback_spans": {"stage"},
    "exhaustive_match_spans": {"expr:Match", "stmt:Match"},
}
class AdapterError(ValueError):
    pass


class ParsedResolvedBindings(NamedTuple):
    expression_namespaces: Mapping[str, str]
    expressions: Mapping[str, str]
    declarations: Mapping[str, str]


def _canonical_owner_span(value: str, line_number: int) -> str:
    try:
        return _canonicalize_span(value)[0]
    except AdapterError:
        raise AdapterError(f"owner index line {line_number}: malformed span") from None


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
        _, source_id, start, length = _canonicalize_span(span)
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
        character = value[index]
        if character in {"\"", "'"}:
            quoted = _quoted_end(value, index)
            if quoted is not None:
                index = quoted
                continue
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
        character = value[index]
        if character in {"\"", "'"}:
            quoted = _quoted_end(value, index)
            if quoted is not None:
                index = quoted
                continue
        if character == "{":
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
        character = body[index]
        if character in {"\"", "'"}:
            quoted = _quoted_end(body, index)
            if quoted is not None:
                index = quoted
                continue
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
    return _parse_struct_fields(entries, expected_type)[1]


def _parse_struct_fields(
    entries: list[str], expected_type: str
) -> tuple[list[tuple[str, str]], dict[str, str]]:
    parsed = [_field(entry) for entry in entries]
    fields = dict(parsed)
    if len(fields) != len(entries):
        raise AdapterError(f"duplicate {expected_type} Debug field")
    return parsed, fields


def _canonicalize_span(value: str) -> tuple[str, int, int, int]:
    try:
        fields = _fields(value, "Span")
    except AdapterError:
        raise AdapterError("malformed Span fields") from None
    if set(fields) != {"source_id", "start", "len"}:
        raise AdapterError("malformed Span fields")
    source_id = re.fullmatch(r"SourceId\((\d+)\)", fields["source_id"])
    start = re.fullmatch(r"\d+", fields["start"])
    length = re.fullmatch(r"\d+", fields["len"])
    if source_id is None or start is None or length is None:
        raise AdapterError("malformed Span field types")
    source_id_value, start_value, length_value = map(
        int, (source_id.group(1), start.group(), length.group())
    )
    canonical = (
        f"Span{{source_id:SourceId({source_id_value}),start:{start_value},len:{length_value}}}"
    )
    return canonical, source_id_value, start_value, length_value


def _canonical_span_set(value: str, label: str) -> set[str]:
    spans = set()
    for raw_span in _set_items(value):
        try:
            span = _canonicalize_span(raw_span)[0]
        except AdapterError:
            raise AdapterError(f"malformed {label} span {raw_span}") from None
        if span in spans:
            raise AdapterError(f"duplicate normalized {label} span: {span}")
        spans.add(span)
    return spans


def _canonical_span_map(value: str, label: str) -> dict[str, str]:
    spans = {}
    for raw_span, item in _map_entries(value).items():
        try:
            span = _canonicalize_span(raw_span)[0]
        except AdapterError:
            raise AdapterError(f"malformed {label} span {raw_span}") from None
        if span in spans:
            raise AdapterError(f"duplicate normalized {label} span: {span}")
        spans[span] = item
    return spans


def _split_top_level(value: str, separator: str) -> list[str]:
    parts = []
    stack: list[str] = []
    closes = {"{": "}", "[": "]", "(": ")"}
    start = 0
    index = 0
    while index < len(value):
        character = value[index]
        if character in {"\"", "'"}:
            quoted = _quoted_end(value, index)
            if quoted is not None:
                index = quoted
                continue
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
        character = value[index]
        if character in {"\"", "'"}:
            quoted = _quoted_end(value, index)
            if quoted is not None:
                index = quoted
                continue
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


def _parse_resolved_bindings(value: str) -> ParsedResolvedBindings:
    fields = _fields(value, "ResolvedBindings")
    return ParsedResolvedBindings(
        expression_namespaces=_map_entries(fields["expression_namespaces"]),
        expressions=_map_entries(fields["expressions"]),
        declarations=_map_entries(fields["declarations"]),
    )


def _set_items_ordered(value: str) -> list[str]:
    value = value.strip()
    if not value.startswith("{") or _group_end(value, 0) != len(value) - 1:
        raise AdapterError("expected Rust Debug set")
    items = _split_top_level(value[1:-1], ",")
    if len(set(items)) != len(items):
        raise AdapterError("duplicate Rust Debug set entry")
    return items


def _set_items(value: str) -> set[str]:
    return set(_set_items_ordered(value))


def _pair_key(value: str) -> tuple[str, str]:
    value = value.strip()
    if not value.startswith("(") or _group_end(value, 0) != len(value) - 1:
        raise AdapterError("expected namespace and name pair")
    parts = _split_top_level(value[1:-1], ",")
    if len(parts) != 2:
        raise AdapterError("expected namespace and name pair")
    return parts[0], parts[1]


def _tuple_items(value: str, label: str) -> tuple[str, ...]:
    value = value.strip()
    if not value.startswith("(") or _group_end(value, 0) != len(value) - 1:
        raise AdapterError(f"expected {label} tuple")
    return tuple(_split_top_level(value[1:-1], ","))


def _canonical_cli_plan_key(value: str, source_schema: str) -> str:
    try:
        key_parts = _tuple_items(value, "CLI plan key")
        if len(key_parts) != 2:
            raise AdapterError("malformed key")
        origin, applet = key_parts
        if source_schema == "legacy":
            expression = re.fullmatch(r"ExprId\((\d+)\)", origin)
            if expression is None or str(int(expression.group(1))) != expression.group(1):
                raise AdapterError("malformed key")
            origin = f"ExprId({expression.group(1)})"
            field_selector = None
        else:
            origin_parts = _tuple_items(origin, "current CLI plan origin")
            expression = re.fullmatch(r"ExprId\((\d+)\)", origin_parts[0]) if len(origin_parts) == 2 else None
            if expression is None or str(int(expression.group(1))) != expression.group(1):
                raise AdapterError("malformed key")
            origin = f"ExprId({expression.group(1)})"
            field_selector = origin_parts[1]
            if field_selector != "None":
                if re.fullmatch(r'Some\(Name\("(?:\\.|[^"\\])*"\)\)', field_selector) is None:
                    raise AdapterError("malformed key")
        if applet not in {"true", "false"}:
            raise AdapterError("malformed key")
    except AdapterError:
        raise AdapterError(f"malformed {source_schema} CLI plan key") from None
    if source_schema == "current" and field_selector != "None":
        raise AdapterError("current CLI plan Some field selector has no legacy comparison key")
    return f"({origin},{applet})"


def _normalize_cli_plan_mutex(value: str, source_schema: str) -> str:
    prefix, entries = _struct_parts(value, "Mutex")
    fields = {}
    other = []
    for entry in entries:
        if entry == "..":
            other.append(entry)
            continue
        name, field_value = _field(entry)
        if name in fields:
            raise AdapterError(f"duplicate Mutex Debug field {name}")
        fields[name] = field_value
    if other != [".."] or set(fields) != {"data", "poisoned"}:
        raise AdapterError("unexpected Mutex Debug fields")
    if fields["poisoned"] not in {"true", "false"}:
        raise AdapterError("malformed Mutex Debug poison state")
    data = fields["data"].strip()
    if not data.startswith("{") or _group_end(data, 0) != len(data) - 1:
        raise AdapterError("expected CLI plan Debug map")
    projected: list[tuple[str, str]] = []
    seen = set()
    for entry in _split_top_level(data[1:-1], ","):
        pair = _split_first_top_level(entry, ":")
        if pair is None:
            raise AdapterError("malformed CLI plan Debug map entry")
        key, payload = pair
        normalized_key = _canonical_cli_plan_key(key, source_schema)
        if normalized_key in seen:
            raise AdapterError(f"duplicate normalized CLI plan key {normalized_key}")
        seen.add(normalized_key)
        projected.append((normalized_key, payload))
    projected.sort(key=lambda entry: entry[0])
    projected_data = ",".join(f"{key}:{payload}" for key, payload in projected)
    rewritten = []
    for entry in entries:
        if entry == "..":
            rewritten.append(entry)
        else:
            name, _ = _field(entry)
            rewritten.append(f"{name}:{{{projected_data}}}" if name == "data" else entry)
    return f"{prefix}{{{','.join(rewritten)}}}"


def _rewrite_field_path(
    value: str,
    path: tuple[tuple[str, str], ...],
    transform: Callable[[str], str],
) -> str:
    expected_type, target_field = path[0]
    prefix, entries = _struct_parts(value, expected_type)
    rebuilt = []
    matches = 0
    for entry in entries:
        name, field_value = _field(entry)
        if name != target_field:
            rebuilt.append(entry)
            continue
        matches += 1
        if len(path) == 1:
            rebuilt.append(f"{name}:{transform(field_value)}")
        else:
            rebuilt.append(f"{name}:{_rewrite_field_path(field_value, path[1:], transform)}")
    if matches != 1:
        raise AdapterError(f"field path {path!r} matched {matches} times")
    return f"{prefix}{{{','.join(rebuilt)}}}"


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
            declaration_span = _canonicalize_span(declaration_fields["span"])[0]
            if statement_owners.get(declaration_statement) != (
                declaration_span, "stmt:Const"
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
    tail_bindings = _canonical_span_map(prepared["tail_bindings"], "legacy tail binding")
    derived_tails = {}
    for statement_id, binding_id in tails.items():
        owner = tail_statement_owners.get(statement_id)
        declaration = declarations.get(binding_id)
        if owner is None or declaration is None:
            raise AdapterError(f"legacy tail has no statement owner or declaration: {statement_id}")
        span, name, owner_kind = owner
        if name == "" or owner_kind != "stmt:TailBareIdent":
            raise AdapterError(f"malformed legacy tail owner for {statement_id}")
        span = _validate_source_span(
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
) -> str:
    try:
        canonical_span, source_id, start, length = _canonicalize_span(span)
    except AdapterError:
        raise AdapterError(f"malformed {label} span {span}")
    if source_id not in source_sizes or start + length > source_sizes[source_id]:
        raise AdapterError(f"{label} span is outside its source {span}")
    owners = source_owners.get(canonical_span, set())
    if not any(owner in allowed_owners or owner.split(":", 1)[0] in allowed_owners for owner in owners):
        raise AdapterError(f"{label} span has no matching AST owner: {span}")
    return canonical_span


def _project_propagating_statements(
    value: str,
    source_schema: str,
    fields: Mapping[str, str],
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
) -> str:
    raw_spans = _set_items_ordered(value)
    seen = set()
    retained = []
    if source_schema == "current":
        statement_positions = _canonical_span_map(fields["statement_positions"], "checked statement position")
    for raw_span in raw_spans:
        try:
            canonical_span, source_id, start, length = _canonicalize_span(raw_span)
        except AdapterError:
            raise AdapterError(f"malformed propagating statement span {raw_span}") from None
        if canonical_span in seen:
            raise AdapterError(f"duplicate normalized propagating statement span: {canonical_span}")
        seen.add(canonical_span)
        if source_id not in source_sizes or start + length > source_sizes[source_id]:
            raise AdapterError(f"propagating statement span is outside its source: {canonical_span}")
        owners = source_owners.get(canonical_span, set())
        if not any(owner.startswith("stmt:") for owner in owners):
            raise AdapterError(f"propagating statement span has no statement AST owner: {canonical_span}")
        if source_schema == "legacy":
            if "stmt:Command" in owners:
                raise AdapterError(f"legacy propagating statements include a command: {canonical_span}")
        elif "stmt:Command" in owners:
            if canonical_span not in statement_positions:
                raise AdapterError(f"propagating statement command has no checked statement position: {canonical_span}")
            continue
        retained.append(raw_span)
    return "{" + ",".join(retained) + "}"


def _current_fact_owners(
    fields: Mapping[str, str],
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
    exact_owners: Mapping[str, object] | None = None,
    statement_owners: Mapping[str, tuple[str, str]] | None = None,
) -> None:
    declarations = _fields(fields["checked_declarations"], "CheckedDeclarations")
    functions = _map_entries(declarations["functions"])
    function_owners = {}
    function_declarations = {}
    function_definitions_by_identity: dict[tuple[str, str], list[str]] = {}
    for definition, declaration in functions.items():
        declaration_fields = _fields(declaration, "CheckedFunctionDeclaration")
        identity = (
            declaration_fields["namespace"],
            declaration_fields["name"],
        )
        function_owners[definition] = identity
        function_definitions_by_identity.setdefault(identity, []).append(definition)
        function_declarations[definition] = declaration_fields
    selected = _canonical_span_map(fields["selected_function_calls"], "selected function-call")
    static = _canonical_span_set(fields["statically_resolved_call_spans"], "statically resolved call")
    other_static_calls = set()
    for name in (
        "standard_call_spans",
        "api_calls",
        "module_commands",
        "record_constructor_instances",
        "message_payload_constructors",
        "error_constructors",
    ):
        other_static_calls.update(_canonical_span_map(fields[name], name.replace("_", " ")))
    if exact_owners is None and set(selected) != static - other_static_calls:
        raise AdapterError("selected function-call sites differ from static call-site facts")
    validated_function_declarations: set[str] = set()
    expression_types = _canonical_span_map(fields["expr_types"], "checked expression type")
    resolved_bindings = (
        _parse_resolved_bindings(fields["resolved_bindings"])
        if exact_owners is not None and (selected or static - other_static_calls)
        else None
    )
    statement_owner_counts: dict[tuple[str, str], int] | None = None
    statement_owner_indexed = False
    exact_exprs: Mapping[object, object] = {}
    exact_calls: Mapping[object, object] = {}
    exact_fields: Mapping[object, object] = {}
    if exact_owners is not None:
        exact_exprs = exact_owners.get("exprs", {})
        exact_calls = exact_owners.get("calls", {})
        exact_fields = exact_owners.get("field_accesses", {})
        if not all(isinstance(value, Mapping) for value in (exact_exprs, exact_calls, exact_fields)):
            raise AdapterError("exact call owner facts are malformed")
    expression_ids_by_span: dict[str, list[int]] = {}
    for expression_id, row in exact_exprs.items():
        if type(expression_id) is not int or not isinstance(row, tuple) or len(row) != 3:
            raise AdapterError("exact expression owner row is malformed")
        span, tag, name = row
        if not isinstance(span, str) or not isinstance(tag, str) or not isinstance(name, str):
            raise AdapterError("exact expression owner row is malformed")
        expression_ids_by_span.setdefault(_canonicalize_span(span)[0], []).append(expression_id)
    for span, target in selected.items():
        exact_call_ids = [
            expression_id
            for expression_id in expression_ids_by_span.get(span, [])
            if exact_exprs[expression_id][1] == "Call"
        ]
        if exact_owners is not None and "expr:Call" in source_owners.get(span, set()) and not exact_call_ids:
            raise AdapterError(f"selected expr:Call owner has no unique exact AST identity: {span}")
        callee_id = None
        if exact_owners is None or exact_call_ids:
            span = _validate_source_span(span, "selected function-call", source_sizes, source_owners, {"expr:Call"})
            if span not in expression_types:
                raise AdapterError(f"selected function-call span has no checked expression type: {span}")
            if exact_owners is not None and len(exact_call_ids) != 1:
                raise AdapterError(f"selected function-call span has ambiguous Call identity: {span}")
            if exact_owners is not None and exact_call_ids[0] not in exact_calls:
                raise AdapterError(f"selected function-call has no exact callee link: {span}")
            if exact_owners is not None:
                callee_id = exact_calls[exact_call_ids[0]]
                if type(callee_id) is not int or callee_id not in exact_exprs or callee_id == exact_call_ids[0]:
                    raise AdapterError(f"selected function-call has an invalid exact callee link: {span}")
        target_fields = _fields(target, "CheckedFunctionTarget")
        if set(target_fields) != {"definition", "namespace", "name"}:
            raise AdapterError(f"selected function-call target has unexpected fields at {span}")
        owner = function_owners.get(target_fields["definition"])
        if owner != (target_fields["namespace"], target_fields["name"]):
            raise AdapterError(f"selected function-call target disagrees with its declaration at {span}")
        if exact_owners is not None and callee_id is not None:
            matching_definitions = function_definitions_by_identity.get(
                (target_fields["namespace"], target_fields["name"]), []
            )
            if matching_definitions != [target_fields["definition"]]:
                raise AdapterError(f"selected function-call target has ambiguous checked declaration identity at {span}")
        if exact_owners is not None:
            declaration = function_declarations[target_fields["definition"]]
            if target_fields["definition"] not in validated_function_declarations:
                if set(declaration) != {
                    "name", "namespace", "internal", "kind", "signature", "body", "parameter_spans"
                }:
                    raise AdapterError(f"selected function-call declaration has unexpected fields at {span}")
                if declaration["kind"] not in {"Pure", "Proc", "Stream", "CliMain"}:
                    raise AdapterError(f"selected function-call declaration has an unknown kind at {span}")
                if declaration["internal"] not in {"true", "false"}:
                    raise AdapterError(f"selected function-call declaration has a malformed internal flag at {span}")
                if declaration["signature"] == "None":
                    raise AdapterError(f"selected function-call declaration has no checked signature at {span}")
                signature = declaration["signature"]
                if not signature.startswith("Some(") or not signature.endswith(")"):
                    raise AdapterError(f"selected function-call declaration has no checked signature at {span}")
                signature_fields = _fields(signature[5:-1], "CompactFunctionSig")
                if set(signature_fields) != {
                    "params", "parameter_schemas", "return_ty", "return_type_expr",
                    "inferred_effects", "effects",
                }:
                    raise AdapterError(f"selected function-call declaration has an incomplete signature at {span}")
                _canonical = _canonicalize_span(declaration["body"])
                if _canonical[1] not in source_sizes or _canonical[2] + _canonical[3] > source_sizes[_canonical[1]]:
                    raise AdapterError(f"selected function-call declaration body is outside its source at {span}")
                validated_function_declarations.add(target_fields["definition"])
            if exact_owners is not None and callee_id is not None:
                if resolved_bindings is None:
                    raise AdapterError("selected function-call validation has no parsed resolver facts")
                _validate_selected_callee_identity(
                    span, callee_id, target_fields, exact_exprs, exact_fields, resolved_bindings,
                    source_sizes, source_owners,
                )

    native_set_spans: set[str] = set()
    native_error_failure_spans: set[str] = set()
    selected_call_spans: set[str] = set()
    if exact_owners is not None:
        for span in selected:
            call_ids = [
                expression_id
                for expression_id in expression_ids_by_span.get(span, [])
                if exact_exprs[expression_id][1] == "Call"
            ]
            if not call_ids:
                continue
            span = _validate_source_span(
                span, "selected function-call", source_sizes, source_owners, {"expr:Call"}
            )
            if len(call_ids) != 1:
                raise AdapterError(f"selected function-call span has ambiguous Call identity: {span}")
            call_id = call_ids[0]
            if call_id not in exact_calls:
                raise AdapterError(f"selected function-call has no exact callee link: {span}")
            selected_call_spans.add(span)

        for span in static - other_static_calls:
            if span in selected_call_spans:
                continue
            if resolved_bindings is None:
                raise AdapterError("static call-site validation has no parsed resolver facts")
            if _is_native_set_call(
                span, exact_exprs, exact_calls, exact_fields, source_sizes, source_owners,
                expression_types, selected, other_static_calls, resolved_bindings,
            ):
                native_set_spans.add(span)
            elif _is_native_error_failure_call(
                span, exact_exprs, exact_calls, exact_fields, source_sizes, source_owners,
                expression_types, selected, other_static_calls,
            ):
                native_error_failure_spans.add(span)
        callable_alias_spans = _canonical_span_map(
            fields["static_callable_aliases"], "static callable alias"
        )
        validated_alias_spans = _validate_static_callable_alias_facts(
            expression_ids_by_span,
            exact_exprs,
            exact_calls,
            exact_fields,
            callable_alias_spans,
            expression_types,
            source_sizes,
            source_owners,
            static,
        )
        missing_static_calls = (
            (static - other_static_calls)
            - selected_call_spans
            - native_set_spans
            - native_error_failure_spans
            - validated_alias_spans
        )
        if missing_static_calls:
            raise AdapterError(
                "static call-site facts have no selected function call or validated native intrinsic route: "
                + ",".join(sorted(missing_static_calls))
            )
        if selected:
            try:
                from w9_publication_reference_facts import validate_function_reference_targets
            except ImportError as error:
                raise AdapterError("function-reference validator is unavailable") from error
            validate_function_reference_targets(
                fields=fields,
                source_sizes=source_sizes,
                source_owners=source_owners,
                exact_owners=exact_owners,
            )
    control_flow = _fields(fields["checked_control_flow"], "CheckedControlFlowFacts")
    expected_fields = {"complete", *CONTROL_FLOW_OWNERS}
    if set(control_flow) != expected_fields:
        missing = sorted(expected_fields - set(control_flow))
        added = sorted(set(control_flow) - expected_fields)
        raise AdapterError(f"checked control-flow fields differ: missing={missing} added={added}")
    if control_flow.get("complete") != "true":
        raise AdapterError("checked control-flow facts are incomplete")
    api_calls = _canonical_span_map(fields["api_calls"], "checked API call")
    statement_positions = _canonical_span_map(fields["statement_positions"], "checked statement position")
    stage_spans = {
        _canonicalize_span(_pair_key(key)[1])[0]
        for key in _map_entries(fields["stream_stage_types"])
    }
    control_flow_owners = dict(source_owners)
    structured_stage_owners = {}
    if exact_owners is not None:
        runs = exact_owners.get("runs")
        if not isinstance(runs, Mapping):
            raise AdapterError("exact run-form facts are malformed")
        for run_id, row in runs.items():
            if type(run_id) is not int or not isinstance(row, tuple) or len(row) != 2 or type(row[1]) is not bool:
                raise AdapterError("exact run-form row is malformed")
            run_span, _propagates = row
            canonical_run_span, source_id, start, length = _canonicalize_span(run_span)
            if source_id not in source_sizes or start + length > source_sizes[source_id]:
                raise AdapterError(f"exact run form span is outside its source: {canonical_run_span}")
            control_flow_owners.setdefault(canonical_run_span, set()).add("run:RunForm")
        structured_stages = exact_owners.get("structured_stages", {})
        if not isinstance(structured_stages, Mapping) or any(
            type(stage_id) is not int or stage_id < 0 for stage_id in structured_stages
        ) or set(structured_stages) != set(range(len(structured_stages))):
            raise AdapterError("exact structured stream-stage facts are malformed")
        for stage_id, row in structured_stages.items():
            if not isinstance(row, tuple) or len(row) != 2 or not all(
                isinstance(value, str) for value in row
            ):
                raise AdapterError(f"exact structured stream-stage row is malformed: {stage_id}")
            raw_stage_span, kind = row
            canonical_stage_span, source_id, start, length = _canonicalize_span(raw_stage_span)
            if source_id not in source_sizes or start + length > source_sizes[source_id]:
                raise AdapterError(
                    f"exact structured stream-stage span is outside its source: {canonical_stage_span}"
                )
            if kind in {"Range", "Repeat"}:
                if canonical_stage_span in structured_stage_owners:
                    raise AdapterError(
                        f"exact structured fallible stage span is ambiguous: {canonical_stage_span}"
                    )
                structured_stage_owners[canonical_stage_span] = kind
                control_flow_owners.setdefault(canonical_stage_span, set()).add(f"stage:{kind}")
    for field, allowed_owners in CONTROL_FLOW_OWNERS.items():
        for span in _canonical_span_set(control_flow[field], field.replace("_", " ")):
            if field == "fallible_pipeline_stage_spans" and structured_stage_owners.get(span) not in {
                "Range", "Repeat"
            }:
                raise AdapterError(
                    f"{field.replace('_', ' ')} span has no exact structured Range or Repeat stage: {span}"
                )
            span = _validate_source_span(
                span, field.replace("_", " "), source_sizes, control_flow_owners, allowed_owners
            )
            if field == "propagating_error_spans":
                owner_kinds = control_flow_owners.get(span, set())
                if any(owner.startswith("expr:") for owner in owner_kinds) and span not in expression_types:
                    raise AdapterError(f"{field.replace('_', ' ')} span has no checked expression type: {span}")
                if owner_kinds & {"stmt:Command", "stmt:Expr", "stmt:Assert", "stmt:AssertBare", "stmt:DeferExpr"} and span not in statement_positions:
                    raise AdapterError(f"{field.replace('_', ' ')} span has no checked statement position: {span}")
                statement_owner_kinds = owner_kinds & {"stmt:Assert", "stmt:AssertBare", "stmt:DeferExpr"}
                if statement_owner_kinds:
                    if not statement_owner_indexed:
                        statement_owner_counts = _index_statement_owners(statement_owners)
                        statement_owner_indexed = True
                    if not _has_unique_statement_owner(
                        span, statement_owner_kinds, statement_owner_counts
                    ):
                        raise AdapterError(f"propagating error span has no unique exact statement owner: {span}")
                if "run:RunForm" in owner_kinds:
                    runs = exact_owners.get("runs") if exact_owners is not None else None
                    if not isinstance(runs, Mapping) or not any(
                        isinstance(row, tuple) and len(row) == 2 and row[0] == span and row[1] is True
                        for row in runs.values()
                    ):
                        raise AdapterError(f"propagating run-form span has no exact propagating run: {span}")
            elif field in {"builtin_range_calls", "latent_stream_error_spans"}:
                if span not in expression_types:
                    raise AdapterError(f"{field.replace('_', ' ')} span has no checked expression type: {span}")
            elif field == "cardinality_preserving_collection_calls":
                if span not in api_calls:
                    raise AdapterError(f"{field.replace('_', ' ')} span has no checked API call: {span}")
            elif field in {"fallible_pipeline_stage_spans", "propagating_stage_callback_spans"}:
                if span not in stage_spans:
                    raise AdapterError(f"{field.replace('_', ' ')} span has no checked stream stage: {span}")
            elif field == "exhaustive_match_spans":
                owner_kinds = source_owners.get(span, set())
                if "expr:Match" in owner_kinds and span not in expression_types:
                    raise AdapterError(f"exhaustive match expression has no checked expression type: {span}")
                if "stmt:Match" in owner_kinds and span not in statement_positions:
                    raise AdapterError(f"exhaustive match statement has no checked statement position: {span}")


def _is_native_set_call(
    span: str,
    expressions: Mapping[object, object],
    calls: Mapping[object, object],
    fields_accessed: Mapping[object, object],
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
    expression_types: Mapping[str, str],
    selected: Mapping[str, str],
    other_static_calls: set[str],
    resolved_bindings: ParsedResolvedBindings,
) -> bool:
    if span in selected or span in other_static_calls or re.fullmatch(
        r"Set\(.+\)", expression_types.get(span, "")
    ) is None:
        return False
    call_ids = [
        expression_id
        for expression_id, row in expressions.items()
        if isinstance(row, tuple) and len(row) == 3 and row[0] == span and row[1] == "Call"
    ]
    if len(call_ids) != 1:
        return False
    call_id = call_ids[0]
    callee_id = calls.get(call_id)
    field_row = fields_accessed.get(callee_id)
    if not isinstance(field_row, tuple) or len(field_row) != 2:
        return False
    callee_row = expressions.get(callee_id)
    if not isinstance(callee_row, tuple) or len(callee_row) != 3 or callee_row[1] != "Field":
        return False
    _validate_source_span(
        callee_row[0], "native Set field access", source_sizes, source_owners, {"expr:Field"}
    )
    base_id, method = field_row
    base_row = expressions.get(base_id)
    if not isinstance(base_row, tuple) or len(base_row) != 3:
        return False
    if base_row[1:] != ("Ident", "set") or method not in {"empty", "from"}:
        return False
    _validate_source_span(span, "native Set call", source_sizes, source_owners, {"expr:Call"})
    _validate_source_span(
        base_row[0], "native Set module identifier", source_sizes, source_owners, {"expr:Ident"}
    )
    expression_key = f"ExprId({base_id + 1})"
    if expression_key not in resolved_bindings.expression_namespaces:
        raise AdapterError(f"native Set module identifier has no checked namespace fact: {span}")
    if expression_key in resolved_bindings.expressions:
        raise AdapterError(f"native Set module identifier is shadowed by a binding: {span}")
    return True


def _validate_static_callable_alias_facts(
    expression_ids_by_span: Mapping[str, list[int]],
    expressions: Mapping[object, object],
    calls: Mapping[object, object],
    fields_accessed: Mapping[object, object],
    aliases: Mapping[str, str],
    expression_types: Mapping[str, str],
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
    static_calls: set[str],
) -> set[str]:
    call_spans_by_callee = {}
    for call_id, callee_id in calls.items():
        call = expressions.get(call_id)
        if (
            type(call_id) is not int
            or type(callee_id) is not int
            or not isinstance(call, tuple)
            or len(call) != 3
            or call[1] != "Call"
            or callee_id not in expressions
        ):
            raise AdapterError("exact callable-alias call link is malformed")
        call_span = _canonicalize_span(call[0])[0]
        call_spans_by_callee.setdefault(callee_id, []).append(call_span)

    alias_calls = set()
    for alias_span, alias_value in aliases.items():
        expression_ids = expression_ids_by_span.get(alias_span, [])
        if len(expression_ids) != 1:
            raise AdapterError(f"static callable alias has no unique exact expression owner: {alias_span}")
        alias_id = expression_ids[0]
        alias_expression = expressions[alias_id]
        if not isinstance(alias_expression, tuple) or len(alias_expression) != 3:
            raise AdapterError(f"static callable alias expression owner is malformed: {alias_span}")
        _, alias_kind, _ = alias_expression
        _validate_source_span(
            alias_span, "static callable alias", source_sizes, source_owners, {f"expr:{alias_kind}"}
        )
        alias = _fields(alias_value, "StaticCallableAlias")
        if set(alias) != {"definition", "method_call", "name", "pure", "signature"}:
            raise AdapterError(f"static callable alias has unexpected fields at {alias_span}")
        if alias["method_call"] not in {"true", "false"} or alias["pure"] not in {"true", "false"}:
            raise AdapterError(f"static callable alias has malformed flags at {alias_span}")
        if _debug_name_value(alias["name"]) is None:
            raise AdapterError(f"static callable alias has a malformed name at {alias_span}")
        definition = alias["definition"]
        if definition != "None":
            if not definition.startswith("Some(") or not definition.endswith(")"):
                raise AdapterError(f"static callable alias has a malformed definition at {alias_span}")
            definition_span = _canonicalize_span(definition[5:-1])[0]
            _, source_id, start, length = _canonicalize_span(definition_span)
            if source_id not in source_sizes or start + length > source_sizes[source_id]:
                raise AdapterError(f"static callable alias definition is outside its source: {alias_span}")
        signature = _fields(alias["signature"], "CallableType")
        if set(signature) != {"effects", "params", "return_ty"}:
            raise AdapterError(f"static callable alias has an incomplete signature at {alias_span}")
        effects = signature["effects"]
        if effects != "None":
            if not effects.startswith("Some([") or not effects.endswith("])"):
                raise AdapterError(f"static callable alias has malformed effects at {alias_span}")
            effect_names = _split_top_level(effects[6:-2], ",")
            if any(re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", effect) is None for effect in effect_names):
                raise AdapterError(f"static callable alias has malformed effects at {alias_span}")
        params = signature["params"]
        if not params.startswith("[") or _group_end(params, 0) != len(params) - 1:
            raise AdapterError(f"static callable alias has malformed parameters at {alias_span}")
        for param in _split_top_level(params[1:-1], ","):
            param_fields = _fields(param, "CallableParamType")
            if set(param_fields) != {"name", "ty", "defaulted", "rest"}:
                raise AdapterError(f"static callable alias has malformed parameter fields at {alias_span}")
            if _debug_name_value(param_fields["name"]) is None:
                raise AdapterError(f"static callable alias has a malformed parameter name at {alias_span}")
            if param_fields["ty"] == "":
                raise AdapterError(f"static callable alias has a malformed parameter type at {alias_span}")
            if param_fields["defaulted"] not in {"true", "false"} or param_fields["rest"] not in {"true", "false"}:
                raise AdapterError(f"static callable alias has malformed parameter flags at {alias_span}")
        if signature["return_ty"] == "":
            raise AdapterError(f"static callable alias has no checked signature at {alias_span}")

        field_access = fields_accessed.get(alias_id) if alias_kind == "Field" else None
        if alias_kind == "Field":
            if not isinstance(field_access, tuple) or len(field_access) != 2:
                raise AdapterError(f"static callable alias field has no exact base link: {alias_span}")
            base_id, member_name = field_access
            if type(base_id) is not int or base_id not in expressions or not isinstance(member_name, str):
                raise AdapterError(f"static callable alias field has an invalid exact base link: {alias_span}")
            base = expressions[base_id]
            if not isinstance(base, tuple) or len(base) != 3:
                raise AdapterError(f"static callable alias base expression is malformed: {alias_span}")
            _validate_source_span(
                base[0], "static callable alias base", source_sizes, source_owners,
                {f"expr:{base[1]}"},
            )
            if alias["method_call"] == "true" and member_name != "call":
                raise AdapterError(f"static callable alias method call has the wrong field: {alias_span}")
        elif alias["method_call"] == "true":
            raise AdapterError(f"static callable alias method call has no exact Field callee: {alias_span}")

        linked_calls = call_spans_by_callee.get(alias_id, [])
        if len(linked_calls) > 1:
            raise AdapterError(f"static callable alias has ambiguous exact Call links: {alias_span}")
        if alias["method_call"] == "true" and not linked_calls:
            raise AdapterError(f"static callable alias method call has no exact Call link: {alias_span}")
        if linked_calls:
            if alias["method_call"] == "false" and alias_kind not in {"Ident", "Field"}:
                raise AdapterError(f"static callable alias Call has an unsupported callee kind: {alias_span}")
            call_span = linked_calls[0]
            _validate_source_span(
                call_span, "static callable alias call", source_sizes, source_owners, {"expr:Call"}
            )
            if call_span not in expression_types:
                raise AdapterError(f"static callable alias Call has no checked type: {call_span}")
            if call_span in static_calls:
                alias_calls.add(call_span)
    return alias_calls


def _debug_name_value(value: str) -> str | None:
    matched = re.fullmatch(r'Name\(("(?:\\.|[^"\\])*")\)', value)
    if matched is None:
        return None
    try:
        decoded = json.loads(matched.group(1))
    except json.JSONDecodeError:
        return None
    return decoded if isinstance(decoded, str) else None


def _validate_selected_callee_identity(
    span: str,
    callee_id: int,
    target: Mapping[str, str],
    expressions: Mapping[object, object],
    fields_accessed: Mapping[object, object],
    resolved_bindings: ParsedResolvedBindings,
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
) -> None:
    callee = expressions.get(callee_id)
    if not isinstance(callee, tuple) or len(callee) != 3:
        raise AdapterError(f"selected function-call has no exact callee expression at {span}")
    callee_span, callee_kind, callee_name = callee
    callee_owner = "expr:Ident" if callee_kind == "Ident" else "expr:Field"
    _validate_source_span(
        callee_span, "selected function-call callee", source_sizes, source_owners,
        {callee_owner},
    )
    target_name = _debug_name_value(target["name"])
    if target_name is None:
        raise AdapterError(f"selected function-call target has a malformed name at {span}")
    if callee_kind == "Ident":
        expression_id = f"ExprId({callee_id + 1})"
        if expression_id not in resolved_bindings.expression_namespaces:
            raise AdapterError(f"selected function-call callee has no resolver namespace at {span}")
        if (
            callee_name != target_name
            or resolved_bindings.expression_namespaces[expression_id] != target["namespace"]
        ):
            raise AdapterError(f"selected function-call callee identity disagrees with its target at {span}")
        return
    if callee_kind != "Field":
        raise AdapterError(f"selected function-call callee has unsupported exact AST kind at {span}")
    field = fields_accessed.get(callee_id)
    if not isinstance(field, tuple) or len(field) != 2:
        raise AdapterError(f"selected function-call field callee has no exact access link at {span}")
    base_id, member_name = field
    if type(base_id) is not int or member_name != target_name:
        raise AdapterError(f"selected function-call callee identity disagrees with its target at {span}")
    base = expressions.get(base_id)
    if not isinstance(base, tuple) or len(base) != 3 or base[1] != "Ident":
        raise AdapterError(f"selected function-call module base is not an exact identifier at {span}")
    _validate_source_span(
        base[0], "selected function-call module base", source_sizes, source_owners,
        {"expr:Ident"},
    )
    base_key = f"ExprId({base_id + 1})"
    if base_key not in resolved_bindings.expression_namespaces or base_key not in resolved_bindings.expressions:
        raise AdapterError(f"selected function-call module base has no resolver binding at {span}")
    binding = resolved_bindings.expressions[base_key]
    declaration = resolved_bindings.declarations.get(binding)
    if declaration is None:
        raise AdapterError(f"selected function-call module binding has no declaration at {span}")
    declaration_fields = _fields(declaration, "BindingDeclaration")
    if set(declaration_fields) != {
        "exported", "global", "kind", "name", "namespace", "span"
    }:
        raise AdapterError(f"selected function-call module binding has unexpected fields at {span}")
    binding_name = _debug_name_value(declaration_fields.get("name", ""))
    module = re.fullmatch(r"Module\((Name\(.*\))\)", declaration_fields.get("kind", ""))
    target_namespace = re.fullmatch(r"Some\((Name\(.*\))\)", target["namespace"])
    module_name = _debug_name_value(module.group(1)) if module is not None else None
    target_module_name = (
        _debug_name_value(target_namespace.group(1)) if target_namespace is not None else None
    )
    if (
        binding_name != base[2]
        or declaration_fields.get("namespace") != resolved_bindings.expression_namespaces[base_key]
        or module_name is None
        or target_module_name is None
        or module_name != target_module_name
    ):
        raise AdapterError(f"selected function-call module binding namespace disagrees with its target at {span}")


def _has_unique_statement_owner(
    span: str,
    owner_kinds: set[str],
    statement_owner_counts: Mapping[tuple[str, str], int] | None,
) -> bool:
    if statement_owner_counts is None:
        return False
    return sum(statement_owner_counts.get((span, kind), 0) for kind in owner_kinds) == 1


def _index_statement_owners(
    statement_owners: Mapping[str, tuple[str, str]] | None,
) -> dict[tuple[str, str], int]:
    owner_counts: dict[tuple[str, str], int] = {}
    if statement_owners is None:
        return owner_counts
    for statement_id, row in statement_owners.items():
        if re.fullmatch(r"StmtId\(\d+\)", statement_id) is None:
            raise AdapterError("statement owner index has a malformed statement id")
        if not isinstance(row, tuple) or len(row) != 2 or not isinstance(row[0], str) or not isinstance(row[1], str):
            raise AdapterError("statement owner index has a malformed statement row")
        statement_span = _canonicalize_span(row[0])[0]
        key = statement_span, row[1]
        owner_counts[key] = owner_counts.get(key, 0) + 1
    return owner_counts


def _is_native_error_failure_call(
    span: str,
    expressions: Mapping[object, object],
    calls: Mapping[object, object],
    fields_accessed: Mapping[object, object],
    source_sizes: Mapping[int, int],
    source_owners: Mapping[str, set[str]],
    expression_types: Mapping[str, str],
    selected: Mapping[str, str],
    other_static_calls: set[str],
) -> bool:
    if span in selected or span in other_static_calls:
        return False
    call_ids = [
        expression_id
        for expression_id, row in expressions.items()
        if isinstance(row, tuple) and len(row) == 3 and row[0] == span and row[1] == "Call"
    ]
    if len(call_ids) != 1:
        return False
    call_id = call_ids[0]
    callee_id = calls.get(call_id)
    field_row = fields_accessed.get(callee_id)
    callee_row = expressions.get(callee_id)
    if not isinstance(field_row, tuple) or len(field_row) != 2:
        return False
    expected_type = {"fail": "Result(Unit,Error)", "failure": "Error"}.get(field_row[1])
    if (
        expected_type is None
        or expression_types.get(span) != expected_type
        or not isinstance(callee_row, tuple)
        or len(callee_row) != 3
        or callee_row[1] != "Field"
    ):
        return False
    _validate_source_span(
        span, "native error failure call", source_sizes, source_owners, {"expr:Call"}
    )
    _validate_source_span(
        callee_row[0], "native error failure field access", source_sizes, source_owners, {"expr:Field"}
    )
    base_id = field_row[0]
    base_row = expressions.get(base_id)
    if not isinstance(base_row, tuple) or len(base_row) != 3 or base_row[1:] != ("Ident", "error"):
        return False
    _validate_source_span(
        base_row[0], "native error failure module identifier", source_sizes, source_owners, {"expr:Ident"}
    )
    return True


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


def _remove_field_pairs(
    fields: list[tuple[str, str]], path: tuple[tuple[str, str], ...]
) -> tuple[list[tuple[str, str]], str]:
    target_field = path[0][1]
    rebuilt = []
    removed_value = None
    matches = 0
    for name, field_value in fields:
        if name != target_field:
            rebuilt.append((name, field_value))
            continue
        matches += 1
        if len(path) == 1:
            removed_value = field_value
        else:
            nested_value, nested_removed = _remove_field_path(field_value, path[1:])
            removed_value = nested_removed
            rebuilt.append((name, nested_value))
    if matches != 1 or removed_value is None:
        raise AdapterError(f"field path {path!r} matched {matches} times")
    return rebuilt, removed_value


def _rewrite_field_pairs(
    fields: list[tuple[str, str]],
    path: tuple[tuple[str, str], ...],
    transform: Callable[[str], str],
) -> list[tuple[str, str]]:
    target_field = path[0][1]
    rebuilt = []
    matches = 0
    for name, field_value in fields:
        if name != target_field:
            rebuilt.append((name, field_value))
            continue
        matches += 1
        if len(path) == 1:
            rebuilt.append((name, transform(field_value)))
        else:
            rebuilt.append((name, _rewrite_field_path(field_value, path[1:], transform)))
    if matches != 1:
        raise AdapterError(f"field path {path!r} matched {matches} times")
    return rebuilt


def _format_struct_fields(prefix: str, fields: list[tuple[str, str]]) -> str:
    return f"{prefix}{{{','.join(f'{name}:{value}' for name, value in fields)}}}"


def project_publication(
    source_schema: str,
    rendered: str,
    source_sizes: Mapping[int, int] | None = None,
    source_owners: Mapping[str, set[str]] | None = None,
    tail_statement_owners: Mapping[str, tuple[str, str, str]] | None = None,
    statement_owners: Mapping[str, tuple[str, str]] | None = None,
    *,
    candidate_exact_owners: Mapping[str, object] | None = None,
) -> str:
    if source_schema not in SOURCE_SCHEMAS:
        raise AdapterError(f"unsupported source schema {source_schema!r}")
    prefix, entries = _struct_parts(rendered, "CheckOutput")
    common_fields, fields = _parse_struct_fields(entries, "CheckOutput")
    for path in PATHS[source_schema]:
        common_fields, _ = _remove_field_pairs(common_fields, path)
    common_fields = _rewrite_field_pairs(
        common_fields,
        (("CheckOutput", "prepared_constants"), ("PreparedConstants", "cli_plans")),
        lambda value: _normalize_cli_plan_mutex(value, source_schema),
    )
    if source_schema == "legacy":
        if source_sizes is None or source_owners is None or tail_statement_owners is None or statement_owners is None:
            raise AdapterError("legacy publication validation requires source and statement owners")
        _legacy_redundancy(
            fields, source_sizes, source_owners, tail_statement_owners, statement_owners
        )
    elif source_sizes is None or source_owners is None:
        raise AdapterError("current publication validation requires source sizes and AST owners")
    else:
        _current_fact_owners(
            fields, source_sizes, source_owners, candidate_exact_owners, statement_owners
        )
    common_fields = _rewrite_field_pairs(
        common_fields,
        (("CheckOutput", "propagating_statements"),),
        lambda value: _project_propagating_statements(
            value, source_schema, fields, source_sizes, source_owners
        ),
    )
    return _format_struct_fields(prefix, common_fields)


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
    *,
    candidate_exact_owners: Mapping[str, object] | None = None,
    candidate_statement_owners: Mapping[str, tuple[str, str]] | None = None,
) -> None:
    if legacy_diagnostics != current_diagnostics:
        raise AdapterError("diagnostic mismatch")
    if project_publication(
        "legacy", legacy_rendered, legacy_source_sizes, legacy_source_owners,
        legacy_tail_statement_owners, legacy_statement_owners
    ) != project_publication(
        "current", current_rendered, current_source_sizes, current_source_owners,
        statement_owners=candidate_statement_owners,
        candidate_exact_owners=candidate_exact_owners,
    ):
        raise AdapterError("common checked facts mismatch")
