import unittest
from copy import deepcopy
from pathlib import Path

import w9_publication_adapter as publication_adapter
import w9_publication_sources as publication_sources
from w9_publication_sources import (
    CURRENT_HEADER,
    SourceAlignmentError,
    _align_tui_statement_owners,
    align_tui_exact_owners,
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
    def test_span_rewriter_preserves_quote_and_identifier_boundaries(self):
        edit = publication_sources._known_source_edit(OLD_SOURCE, CURRENT_SOURCE)
        start = CURRENT_SOURCE.index(b"export pure reset")
        length = len(b"export pure reset")
        target = span(5, start, length)
        aligned_target = span(5, start - DELTA, length)
        for rendered in (f'quoted:"  {target}  "', f"quoted:'{target}'"):
            with self.subTest(rendered=rendered[:20]):
                self.assertEqual(
                    publication_sources._rewrite_publication_spans(rendered, 5, edit),
                    rendered,
                )

        self.assertEqual(
            publication_sources._rewrite_publication_spans(f"lifetime:'a + {target}", 5, edit),
            f"lifetime:'a + {aligned_target}",
        )
        for rendered in (
            f"boundaries:Spanish{{source_id:SourceId(5),start:{start},len:{length}}}",
            f"boundaries:xSpan{{source_id:SourceId(5),start:{start},len:{length}}}",
            f"boundaries:éSpan{{source_id:SourceId(5),start:{start},len:{length}}}",
            f"boundaries:Spané{{source_id:SourceId(5),start:{start},len:{length}}}",
        ):
            with self.subTest(rendered=rendered[:20]):
                self.assertEqual(
                    publication_sources._rewrite_publication_spans(rendered, 5, edit),
                    rendered,
                )

        with self.assertRaisesRegex(publication_adapter.AdapterError, "unterminated Rust Debug value"):
            publication_sources._rewrite_publication_spans(
                f"malformed:Span{{source_id:SourceId(5),start:{start},len:{length}",
                5,
                edit,
            )

    def test_exact_owner_spans_align_across_all_tables_and_keep_unrelated_spans(self):
        source_id = 5
        candidate_start = CURRENT_SOURCE.index(CURRENT_HEADER) + len(CURRENT_HEADER) + 4
        field_start = candidate_start + 1
        run_start = candidate_start + 2
        tail_start = candidate_start + 3
        unrelated_span = debug_span(0, 2, 1)
        exact_owners = {
            "exprs": {
                0: (span(source_id, candidate_start, 1), "Record", ""),
                1: (span(source_id, field_start, 1), "Ident", "name"),
                2: (unrelated_span, "Ident", "spare"),
                3: (unrelated_span, "Record", ""),
                4: (unrelated_span, "Ident", "other"),
            },
            "runs": {
                0: (debug_span(source_id, run_start, 1), True),
                1: (unrelated_span, False),
            },
            "records": {
                0: {
                    "span": span(source_id, candidate_start, 1),
                    "count": 1,
                    "fields": {
                        0: ("shorthand", "name", span(source_id, field_start, 1), None, None),
                    },
                },
                3: {
                    "span": unrelated_span,
                    "count": 1,
                    "fields": {
                        1: ("shorthand", "other", unrelated_span, None, None),
                    },
                },
            },
            "aliases": {
                (0, 0): (1, span(source_id, field_start, 1), "name"),
                (3, 1): (4, unrelated_span, "other"),
            },
            "tails": {
                0: (span(source_id, tail_start, 1), "tail", "stmt:TailBareIdent"),
                1: (unrelated_span, "other", "stmt:TailBareIdent"),
            },
            "binding_initializers": {
                (17, "%dest"): (span(source_id, tail_start + 1, 1), 4),
                (18, "spare"): (unrelated_span, 5),
            },
            "expr_count": 5,
            "record_field_count": 2,
            "extra": {"names": ["preserved"]},
        }
        original = deepcopy(exact_owners)

        aligned = align_tui_exact_owners(
            exact_owners=exact_owners,
            tui_source_id=source_id,
            reference_tui_source=OLD_SOURCE,
            candidate_tui_source=CURRENT_SOURCE,
        )

        expected_start = candidate_start - DELTA
        self.assertEqual(aligned["exprs"][0], (span(source_id, expected_start, 1), "Record", ""))
        self.assertEqual(
            aligned["exprs"][1],
            (span(source_id, expected_start + 1, 1), "Ident", "name"),
        )
        self.assertEqual(aligned["exprs"][2], (unrelated_span, "Ident", "spare"))
        self.assertEqual(
            aligned["runs"],
            {0: (debug_span(source_id, expected_start + 2, 1), True), 1: (unrelated_span, False)},
        )
        self.assertEqual(aligned["records"][0]["span"], span(source_id, expected_start, 1))
        self.assertEqual(
            aligned["records"][0]["fields"][0],
            ("shorthand", "name", span(source_id, expected_start + 1, 1), None, None),
        )
        self.assertEqual(
            aligned["aliases"],
            {
                (0, 0): (1, span(source_id, expected_start + 1, 1), "name"),
                (3, 1): (4, unrelated_span, "other"),
            },
        )
        self.assertEqual(aligned["records"][3]["span"], unrelated_span)
        self.assertEqual(aligned["records"][3]["count"], exact_owners["records"][3]["count"])
        self.assertEqual(
            aligned["records"][3]["fields"][1],
            ("shorthand", "other", unrelated_span, None, None),
        )
        self.assertEqual(
            aligned["tails"],
            {
                0: (span(source_id, expected_start + 3, 1), "tail", "stmt:TailBareIdent"),
                1: (unrelated_span, "other", "stmt:TailBareIdent"),
            },
        )
        self.assertEqual(
            aligned["binding_initializers"],
            {
                (17, "%dest"): (span(source_id, expected_start + 4, 1), 4),
                (18, "spare"): (unrelated_span, 5),
            },
        )
        self.assertEqual(aligned["extra"], exact_owners["extra"])
        self.assertEqual(aligned["expr_count"], exact_owners["expr_count"])
        self.assertEqual(aligned["record_field_count"], exact_owners["record_field_count"])
        self.assertIsNot(aligned, exact_owners)
        self.assertIsNot(aligned["extra"], exact_owners["extra"])
        self.assertEqual(exact_owners, original)

    def test_exact_owner_facts_without_tui_source_id_are_deep_copied_unchanged(self):
        exact_owners = {
            "exprs": {0: (debug_span(5, 9, 2), "Ident", "value")},
            "runs": {},
            "records": {},
            "aliases": {},
            "tails": {},
            "binding_initializers": {},
            "expr_count": 1,
            "record_field_count": 0,
            "extra": {"names": ["preserved"]},
        }

        aligned = align_tui_exact_owners(
            exact_owners=exact_owners,
            tui_source_id=None,
            reference_tui_source=b"unrelated reference bytes",
            candidate_tui_source=b"unrelated candidate bytes",
        )

        self.assertEqual(aligned, exact_owners)
        self.assertIsNot(aligned, exact_owners)
        self.assertIsNot(aligned["exprs"], exact_owners["exprs"])
        self.assertIsNot(aligned["extra"], exact_owners["extra"])

    def test_exact_owner_span_crossing_changed_header_is_rejected(self):
        header_start = CURRENT_SOURCE.index(CURRENT_HEADER)
        exact_owners = {
            "exprs": {0: (span(5, header_start + 1, 1), "Ident", "value")},
            "runs": {},
            "records": {},
            "aliases": {},
            "tails": {},
            "binding_initializers": {},
            "expr_count": 1,
            "record_field_count": 0,
        }

        with self.assertRaisesRegex(SourceAlignmentError, "intersects the changed header"):
            align_tui_exact_owners(
                exact_owners=exact_owners,
                tui_source_id=5,
                reference_tui_source=OLD_SOURCE,
                candidate_tui_source=CURRENT_SOURCE,
            )

    def test_exact_owner_alignment_rejects_source_edits_outside_known_header(self):
        edited = CURRENT_SOURCE.replace(b"\\u{1b}[0m", b"\\u{1b}[31m", 1)
        exact_owners = {
            "exprs": {},
            "runs": {},
            "records": {},
            "aliases": {},
            "tails": {},
            "binding_initializers": {},
            "expr_count": 0,
            "record_field_count": 0,
        }

        with self.assertRaisesRegex(SourceAlignmentError, "differs outside the known TUI header"):
            align_tui_exact_owners(
                exact_owners=exact_owners,
                tui_source_id=5,
                reference_tui_source=OLD_SOURCE,
                candidate_tui_source=edited,
            )

    def test_exact_owner_alignment_rejects_malformed_span(self):
        exact_owners = {
            "exprs": {0: ("not-a-span", "Ident", "value")},
            "runs": {},
            "records": {},
            "aliases": {},
            "tails": {},
            "binding_initializers": {},
            "expr_count": 1,
            "record_field_count": 0,
        }

        with self.assertRaises(publication_adapter.AdapterError):
            align_tui_exact_owners(
                exact_owners=exact_owners,
                tui_source_id=5,
                reference_tui_source=OLD_SOURCE,
                candidate_tui_source=CURRENT_SOURCE,
            )

    def test_statement_owners_align_tui_spans_and_preserve_statement_identity(self):
        source_id = 5
        current_start = CURRENT_SOURCE.index(b"export pure reset")
        unrelated_span = debug_span(0, 2, 1)
        statement_owners = {
            "StmtId(20)": (span(source_id, current_start, 1), "stmt:Assert"),
            "StmtId(21)": (span(source_id, current_start + 1, 1), "stmt:AssertBare"),
            "StmtId(22)": (span(source_id, current_start + 2, 1), "stmt:DeferExpr"),
            "StmtId(23)": (unrelated_span, "stmt:LetExprNoTy"),
        }
        original = deepcopy(statement_owners)

        aligned = _align_tui_statement_owners(
            statement_owners,
            source_id,
            OLD_SOURCE,
            CURRENT_SOURCE,
        )

        reference_start = current_start - DELTA
        self.assertEqual(
            aligned,
            {
                "StmtId(20)": (span(source_id, reference_start, 1), "stmt:Assert"),
                "StmtId(21)": (span(source_id, reference_start + 1, 1), "stmt:AssertBare"),
                "StmtId(22)": (span(source_id, reference_start + 2, 1), "stmt:DeferExpr"),
                "StmtId(23)": (unrelated_span, "stmt:LetExprNoTy"),
            },
        )
        self.assertEqual(statement_owners, original)

    def test_statement_owner_alignment_rejects_malformed_rows_and_header_spans(self):
        cases = (
            ({"not-a-statement-id": (span(5, 100, 1), "stmt:Assert")}, "malformed statement id"),
            ({"StmtId(20)": (span(5, 100, 1), "expr:Ident")}, "malformed statement kind"),
            ({"StmtId(20)": (span(5, 100, 1),)}, "malformed statement row"),
            ({"StmtId(20)": ("not-a-span", "stmt:Assert")}, "malformed Span"),
        )
        for owners, message in cases:
            with self.subTest(message=message), self.assertRaisesRegex(
                SourceAlignmentError, message
            ):
                _align_tui_statement_owners(owners, 5, OLD_SOURCE, CURRENT_SOURCE)

        header_span = span(5, CURRENT_SOURCE.index(CURRENT_HEADER) + 1, 1)
        with self.assertRaisesRegex(SourceAlignmentError, "intersects the changed header"):
            _align_tui_statement_owners(
                {"StmtId(20)": (header_span, "stmt:Assert")},
                5,
                OLD_SOURCE,
                CURRENT_SOURCE,
            )

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
