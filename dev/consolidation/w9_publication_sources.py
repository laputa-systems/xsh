from __future__ import annotations

import re
from copy import deepcopy
from dataclasses import dataclass
from typing import Mapping, Sequence

import w9_publication_adapter as publication_adapter


TUI_NAMESPACE = "<xsh-stdlib:tui>"
TUI_NAMESPACE_DEBUG = f'Some(Name("{TUI_NAMESPACE}"))'
OLD_HEADER = (
    b"# `tui.left_pad`, `tui.right_pad`, and `tui.read_secret` stay native. Padding\n"
    b"# retains its width scan, while secret input owns terminal mode and descriptor\n"
    b"# lifetime.\n"
)
CURRENT_HEADER = (
    b"# `tui.left_pad`, `tui.right_pad`, the cell-aware text operations, and\n"
    b"# `tui.read_secret` stay native. The cell operations use pinned Unicode width\n"
    b"# data; secret input owns terminal mode and descriptor lifetime.\n"
)
ORDERED_SPAN_MAP_FIELDS = (
    "function_return_types",
    "expr_types",
    "statement_positions",
)
SOURCE_ID = re.compile(r"SourceId\((\d+)\)\Z")
DECIMAL = re.compile(r"\d+\Z")
FUNCTION_ID = re.compile(r"FunctionDefId\((\d+)\)\Z")
FUNCTION_NAME = re.compile(r'Name\("([A-Za-z_][A-Za-z0-9_]*)"\)\Z')
FUNCTION_KIND = re.compile(r"[A-Za-z][A-Za-z0-9_]*\Z")
SPAN_START = re.compile(r"(?<![A-Za-z0-9_])start\s*:\s*(\d+)")
_SPAN_REWRITE_TOKENS = re.compile(r"['\"]|Span")


class SourceAlignmentError(publication_adapter.AdapterError):
    pass


@dataclass(frozen=True)
class _FunctionDeclaration:
    definition_id: str
    name: str
    kind: str
    body: tuple[int, int, int]
    parameter_spans: tuple[tuple[int, int, int], ...]


@dataclass(frozen=True)
class _PublicationField:
    name: str
    rendered: str


@dataclass(frozen=True)
class _ParsedPublication:
    prefix: str
    fields: tuple[_PublicationField, ...]
    values: dict[str, str]


@dataclass(frozen=True)
class AlignedTuiPublications:
    reference_publications: tuple[str, ...]
    candidate_publications: tuple[str, ...]
    candidate_source_sizes: dict[int, int]
    candidate_source_owners: dict[str, set[str]]
    tui_source_id: int | None


@dataclass(frozen=True)
class _AlignedTuiPublicationParts:
    aligned: AlignedTuiPublications
    reference: tuple[_ParsedPublication | None, ...]
    candidate: tuple[_ParsedPublication | None, ...]


@dataclass(frozen=True, init=False)
class _SourceEdit:
    reference_source: bytes
    candidate_source: bytes
    old_header_end: int
    current_header_end: int
    delta: int
    changed: bool

    def __init__(self, reference_source: bytes, candidate_source: bytes):
        if not isinstance(reference_source, bytes) or not isinstance(candidate_source, bytes):
            raise SourceAlignmentError("TUI source alignment requires verified source bytes")
        if reference_source == candidate_source:
            old_header_end = 0
            current_header_end = 0
            delta = 0
            changed = False
        else:
            if reference_source.count(OLD_HEADER) != 1 or candidate_source.count(CURRENT_HEADER) != 1:
                raise SourceAlignmentError("TUI source differs outside the known TUI header")
            old_prefix, old_suffix = reference_source.split(OLD_HEADER, 1)
            current_prefix, current_suffix = candidate_source.split(CURRENT_HEADER, 1)
            if old_prefix != current_prefix or old_suffix != current_suffix:
                raise SourceAlignmentError("TUI source differs outside the known TUI header")
            old_header_end = len(old_prefix) + len(OLD_HEADER)
            current_header_end = len(current_prefix) + len(CURRENT_HEADER)
            delta = len(CURRENT_HEADER) - len(OLD_HEADER)
            changed = True
        object.__setattr__(self, "reference_source", reference_source)
        object.__setattr__(self, "candidate_source", candidate_source)
        object.__setattr__(self, "old_header_end", old_header_end)
        object.__setattr__(self, "current_header_end", current_header_end)
        object.__setattr__(self, "delta", delta)
        object.__setattr__(self, "changed", changed)

    def current_span_to_reference(self, start: int, length: int) -> int:
        self._require_source_range(self.candidate_source, start, length)
        if not self.changed:
            return start
        if start < self.current_header_end:
            raise SourceAlignmentError("TUI span intersects the changed header")
        reference_start = start - self.delta
        if reference_start < self.old_header_end:
            raise SourceAlignmentError("TUI span intersects the changed header")
        self._require_source_range(self.reference_source, reference_start, length)
        if self.candidate_source[start : start + length] != self.reference_source[
            reference_start : reference_start + length
        ]:
            raise SourceAlignmentError("TUI span does not select unchanged source bytes")
        return reference_start

    def validate_reference_span(self, start: int, length: int) -> None:
        self._require_source_range(self.reference_source, start, length)
        if self.changed and start < self.old_header_end:
            raise SourceAlignmentError("reference TUI span intersects the changed header")

    @staticmethod
    def _require_source_range(source: bytes, start: int, length: int) -> None:
        if start < 0 or length < 0 or start + length > len(source):
            raise SourceAlignmentError("TUI span is outside its verified source bytes")


def _parse_decimal(value: str, label: str) -> int:
    if DECIMAL.fullmatch(value) is None:
        raise SourceAlignmentError(f"malformed {label}")
    return int(value)


def _parse_span(value: str) -> tuple[int, int, int]:
    fields = publication_adapter._fields(value, "Span")
    if set(fields) != {"source_id", "start", "len"}:
        raise SourceAlignmentError("Span has unexpected fields")
    source_match = SOURCE_ID.fullmatch(fields["source_id"])
    if source_match is None:
        raise SourceAlignmentError("Span has malformed source id")
    return (
        int(source_match.group(1)),
        _parse_decimal(fields["start"], "Span start"),
        _parse_decimal(fields["len"], "Span length"),
    )


def _parse_parameter_spans(value: str) -> tuple[tuple[int, int, int], ...]:
    value = value.strip()
    if not value.startswith("[") or publication_adapter._group_end(value, 0) != len(value) - 1:
        raise SourceAlignmentError("function declaration has malformed parameter spans")
    return tuple(
        _parse_span(span)
        for span in publication_adapter._split_top_level(value[1:-1], ",")
    )


def _parse_publication(rendered: str) -> _ParsedPublication:
    prefix, entries = publication_adapter._struct_parts(rendered, "CheckOutput")
    fields = []
    values: dict[str, str] = {}
    for entry in entries:
        name, value = publication_adapter._field(entry)
        if name in values:
            raise publication_adapter.AdapterError("duplicate CheckOutput Debug field")
        fields.append(_PublicationField(name, entry))
        values[name] = value
    return _ParsedPublication(prefix, tuple(fields), values)


def _tui_function_declarations(output: Mapping[str, str]) -> dict[str, _FunctionDeclaration]:
    checked = publication_adapter._fields(output["checked_declarations"], "CheckedDeclarations")
    functions = publication_adapter._map_entries(checked["functions"])
    declarations = {}
    for definition_id, declaration in functions.items():
        if FUNCTION_ID.fullmatch(definition_id) is None:
            raise SourceAlignmentError("checked function declaration has malformed id")
        fields = publication_adapter._fields(declaration, "CheckedFunctionDeclaration")
        namespace = fields.get("namespace", "")
        if TUI_NAMESPACE in namespace and namespace != TUI_NAMESPACE_DEBUG:
            raise SourceAlignmentError("checked function declaration has a spoofed TUI namespace")
        if namespace != TUI_NAMESPACE_DEBUG:
            continue
        name_match = FUNCTION_NAME.fullmatch(fields.get("name", ""))
        if name_match is None or fields.get("internal") != "true":
            raise SourceAlignmentError("checked TUI declaration is not an internal named function")
        kind = fields.get("kind", "")
        if FUNCTION_KIND.fullmatch(kind) is None:
            raise SourceAlignmentError("checked TUI declaration has malformed function kind")
        declaration_value = _FunctionDeclaration(
            definition_id,
            name_match.group(1),
            kind,
            _parse_span(fields.get("body", "")),
            _parse_parameter_spans(fields.get("parameter_spans", "")),
        )
        declarations[definition_id] = declaration_value
    return declarations


def _known_source_edit(reference_source: bytes, candidate_source: bytes) -> _SourceEdit:
    return _SourceEdit(reference_source, candidate_source)


def _aligned_exact_owner_span(value: str, source_id: int, edit: _SourceEdit) -> str:
    try:
        span_source_id, start, length = _parse_span(value)
    except publication_adapter.AdapterError:
        raise SourceAlignmentError("exact owner has a malformed Span") from None
    if span_source_id != source_id:
        return value
    aligned_start = edit.current_span_to_reference(start, length)
    if aligned_start == start:
        return value
    matches = list(SPAN_START.finditer(value))
    if len(matches) != 1:
        raise SourceAlignmentError("Span has malformed start field")
    match = matches[0]
    return value[: match.start(1)] + str(aligned_start) + value[match.end(1) :]


def _align_statement_owner_rows(
    statement_owners: Mapping[str, tuple[str, str]],
    source_id: int,
    edit: _SourceEdit,
) -> dict[str, tuple[str, str]]:
    if not isinstance(statement_owners, Mapping):
        raise SourceAlignmentError("exact statement owners are malformed")
    aligned = {}
    for statement_id, row in statement_owners.items():
        if not isinstance(statement_id, str) or re.fullmatch(
            r"StmtId\(\d+\)", statement_id
        ) is None:
            raise SourceAlignmentError("exact statement owner has a malformed statement id")
        if (
            not isinstance(row, tuple)
            or len(row) != 2
            or not isinstance(row[0], str)
            or not isinstance(row[1], str)
        ):
            raise SourceAlignmentError("exact statement owner has a malformed statement row")
        if re.fullmatch(r"stmt:[A-Za-z][A-Za-z0-9_]*", row[1]) is None:
            raise SourceAlignmentError("exact statement owner has a malformed statement kind")
        aligned[statement_id] = (
            _aligned_exact_owner_span(row[0], source_id, edit),
            row[1],
        )
    return aligned


def _align_tui_statement_owners(
    statement_owners: Mapping[str, tuple[str, str]],
    source_id: int,
    reference_tui_source: bytes,
    candidate_tui_source: bytes,
) -> dict[str, tuple[str, str]]:
    edit = _known_source_edit(reference_tui_source, candidate_tui_source)
    return _align_statement_owner_rows(statement_owners, source_id, edit)


def _align_tui_owner_facts(
    *,
    exact_owners: Mapping[str, object],
    statement_owners: Mapping[str, tuple[str, str]],
    tui_source_id: int | None,
    reference_tui_source: bytes,
    candidate_tui_source: bytes,
) -> tuple[dict[str, object], dict[str, tuple[str, str]]]:
    aligned = deepcopy(dict(exact_owners))
    if tui_source_id is None:
        return aligned, dict(statement_owners)

    edit = _known_source_edit(reference_tui_source, candidate_tui_source)
    exprs = aligned["exprs"]
    for expression_id, (span_value, tag, name) in exprs.items():
        exprs[expression_id] = (
            _aligned_exact_owner_span(span_value, tui_source_id, edit),
            tag,
            name,
        )
    runs = aligned["runs"]
    for run_id, (span_value, propagate, head_kind) in runs.items():
        runs[run_id] = (
            _aligned_exact_owner_span(span_value, tui_source_id, edit),
            propagate,
            head_kind,
        )
    structured_stages = aligned.get("structured_stages", {})
    if not isinstance(structured_stages, Mapping):
        raise SourceAlignmentError("exact structured stages are malformed")
    for stage_id, row in structured_stages.items():
        if (
            type(stage_id) is not int
            or stage_id < 0
            or not isinstance(row, tuple)
            or len(row) != 4
            or not isinstance(row[0], str)
            or not isinstance(row[1], str)
            or type(row[2]) is not bool
            or type(row[3]) is not bool
        ):
            raise SourceAlignmentError("exact structured stage row is malformed")
        structured_stages[stage_id] = (
            _aligned_exact_owner_span(row[0], tui_source_id, edit),
            row[1],
            row[2],
            row[3],
        )
    for record in aligned["records"].values():
        record["span"] = _aligned_exact_owner_span(record["span"], tui_source_id, edit)
        fields = record["fields"]
        for field_id, (kind, name, span_value, key_expr, value_expr) in fields.items():
            fields[field_id] = (
                kind,
                name,
                _aligned_exact_owner_span(span_value, tui_source_id, edit),
                key_expr,
                value_expr,
            )
    aliases = aligned["aliases"]
    for alias_key, (expression_id, span_value, name) in aliases.items():
        aliases[alias_key] = (
            expression_id,
            _aligned_exact_owner_span(span_value, tui_source_id, edit),
            name,
        )
    tails = aligned["tails"]
    for statement_id, (span_value, name, kind) in tails.items():
        tails[statement_id] = (
            _aligned_exact_owner_span(span_value, tui_source_id, edit),
            name,
            kind,
        )
    binding_initializers = aligned["binding_initializers"]
    if not isinstance(binding_initializers, Mapping):
        raise SourceAlignmentError("exact binding initializers are malformed")
    for key, row in binding_initializers.items():
        if (
            not isinstance(key, tuple)
            or len(key) != 2
            or type(key[0]) is not int
            or key[0] < 0
            or not isinstance(key[1], str)
            or not key[1]
            or not isinstance(row, tuple)
            or len(row) != 2
            or not isinstance(row[0], str)
            or type(row[1]) is not int
            or row[1] < 0
        ):
            raise SourceAlignmentError("exact binding initializer row is malformed")
        binding_initializers[key] = (
            _aligned_exact_owner_span(row[0], tui_source_id, edit),
            row[1],
        )
    aligned_statement_owners = _align_statement_owner_rows(
        statement_owners, tui_source_id, edit
    )
    return aligned, aligned_statement_owners


def align_tui_exact_owners(
    *,
    exact_owners: Mapping[str, object],
    tui_source_id: int | None,
    reference_tui_source: bytes,
    candidate_tui_source: bytes,
) -> dict[str, object]:
    aligned, _statement_owners = _align_tui_owner_facts(
        exact_owners=exact_owners,
        statement_owners={},
        tui_source_id=tui_source_id,
        reference_tui_source=reference_tui_source,
        candidate_tui_source=candidate_tui_source,
    )
    return aligned


def _function_identity(declaration: _FunctionDeclaration) -> tuple[str, str, str]:
    return declaration.definition_id, declaration.name, declaration.kind


def _validate_profile_declarations(
    reference: Mapping[str, _FunctionDeclaration],
    candidate: Mapping[str, _FunctionDeclaration],
    edit: _SourceEdit,
) -> None:
    if set(reference) != set(candidate):
        raise SourceAlignmentError("TUI function identities differ between publications")
    for definition_id in sorted(reference):
        old = reference[definition_id]
        new = candidate[definition_id]
        if _function_identity(old) != _function_identity(new):
            raise SourceAlignmentError("TUI function identities differ between publications")
        _, old_start, old_length = old.body
        _, new_start, new_length = new.body
        edit.validate_reference_span(old_start, old_length)
        if edit.current_span_to_reference(new_start, new_length) != old_start or old_length != new_length:
            raise SourceAlignmentError("TUI function body spans differ after source alignment")
        if len(old.parameter_spans) != len(new.parameter_spans):
            raise SourceAlignmentError("TUI function parameter spans differ after source alignment")
        for old_parameter, new_parameter in zip(old.parameter_spans, new.parameter_spans):
            _, old_parameter_start, old_parameter_length = old_parameter
            _, new_parameter_start, new_parameter_length = new_parameter
            edit.validate_reference_span(old_parameter_start, old_parameter_length)
            if (
                edit.current_span_to_reference(new_parameter_start, new_parameter_length)
                != old_parameter_start
                or old_parameter_length != new_parameter_length
            ):
                raise SourceAlignmentError("TUI function parameter spans differ after source alignment")


def _validate_profile_consistency(
    declarations_by_profile: Sequence[dict[str, _FunctionDeclaration]],
    side: str,
) -> None:
    expected_ids = set(declarations_by_profile[0])
    expected_declarations = {}
    for declarations in declarations_by_profile:
        if set(declarations) != expected_ids:
            raise SourceAlignmentError(
                f"TUI function identities differ across declaration-bearing {side} profiles"
            )
        for definition_id, declaration in declarations.items():
            previous = expected_declarations.setdefault(definition_id, declaration)
            if previous != declaration:
                raise SourceAlignmentError(
                    f"TUI function declarations disagree across {side} profiles"
                )


def _span_key(value: str) -> tuple[int, int, int]:
    return _parse_span(value)


def _sort_span_map(value: str) -> str:
    if not value.startswith("{") or publication_adapter._group_end(value, 0) != len(value) - 1:
        raise SourceAlignmentError("expected a span-keyed publication map")
    publication_adapter._map_entries(value)
    entries = []
    for entry in publication_adapter._split_top_level(value[1:-1], ","):
        pair = publication_adapter._split_first_top_level(entry, ":")
        if pair is None:
            raise SourceAlignmentError("malformed span-keyed publication map entry")
        entries.append((_span_key(pair[0]), entry))
    entries.sort(key=lambda item: item[0])
    return "{" + ",".join(entry for _, entry in entries) + "}"


def _sorted_publication_span_maps(publication: _ParsedPublication) -> _ParsedPublication:
    replacements = {}
    for field_name in ORDERED_SPAN_MAP_FIELDS:
        if field_name not in publication.values:
            raise SourceAlignmentError(f"CheckOutput lacks {field_name} span map")
        replacements[field_name] = _sort_span_map(publication.values[field_name])
    fields = tuple(
        _PublicationField(field.name, f"{field.name}:{replacements[field.name]}")
        if field.name in replacements
        else field
        for field in publication.fields
    )
    values = {**publication.values, **replacements}
    return _ParsedPublication(publication.prefix, fields, values)


def _render_publication(publication: _ParsedPublication) -> str:
    entries = ",".join(field.rendered for field in publication.fields)
    return f"{publication.prefix}{{{entries}}}"


def _span_struct_start(value: str, index: int) -> int | None:
    if not value.startswith("Span", index):
        return None
    if index and (value[index - 1].isalnum() or value[index - 1] == "_"):
        return None
    cursor = index + len("Span")
    if cursor < len(value) and (value[cursor].isalnum() or value[cursor] == "_"):
        return None
    while cursor < len(value) and value[cursor].isspace():
        cursor += 1
    return cursor if cursor < len(value) and value[cursor] == "{" else None


def _rewrite_publication_spans(rendered: str, source_id: int, edit: _SourceEdit) -> str:
    pieces = []
    unchanged_start = 0
    index = 0
    while match := _SPAN_REWRITE_TOKENS.search(rendered, index):
        index = match.start()
        quoted_end = publication_adapter._quoted_end(rendered, index)
        if quoted_end is not None:
            index = quoted_end
            continue
        opening = _span_struct_start(rendered, index)
        if opening is None:
            index += 1
            continue
        closing = publication_adapter._group_end(rendered, opening)
        span_end = closing + 1
        span_value = rendered[index:span_end]
        found_source_id, start, length = _parse_span(span_value)
        if found_source_id == source_id:
            aligned_start = edit.current_span_to_reference(start, length)
            if aligned_start != start:
                matches = list(SPAN_START.finditer(span_value))
                if len(matches) != 1:
                    raise SourceAlignmentError("Span has malformed start field")
                match = matches[0]
                span_value = (
                    span_value[: match.start(1)]
                    + str(aligned_start)
                    + span_value[match.end(1) :]
                )
            pieces.append(rendered[unchanged_start:index])
            pieces.append(span_value)
            unchanged_start = span_end
        index = span_end
    pieces.append(rendered[unchanged_start:])
    return "".join(pieces)


def _aligned_owner_map(
    owners: Mapping[str, set[str]],
    source_id: int,
    edit: _SourceEdit,
    candidate: bool,
) -> dict[str, set[str]]:
    aligned = {}
    for raw_span, kinds in owners.items():
        span_source, start, length = _parse_span(raw_span)
        if span_source != source_id:
            continue
        if not kinds:
            raise SourceAlignmentError(f"AST owner has no kinds at {raw_span}")
        if candidate:
            aligned_start = edit.current_span_to_reference(start, length)
        else:
            edit.validate_reference_span(start, length)
            aligned_start = start
        key = f"Span{{source_id:SourceId({span_source}),start:{aligned_start},len:{length}}}"
        if key in aligned:
            raise SourceAlignmentError(f"TUI AST owner spans collide after source alignment: {key}")
        aligned[key] = set(kinds)
    return aligned


def _all_tui_source_ids(
    reference_profiles: Sequence[dict[str, _FunctionDeclaration]],
    candidate_profiles: Sequence[dict[str, _FunctionDeclaration]],
) -> set[int]:
    ids = set()
    for declarations in (*reference_profiles, *candidate_profiles):
        for declaration in declarations.values():
            ids.add(declaration.body[0])
            ids.update(span[0] for span in declaration.parameter_spans)
    return ids


def align_tui_publications(
    *,
    reference_publications: Sequence[str],
    candidate_publications: Sequence[str],
    reference_source_sizes: Mapping[int, int],
    reference_source_owners: Mapping[str, set[str]],
    candidate_source_sizes: Mapping[int, int],
    candidate_source_owners: Mapping[str, set[str]],
    reference_tui_source: bytes,
    candidate_tui_source: bytes,
) -> AlignedTuiPublications:
    return _align_tui_publications_with_parsed(
        reference_publications=reference_publications,
        candidate_publications=candidate_publications,
        reference_source_sizes=reference_source_sizes,
        reference_source_owners=reference_source_owners,
        candidate_source_sizes=candidate_source_sizes,
        candidate_source_owners=candidate_source_owners,
        reference_tui_source=reference_tui_source,
        candidate_tui_source=candidate_tui_source,
    ).aligned


def _align_tui_publications_with_parsed(
    *,
    reference_publications: Sequence[str],
    candidate_publications: Sequence[str],
    reference_source_sizes: Mapping[int, int],
    reference_source_owners: Mapping[str, set[str]],
    candidate_source_sizes: Mapping[int, int],
    candidate_source_owners: Mapping[str, set[str]],
    reference_tui_source: bytes,
    candidate_tui_source: bytes,
) -> _AlignedTuiPublicationParts:
    if len(reference_publications) != len(candidate_publications):
        raise SourceAlignmentError("reference and candidate publication counts differ")
    edit = _known_source_edit(reference_tui_source, candidate_tui_source)
    reference_declarations = []
    candidate_declarations = []
    parsed_reference = []
    parsed_candidate = []
    reference_publication_cache: dict[str, _ParsedPublication] = {}
    candidate_publication_cache: dict[str, _ParsedPublication] = {}
    declaration_profiles = []
    for reference, candidate in zip(reference_publications, candidate_publications):
        if not reference or not candidate:
            if reference != candidate:
                raise SourceAlignmentError("reference and candidate publication presence differs")
            reference_declarations.append({})
            candidate_declarations.append({})
            parsed_reference.append(None)
            parsed_candidate.append(None)
            continue
        reference_parsed = reference_publication_cache.get(reference)
        if reference_parsed is None:
            reference_parsed = _parse_publication(reference)
            reference_publication_cache[reference] = reference_parsed
        candidate_parsed = candidate_publication_cache.get(candidate)
        if candidate_parsed is None:
            candidate_parsed = _parse_publication(candidate)
            candidate_publication_cache[candidate] = candidate_parsed
        reference_functions = _tui_function_declarations(reference_parsed.values)
        candidate_functions = _tui_function_declarations(candidate_parsed.values)
        if bool(reference_functions) != bool(candidate_functions):
            raise SourceAlignmentError("TUI function identities differ between publications")
        if reference_functions:
            declaration_profiles.append((reference_functions, candidate_functions))
        reference_declarations.append(reference_functions)
        candidate_declarations.append(candidate_functions)
        parsed_reference.append(reference_parsed)
        parsed_candidate.append(candidate_parsed)
    if not declaration_profiles:
        if dict(reference_source_sizes) != dict(candidate_source_sizes):
            raise SourceAlignmentError("source sizes differ without TUI declaration metadata")
        normalized_reference_parsed = tuple(
            _sorted_publication_span_maps(parsed) if parsed is not None else None
            for parsed in parsed_reference
        )
        normalized_candidate_parsed = tuple(
            _sorted_publication_span_maps(parsed) if parsed is not None else None
            for parsed in parsed_candidate
        )
        normalized_reference = tuple(
            _render_publication(parsed) if parsed is not None else value
            for value, parsed in zip(reference_publications, normalized_reference_parsed)
        )
        normalized_candidate = tuple(
            _render_publication(parsed) if parsed is not None else value
            for value, parsed in zip(candidate_publications, normalized_candidate_parsed)
        )
        return _AlignedTuiPublicationParts(
            AlignedTuiPublications(
                normalized_reference,
                normalized_candidate,
                dict(candidate_source_sizes),
                {span: set(kinds) for span, kinds in candidate_source_owners.items()},
                None,
            ),
            normalized_reference_parsed,
            normalized_candidate_parsed,
        )
    source_ids = _all_tui_source_ids(reference_declarations, candidate_declarations)
    if len(source_ids) != 1:
        raise SourceAlignmentError("TUI source ids disagree across profiles")
    source_id = next(iter(source_ids))
    _validate_profile_consistency(
        [reference for reference, _candidate in declaration_profiles],
        "reference",
    )
    _validate_profile_consistency(
        [candidate for _reference, candidate in declaration_profiles],
        "candidate",
    )
    if source_id not in reference_source_sizes or source_id not in candidate_source_sizes:
        raise SourceAlignmentError("checked TUI declaration source id is absent from source sizes")
    reference_size = reference_source_sizes.get(source_id)
    candidate_size = candidate_source_sizes.get(source_id)
    if reference_size != len(reference_tui_source) or candidate_size != len(candidate_tui_source):
        raise SourceAlignmentError("TUI source size differs from source bytes")
    for reference_functions, candidate_functions in declaration_profiles:
        _validate_profile_declarations(reference_functions, candidate_functions, edit)
    reference_tui_owners = _aligned_owner_map(
        reference_source_owners, source_id, edit, candidate=False
    )
    candidate_tui_owners = _aligned_owner_map(
        candidate_source_owners, source_id, edit, candidate=True
    )
    if reference_tui_owners != candidate_tui_owners:
        raise SourceAlignmentError("TUI AST owners differ after source alignment")
    if not candidate_tui_owners:
        raise SourceAlignmentError("TUI AST owner index has no spans for its checked declarations")
    aligned_candidate_sizes = dict(candidate_source_sizes)
    aligned_candidate_sizes[source_id] = len(reference_tui_source)
    if aligned_candidate_sizes != dict(reference_source_sizes):
        raise SourceAlignmentError("source sizes differ outside TUI alignment")
    aligned_candidate_owners = {}
    for span, kinds in candidate_source_owners.items():
        span_source, _start, _length = _parse_span(span)
        if span_source != source_id:
            aligned_candidate_owners[span] = set(kinds)
    aligned_candidate_owners.update(candidate_tui_owners)
    aligned_reference_parsed = tuple(
        _sorted_publication_span_maps(parsed) if parsed is not None else None
        for parsed in parsed_reference
    )
    aligned_reference_publications = tuple(
        _render_publication(parsed) if parsed is not None else value
        for value, parsed in zip(reference_publications, aligned_reference_parsed)
    )
    aligned_candidate_publications = []
    aligned_candidate_parsed = []
    for value in candidate_publications:
        if not value:
            aligned_candidate_publications.append(value)
            aligned_candidate_parsed.append(None)
            continue
        rewritten = _rewrite_publication_spans(value, source_id, edit)
        parsed = _sorted_publication_span_maps(_parse_publication(rewritten))
        aligned_candidate_publications.append(_render_publication(parsed))
        aligned_candidate_parsed.append(parsed)
    return _AlignedTuiPublicationParts(
        AlignedTuiPublications(
            aligned_reference_publications,
            tuple(aligned_candidate_publications),
            aligned_candidate_sizes,
            aligned_candidate_owners,
            source_id,
        ),
        aligned_reference_parsed,
        tuple(aligned_candidate_parsed),
    )
