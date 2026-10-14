import unittest
from pathlib import Path

import w9_publication_adapter as publication_adapter
import w9_publication_sources as publication_sources
from w9_publication_sources import (
    CURRENT_HEADER,
    SourceAlignmentError,
    align_tui_publications,
)


TUI_NAMESPACE = "<xsh-stdlib:tui>"
PINNED_REFERENCE_HEADER = (
    b"# `tui.left_pad`, `tui.right_pad`, and `tui.read_secret` stay native. Padding\n"
    b"# retains its width scan, while secret input owns terminal mode and descriptor\n"
    b"# lifetime.\n"
)
CURRENT_SOURCE = (Path(__file__).resolve().parents[2] / "stdlib/tui.xsh").read_bytes()
OLD_SOURCE = CURRENT_SOURCE.replace(CURRENT_HEADER, PINNED_REFERENCE_HEADER, 1)
DELTA = len(CURRENT_HEADER) - len(PINNED_REFERENCE_HEADER)


def span(source_id, start, length):
    return f"Span{{source_id:SourceId({source_id}),start:{start},len:{length}}}"


def debug_span(source_id, start, length):
    return f"Span{{len:{length},source_id:SourceId({source_id}),start:{start}}}"


def function_rows(source, source_id=5, name_change=None, body_start_override=None):
    rows = []
    spans = {}
    for definition_id, name in ((44, "reset"), (45, "bold")):
        declaration_start = source.index(f"export pure {name}".encode())
        block_start = source.index(b"{", declaration_start)
        block_end = source.index(b"}", block_start) + 1
        body_start = block_start if body_start_override is None else body_start_override
        body_length = block_end - body_start
        body = debug_span(source_id, body_start, body_length)
        selected_name = name_change if name == "bold" and name_change else name
        rows.append(
            f"FunctionDefId({definition_id}):CheckedFunctionDeclaration{{"
            f"body:{body},internal:true,kind:Pure,name:Name(\"{selected_name}\"),"
            f"namespace:Some(Name(\"{TUI_NAMESPACE}\")),parameter_spans:[],signature:None}}"
        )
        declaration = span(source_id, declaration_start, block_end - declaration_start)
        expression = span(source_id, block_start + 2, block_end - block_start - 4)
        spans[declaration] = {"stmt:Export", "stmt:PureDef"}
        spans[expression] = {"expr:Str"}
    return rows, spans


def publication(
    source,
    reverse_maps=False,
    name_change=None,
    body_start_override=None,
    spoof=False,
    include_tui=True,
):
    if include_tui:
        rows, _owner_spans = function_rows(
            source,
            name_change=name_change,
            body_start_override=body_start_override,
        )
    else:
        rows = []
        _owner_spans = {}
    if spoof:
        rows.append(
            "FunctionDefId(99):CheckedFunctionDeclaration{"
            "body:Span{len:1,source_id:SourceId(0),start:0},internal:true,kind:Pure,"
            f"name:Name(\"fake\"),namespace:Some(Name(\"{TUI_NAMESPACE}\")),"
            "parameter_spans:[],signature:None}"
        )
    keys = [span(0, 3, 2)]
    if include_tui:
        reset_start = source.index(b"export pure reset")
        reset_body = source.index(b"{", reset_start)
        bold_start = source.index(b"export pure bold")
        bold_body = source.index(b"{", bold_start)
        keys.extend((span(5, reset_body, 2), span(5, bold_body, 2)))
    entries = list(zip(keys, ("Bool", "Str", "Str")))
    if reverse_maps:
        entries.reverse()
    value_map = "{" + ",".join(f"{key}:{value}" for key, value in entries) + "}"
    statements = "{" + ",".join(f"{key}:Value" for key, _ in entries) + "}"
    quoted_span = 'Span{source_id:SourceId(5),start:999,len:2}'
    return (
        "CheckOutput{"
        f"checked_declarations:CheckedDeclarations{{functions:{{{','.join(rows)}}}}},"
        f"function_return_types:{value_map},expr_types:{value_map},"
        f"statement_positions:{statements},"
        f"opaque:Other{{literal:\"{quoted_span}\"}},"
        "selected_function_calls:{},statically_resolved_call_spans:{},"
        "standard_call_spans:{},api_calls:{},module_commands:{},"
        "record_constructor_instances:{},message_payload_constructors:{},error_constructors:{},"
        "stream_stage_types:{},propagating_statements:{},"
        "checked_control_flow:CheckedControlFlowFacts{complete:true,"
        "builtin_range_calls:{},cardinality_preserving_collection_calls:{},"
        "propagating_error_spans:{},latent_stream_error_spans:{},"
        "fallible_pipeline_stage_spans:{},propagating_stage_callback_spans:{},"
        "exhaustive_match_spans:{}}}"
    ), _owner_spans


def owner_index(source, source_id=5):
    rows, owners = function_rows(source, source_id=source_id)
    return {span_text: kinds for span_text, kinds in owners.items()}


def source_sizes(source, source_id=5, other_size=7):
    return {0: other_size, source_id: len(source)}


def aligned(*, reference=None, candidate=None, reference_owners=None, candidate_owners=None,
            reference_sizes=None, candidate_sizes=None):
    reference_publications = [
        publication(OLD_SOURCE, include_tui=False)[0],
        publication(OLD_SOURCE, include_tui=False)[0],
        reference or publication(OLD_SOURCE)[0],
        reference or publication(OLD_SOURCE)[0],
    ]
    candidate_publications = [
        publication(CURRENT_SOURCE, reverse_maps=True, include_tui=False)[0],
        publication(CURRENT_SOURCE, reverse_maps=True, include_tui=False)[0],
        candidate or publication(CURRENT_SOURCE, reverse_maps=True)[0],
        candidate or publication(CURRENT_SOURCE, reverse_maps=True)[0],
    ]
    return align_tui_publications(
        reference_publications=reference_publications,
        candidate_publications=candidate_publications,
        reference_source_sizes=reference_sizes or source_sizes(OLD_SOURCE),
        reference_source_owners=reference_owners or owner_index(OLD_SOURCE),
        candidate_source_sizes=candidate_sizes or source_sizes(CURRENT_SOURCE),
        candidate_source_owners=candidate_owners or owner_index(CURRENT_SOURCE),
        reference_tui_source=OLD_SOURCE,
        candidate_tui_source=CURRENT_SOURCE,
    )


class TuiPublicationSourceAlignmentTests(unittest.TestCase):
    def test_pinned_reference_tui_header_is_the_only_accepted_source_edit(self):
        self.assertEqual(DELTA, 45)
        self.assertEqual(CURRENT_SOURCE.count(CURRENT_HEADER), 1)
        self.assertEqual(OLD_SOURCE.count(PINNED_REFERENCE_HEADER), 1)
        self.assertEqual(
            CURRENT_SOURCE,
            OLD_SOURCE.replace(PINNED_REFERENCE_HEADER, CURRENT_HEADER, 1),
        )

        result = aligned()

        self.assertEqual(result.tui_source_id, 5)
        self.assertEqual(result.reference_publications, result.candidate_publications)
        self.assertEqual(result.candidate_source_sizes, source_sizes(OLD_SOURCE))
        self.assertEqual(result.candidate_source_owners, owner_index(OLD_SOURCE))
        for rendered in result.candidate_publications:
            self.assertIn('literal:"Span{source_id:SourceId(5),start:999,len:2}"', rendered)

    def test_actual_adapter_alignment_parses_noncanonical_owner_span_fields(self):
        self.assertIs(publication_sources.publication_adapter, publication_adapter)
        candidate_owners = owner_index(CURRENT_SOURCE)
        canonical_span = next(iter(candidate_owners))
        source_id, start, length = publication_sources._parse_span(canonical_span)
        kinds = candidate_owners.pop(canonical_span)
        candidate_owners[debug_span(source_id, start, length)] = kinds

        result = aligned(candidate_owners=candidate_owners)

        self.assertEqual(result.candidate_source_owners, owner_index(OLD_SOURCE))

    def test_rejects_tui_declarations_from_multiple_source_ids(self):
        reference, _ = publication(OLD_SOURCE, spoof=True)
        candidate, _ = publication(CURRENT_SOURCE, spoof=True)

        with self.assertRaisesRegex(SourceAlignmentError, "source ids disagree across profiles"):
            aligned(reference=reference, candidate=candidate)

    def test_rejects_a_spoofed_tui_namespace(self):
        candidate, _ = publication(CURRENT_SOURCE)
        candidate = candidate.replace(
            f'namespace:Some(Name("{TUI_NAMESPACE}"))',
            f'namespace:Some(Name("{TUI_NAMESPACE}-spoof"))',
            1,
        )

        with self.assertRaisesRegex(SourceAlignmentError, "spoofed TUI namespace"):
            aligned(candidate=candidate)

    def test_rejects_source_size_that_does_not_match_verified_tui_bytes(self):
        bad_sizes = source_sizes(CURRENT_SOURCE)
        bad_sizes[5] -= 1

        with self.assertRaisesRegex(SourceAlignmentError, "size differs from source bytes"):
            aligned(candidate_sizes=bad_sizes)

    def test_rejects_source_size_drift_outside_the_tui_source(self):
        bad_sizes = source_sizes(CURRENT_SOURCE, other_size=8)

        with self.assertRaisesRegex(SourceAlignmentError, "source sizes differ outside TUI alignment"):
            aligned(candidate_sizes=bad_sizes)

    def test_rejects_code_edits_outside_the_known_header(self):
        edited = CURRENT_SOURCE.replace(b"\\u{1b}[0m", b"\\u{1b}[31m", 1)

        with self.assertRaisesRegex(SourceAlignmentError, "differs outside the known TUI header"):
            align_tui_publications(
                reference_publications=[publication(OLD_SOURCE)[0]],
                candidate_publications=[publication(edited)[0]],
                reference_source_sizes=source_sizes(OLD_SOURCE),
                reference_source_owners=owner_index(OLD_SOURCE),
                candidate_source_sizes=source_sizes(edited),
                candidate_source_owners=owner_index(edited),
                reference_tui_source=OLD_SOURCE,
                candidate_tui_source=edited,
            )

    def test_rejects_a_span_that_crosses_the_changed_header(self):
        changed_start = CURRENT_SOURCE.index(CURRENT_HEADER) + len(CURRENT_HEADER) - 1
        candidate, _ = publication(CURRENT_SOURCE, body_start_override=changed_start)

        with self.assertRaisesRegex(SourceAlignmentError, "intersects the changed header"):
            aligned(candidate=candidate)

    def test_requires_every_tui_ast_owner_span_and_kind_to_match(self):
        owners = owner_index(CURRENT_SOURCE)
        first_span = next(iter(owners))
        owners[first_span] = {"stmt:Other"}

        with self.assertRaisesRegex(SourceAlignmentError, "TUI AST owners differ"):
            aligned(candidate_owners=owners)

    def test_unrelated_owner_maps_may_differ_when_tui_owners_match(self):
        reference_owners = owner_index(OLD_SOURCE)
        candidate_owners = owner_index(CURRENT_SOURCE)
        unrelated_span = span(0, 3, 2)
        reference_owners[unrelated_span] = {"expr:Name"}
        candidate_owners[unrelated_span] = {"stmt:Return"}

        result = aligned(
            reference_owners=reference_owners,
            candidate_owners=candidate_owners,
        )

        self.assertEqual(result.candidate_source_owners[unrelated_span], {"stmt:Return"})

    def test_no_tui_publications_allow_unrelated_owner_map_differences(self):
        reference = publication(OLD_SOURCE, include_tui=False)[0]
        candidate = publication(OLD_SOURCE, include_tui=False)[0]
        reference_owners = {span(0, 3, 2): {"expr:Name"}}
        candidate_owners = {span(0, 3, 2): {"stmt:Return"}}

        result = align_tui_publications(
            reference_publications=[reference],
            candidate_publications=[candidate],
            reference_source_sizes=source_sizes(OLD_SOURCE),
            reference_source_owners=reference_owners,
            candidate_source_sizes=source_sizes(OLD_SOURCE),
            candidate_source_owners=candidate_owners,
            reference_tui_source=OLD_SOURCE,
            candidate_tui_source=OLD_SOURCE,
        )

        self.assertIsNone(result.tui_source_id)
        self.assertEqual(result.candidate_source_owners, candidate_owners)

    def test_no_tui_publications_still_reject_malformed_span_maps(self):
        reference = publication(OLD_SOURCE, include_tui=False)[0]
        candidate = reference.replace(
            "function_return_types:{",
            "function_return_types:{not-a-span:Bool,",
            1,
        )

        with self.assertRaisesRegex(
            publication_adapter.AdapterError,
            "expected named Span Debug struct",
        ):
            aligned(
                reference=reference,
                candidate=candidate,
                reference_sizes=source_sizes(OLD_SOURCE),
                candidate_sizes=source_sizes(OLD_SOURCE),
            )

    def test_requires_function_identities_to_match_across_publications(self):
        candidate, _ = publication(CURRENT_SOURCE, name_change="other")

        with self.assertRaisesRegex(SourceAlignmentError, "function identities differ"):
            aligned(candidate=candidate)

    def test_declaration_bearing_profiles_must_agree_on_function_identity(self):
        reference, _ = publication(OLD_SOURCE)
        reference_changed, _ = publication(OLD_SOURCE, name_change="other")
        candidate, _ = publication(CURRENT_SOURCE)
        candidate_changed, _ = publication(CURRENT_SOURCE, name_change="other")

        with self.assertRaisesRegex(SourceAlignmentError, "declarations disagree across reference profiles"):
            align_tui_publications(
                reference_publications=[reference, reference_changed],
                candidate_publications=[candidate, candidate_changed],
                reference_source_sizes=source_sizes(OLD_SOURCE),
                reference_source_owners=owner_index(OLD_SOURCE),
                candidate_source_sizes=source_sizes(CURRENT_SOURCE),
                candidate_source_owners=owner_index(CURRENT_SOURCE),
                reference_tui_source=OLD_SOURCE,
                candidate_tui_source=CURRENT_SOURCE,
            )

    def test_candidate_facts_are_validated_before_parity_rebasing(self):
        base, _ = publication(CURRENT_SOURCE)
        invalid_span = span(5, len(CURRENT_SOURCE) + 1, 1)
        invalid = base.replace(
            "selected_function_calls:{}",
            "selected_function_calls:{"
            f"{invalid_span}:CheckedFunctionTarget{{definition:FunctionDefId(44),"
            f"namespace:Some(Name(\"{TUI_NAMESPACE}\")),name:Name(\"reset\")}}"
            "}",
        ).replace(
            "statically_resolved_call_spans:{}",
            f"statically_resolved_call_spans:{{{invalid_span}}}",
        )

        with self.assertRaisesRegex(SourceAlignmentError, "outside its verified source"):
            aligned(candidate=invalid)

    def test_all_declaration_bearing_profiles_must_agree_on_source_id(self):
        reference, _ = publication(OLD_SOURCE)
        candidate, _ = publication(CURRENT_SOURCE)
        other_reference = reference.replace("SourceId(5)", "SourceId(6)")
        other_candidate = candidate.replace("SourceId(5)", "SourceId(6)")
        reference_owners = owner_index(OLD_SOURCE)
        candidate_owners = owner_index(CURRENT_SOURCE)
        reference_owners.update({key.replace("SourceId(5)", "SourceId(6)"): value
                                 for key, value in list(reference_owners.items())})
        candidate_owners.update({key.replace("SourceId(5)", "SourceId(6)"): value
                                 for key, value in list(candidate_owners.items())})
        reference_sizes = source_sizes(OLD_SOURCE)
        candidate_source_sizes = source_sizes(CURRENT_SOURCE)
        reference_sizes[6] = len(OLD_SOURCE)
        candidate_source_sizes[6] = len(CURRENT_SOURCE)

        with self.assertRaisesRegex(SourceAlignmentError, "source ids disagree across profiles"):
            align_tui_publications(
                reference_publications=[reference, other_reference],
                candidate_publications=[candidate, other_candidate],
                reference_source_sizes=reference_sizes,
                reference_source_owners=reference_owners,
                candidate_source_sizes=candidate_source_sizes,
                candidate_source_owners=candidate_owners,
                reference_tui_source=OLD_SOURCE,
                candidate_tui_source=CURRENT_SOURCE,
            )


if __name__ == "__main__":
    unittest.main()
