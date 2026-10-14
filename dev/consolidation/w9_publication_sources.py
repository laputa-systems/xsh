from __future__ import annotations

import re
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


def _sort_publication_span_maps(publication: _ParsedPublication) -> str:
    replacements = {}
    for field_name in ORDERED_SPAN_MAP_FIELDS:
        if field_name not in publication.values:
            raise SourceAlignmentError(f"CheckOutput lacks {field_name} span map")
        replacements[field_name] = _sort_span_map(publication.values[field_name])
    entries = (
        f"{field.name}:{replacements[field.name]}"
        if field.name in replacements
        else field.rendered
        for field in publication.fields
    )
    return f"{publication.prefix}{{{','.join(entries)}}}"


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
    while index < len(rendered):
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
    if len(reference_publications) != len(candidate_publications):
        raise SourceAlignmentError("reference and candidate publication counts differ")
    edit = _known_source_edit(reference_tui_source, candidate_tui_source)
    reference_declarations = []
    candidate_declarations = []
    parsed_reference = []
    parsed_candidate = []
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
        reference_parsed = _parse_publication(reference)
        candidate_parsed = _parse_publication(candidate)
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
        normalized_reference = tuple(
            _sort_publication_span_maps(parsed) if parsed is not None else value
            for value, parsed in zip(reference_publications, parsed_reference)
        )
        normalized_candidate = tuple(
            _sort_publication_span_maps(parsed) if parsed is not None else value
            for value, parsed in zip(candidate_publications, parsed_candidate)
        )
        return AlignedTuiPublications(
            normalized_reference,
            normalized_candidate,
            dict(candidate_source_sizes),
            {span: set(kinds) for span, kinds in candidate_source_owners.items()},
            None,
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
    aligned_reference_publications = tuple(
        _sort_publication_span_maps(parsed) if parsed is not None else value
        for value, parsed in zip(reference_publications, parsed_reference)
    )
    aligned_candidate_publications = []
    for value in candidate_publications:
        if not value:
            aligned_candidate_publications.append(value)
            continue
        rewritten = _rewrite_publication_spans(value, source_id, edit)
        aligned_candidate_publications.append(
            _sort_publication_span_maps(_parse_publication(rewritten))
        )
    return AlignedTuiPublications(
        aligned_reference_publications,
        tuple(aligned_candidate_publications),
        aligned_candidate_sizes,
        aligned_candidate_owners,
        source_id,
    )
