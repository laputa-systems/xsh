import pathlib
import sys
import os
import hashlib
import json
import tempfile
import unittest
from unittest import mock
from types import SimpleNamespace


sys.path.insert(0, str(pathlib.Path(__file__).parent))
import w9_publication_adapter_runner as adapter_runner
import w9_publication_exact_owner_runner as exact_runner


SOURCE = "SourceId(0)"


def span(start, length):
    return f"Span {{ source_id: {SOURCE}, start: {start}, len: {length} }}"


def owner_index():
    rows = ["source\tSourceId(0)\t100"]
    expressions = (
        (1, 0, 50, "Record", '""'),
        (2, 5, 5, "Ident", '"first"'),
        (3, 10, 5, "Ident", '"alias"'),
        (4, 50, 40, "Record", '""'),
        (5, 55, 5, "Ident", '"second"'),
        (6, 60, 5, "Ident", '"again"'),
    )
    for expression_id, start, length, tag, name in expressions:
        current_span = span(start, length)
        rows.append(f"owner\t{current_span}\texpr:{tag}")
        rows.append(f"expr\tExprId({expression_id})\t{current_span}\t{tag}\t{name}")
    tail_span = span(90, 5)
    rows.extend(
        (
            f"stmt\tStmtId(1)\t{tail_span}\tstmt:TailBareIdent",
            f"tail\tStmtId(1)\t{tail_span}\tlast\tstmt:TailBareIdent",
            f"owner\t{tail_span}\tstmt:TailBareIdent",
            "exact-counts\t6\t4",
            "binding-initializer-count\t0",
            "run-form-count\t0",
            "structured-stage-count\t0",
            "structured-stage-callback-count\t0",
            f"record\tExprId(1)\t{span(0, 50)}\t2",
        f'record-field\tExprId(1)\t0\tnamed\t"first"\t{span(1, 4)}\t\tExprId(2)',
        f'record-field\tExprId(1)\t1\tshorthand\t"alias"\t{span(10, 5)}\t\t',
        f'record-shorthand-alias\tExprId(1)\t1\tExprId(3)\t{span(10, 5)}\t"alias"',
            f"record\tExprId(4)\t{span(50, 40)}\t2",
        f'record-field\tExprId(4)\t2\tnamed\t"second"\t{span(51, 4)}\t\tExprId(5)',
        f'record-field\tExprId(4)\t3\tshorthand\t"again"\t{span(60, 5)}\t\t',
        f'record-shorthand-alias\tExprId(4)\t3\tExprId(6)\t{span(60, 5)}\t"again"',
        )
    )
    return "\n".join(rows) + "\n"


def call_and_field_owner_index():
    rows = [
        "source\tSourceId(0)\t100",
        f'owner\t{span(0, 5)}\texpr:Ident',
        f'expr\tExprId(1)\t{span(0, 5)}\tIdent\t"callee"',
        f'owner\t{span(0, 12)}\texpr:Call',
        f'expr\tExprId(2)\t{span(0, 12)}\tCall\t""',
        f'owner\t{span(20, 4)}\texpr:Ident',
        f'expr\tExprId(3)\t{span(20, 4)}\tIdent\t"base"',
        f'owner\t{span(20, 9)}\texpr:Field',
        f'expr\tExprId(4)\t{span(20, 9)}\tField\t""',
        "call\tExprId(2)\tExprId(1)",
        "field-access\tExprId(4)\tExprId(3)\t" + r'"a\tb\n\r\0\\\"\u{1f642}"',
        "exact-counts\t4\t0",
        "run-form-count\t0",
        "structured-stage-count\t0",
        "structured-stage-callback-count\t0",
        "binding-initializer-count\t0",
    ]
    return "\n".join(rows) + "\n"


def named_and_path_owner_index():
    rows = [
        "source\tSourceId(0)\t100",
        f'owner\t{span(0, 50)}\texpr:Record',
        f'expr\tExprId(1)\t{span(0, 50)}\tRecord\t""',
        f'owner\t{span(60, 5)}\texpr:Ident',
        f'expr\tExprId(2)\t{span(60, 5)}\tIdent\t"value"',
        f'record\tExprId(1)\t{span(0, 50)}\t4',
        f'record-field\tExprId(1)\t0\tpath\t["a.b", "c,]d"]\t{span(1, 20)}\t\tExprId(2)',
        f'record-field\tExprId(1)\t1\tnamed\t"a\\nb"\t{span(25, 15)}\t\tExprId(2)',
        f'record-field\tExprId(1)\t2\tnamed\t""\t{span(41, 5)}\t\tExprId(2)',
        f'record-field\tExprId(1)\t3\tpath\t[""]\t{span(47, 2)}\t\tExprId(2)',
        "exact-counts\t2\t4",
        "run-form-count\t0",
        "structured-stage-count\t0",
        "structured-stage-callback-count\t0",
        "binding-initializer-count\t0",
    ]
    return "\n".join(rows) + "\n"


def named_keys_with_nonidentifier_names_owner_index():
    names = ("=", "MAJ:MIN", "-c", "full-dev-path", "", "internal key", "a\nb")
    rows = [
        "source\tSourceId(0)\t512",
        "exact-counts\t15\t7",
        "run-form-count\t0",
        "structured-stage-count\t0",
        "structured-stage-callback-count\t0",
        "binding-initializer-count\t0",
    ]
    record_span = span(0, 512)
    rows.extend((f"owner\t{record_span}\texpr:Record", f"expr\tExprId(1)\t{record_span}\tRecord\t\"\""))
    rows.append(f"record\tExprId(1)\t{record_span}\t{len(names)}")
    next_expr_id = 1
    for field_index, name in enumerate(names):
        key_start = field_index * 32 + 1
        key_length = max(2, len(name.encode("utf-8")))
        key_span = span(key_start, key_length)
        value_span = span(key_start + key_length + 1, 1)
        field_span = span(key_start, key_length + 2)
        key_expr_id = next_expr_id
        value_expr_id = next_expr_id + 1
        rows.extend(
            (
                f"owner\t{key_span}\texpr:Ident",
                f"expr\tExprId({key_expr_id + 1})\t{key_span}\tIdent\t{json.dumps(name, ensure_ascii=False)}",
                f"owner\t{value_span}\texpr:Str",
                f"expr\tExprId({value_expr_id + 1})\t{value_span}\tStr\t\"\"",
                f"record-field\tExprId(1)\t{field_index}\tnamed\t{json.dumps(name, ensure_ascii=False)}\t{field_span}\t\tExprId({value_expr_id + 1})",
            )
        )
        next_expr_id += 2
    return "\n".join(rows) + "\n"


def structured_stage_owner_index():
    return "\n".join(
        (
            "source\tSourceId(0)\t100",
            "exact-counts\t0\t0",
            "run-form-count\t0",
            "structured-stage-count\t2",
            "structured-stage-callback-count\t0",
            "binding-initializer-count\t0",
            f"structured-stage\t0\t{span(10, 5)}\tRange\tfalse\tfalse\t0",
            f"structured-stage\t1\t{span(20, 5)}\tRepeat\tfalse\tfalse\t0",
        )
    ) + "\n"


def structured_stage_callback_owner_index(
    arg_rows,
    *,
    stage_kind="Map",
    inline_block="false",
    accepts="true",
    callback_rows=None,
    arg_count=None,
    extra_expression_rows=(),
):
    callback_rows = callback_rows if callback_rows is not None else ("structured-stage-callback\t0\tExprId(1)",)
    arg_count = len(arg_rows) if arg_count is None else arg_count
    rows = [
        "source\tSourceId(0)\t100",
        f"owner\t{span(20, 5)}\texpr:Ident",
        f'expr\tExprId(1)\t{span(20, 5)}\tIdent\t"usage"',
        f"exact-counts\t{1 + sum(row.startswith('expr') for row in extra_expression_rows)}\t0",
        "run-form-count\t0",
        "structured-stage-count\t1",
        f"structured-stage-callback-count\t{len(callback_rows)}",
        "binding-initializer-count\t0",
        f"structured-stage\t0\t{span(10, 30)}\t{stage_kind}\t{inline_block}\t{accepts}\t{arg_count}",
        *arg_rows,
        *callback_rows,
        *extra_expression_rows,
    ]
    return "\n".join(rows) + "\n"


def run_form_owner_index(run_rows=None):
    run_span = span(10, 20)
    rows = [
        "source\tSourceId(0)\t100",
        f"owner\t{run_span}\tstmt:Command",
        f"owner\t{run_span}\trun:RunForm",
        f"stmt\tStmtId(1)\t{run_span}\tstmt:Command",
        "exact-counts\t0\t0",
        "run-form-count\t1",
        "structured-stage-count\t0",
        "structured-stage-callback-count\t0",
        "binding-initializer-count\t0",
    ]
    rows.extend(
        run_rows
        if run_rows is not None
        else (f"run\tRunFormId(1)\t{run_span}\tfalse\tPlain",)
    )
    return "\n".join(rows) + "\n"


def binding_initializer_owner_index(
    statement_kind="LetExprNoTy", expression_source=SOURCE, binding_name="format"
):
    statement_span = span(0, 12)
    expression_span = f"Span {{ source_id: {expression_source}, start: 8, len: 3 }}"
    rows = [
        "source\tSourceId(0)\t100",
    ]
    if expression_source == "SourceId(1)":
        rows.append("source\tSourceId(1)\t100")
    rows.extend(
        (
            f"owner\t{statement_span}\tstmt:{statement_kind}",
            f"stmt\tStmtId(1)\t{statement_span}\tstmt:{statement_kind}",
            f"owner\t{expression_span}\texpr:Ident",
            f'expr\tExprId(1)\t{expression_span}\tIdent\t"value"',
            "exact-counts\t1\t0",
            "run-form-count\t0",
            "structured-stage-count\t0",
            "structured-stage-callback-count\t0",
            "binding-initializer-count\t1",
            f'binding-initializer\tStmtId(1)\t{json.dumps(binding_name)}\tExprId(1)',
        )
    )
    return "\n".join(rows) + "\n"


def legacy_owner_index(contents):
    return "".join(
        line + "\n"
        for line in contents.splitlines()
        if line.split("\t", 1)[0] in {"source", "stmt", "tail", "owner"}
    )


class ExactOwnerIndexTests(unittest.TestCase):
    def test_structured_stage_tags_match_the_rust_enum(self):
        source_path = adapter_runner.ROOT / "src/syntax/node.rs"
        source = source_path.read_text(encoding="utf-8")
        enum_start = source.index("pub enum StreamStageKind {") + len("pub enum StreamStageKind {")
        enum_end = source.index("\n}", enum_start)
        body = source[enum_start:enum_end]
        variants = []
        for line in body.splitlines():
            entry = line.strip()
            if not entry:
                continue
            self.assertTrue(entry.endswith(","), "StreamStageKind entries must stay simple variants")
            tag = entry[:-1]
            self.assertTrue(tag.isidentifier(), "StreamStageKind entries must expose Rust Debug variant names")
            variants.append(tag)

        self.assertEqual(exact_runner.STREAM_STAGE_TAGS, frozenset(variants))

    def test_run_form_head_kinds_match_the_rust_enum(self):
        source_path = adapter_runner.ROOT / "src/syntax/node.rs"
        source = source_path.read_text(encoding="utf-8")
        enum_start = source.index("pub enum RunKind {") + len("pub enum RunKind {")
        enum_end = source.index("\n}", enum_start)
        body = source[enum_start:enum_end]
        variants = []
        for line in body.splitlines():
            entry = line.strip()
            if not entry:
                continue
            self.assertTrue(entry.endswith(","), "RunKind entries must stay simple variants")
            tag = entry[:-1]
            self.assertTrue(tag.isidentifier(), "RunKind entries must expose Rust Debug variant names")
            variants.append(tag)

        self.assertEqual(exact_runner.adapter.RUN_FORM_KINDS, frozenset(variants))

    def test_run_form_rows_bind_head_kind_and_propagation_to_exact_ids(self):
        legacy, exact = exact_runner.parse_exact_owner_index(run_form_owner_index())

        self.assertEqual(
            legacy[3]["StmtId(1)"],
            ("Span{source_id:SourceId(0),start:10,len:20}", "stmt:Command"),
        )
        self.assertEqual(
            exact["runs"],
            {0: ("Span{source_id:SourceId(0),start:10,len:20}", False, "Plain")},
        )

    def test_run_form_rows_reject_missing_or_malformed_head_kind_evidence(self):
        valid = run_form_owner_index()
        cases = (
            (valid.replace("\tfalse\tPlain", "\tfalse"), "malformed run row"),
            (valid.replace("\tfalse\tPlain", "\tfalse\tPlainish"), "unknown run form head kind"),
            (valid.replace("run\tRunFormId(1)", "run\tRunFormId(0)"), "malformed run form id"),
            (valid + "run\t" + valid.split("run\t", 1)[1], "duplicate run form id"),
            (valid.replace("run-form-count\t1\n", ""), "missing its run-form-count row"),
            (
                valid.replace("run-form-count\t1\n", "run-form-count\t1\nrun-form-count\t1\n"),
                "malformed or duplicate run-form-count row",
            ),
            (valid.replace("run-form-count\t1", "run-form-count\t0"), "rows do not cover the declared table"),
            (valid.replace("run\tRunFormId(1)", "run\tRunFormId(2)"), "exact run form ids are not contiguous"),
            (valid.replace("run\tRunFormId(1)\t" + span(10, 20) + "\tfalse\tPlain\n", ""), "run form rows do not cover the declared table"),
            (valid.replace(f"owner\t{span(10, 20)}\trun:RunForm\n", ""), "run AST owners do not match exact run forms"),
        )
        for contents, message in cases:
            with self.subTest(message=message), self.assertRaisesRegex(exact_runner.ExactOwnerError, message):
                exact_runner.parse_exact_owner_index(contents)

    def test_parses_exact_binding_initializer_ids_without_changing_legacy_projection(self):
        legacy, exact = exact_runner.parse_exact_owner_index(binding_initializer_owner_index())

        self.assertEqual(
            legacy[3]["StmtId(1)"],
            ("Span{source_id:SourceId(0),start:0,len:12}", "stmt:LetExprNoTy"),
        )
        self.assertEqual(
            exact["binding_initializers"],
            {(0, "format"): ("Span{source_id:SourceId(0),start:0,len:12}", 0)},
        )

    def test_parses_parser_generated_binding_names_without_widening_call_names(self):
        for name in ("%dest", "%delay", "%cap"):
            with self.subTest(name=name):
                _legacy, exact = exact_runner.parse_exact_owner_index(
                    binding_initializer_owner_index(binding_name=name)
                )
                self.assertEqual(
                    exact["binding_initializers"],
                    {(0, name): ("Span{source_id:SourceId(0),start:0,len:12}", 0)},
                )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "malformed binding initializer name"):
            exact_runner.parse_exact_owner_index(
                binding_initializer_owner_index(binding_name="%unknown")
            )

    def test_binding_initializer_rows_fail_closed(self):
        valid = binding_initializer_owner_index()
        cases = (
            (valid.replace("binding-initializer-count\t1\n", ""), "missing its binding-initializer-count"),
            (valid + "binding-initializer-count\t1\n", "duplicate binding-initializer-count"),
            (valid.replace("binding-initializer-count\t1", "binding-initializer-count\t0"), "do not cover the declared table"),
            (valid + 'binding-initializer\tStmtId(1)\t"format"\tExprId(1)\n', "duplicate binding initializer"),
            (valid.replace('binding-initializer\tStmtId(1)\t"format"\tExprId(1)', 'binding-initializer\tStmtId(1)\t"format"'), "malformed binding-initializer row"),
            (valid.replace('binding-initializer\tStmtId(1)\t"format"\tExprId(1)', 'binding-initializer\tStmtId(0)\t"format"\tExprId(1)'), "malformed statement id"),
            (valid.replace('binding-initializer\tStmtId(1)\t"format"\tExprId(1)', 'binding-initializer\tStmtId(2)\t"format"\tExprId(1)'), "missing statement"),
            (valid.replace('binding-initializer\tStmtId(1)\t"format"\tExprId(1)', 'binding-initializer\tStmtId(1)\t"two words"\tExprId(1)'), "malformed binding initializer name"),
            (valid.replace('binding-initializer\tStmtId(1)\t"format"\tExprId(1)', 'binding-initializer\tStmtId(1)\t"format"\tExprId(0)'), "malformed binding initializer expression id"),
            (valid.replace('binding-initializer\tStmtId(1)\t"format"\tExprId(1)', 'binding-initializer\tStmtId(1)\t"format"\tExprId(99)'), "references a missing expression"),
            (valid.replace("stmt:LetExprNoTy", "stmt:TailBareIdent"), "non-binding statement"),
            (binding_initializer_owner_index(expression_source="SourceId(1)"), "crosses source ownership"),
        )
        for contents, message in cases:
            with self.subTest(message=message), self.assertRaisesRegex(exact_runner.ExactOwnerError, message):
                exact_runner.parse_exact_owner_index(contents)

    def test_parses_complete_structured_stream_stage_table_without_changing_legacy_projection(self):
        legacy, exact = exact_runner.parse_exact_owner_index(structured_stage_owner_index())

        self.assertEqual(legacy[1], {})
        self.assertEqual(
            exact["structured_stages"],
            {
                0: ("Span{source_id:SourceId(0),start:10,len:5}", "Range", False, False),
                1: ("Span{source_id:SourceId(0),start:20,len:5}", "Repeat", False, False),
            },
        )

    def test_parses_named_and_first_positional_stage_callbacks(self):
        cases = (
            (
                (
                    'structured-stage-arg\t0\t0\tNamed\t"desc"\tExprId(2)',
                    'structured-stage-arg\t0\t1\tNamed\t"block"\tExprId(1)',
                    'structured-stage-arg\t0\t2\tPositional\t-\tExprId(2)',
                ),
                ((0, "Named", "desc", 1), (1, "Named", "block", 0), (2, "Positional", None, 1)),
            ),
            (("structured-stage-arg\t0\t0\tPositional\t-\tExprId(1)",), ((0, "Positional", None, 0),)),
        )
        for arg_rows, expected_args in cases:
            with self.subTest(expected_args=expected_args):
                _legacy, exact = exact_runner.parse_exact_owner_index(
                    structured_stage_callback_owner_index(
                        arg_rows,
                        extra_expression_rows=(
                            f"owner\t{span(26, 5)}\texpr:Ident",
                            f'expr\tExprId(2)\t{span(26, 5)}\tIdent\t"descending"',
                        ) if expected_args[0][1] == "Named" else (),
                    )
                )
                self.assertEqual(
                    exact["structured_stages"],
                    {0: ("Span{source_id:SourceId(0),start:10,len:30}", "Map", False, True)},
                )
                self.assertEqual(exact["stage_arguments"], {0: expected_args})
                self.assertEqual(exact["stage_callbacks"], {0: 0})

    def test_stage_callback_graph_rejects_inconsistent_or_ambiguous_edges(self):
        valid_arg = 'structured-stage-arg\t0\t0\tNamed\t"block"\tExprId(1)'
        valid = structured_stage_callback_owner_index((valid_arg,))
        stage_row = f"structured-stage\t0\t{span(10, 30)}\tMap\tfalse\ttrue\t1"
        cases = (
            (valid.replace("structured-stage-callback-count\t1", "structured-stage-callback-count\t0"), "do not cover the declared table"),
            (valid.replace(stage_row, stage_row.replace("false\ttrue", "true\ttrue")), "inline block"),
            (valid.replace(stage_row, stage_row.replace("false\ttrue", "false\tfalse")), "does not accept callable"),
            (valid.replace(valid_arg, valid_arg.replace("ExprId(1)", "ExprId(2)")), "missing expression"),
            (
                valid.replace(f'{span(20, 5)}\tIdent\t"usage"', f'{span(20, 5)}\tStr\t""'),
                "not a direct identifier or field",
            ),
            (valid.replace(valid_arg, valid_arg.replace('Named\t"block"', 'Named\t"workers"')), "does not identify its callback argument"),
            (valid.replace(valid_arg, valid_arg.replace('Named\t"block"', "NamedSpread\t-")), "ambiguous spread arguments"),
            (valid.replace(valid_arg, valid_arg.replace("\t0\tNamed", "\t1\tNamed")), "do not cover the declared argument table"),
            (
                valid.replace("exact-counts\t1\t0", "exact-counts\t2\t0")
                .replace(
                    "structured-stage-callback\t0\tExprId(1)",
                    "structured-stage-callback\t0\tExprId(2)",
                )
                + f"owner\t{span(25, 3)}\texpr:Ident\nexpr\tExprId(2)\t{span(25, 3)}\tIdent\t\"other\"\n",
                "does not match its argument graph",
            ),
            (valid.replace("structured-stage-callback-count\t1\n", ""), "missing its structured-stage-callback-count"),
            (valid.replace(valid_arg + "\n", ""), "do not cover the declared argument table"),
        )
        for contents, message in cases:
            with self.subTest(message=message), self.assertRaisesRegex(exact_runner.ExactOwnerError, message):
                exact_runner.parse_exact_owner_index(contents)

    def test_structured_stage_count_and_rows_fail_closed(self):
        valid = structured_stage_owner_index()
        cases = (
            (valid.replace("structured-stage-count\t2\n", ""), "is missing its structured-stage-count"),
            (valid + f"structured-stage\t0\t{span(30, 5)}\tMap\tfalse\tfalse\t0\n", "duplicate structured-stage index"),
            (valid.replace(f"structured-stage\t0\t{span(10, 5)}\tRange\tfalse\tfalse\t0", f"structured-stage\t0\t{span(10, 5)}\tUnknown\tfalse\tfalse\t0"), "unknown structured-stage kind"),
            (valid.replace(f"structured-stage\t0\t{span(10, 5)}\tRange\tfalse\tfalse\t0", f"structured-stage\t0\t{span(96, 5)}\tRange\tfalse\tfalse\t0"), "outside its source"),
            (valid.replace(f"structured-stage\t1\t{span(20, 5)}\tRepeat\tfalse\tfalse\t0\n", ""), "do not cover the declared table"),
        )
        for contents, message in cases:
            with self.subTest(message=message), self.assertRaisesRegex(exact_runner.ExactOwnerError, message):
                exact_runner.parse_exact_owner_index(contents)

    def test_parses_complete_global_ids_with_nonzero_record_field_ranges(self):
        legacy, exact = exact_runner.parse_exact_owner_index(owner_index())

        self.assertEqual(legacy[0], {0: 100})
        self.assertEqual(legacy[2]["StmtId(1)"], (
            "Span{source_id:SourceId(0),start:90,len:5}", "last", "stmt:TailBareIdent"
        ))
        self.assertEqual(exact["expr_count"], 6)
        self.assertEqual(exact["record_field_count"], 4)
        self.assertEqual(exact["exprs"][0], (
            "Span{source_id:SourceId(0),start:0,len:50}", "Record", ""
        ))
        self.assertEqual(exact["tails"][0], legacy[2]["StmtId(1)"])
        self.assertEqual(set(exact["records"][3]["fields"]), {2, 3})
        self.assertEqual(exact["aliases"][(3, 3)], (
            5, "Span{source_id:SourceId(0),start:60,len:5}", "again"
        ))


    def test_rejects_missing_expression_or_record_field_coverage(self):
        missing_expression = owner_index().replace(
            f'expr\tExprId(6)\t{span(60, 5)}\tIdent\t"again"\n', ""
        )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "expression ids"):
            exact_runner.parse_exact_owner_index(missing_expression)
        missing_field = owner_index().replace(
            f'record-field\tExprId(4)\t3\tshorthand\t"again"\t{span(60, 5)}\t\t\n', ""
        )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "field count"):
            exact_runner.parse_exact_owner_index(missing_field)

    def test_rejects_unparented_record_field_arena_slots(self):
        contents = owner_index().replace("exact-counts\t6\t4", "exact-counts\t6\t6")

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "do not cover the declared arena table"):
            exact_runner.parse_exact_owner_index(contents)

    def test_rejects_record_field_ids_outside_the_arena_table(self):
        contents = owner_index().replace("exact-counts\t6\t4", "exact-counts\t6\t3")

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "outside the declared arena table"):
            exact_runner.parse_exact_owner_index(contents)

    def test_rejects_malformed_expression_references_and_zero_based_debug_ids(self):
        dangling = owner_index().replace("\t\tExprId(2)\n", "\t\tExprId(99)\n")
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "missing expression"):
            exact_runner.parse_exact_owner_index(dangling)
        zero = owner_index().replace("ExprId(1)", "ExprId(0)", 1)
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "malformed"):
            exact_runner.parse_exact_owner_index(zero)

    def test_rejects_nonmatching_or_missing_shorthand_aliases(self):
        wrong_name = owner_index().replace(
            f'record-shorthand-alias\tExprId(1)\t1\tExprId(3)\t{span(10, 5)}\t"alias"',
            f'record-shorthand-alias\tExprId(1)\t1\tExprId(3)\t{span(10, 5)}\t"other"',
        )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "disagrees"):
            exact_runner.parse_exact_owner_index(wrong_name)
        missing_alias = owner_index().replace(
            f'record-shorthand-alias\tExprId(4)\t3\tExprId(6)\t{span(60, 5)}\t"again"\n', ""
        )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "aliases do not match"):
            exact_runner.parse_exact_owner_index(missing_alias)

    def test_decodes_rust_debug_escapes_without_accepting_malformed_escapes(self):
        value = r'"tab\tline\nreturn\rc0\0slash\\quote\"unicode\u{1f642}"'

        self.assertEqual(
            exact_runner._decode_rust_string(value, "fixture"),
            "tab\tline\nreturn\rc0\0slash\\quote\"unicode🙂",
        )
        for malformed in (
            r'"bad\x41"',
            r'"bad\u{}"',
            r'"bad\u{d800}"',
            r'"bad\u{110000}"',
            '"bad\nline"',
            '"bad\\',
        ):
            with self.subTest(malformed=malformed), self.assertRaises(exact_runner.ExactOwnerError):
                exact_runner._decode_rust_string(malformed, "fixture")

    def test_preserves_path_components_and_input_1593_named_keys(self):
        _legacy, exact = exact_runner.parse_exact_owner_index(named_and_path_owner_index())

        self.assertEqual(
            exact["records"][0]["fields"][0],
            ("path", ("a.b", "c,]d"), "Span{source_id:SourceId(0),start:1,len:20}", None, 1),
        )
        self.assertEqual(
            exact["records"][0]["fields"][1],
            ("named", "a\nb", "Span{source_id:SourceId(0),start:25,len:15}", None, 1),
        )
        self.assertEqual(
            exact["records"][0]["fields"][2],
            ("named", "", "Span{source_id:SourceId(0),start:41,len:5}", None, 1),
        )
        self.assertEqual(
            exact["records"][0]["fields"][3],
            ("path", ("",), "Span{source_id:SourceId(0),start:47,len:2}", None, 1),
        )

    def test_parses_named_keys_with_nonidentifier_generic_expression_names(self):
        names = ("=", "MAJ:MIN", "-c", "full-dev-path", "", "internal key", "a\nb")

        _legacy, exact = exact_runner.parse_exact_owner_index(
            named_keys_with_nonidentifier_names_owner_index()
        )

        self.assertEqual(
            [expression[2] for expression in exact["exprs"].values() if expression[1] == "Ident"],
            list(names),
        )
        self.assertEqual(
            [exact["records"][0]["fields"][index][1] for index in range(len(names))],
            list(names),
        )

    def test_keeps_identifier_constraints_for_shorthand_aliases_and_legacy_tails(self):
        invalid_alias = owner_index().replace(
            f'record-shorthand-alias\tExprId(1)\t1\tExprId(3)\t{span(10, 5)}\t"alias"',
            f'record-shorthand-alias\tExprId(1)\t1\tExprId(3)\t{span(10, 5)}\t"-"',
        )
        invalid_tail = owner_index().replace(
            f"tail\tStmtId(1)\t{span(90, 5)}\tlast\tstmt:TailBareIdent",
            f"tail\tStmtId(1)\t{span(90, 5)}\ttwo words\tstmt:TailBareIdent",
        )

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "malformed record shorthand alias name"):
            exact_runner.parse_exact_owner_index(invalid_alias)
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "malformed tail row"):
            exact_runner.parse_exact_owner_index(invalid_tail)

    def test_rejects_malformed_path_vectors(self):
        path_row = f'record-field\tExprId(1)\t0\tpath\t["a.b", "c,]d"]\t{span(1, 20)}\t\tExprId(2)'
        for value in ('[]', '["a",]', '["a", 2]', '["a", "b"'):
            with self.subTest(value=value), self.assertRaises(exact_runner.ExactOwnerError):
                exact_runner.parse_exact_owner_index(
                    named_and_path_owner_index().replace(path_row, path_row.replace('["a.b", "c,]d"]', value))
                )

    def test_parses_complete_call_and_field_access_child_maps(self):
        _legacy, exact = exact_runner.parse_exact_owner_index(call_and_field_owner_index())

        self.assertEqual(exact["calls"], {1: 0})
        self.assertEqual(exact["field_accesses"], {3: (2, "a\tb\n\r\0\\\"🙂")})

    def test_rejects_missing_extra_duplicate_and_dangling_ast_links(self):
        contents = call_and_field_owner_index()
        field_link = "field-access\tExprId(4)\tExprId(3)\t" + r'"a\tb\n\r\0\\\"\u{1f642}"' + "\n"
        call_link = "call\tExprId(2)\tExprId(1)\n"
        cases = (
            (contents.replace(call_link, ""), "call links do not cover"),
            (contents + "call\tExprId(1)\tExprId(2)\n", "call links do not cover"),
            (contents.replace(call_link, "call\tExprId(2)\tExprId(99)\n"), "invalid callee"),
            (contents.replace(field_link, ""), "field-access links do not cover"),
            (contents.replace(field_link, "field-access\tExprId(4)\tExprId(99)\t\"x\"\n"), "invalid base"),
            (contents.replace(f'owner\t{span(20, 9)}\texpr:Field\n', ""), "no matching generic AST owner"),
            (
                contents.replace(f'owner\t{span(20, 9)}\texpr:Field\n', f'owner\t{span(20, 9)}\texpr:Call\n'),
                "no matching generic AST owner",
            ),
            (
                contents.replace(f'expr\tExprId(4)\t{span(20, 9)}\tField', f'expr\tExprId(4)\t{span(99, 9)}\tField'),
                "outside its source",
            ),
            (contents.replace(call_link, call_link + call_link), "duplicate call expression id"),
            (contents.replace(field_link, field_link + field_link), "duplicate field-access expression id"),
        )
        for candidate, message in cases:
            with self.subTest(message=message), self.assertRaisesRegex(exact_runner.ExactOwnerError, message):
                exact_runner.parse_exact_owner_index(candidate)


class ExactOwnerHelperSourceTests(unittest.TestCase):
    def test_helper_path_is_relative_to_the_controller_module_and_hash_is_pinned(self):
        expected_helper = pathlib.Path(exact_runner.__file__).with_name(
            "w9-publication-exact-owners.rs"
        )

        self.assertEqual(exact_runner.EXACT_OWNER_HELPER, expected_helper)
        self.assertEqual(
            hashlib.sha256(expected_helper.read_bytes()).hexdigest(),
            exact_runner.EXACT_OWNER_HELPER_SHA256,
        )

    def test_source_qualified_projection_removes_only_three_unused_imports(self):
        original = exact_runner.EXACT_OWNER_HELPER.read_bytes()
        projected = exact_runner._project_exact_owner_helper(original)

        self.assertEqual(
            original,
            exact_runner.EXACT_OWNER_HELPER.read_bytes(),
        )
        self.assertEqual(projected.count(b"use std::io::Read as _;"), 2)
        self.assertIn(b"use rustix::fs::Dir;", projected)
        self.assertIn(
            b"fn w9_exact_hash_owner_directory(\n"
            b"    root_path: &std::path::Path,\n"
            b"    count: usize,\n"
            b") -> std::io::Result<Vec<(usize, String)>> {\n"
            b"    use rustix::fs::Dir;\n"
            b"    use std::collections::BTreeSet;\n"
            b"    use std::io::Read as _;\n",
            projected,
        )
        self.assertNotIn(b"use rustix::fs::{Dir, OFlags};", projected)
        self.assertNotIn(b"use std::os::unix::ffi::OsStrExt as _;", projected)
        self.assertIn(b"w9_exact_hash_owner_directory", projected)
        self.assertIn(b"w9_exact_repository_owners_receipt", projected)
        self.assertEqual(
            projected,
            original.replace(
                b"fn w9_exact_repository_owners_receipt() {\n"
                b"    use std::io::Read as _;\n",
                b"fn w9_exact_repository_owners_receipt() {\n",
                1,
            ).replace(
                b"    use rustix::fs::{Dir, OFlags};\n",
                b"    use rustix::fs::Dir;\n",
                1,
            ).replace(
                b"    use std::os::unix::ffi::OsStrExt as _;\n",
                b"",
                1,
            ),
        )

    def test_exact_owner_composition_keeps_compact_publication_formatter(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        scratch_root = repository / ".work/tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        original = exact_runner.EXACT_OWNER_HELPER.read_bytes()
        runner = SimpleNamespace(SOURCE_COHORT_SCHEMA="3")
        source_record = {"schema": runner.SOURCE_COHORT_SCHEMA}
        with tempfile.TemporaryDirectory(
            prefix="w9-exact-owner-compact-", dir=scratch_root
        ) as scratch:
            helper, _, _, _ = adapter_runner.derive_helper(
                approved_helper=adapter_runner.SOURCE_APPROVED_HELPER,
                output_directory=pathlib.Path(scratch) / "derived",
                output_root=repository,
            )
            composed = exact_runner._compose_exact_helper(
                helper.read_bytes(), source_record, original, runner
            )

        self.assertIn(
            b'let publication_facts = w9_canonical_debug(&format!("{checked:?}"));',
            composed,
        )
        self.assertNotIn(
            b'let publication_facts = w9_canonical_debug(&format!("{checked:#?}"));',
            composed,
        )
        self.assertTrue(
            composed.endswith(exact_runner._project_exact_owner_helper(original))
        )

    def test_source_qualified_projection_rejects_missing_or_duplicate_imports(self):
        original = exact_runner.EXACT_OWNER_HELPER.read_bytes()
        import_fragments = (
            (
                b"fn w9_exact_repository_owners_receipt() {\n"
                b"    use std::io::Read as _;\n",
                b"fn w9_exact_repository_owners_receipt() {\n",
            ),
            (b"    use rustix::fs::{Dir, OFlags};\n", b"    use rustix::fs::Dir;\n"),
            (b"    use std::os::unix::ffi::OsStrExt as _;\n", b""),
        )

        for source, replacement in import_fragments:
            missing = original.replace(source, replacement, 1)
            duplicate = original + b"\n" + source
            for label, candidate in (("missing", missing), ("duplicate", duplicate)):
                with self.subTest(import_source=source, label=label), self.assertRaisesRegex(
                    exact_runner.ExactOwnerError,
                    "source-qualified exact-owner helper projection drift",
                ):
                    exact_runner._project_exact_owner_helper(candidate)

    def test_legacy_identity_and_modern_build_capture_proof_share_projected_suffix(self):
        original = exact_runner.EXACT_OWNER_HELPER.read_bytes()
        adapter_record = SimpleNamespace(SOURCE_COHORT_SCHEMA="3")
        legacy = {"schema": "2"}
        modern = {"schema": "3"}
        projected = exact_runner._project_exact_owner_helper(original)

        self.assertEqual(
            exact_runner._exact_helper_payload(legacy, original, adapter_record),
            original,
        )
        self.assertEqual(
            exact_runner._exact_helper_payload(modern, original, adapter_record),
            projected,
        )
        for source_record, suffix in ((legacy, original), (modern, projected)):
            with self.subTest(schema=source_record["schema"]):
                combined = exact_runner._compose_exact_helper(
                    b"base helper\n", source_record, original, adapter_record
                )
                self.assertTrue(combined.endswith(suffix))
                exact_runner._validate_exact_helper_artifact(
                    combined,
                    hashlib.sha256(combined).hexdigest(),
                    source_record,
                    original,
                    adapter_record,
                    "fixture artifact",
                )

        modern_unprojected = b"base helper\n" + original
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "approved helper projection"):
            exact_runner._validate_exact_helper_artifact(
                modern_unprojected,
                hashlib.sha256(modern_unprojected).hexdigest(),
                modern,
                original,
                adapter_record,
                "modern fixture artifact",
            )

        legacy_projected = b"base helper\n" + projected
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "approved helper projection"):
            exact_runner._validate_exact_helper_artifact(
                legacy_projected,
                hashlib.sha256(legacy_projected).hexdigest(),
                legacy,
                original,
                adapter_record,
                "legacy fixture artifact",
            )

        valid_modern = exact_runner._compose_exact_helper(
            b"base helper\n", modern, original, adapter_record
        )
        valid_modern_sha256 = hashlib.sha256(valid_modern).hexdigest()
        wrong_modern_sha256 = (
            "0" if valid_modern_sha256[0] != "0" else "1"
        ) + valid_modern_sha256[1:]
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "hash differs"):
            exact_runner._validate_exact_helper_artifact(
                valid_modern,
                wrong_modern_sha256,
                modern,
                original,
                adapter_record,
                "modern fixture artifact",
            )

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "unsupported source receipt schema"):
            exact_runner._exact_helper_payload({"schema": "4"}, original, adapter_record)


class ExactOwnerHashOutputTests(unittest.TestCase):
    def test_accepts_one_exactly_prefixed_libtest_row_and_complete_hashes(self):
        rows = [f"w9exacthash\t{index}\t{'a' * 64}" for index in range(1690)]
        output = f"test {exact_runner.EXACT_OWNER_TEST} ... {rows[0]}\n" + "\n".join(rows[1:])

        hashes = exact_runner.parse_exact_owner_hash_output(output, 1690)

        self.assertEqual(len(hashes), 1690)
        self.assertEqual(hashes[0], "a" * 64)

    def test_rejects_duplicate_missing_or_unknown_prefixed_hash_rows(self):
        duplicate = "w9exacthash\t0\t" + "a" * 64
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "repeats"):
            exact_runner.parse_exact_owner_hash_output(duplicate + "\n" + duplicate, 1)
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "coverage differs"):
            exact_runner.parse_exact_owner_hash_output("", 1)
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "unknown prefix"):
            exact_runner.parse_exact_owner_hash_output(
                "test sema::different ... w9exacthash\t0\t" + "a" * 64, 1
            )


class HarnessTestListingTests(unittest.TestCase):
    def test_parses_qualified_rust_test_names(self):
        listing = (
            "sema::existing_test: test\n"
            f"{exact_runner.EXACT_OWNER_TEST}: test\n"
        )

        names = exact_runner.parse_harness_test_names(listing)

        self.assertEqual(names, ["sema::existing_test", exact_runner.EXACT_OWNER_TEST])

    def test_rejects_duplicate_and_missing_exact_owner_tests(self):
        duplicate = f"{exact_runner.EXACT_OWNER_TEST}: test\n" * 2
        missing = "sema::existing_test: test\n"

        for listing, count in ((duplicate, 2), (missing, 0)):
            with self.subTest(count=count), self.assertRaisesRegex(
                exact_runner.ExactOwnerError,
                f"listing contains {count} exact proof tests",
            ):
                exact_runner.require_exact_owner_test_listing(listing)

    def test_source_qualified_exact_listing_runs_binary_from_external_target_root(self):
        builder = pathlib.Path("/repository/dev/consolidation/w9-source-build.sh")
        source_root = pathlib.Path("/repository/.work/wt/reference")
        binary = pathlib.Path(
            "/repository/.work/tmp/w9-target-reference/integration-harness"
        )
        environment = {
            "W9_SOURCE_COMMIT": "a" * 40,
            "W9_SOURCE_TREE": "b" * 40,
        }
        output = f"{exact_runner.EXACT_OWNER_TEST}: test\n"
        completed = exact_runner.subprocess.CompletedProcess([], 0, output, "")

        with mock.patch.object(exact_runner.subprocess, "run", return_value=completed) as launch:
            exact_runner._check_exact_harness_listing(builder, source_root, binary, environment)

        self.assertEqual(
            launch.call_args.args[0],
            [str(builder), str(source_root), "run", str(binary), "--list"],
        )
        self.assertEqual(launch.call_args.kwargs["env"], environment)


class ReferenceBuildReuseTests(unittest.TestCase):
    def setUp(self):
        self.receipt = pathlib.Path("/scratch/reference.receipt")
        self.reference_root = pathlib.Path("/scratch/reference")
        self.owner_helper_source = pathlib.Path("/source/owner-helper.rs")
        self.derived_helper = pathlib.Path("/scratch/fresh-helper.rs")
        self.build = {
            "source_root": str(self.reference_root),
            "binary": "/scratch/reference-test-binary",
            "helper_path": "/scratch/retained-helper.rs",
            "helper_sha256": "a" * 64,
            "owner_helper_sha256": "b" * 64,
            "owner_helper_source_sha256": "c" * 64,
        }
        self.runner = SimpleNamespace(
            regular_file=lambda path, _label: pathlib.Path(path),
            read_regular_bytes=lambda path: b"matching combined helper"
            if pathlib.Path(path) in {self.derived_helper, pathlib.Path(self.build["helper_path"])}
            else b"",
        )

    def test_reuses_a_receipt_only_when_source_and_fresh_helper_match(self):
        with mock.patch.object(
            exact_runner.adapter_runner,
            "_parse_adapter_build_receipt",
            return_value=self.build,
        ) as parse_receipt:
            result = exact_runner._reuse_reference_build(
                self.receipt,
                self.reference_root,
                self.owner_helper_source,
                "a" * 64,
                "b" * 64,
                "c" * 64,
                self.derived_helper,
                self.runner,
            )

        self.assertEqual(result, (pathlib.Path(self.build["binary"]), self.receipt, self.build))
        parse_receipt.assert_called_once_with(
            self.receipt, "reference", self.runner, self.owner_helper_source
        )

    def test_rejects_a_stale_reference_source_or_helper(self):
        for field, replacement in (
            ("source_root", "/scratch/stale-reference"),
            ("helper_sha256", "d" * 64),
            ("owner_helper_sha256", "d" * 64),
            ("owner_helper_source_sha256", "d" * 64),
        ):
            with self.subTest(field=field):
                build = dict(self.build)
                build[field] = replacement
                with mock.patch.object(
                    exact_runner.adapter_runner,
                    "_parse_adapter_build_receipt",
                    return_value=build,
                ), self.assertRaises(exact_runner.ExactOwnerError):
                    exact_runner._reuse_reference_build(
                        self.receipt,
                        self.reference_root,
                        self.owner_helper_source,
                        "a" * 64,
                        "b" * 64,
                        "c" * 64,
                        self.derived_helper,
                        self.runner,
                    )


class ExactOwnerHashJobDirectoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temporary.name)
        self.output = self.root / "proof"
        self.output.mkdir()
        self.candidate_root = self.root / "candidate"
        (self.candidate_root / ".work").mkdir(parents=True)
        self.wrapper = self.root / "native-test-wrapper"
        self.wrapper.write_text("wrapper")
        self.wrapper_root = self.root / "wrapper-root"
        self.wrapper_root.mkdir()
        self.source_record = {
            "wrapper": str(self.wrapper),
            "wrapper_root": str(self.wrapper_root),
            "candidate_root": str(self.candidate_root),
            "input_root": str(self.root / "inputs"),
        }
        hash_rows = [f"w9exacthash\t{index}\t{'a' * 64}" for index in range(1690)]
        self.hash_output = (
            f"test {exact_runner.EXACT_OWNER_TEST} ... {hash_rows[0]}\n"
            + "\n".join(hash_rows[1:])
        )
        self.runner = SimpleNamespace(
            SOURCE_COHORT_SCHEMA="3",
            WRAPPER_SHA256="b" * 64,
            CleanupFailure=RuntimeError,
            regular_file=lambda path, _label, **_kwargs: pathlib.Path(path),
            real_directory=lambda path, _label: pathlib.Path(path),
            validate_wrapper_root=lambda *_args: None,
            require_sha256=lambda *_args: None,
            under=lambda *_args: True,
            open_absolute_nofollow=self.open_directory,
            existing_container=lambda *_args: False,
            docker_shim=lambda *_args: self.root / "docker-shim",
            sha256_file=lambda _path: "c" * 64,
            start_and_register_job=self.start_job,
            inspect_owned_containers=lambda *_args: [],
            read_regular_text=lambda path: pathlib.Path(path).read_text(),
            stop_jobs=lambda *_args: None,
        )

    def tearDown(self):
        self.temporary.cleanup()

    @staticmethod
    def open_directory(path, directory=True):
        flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0)
        return os.open(path, flags)

    def start_job(self, job, active, _command, _working_directory, _environment):
        job["output"].write(self.hash_output.encode())
        job["output"].flush()
        job["process"] = SimpleNamespace(poll=lambda: 0, returncode=0)
        active.append(job)

    def run_pass(self, pass_number):
        return exact_runner._run_exact_owner_hashes(
            self.root / "harness",
            self.candidate_root / ".work" / "owners",
            self.source_record,
            self.output,
            "candidate",
            pass_number,
            {},
            self.runner,
        )

    def test_both_passes_create_distinct_real_job_directories(self):
        first = self.run_pass(1)
        second = self.run_pass(2)

        self.assertEqual(first, second)
        self.assertTrue((self.output / "hashjobs" / "candidate-1" / "worker.log").is_file())
        self.assertTrue((self.output / "hashjobs" / "candidate-2" / "worker.log").is_file())

    def test_duplicate_passes_are_rejected(self):
        self.run_pass(1)
        with self.assertRaises(FileExistsError):
            self.run_pass(1)
        self.run_pass(2)
        with self.assertRaises(FileExistsError):
            self.run_pass(2)

    def test_second_pass_rejects_symlinked_hashjobs_parent(self):
        external = self.root / "external"
        external.mkdir()
        (self.output / "hashjobs").symlink_to(external, target_is_directory=True)

        with self.assertRaises(OSError):
            self.run_pass(2)

        self.assertEqual(list(external.iterdir()), [])


class ExactOwnerReceiptTests(unittest.TestCase):
    def setUp(self):
        self.reference = SimpleNamespace(
            inputs={0: SimpleNamespace(relative_path="src/a.xsh", blake3="a" * 64)}
        )
        self.receipt = (
            "corpus=1\n"
            "shard=0/1\n"
            "range=0..1\n"
            "shard_size=100\n"
            f"input_digest={'c' * 64}\n"
            f"owner\t0\tsrc/a.xsh\t{'a' * 64}\t{'b' * 64}\n"
            "complete=1\n"
        )

    def test_validates_exact_source_identity_and_range_coverage(self):
        hashes = exact_runner.parse_exact_owner_range_receipt(
            self.receipt, 0, 1, self.reference, "c" * 64
        )

        self.assertEqual(hashes, {0: "b" * 64})

    def test_rejects_stale_manifest_path_or_digest(self):
        stale_path = self.receipt.replace("src/a.xsh", "src/old.xsh", 1)
        stale_hash = self.receipt.replace("\t" + "a" * 64 + "\t", "\t" + "d" * 64 + "\t")

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "source differs"):
            exact_runner.parse_exact_owner_range_receipt(
                stale_path, 0, 1, self.reference, "c" * 64
            )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "source differs"):
            exact_runner.parse_exact_owner_range_receipt(
                stale_hash, 0, 1, self.reference, "c" * 64
            )

    def test_rejects_partial_range_coverage_and_stale_corpus_digest(self):
        incomplete = self.receipt.replace("complete=1\n", "complete=0\n")
        stale = self.receipt.replace("c" * 64, "e" * 64, 1)

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "completion count"):
            exact_runner.parse_exact_owner_range_receipt(
                incomplete, 0, 1, self.reference, "c" * 64
            )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "input_digest"):
            exact_runner.parse_exact_owner_range_receipt(
                stale, 0, 1, self.reference, "c" * 64
            )

    def test_rejects_malformed_or_incomplete_capture_ledgers(self):
        lines = [f"range\t0\t1\t{'a' * 64}", f"range\t1\t2\t{'b' * 64}"]
        rows, digest = exact_runner.parse_exact_owner_ledger(lines, [(0, 1), (1, 2)])
        canonical = "".join(f"range\t{start}\t{end}\t{hash_value}\n" for start, end, hash_value in rows)
        self.assertEqual(digest, hashlib.sha256(canonical.encode()).hexdigest())

        for invalid in (
            lines[:1],
            [lines[1], lines[0]],
            [lines[0], lines[1].replace("b" * 64, "invalid")],
        ):
            with self.subTest(invalid=invalid), self.assertRaises(exact_runner.ExactOwnerError):
                exact_runner.parse_exact_owner_ledger(invalid, [(0, 1), (1, 2)])


class SourceQualifiedExactOwnerReceiptTests(unittest.TestCase):
    def setUp(self):
        repository = pathlib.Path(__file__).resolve().parents[2]
        self.runner = adapter_runner._load_publication_runner(
            repository / "dev/consolidation/w9-publication-run.py"
        )
        scratch = repository / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="w9-source-exact-receipt-", dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.candidate_root = self.root / "candidate"
        self.candidate_root.mkdir()
        self.source_root = self.candidate_root / "reference"
        self.source_root.mkdir()
        self.input_root = self.root / "inputs"
        self.input_root.mkdir()
        self.input_manifest = self.root / "input-files.tsv"
        self.input_manifest.write_text("0\tsrc/a.xsh\t" + "a" * 64 + "\t444\n", encoding="utf-8")
        self.source_receipt = self.root / "source-cohort.receipt"
        self.source_receipt.write_text("source cohort fixture\n", encoding="utf-8")
        self.source_record = {
            "schema": self.runner.SOURCE_COHORT_SCHEMA,
            "candidate_root": str(self.candidate_root),
            "input_root": str(self.input_root),
            "input_manifest": str(self.input_manifest),
            "input_manifest_sha256": self.runner.sha256_file(self.input_manifest),
            "corpus_blake3": self.runner.CORPUS_BLAKE3,
        }
        self.source_commit = "a" * 40
        self.source_tree = "b" * 40
        self.binary = self.root / "exact-owner-binary"
        self.binary.write_bytes(b"exact owner harness")
        self.build_receipt = self.root / "exact-owner-build.receipt"
        self.build_receipt.write_text("build fixture\n", encoding="utf-8")
        self.adapter_capture_receipt = self.root / "adapter-capture.receipt"
        self.adapter_capture_receipt.write_text("adapter capture fixture\n", encoding="utf-8")
        self.adapter_raw_root = self.root / "adapter-raw"
        self.adapter_raw_root.mkdir()
        self.adapter_receipt_directory = self.root / "adapter-receipts"
        self.adapter_receipt_directory.mkdir()
        self.adapter_build = {
            "source_receipt_sha256": self.runner.sha256_file(self.source_receipt),
        }
        self.adapter_capture = {
            "source_commit": self.source_commit,
            "source_tree": self.source_tree,
            "raw_root": str(self.adapter_raw_root),
        }
        self.adapter_inputs = {
            0: self.runner.ReceiptSource("src/a.xsh", "c" * 64)
        }
        self.build = {
            "schema": "2",
            "role": "reference",
            "source_receipt": str(self.source_receipt),
            "source_receipt_sha256": self.runner.sha256_file(self.source_receipt),
            "source_root": str(self.source_root),
            "source_commit": self.source_commit,
            "source_tree": self.source_tree,
            "binary": str(self.binary),
            "binary_sha256": self.runner.sha256_file(self.binary),
            "helper_sha256": "c" * 64,
            "input_root": str(self.input_root),
            "input_manifest": str(self.input_manifest),
            "input_manifest_sha256": self.source_record["input_manifest_sha256"],
            "corpus_blake3": self.runner.CORPUS_BLAKE3,
        }
        self.capture_root = self.root / "exact-owner-capture"
        self.raw_root = self.capture_root / "owners"
        self.receipt_directory = self.capture_root / "receipts"
        self.raw_root.mkdir(parents=True)
        self.receipt_directory.mkdir()
        self.range_receipt = self.receipt_directory / "range-0-1.tsv"
        self.range_receipt.write_text(
            "corpus=1\n"
            "shard=0/1\n"
            "range=0..1\n"
            "shard_size=100\n"
            f"input_digest={self.runner.CORPUS_BLAKE3}\n"
            f"owner\t0\tsrc/a.xsh\t{'c' * 64}\t{'d' * 64}\n"
            "complete=1\n",
            encoding="utf-8",
        )
        self.exact_capture_receipt = self.capture_root / "capture.receipt"
        self.expected_inputs = {
            0: self.runner.ManifestSource("src/a.xsh", "a" * 64, 0o444),
        }
        self.capture_record = exact_runner._exact_capture_record(
            "reference",
            self.build_receipt,
            self.build,
            self.adapter_capture_receipt,
            self.capture_root,
            self.raw_root,
            self.receipt_directory,
            self.input_root,
            self.input_manifest,
            {(0, 1): self.range_receipt},
            {0: "d" * 64},
            "e" * 64,
            self.runner,
        )
        exact_runner._write_exact_capture_receipt(
            self.exact_capture_receipt,
            self.capture_record,
            {(0, 1): self.range_receipt},
            self.runner,
        )

    def parse_capture(self, record=None):
        if record is not None:
            lines = [f"{field}\t{record[field]}" for field in exact_runner.SOURCE_EXACT_CAPTURE_FIELDS]
            lines.append(f"range\t0\t1\t{self.runner.sha256_file(self.range_receipt)}")
            self.exact_capture_receipt.write_text("\n".join(lines) + "\n", encoding="utf-8")
        with (
            mock.patch.object(self.runner, "parse_candidate_receipt", return_value=self.source_record),
            mock.patch.object(
                exact_runner,
                "_source_adapter_capture",
                return_value=(
                    self.adapter_build,
                    self.adapter_raw_root,
                    self.adapter_receipt_directory,
                    self.adapter_capture,
                    self.adapter_inputs,
                ),
            ),
        ):
            return exact_runner._parse_exact_capture_receipt(
                self.exact_capture_receipt,
                "reference",
                self.build_receipt,
                self.build,
                self.expected_inputs,
                self.expected_inputs,
                self.input_root,
                self.input_manifest,
                "e" * 64,
                self.runner,
            )

    def source_adapter_capture(self, *, capture_schema="2", raw_schema=None, parsed_changes=None, missing_name=None):
        capture_root = pathlib.Path(tempfile.mkdtemp(prefix="adapter-capture-", dir=self.root))
        raw_root = capture_root / "raw"
        receipt_directory = capture_root / "receipts"
        raw_root.mkdir(parents=True)
        receipt_directory.mkdir()
        if raw_schema is None:
            raw_schema = adapter_runner.RAW_SCHEMA
        (raw_root / "schema").write_text(raw_schema, encoding="utf-8")
        expected_inputs = {
            index: self.runner.ManifestSource(f"src/{index}.xsh", f"{index + 1:064x}", 0o444)
            for index in range(2)
        }
        names = (
            {f"owners-{index}.tsv" for index in expected_inputs}
            | {
                f"record-{index}-{profile}"
                for index in expected_inputs
                for profile in adapter_runner.PROFILES
            }
        )
        names.discard(missing_name)
        for name in names:
            (raw_root / name).write_bytes(b"")
        source_root = capture_root / "source-root"
        source_root.mkdir()
        helper_source = self.candidate_root / "dev/consolidation/w9-publication-owner-helper.rs"
        helper_source.parent.mkdir(parents=True, exist_ok=True)
        helper_source.write_bytes(b"owner helper source")
        build_receipt = self.root / "source-adapter-build.receipt"
        build_receipt.write_text("build receipt\n", encoding="utf-8")
        source_record = {
            "candidate_root": str(self.candidate_root),
            "input_manifest_sha256": self.source_record["input_manifest_sha256"],
            "corpus_blake3": self.runner.CORPUS_BLAKE3,
        }
        build = {
            "source_root": str(source_root),
            "source_receipt": str(self.source_receipt),
            "source_receipt_sha256": self.runner.sha256_file(self.source_receipt),
            "source_commit": self.source_commit,
            "source_tree": self.source_tree,
        }
        capture_receipt = capture_root / "capture.receipt"
        record = {
            "schema": capture_schema,
            "role": "candidate",
            "source_receipt": str(self.source_receipt),
            "source_receipt_sha256": self.runner.sha256_file(self.source_receipt),
            "source_root": str(source_root),
            "source_commit": self.source_commit,
            "source_tree": self.source_tree,
            "binary": str(self.binary),
            "binary_sha256": self.runner.sha256_file(self.binary),
            "build_receipt": str(build_receipt),
            "build_receipt_sha256": self.runner.sha256_file(build_receipt),
            "builder_path": str(self.runner.SOURCE_BUILDER_PATH),
            "builder_sha256": self.runner.sha256_file(self.runner.SOURCE_BUILDER_PATH),
            "runner_path": str(self.runner.ROOT / "dev/consolidation/w9-publication-run.py"),
            "runner_sha256": self.runner.sha256_file(self.runner.ROOT / "dev/consolidation/w9-publication-run.py"),
            "linker_path": str(self.runner.SOURCE_LINKER_PATH),
            "linker_sha256": self.runner.sha256_file(self.runner.SOURCE_LINKER_PATH),
            "helper_sha256": "c" * 64,
            "owner_helper_sha256": "d" * 64,
            "input_root": str(self.input_root),
            "input_manifest": str(self.input_manifest),
            "input_manifest_sha256": source_record["input_manifest_sha256"],
            "corpus_blake3": source_record["corpus_blake3"],
            "capture_root": str(capture_root),
            "raw_root": str(raw_root),
            "receipt_directory": str(receipt_directory),
            "range_count": "1",
            "outcome_count": str(len(expected_inputs) * len(adapter_runner.PROFILES)),
        }
        capture_receipt.write_text(
            "".join(f"{field}\t{record[field]}\n" for field in adapter_runner.SOURCE_CAPTURE_FIELDS),
            encoding="utf-8",
        )
        parsed = dict(record)
        parsed.update(parsed_changes or {})
        adapter_inputs = SimpleNamespace(
            inputs={
                index: self.runner.ReceiptSource(source.relative_path, f"{index + 1:064x}")
                for index, source in expected_inputs.items()
            }
        )
        with (
            mock.patch.object(adapter_runner, "_parse_adapter_build_receipt", return_value=build),
            mock.patch.object(adapter_runner, "_parse_capture_receipt", return_value=parsed),
            mock.patch.object(
                adapter_runner,
                "aggregate_candidate_receipts",
                return_value=adapter_inputs,
            ) as aggregate,
        ):
            result = exact_runner._source_adapter_capture(
                capture_receipt,
                "candidate",
                source_record,
                source_root,
                expected_inputs,
                self.input_root,
                self.input_manifest,
                self.source_receipt,
                self.runner,
            )
        return result, aggregate, expected_inputs, capture_receipt

    def test_fresh_adapter_capture_supplies_source_b3_and_requires_complete_raw_schema(self):
        (build, raw_root, receipt_directory, capture, adapter_inputs), aggregate, expected_inputs, _path = (
            self.source_adapter_capture()
        )

        self.assertEqual(adapter_inputs[1].relative_path, "src/1.xsh")
        self.assertEqual(adapter_inputs[1].blake3, f"{2:064x}")
        self.assertEqual(raw_root / "schema", pathlib.Path(capture["raw_root"]) / "schema")
        aggregate.assert_called_once_with(self.runner, receipt_directory, expected_inputs)
        self.assertEqual(build["source_receipt_sha256"], self.runner.sha256_file(self.source_receipt))

    def test_fresh_adapter_capture_rejects_stale_identity_schema_and_sidecar_coverage(self):
        for changes, raw_schema, missing_name, message in (
            ({"source_receipt_sha256": "f" * 64}, None, None, "source or cohort identity differs"),
            ({"input_manifest": str(self.root / "other.tsv")}, None, None, "source or cohort identity differs"),
            ({"source_commit": "f" * 40}, None, None, "source or cohort identity differs"),
            ({}, "wrong schema\n", None, "raw schema differs"),
            ({}, None, "owners-1.tsv", "raw owner coverage differs"),
        ):
            with self.subTest(changes=changes, raw_schema=raw_schema, missing_name=missing_name):
                with self.assertRaisesRegex(exact_runner.ExactOwnerError, message):
                    self.source_adapter_capture(
                        raw_schema=raw_schema,
                        parsed_changes=changes,
                        missing_name=missing_name,
                    )

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "not source-qualified"):
            self.source_adapter_capture(capture_schema="1")

    def test_schema_two_capture_and_proof_records_bind_source_and_adapter_capture(self):
        capture, ranges, hashes = self.parse_capture()
        proof = exact_runner._proof_record(
            "reference",
            self.build_receipt,
            {
                **self.build,
                "owner_helper_sha256": "f" * 64,
            },
            self.root / "exact-owner-helper.rs",
            "e" * 64,
            self.adapter_capture_receipt,
            self.exact_capture_receipt,
            capture,
            hashes,
            self.runner,
        )
        proof_path = self.root / "proof.receipt"
        exact_runner._write_proof_receipt(proof_path, proof, self.runner)

        self.assertEqual(capture["schema"], "2")
        self.assertEqual(capture["adapter_capture_receipt"], str(self.adapter_capture_receipt))
        self.assertEqual(capture["adapter_capture_receipt_sha256"], self.runner.sha256_file(self.adapter_capture_receipt))
        self.assertNotIn("legacy_capture_receipt", capture)
        self.assertEqual(ranges, {(0, 1): self.range_receipt})
        self.assertEqual(hashes, {0: "d" * 64})
        self.assertEqual(proof["adapter_capture_receipt"], str(self.adapter_capture_receipt))
        self.assertEqual(proof["adapter_capture_receipt_sha256"], self.runner.sha256_file(self.adapter_capture_receipt))
        self.assertEqual(proof_path.read_text(encoding="utf-8").splitlines()[0], exact_runner.SOURCE_EXACT_PROOF_SCHEMA.rstrip())

    def test_schema_two_capture_rejects_stale_source_capture_counts_ranges_and_mixed_schema(self):
        valid = dict(self.capture_record)
        for field, value, message in (
            ("source_receipt_sha256", "f" * 64, "source-cohort receipt identity differs"),
            ("adapter_capture_receipt_sha256", "f" * 64, "adapter capture receipt identity differs"),
            ("source_root", str(self.root / "stale-reference"), "source_root differs"),
            ("input_manifest", str(self.root / "stale-manifest"), "source-qualified input cohort differs"),
            ("range_count", "2", "complete exact-owner range coverage differs"),
            ("owner_count", "2", "complete exact-owner count differs"),
        ):
            with self.subTest(field=field), self.assertRaisesRegex(exact_runner.ExactOwnerError, message):
                self.parse_capture(dict(valid, **{field: value}))

        self.parse_capture(dict(valid))
        wrong_range = self.exact_capture_receipt.read_text(encoding="utf-8").replace(
            "range\t0\t1\t", "range\t1\t2\t", 1
        )
        self.exact_capture_receipt.write_text(wrong_range, encoding="utf-8")
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "range set differs"):
            self.parse_capture()

        self.parse_capture(dict(valid))
        mixed = self.exact_capture_receipt.read_text(encoding="utf-8").replace(
            "schema\t2\n", "schema\t1\n", 1
        )
        self.exact_capture_receipt.write_text(mixed, encoding="utf-8")
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "schemas differ"):
            self.parse_capture()

    def test_source_qualified_proof_rejects_legacy_capture_before_legacy_validation(self):
        helper_path = self.candidate_root / ".work" / "exact-owner-helper.rs"
        helper_path.parent.mkdir(parents=True)
        exact_helper_path = exact_runner.EXACT_OWNER_HELPER
        exact_helper = exact_helper_path.read_bytes()
        projected_helper = exact_runner._project_exact_owner_helper(exact_helper)
        helper_path.write_bytes(b"prefix\n" + projected_helper)
        build = dict(
            self.build,
            source_root=str(self.source_root),
            source_commit=self.source_commit,
            source_tree=self.source_tree,
            helper_path=str(helper_path),
            helper_sha256=hashlib.sha256(helper_path.read_bytes()).hexdigest(),
        )
        source_record = dict(
            self.source_record,
            candidate_commit=self.source_commit,
            candidate_tree=self.source_tree,
        )
        expected_inputs = {
            index: self.runner.ManifestSource(f"src/{index}.xsh", f"{index + 1:064x}", 0o444)
            for index in range(1690)
        }
        legacy_capture = self.root / "legacy-exact-capture.receipt"
        legacy_capture.write_text("schema\t1\n", encoding="utf-8")
        self.binary.chmod(0o700)

        with (
            mock.patch.object(adapter_runner, "_load_candidate_publication_runner", return_value=self.runner),
            mock.patch.object(self.runner, "parse_candidate_receipt", return_value=source_record),
            mock.patch.object(self.runner, "source_identity", return_value=(self.source_commit, self.source_tree)),
            mock.patch.object(
                exact_runner,
                "_validate_source_inputs",
                return_value=(expected_inputs, {}, None),
            ),
            mock.patch.object(adapter_runner, "_prepare_reference_source", return_value=self.source_root),
            mock.patch.object(adapter_runner, "_parse_adapter_build_receipt", return_value=build),
            mock.patch.object(
                exact_runner,
                "_load_exact_helper",
                return_value=(
                    exact_helper_path,
                    exact_helper,
                    hashlib.sha256(exact_helper).hexdigest(),
                ),
            ),
            mock.patch.object(exact_runner, "_legacy_capture") as legacy_capture_reader,
        ):
            with self.assertRaisesRegex(
                exact_runner.ExactOwnerError,
                "source-qualified exact-owner proof requires a schema-2 capture",
            ):
                exact_runner.validate_proof(
                    self.candidate_root,
                    self.source_receipt,
                    self.source_root,
                    "reference",
                    self.build_receipt,
                    legacy_capture,
                    self.root / "proof-output",
                    self.runner,
                )

        legacy_capture_reader.assert_not_called()

    def test_unknown_exact_owner_schema_cannot_fall_back_to_historical_receipts(self):
        unknown_capture = self.exact_capture_receipt.read_text(encoding="utf-8").replace(
            "schema\t2\n", "schema\t3\n", 1
        )
        self.exact_capture_receipt.write_text(unknown_capture, encoding="utf-8")
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "unsupported exact-owner capture receipt schema"):
            self.parse_capture()

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "unsupported schema"):
            exact_runner._exact_capture_record(
                "reference",
                self.build_receipt,
                dict(self.build, schema="3"),
                self.adapter_capture_receipt,
                self.capture_root,
                self.raw_root,
                self.receipt_directory,
                self.input_root,
                self.input_manifest,
                {(0, 1): self.range_receipt},
                {0: "d" * 64},
                "e" * 64,
                self.runner,
            )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "unsupported schema"):
            exact_runner._proof_record(
                "reference",
                self.build_receipt,
                dict(self.build, schema="3"),
                self.root / "exact-owner-helper.rs",
                "e" * 64,
                self.adapter_capture_receipt,
                self.exact_capture_receipt,
                self.capture_record,
                {0: "d" * 64},
                self.runner,
            )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "unsupported schema"):
            exact_runner._write_exact_capture_receipt(
                self.root / "unknown-capture.receipt",
                dict(self.capture_record, schema="3"),
                {(0, 1): self.range_receipt},
                self.runner,
            )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "unsupported schema"):
            exact_runner._write_proof_receipt(
                self.root / "unknown-proof.receipt",
                dict(self.capture_record, schema="3"),
                self.runner,
            )


class ExactOwnerCaptureSchedulingTests(unittest.TestCase):
    def run_capture(self, failing_worker=None):
        class OutputHandle:
            closed = False

            def close(self):
                self.closed = True

        class Process:
            def __init__(self, polls_before_exit, status=0):
                self.polls_before_exit = polls_before_exit
                self.status = status

            def poll(self):
                if self.polls_before_exit:
                    self.polls_before_exit -= 1
                    return None
                return self.status

        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory)
            launched = []
            stops = []
            completed = []
            facts = {}
            build = {"source_root": str(output), "source_commit": "commit", "source_tree": "tree"}

            def launch(_runner, _source_record, _role, role_output, _build, _raw_root,
                       _receipt_directory, start, end, worker, active, _docker_environment):
                process = Process(
                    4 if worker == "A" and failing_worker is None else 0,
                    1 if worker == failing_worker else 0,
                )
                job = {
                    "worker": worker,
                    "start": start,
                    "end": end,
                    "process": process,
                    "output": OutputHandle(),
                    "receipt": role_output / "receipts" / f"range-{start}-{end}.tsv",
                    "log": role_output / "jobs" / worker / f"{start}-{end}" / "worker.log",
                }
                launched.append((worker, start, end))
                active.append(job)
                return job

            def capture_record(*args):
                facts["range_receipts"] = dict(args[9])
                facts["owner_hashes"] = dict(args[10])
                return {"role": "candidate"}

            runner = SimpleNamespace(
                SOURCE_COHORT_SCHEMA="3",
                CleanupFailure=RuntimeError,
                expected_ranges=lambda count: [(index, index + 1) for index in range(count)],
                regular_file=lambda path, _label: pathlib.Path(path),
                write_status=lambda *_args: None,
                inspect_owned_containers=lambda *_args: [],
                stop_jobs=lambda active, _environment: stops.append(tuple(active)),
                validate_all_inputs=lambda _record: None,
                source_identity=lambda _path: ("commit", "tree"),
            )
            observed = {index: f"{index:064x}" for index in range(10)}
            patches = (
                mock.patch.object(
                    adapter_runner,
                    "_mkdir_new_directory",
                    side_effect=lambda path, _runner: pathlib.Path(path).mkdir(mode=0o700),
                ),
                mock.patch.object(exact_runner, "_launch_exact_owner_job", side_effect=launch),
                mock.patch.object(
                    exact_runner,
                    "_read_exact_owner_receipt",
                    side_effect=lambda _path, start, end, _reference, _runner:
                        {index: observed[index] for index in range(start, end)},
                ),
                mock.patch.object(exact_runner, "_exact_capture_record", side_effect=capture_record),
                mock.patch.object(exact_runner, "_write_exact_capture_receipt"),
                mock.patch.object(exact_runner.time, "sleep"),
            )
            with patches[0], patches[1], patches[2], patches[3], patches[4], patches[5]:
                try:
                    record = exact_runner._capture_exact_owner_role(
                        runner,
                        {},
                        "candidate",
                        output / "build.receipt",
                        build,
                        output / "legacy.receipt",
                        "helper-hash",
                        {index: object() for index in range(10)},
                        object(),
                        output / "inputs",
                        output / "manifest.tsv",
                        output,
                        completed,
                        {},
                    )
                except exact_runner.ExactOwnerError as error:
                    record = error
            return launched, completed, observed, record, stops, facts

    def test_capture_backfills_global_ranges_and_validates_full_coverage(self):
        launched, completed, observed, record, stops, facts = self.run_capture()

        self.assertEqual(
            launched,
            [("A", 0, 1), ("B", 1, 2), ("C", 2, 3), ("D", 3, 4),
             ("B", 4, 5), ("C", 5, 6), ("D", 6, 7), ("B", 7, 8),
             ("C", 8, 9), ("D", 9, 10)],
        )
        self.assertEqual(sorted(completed), [(index, index + 1) for index in range(10)])
        self.assertEqual(facts["owner_hashes"], observed)
        self.assertEqual(len(facts["range_receipts"]), 10)
        self.assertEqual(record, {"role": "candidate"})
        self.assertEqual(stops, [])

    def test_capture_stops_active_jobs_after_worker_failure(self):
        _launched, _completed, _observed, error, stops, facts = self.run_capture(failing_worker="A")

        self.assertIsInstance(error, exact_runner.ExactOwnerError)
        self.assertIn("worker A range 0..1 failed with status 1", str(error))
        self.assertEqual(len(stops), 1)
        self.assertEqual(len(stops[0]), 4)
        self.assertEqual(facts, {})


class ExactOwnerProjectionTests(unittest.TestCase):
    def test_returns_validated_legacy_and_exact_owners(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        work = root / ".work/tmp"
        work.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=work) as temporary:
            root_path = pathlib.Path(temporary)
            exact_root = root_path / "exact"
            legacy_root = root_path / "legacy"
            exact_root.mkdir()
            legacy_root.mkdir()
            (exact_root / "owners-0.tsv").write_text(owner_index(), encoding="utf-8")
            (legacy_root / "owners-0.tsv").write_text(
                legacy_owner_index(owner_index()), encoding="utf-8"
            )
            runner = SimpleNamespace(
                open_absolute_nofollow=lambda path, directory=False: os.open(
                    path, os.O_RDONLY | os.O_DIRECTORY
                )
            )

            validated = exact_runner._validate_owner_projections(
                exact_root, legacy_root, {0: object()}, {0: "b" * 64}, runner
            )

        self.assertEqual(validated[0][1]["expr_count"], 6)
        self.assertEqual(validated[0][1]["aliases"][(3, 3)][2], "again")

    def test_structured_stages_keep_the_original_generic_projection(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        work = root / ".work/tmp"
        work.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=work) as temporary:
            root_path = pathlib.Path(temporary)
            exact_root = root_path / "exact"
            legacy_root = root_path / "legacy"
            exact_root.mkdir()
            legacy_root.mkdir()
            exact_text = structured_stage_owner_index()
            (exact_root / "owners-0.tsv").write_text(exact_text, encoding="utf-8")
            (legacy_root / "owners-0.tsv").write_text(
                legacy_owner_index(exact_text), encoding="utf-8"
            )
            runner = SimpleNamespace(
                open_absolute_nofollow=lambda path, directory=False: os.open(
                    path, os.O_RDONLY | os.O_DIRECTORY
                )
            )

            validated = exact_runner._validate_owner_projections(
                exact_root, legacy_root, {0: object()}, {0: "b" * 64}, runner
            )

        self.assertEqual(validated[0][0][1], {})
        self.assertEqual(
            validated[0][1]["structured_stages"],
            {
                0: ("Span{source_id:SourceId(0),start:10,len:5}", "Range", False, False),
                1: ("Span{source_id:SourceId(0),start:20,len:5}", "Repeat", False, False),
            },
        )

    def test_rejects_disagreement_with_original_semantic_owner_rows(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        work = root / ".work/tmp"
        work.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=work) as temporary:
            root_path = pathlib.Path(temporary)
            exact_root = root_path / "exact"
            legacy_root = root_path / "legacy"
            exact_root.mkdir()
            legacy_root.mkdir()
            (exact_root / "owners-0.tsv").write_text(owner_index(), encoding="utf-8")
            changed_legacy = legacy_owner_index(owner_index()).replace(
                "\tlast\tstmt:TailBareIdent", "\tchanged\tstmt:TailBareIdent"
            )
            (legacy_root / "owners-0.tsv").write_text(changed_legacy, encoding="utf-8")
            runner = SimpleNamespace(
                open_absolute_nofollow=lambda path, directory=False: os.open(
                    path, os.O_RDONLY | os.O_DIRECTORY
                )
            )

            with self.assertRaisesRegex(exact_runner.ExactOwnerError, "rows differ"):
                exact_runner._validate_owner_projections(
                    exact_root, legacy_root, {0: object()}, {0: "b" * 64}, runner
                )


if __name__ == "__main__":
    unittest.main()
