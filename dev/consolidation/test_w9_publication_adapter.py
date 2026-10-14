import unittest

from w9_publication_adapter import (
    AdapterError,
    compare_publications,
    parse_owner_index,
    project_publication,
)


SPAN = "Span{source_id:SourceId(0),start:10,len:6}"
CALL_SPAN = "Span{source_id:SourceId(0),start:20,len:12}"
CONST_SPAN = "Span{source_id:SourceId(0),start:0,len:17}"
TAIL_CONST_SPAN = "Span{source_id:SourceId(0),start:30,len:6}"
TAIL_SPAN = "Span{source_id:SourceId(0),start:40,len:6}"
RUNTIME_TAIL_SPAN = "Span{source_id:SourceId(0),start:50,len:7}"
EXTRA_CALL_SPAN = "Span{source_id:SourceId(0),start:70,len:12}"
SET_CALL_SPAN = "Span{source_id:SourceId(0),start:70,len:12}"
SET_FIELD_SPAN = "Span{source_id:SourceId(0),start:70,len:8}"
SET_IDENT_SPAN = "Span{source_id:SourceId(0),start:70,len:3}"
ERROR_FAILURE_SPAN = "Span{source_id:SourceId(0),start:70,len:12}"
ERROR_FAILURE_FIELD_SPAN = "Span{source_id:SourceId(0),start:70,len:8}"
ERROR_FAILURE_IDENT_SPAN = "Span{source_id:SourceId(0),start:70,len:5}"
ALIAS_FIELD_SPAN = "Span{source_id:SourceId(0),start:70,len:8}"
ALIAS_IDENT_SPAN = "Span{source_id:SourceId(0),start:70,len:3}"
ALIAS_DEFINITION_SPAN = "Span{source_id:SourceId(5),start:0,len:17}"
RUN_FORM_SPAN = "Span{source_id:SourceId(0),start:90,len:7}"
RUN_GLOBAL_SPAN = "Span{source_id:SourceId(0),start:60,len:7}"
STAGE_SPAN = "Span{source_id:SourceId(0),start:80,len:7}"
COMMAND_SPAN = "Span{source_id:SourceId(0),start:36,len:40}"
CAPTURED_COMMAND_SPAN = "Span{source_id:SourceId(0),start:10728,len:74}"
CAPTURED_RUN_FORM_SPAN = "Span{source_id:SourceId(0),start:10728,len:73}"
SERVICE_STOP_SPAN = "Span{source_id:SourceId(0),start:458,len:15}"
SERVICE_START_SPAN = "Span{source_id:SourceId(0),start:479,len:16}"
PROPAGATING_STATEMENT_SPAN = "Span{source_id:SourceId(0),start:70,len:7}"
UNPREPARED_TAIL_DECL_SPAN = "Span{source_id:SourceId(0),start:70,len:6}"
UNPREPARED_TAIL_SPAN = "Span{source_id:SourceId(0),start:76,len:6}"
MISSING_VALUE_DECL_SPAN = "Span{source_id:SourceId(0),start:82,len:6}"
MISSING_VALUE_TAIL_SPAN = "Span{source_id:SourceId(0),start:88,len:6}"
BINDING = "BindingId(Target(BindingTargetId(1)))"
TAIL_BINDING = "BindingId(Target(BindingTargetId(2)))"
RUNTIME_BINDING = "BindingId(Target(BindingTargetId(3)))"
RUN_GLOBAL_BINDING = "BindingId(Target(BindingTargetId(4)))"
UNPREPARED_TAIL_BINDING = "BindingId(Target(BindingTargetId(5)))"
MISSING_VALUE_TAIL_BINDING = "BindingId(Target(BindingTargetId(6)))"
EXPR = "ExprId(2)"
TAIL_EXPR = "ExprId(3)"
MISSING_VALUE_EXPR = "ExprId(4)"
DECLARATION = (
    "BindingDeclaration{exported:false,global:true,kind:Constant(StmtId(1)),"
    f"name:Name(\"answer\"),namespace:None,span:{CONST_SPAN}}}"
)
COMMON = (
    "CheckOutput{checked_declarations:CheckedDeclarations{functions:{"
    "FunctionDefId(1):CheckedFunctionDeclaration{namespace:None,name:Name(\"run\")}}},"
    "resolved_bindings:ResolvedBindings{declarations:{"
    f"{BINDING}:{DECLARATION},"
    f"{TAIL_BINDING}:BindingDeclaration{{exported:false,global:false,kind:Constant(StmtId(4)),"
    f"name:Name(\"answer\"),namespace:None,span:{TAIL_CONST_SPAN}}}"
    f",{RUNTIME_BINDING}:BindingDeclaration{{exported:false,global:false,kind:Runtime,"
    "name:Name(\"runtime\"),namespace:None,span:Span{source_id:SourceId(0),start:60,len:7}}"
    f",{RUN_GLOBAL_BINDING}:BindingDeclaration{{exported:false,global:true,kind:Constant(StmtId(12)),"
    f"name:Name(\"not_prepared\"),namespace:None,span:{RUN_GLOBAL_SPAN}}}"
    f",{UNPREPARED_TAIL_BINDING}:BindingDeclaration{{exported:false,global:false,kind:Constant(StmtId(14)),"
    f"name:Name(\"no_binding\"),namespace:None,span:{UNPREPARED_TAIL_DECL_SPAN}}}"
    f",{MISSING_VALUE_TAIL_BINDING}:BindingDeclaration{{exported:false,global:false,kind:Constant(StmtId(16)),"
    f"name:Name(\"no_value\"),namespace:None,span:{MISSING_VALUE_DECL_SPAN}}}"
    "},"
    f"value_sites:{{({SPAN},Name(\"answer\")):{BINDING}}}"
    ",tails:{StmtId(3):" + TAIL_BINDING + ",StmtId(5):" + RUNTIME_BINDING
    + ",StmtId(13):" + UNPREPARED_TAIL_BINDING
    + ",StmtId(15):" + MISSING_VALUE_TAIL_BINDING + "}},"
    f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value}},"
    "prepared_constants:PreparedConstants{"
    f"binding_values:{{{BINDING}:{EXPR},{TAIL_BINDING}:{TAIL_EXPR},"
    f"{MISSING_VALUE_TAIL_BINDING}:{MISSING_VALUE_EXPR}}},"
    f"values:{{{EXPR}:LiteralConstant::Int(42),{TAIL_EXPR}:LiteralConstant::Int(43)}},"
    'cli_plans:Mutex{..,data:{(ExprId(2),false):Ok(Plan("payload"))},poisoned:false}'
)
COMMON_OUTPUT_FIELDS = (
    "},diagnostics:[],opaque:Other{global_bindings:\"keep\"},"
    "propagating_statements:{},"
    f"expr_types:{{{CALL_SPAN}:Bool}},stream_stage_types:{{}},"
    "statically_resolved_call_spans:{"
    + CALL_SPAN
    + "},standard_call_spans:{},api_calls:{},module_commands:{},"
    + "record_constructor_instances:{},message_payload_constructors:{},error_constructors:{},"
    + "static_callable_aliases:{}"
)
LEGACY_PUBLICATION = (
    COMMON
    + f",global_bindings:{{(None,Name(\"answer\")):{EXPR}}},tail_bindings:{{{TAIL_SPAN}:{TAIL_EXPR}}}"
    + COMMON_OUTPUT_FIELDS
    + "}"
).replace("propagating_statements:{}", f"propagating_statements:{{{SPAN}}}")
CURRENT_PUBLICATION = (
    COMMON.replace("(ExprId(2),false)", "((ExprId(2),None),false)")
    + COMMON_OUTPUT_FIELDS
    + ","
    + "selected_function_calls:{"
    + f"{CALL_SPAN}:CheckedFunctionTarget{{definition:FunctionDefId(1),namespace:None,name:Name(\"run\")}}"
    + "},checked_control_flow:CheckedControlFlowFacts{complete:true,"
    + f"exhaustive_match_spans:{{{SPAN}}},builtin_range_calls:{{}},"
    + "cardinality_preserving_collection_calls:{},propagating_error_spans:{},"
    + "latent_stream_error_spans:{},fallible_pipeline_stage_spans:{},"
    + "propagating_stage_callback_spans:{}}}"
).replace(
    "propagating_statements:{}",
    f"propagating_statements:{{{SPAN},{COMMAND_SPAN}}}",
).replace(
    "propagating_error_spans:{}",
    f"propagating_error_spans:{{{COMMAND_SPAN}}}",
)
OWNER_INDEX = (
    "source\tSourceId(0)\t100\n"
    f"owner\t{CONST_SPAN}\tstmt:Const\n"
    f"owner\t{TAIL_CONST_SPAN}\tstmt:Const\n"
    f"owner\t{SPAN}\tstmt:Match\n"
    f"owner\t{CALL_SPAN}\texpr:Call\n"
    f"owner\t{COMMAND_SPAN}\tstmt:Command\n"
    f"owner\t{TAIL_SPAN}\tstmt:TailBareIdent\n"
    f"owner\t{RUNTIME_TAIL_SPAN}\tstmt:TailBareIdent\n"
    f"owner\t{RUN_GLOBAL_SPAN}\tstmt:Const\n"
    f"owner\t{UNPREPARED_TAIL_DECL_SPAN}\tstmt:Const\n"
    f"owner\t{UNPREPARED_TAIL_SPAN}\tstmt:TailBareIdent\n"
    f"owner\t{MISSING_VALUE_DECL_SPAN}\tstmt:Const\n"
    f"owner\t{MISSING_VALUE_TAIL_SPAN}\tstmt:TailBareIdent\n"
    f"stmt\tStmtId(1)\t{CONST_SPAN}\tstmt:Const\n"
    f"stmt\tStmtId(4)\t{TAIL_CONST_SPAN}\tstmt:Const\n"
    f"stmt\tStmtId(6)\t{SPAN}\tstmt:Match\n"
    f"stmt\tStmtId(17)\t{COMMAND_SPAN}\tstmt:Command\n"
    f"stmt\tStmtId(3)\t{TAIL_SPAN}\tstmt:TailBareIdent\n"
    f"stmt\tStmtId(5)\t{RUNTIME_TAIL_SPAN}\tstmt:TailBareIdent\n"
    f"stmt\tStmtId(12)\t{RUN_GLOBAL_SPAN}\tstmt:Const\n"
    f"stmt\tStmtId(14)\t{UNPREPARED_TAIL_DECL_SPAN}\tstmt:Const\n"
    f"stmt\tStmtId(13)\t{UNPREPARED_TAIL_SPAN}\tstmt:TailBareIdent\n"
    f"stmt\tStmtId(16)\t{MISSING_VALUE_DECL_SPAN}\tstmt:Const\n"
    f"stmt\tStmtId(15)\t{MISSING_VALUE_TAIL_SPAN}\tstmt:TailBareIdent\n"
    f"tail\tStmtId(3)\t{TAIL_SPAN}\tanswer\tstmt:TailBareIdent\n"
    f"tail\tStmtId(5)\t{RUNTIME_TAIL_SPAN}\truntime\tstmt:TailBareIdent\n"
    f"tail\tStmtId(13)\t{UNPREPARED_TAIL_SPAN}\tno_binding\tstmt:TailBareIdent\n"
    f"tail\tStmtId(15)\t{MISSING_VALUE_TAIL_SPAN}\tno_value\tstmt:TailBareIdent\n"
)
SOURCE_SIZES, SOURCE_OWNERS, TAIL_STATEMENT_OWNERS, STATEMENT_OWNERS = parse_owner_index(OWNER_INDEX)


def project_legacy(rendered, tail_statement_owners=TAIL_STATEMENT_OWNERS):
    return project_publication(
        "legacy",
        rendered,
        SOURCE_SIZES,
        SOURCE_OWNERS,
        tail_statement_owners,
        STATEMENT_OWNERS,
    )


def exact_call_owners(extra_call_span=None):
    expressions = {
        0: (CALL_SPAN, "Call", ""),
        1: ("Span{source_id:SourceId(0),start:20,len:3}", "Ident", "run"),
    }
    calls = {0: 1}
    if extra_call_span is not None:
        expressions.update(
            {
                2: (extra_call_span, "Call", ""),
                3: ("Span{source_id:SourceId(0),start:70,len:3}", "Ident", "lines"),
            }
        )
        calls[2] = 3
    return {
        "exprs": expressions,
        "calls": calls,
        "field_accesses": {},
        "runs": {},
        "records": {},
        "aliases": {},
        "tails": {},
        "expr_count": len(expressions),
        "record_field_count": 0,
    }


def exact_stage_publication(kind="Range", include_type=True):
    current, owners = exact_current_call_publication()
    stage_type = f"(StreamStageId(0),{STAGE_SPAN}):Range" if include_type else ""
    current = current.replace(
        "stream_stage_types:{}", f"stream_stage_types:{{{stage_type}}}"
    ).replace(
        "fallible_pipeline_stage_spans:{}",
        f"fallible_pipeline_stage_spans:{{{STAGE_SPAN}}}",
    )
    exact_owners = exact_call_owners()
    exact_owners["structured_stages"] = {0: (STAGE_SPAN, kind)}
    return current, owners, exact_owners


def exact_current_call_publication(extra_call_span=None):
    declaration = (
        'CheckedFunctionDeclaration{namespace:None,name:Name("run"),internal:false,kind:Pure,'
        "signature:Some(CompactFunctionSig{params:[],parameter_schemas:[],return_ty:Bool,"
        "return_type_expr:TypeExprId(1),inferred_effects:false,effects:None}),"
        f"body:{CONST_SPAN},parameter_spans:[]}}"
    )
    current = CURRENT_PUBLICATION.replace(
        'CheckedFunctionDeclaration{namespace:None,name:Name("run")}', declaration
    )
    resolver_namespaces = ["ExprId(2):None"]
    if extra_call_span is not None:
        resolver_namespaces.append("ExprId(4):None")
    tails_end = "StmtId(15):" + MISSING_VALUE_TAIL_BINDING + "}},statement_positions:"
    current = current.replace(
        tails_end,
        "StmtId(15):" + MISSING_VALUE_TAIL_BINDING + "},declaration_sites:{},expressions:{},expression_namespaces:{"
        + ",".join(resolver_namespaces) + "}},statement_positions:",
    )
    owners = dict(SOURCE_OWNERS)
    owners["Span{source_id:SourceId(0),start:20,len:3}"] = {"expr:Ident"}
    if extra_call_span is not None:
        stream_declaration = (
            'FunctionDefId(2):CheckedFunctionDeclaration{namespace:None,name:Name("lines"),'
            "internal:false,kind:Stream,signature:Some(CompactFunctionSig{params:[],"
            "parameter_schemas:[],return_ty:Stream(Str),return_type_expr:TypeExprId(2),"
            "inferred_effects:false,effects:None}),body:" + CONST_SPAN + ",parameter_spans:[]}"
        )
        alternate_declaration = stream_declaration.replace(
            "FunctionDefId(2)", "FunctionDefId(3)"
        ).replace('name:Name("lines")', 'name:Name("alternate")')
        current = current.replace(
            "FunctionDefId(1):" + declaration + "}},resolved_bindings:",
            "FunctionDefId(1):" + declaration + "," + stream_declaration + ","
            + alternate_declaration + "}},resolved_bindings:",
        )
        extra_ident = "Span{source_id:SourceId(0),start:70,len:3}"
        owners[extra_call_span] = {"expr:Call"}
        owners[extra_ident] = {"expr:Ident"}
        target = (
            f"{extra_call_span}:CheckedFunctionTarget{{definition:FunctionDefId(2),"
            'namespace:None,name:Name("lines")}'
        )
        current = current.replace(
            f"selected_function_calls:{{{CALL_SPAN}:",
            f"selected_function_calls:{{{target},{CALL_SPAN}:",
        ).replace(
            f"expr_types:{{{CALL_SPAN}:Bool}}",
            f"expr_types:{{{CALL_SPAN}:Bool,{extra_call_span}:Stream(Str)}}",
        )
    return current, owners


def exact_native_set_publication(shadowed=False):
    current, owners = exact_current_call_publication()
    current = current.replace(
        f"statically_resolved_call_spans:{{{CALL_SPAN}}}",
        f"statically_resolved_call_spans:{{{CALL_SPAN},{SET_CALL_SPAN}}}",
    ).replace(
        f"expr_types:{{{CALL_SPAN}:Bool}}",
        f"expr_types:{{{CALL_SPAN}:Bool,{SET_CALL_SPAN}:Set(Int)}}",
    )
    bindings = (
        "ExprId(5):BindingId(Import(UseStmtId(9)))"
        if shadowed
        else ""
    )
    current = current.replace(
        "expressions:{},expression_namespaces:{ExprId(2):None}",
        "expressions:{" + bindings + "},expression_namespaces:{ExprId(2):None,ExprId(5):None}",
    )
    owners.update(
        {
            SET_CALL_SPAN: {"expr:Call"},
            SET_FIELD_SPAN: {"expr:Field"},
            SET_IDENT_SPAN: {"expr:Ident"},
        }
    )
    return current, owners, {
        "exprs": {
            0: (CALL_SPAN, "Call", ""),
            1: ("Span{source_id:SourceId(0),start:20,len:3}", "Ident", "run"),
            2: (SET_CALL_SPAN, "Call", ""),
            3: (SET_FIELD_SPAN, "Field", ""),
            4: (SET_IDENT_SPAN, "Ident", "set"),
        },
        "calls": {0: 1, 2: 3},
        "field_accesses": {3: (4, "empty")},
        "runs": {},
        "records": {},
        "aliases": {},
        "tails": {},
        "expr_count": 5,
        "record_field_count": 0,
    }


def exact_static_alias_publication(callee_kind="Field", method_call=False, definition="None"):
    current, owners = exact_current_call_publication(EXTRA_CALL_SPAN)
    current = current.replace(
        f"statically_resolved_call_spans:{{{CALL_SPAN}}}",
        f"statically_resolved_call_spans:{{{CALL_SPAN},{EXTRA_CALL_SPAN}}}",
    ).replace(
        f"expr_types:{{{CALL_SPAN}:Bool,{EXTRA_CALL_SPAN}:Stream(Str)}}",
        f"expr_types:{{{CALL_SPAN}:Bool,{EXTRA_CALL_SPAN}:Bool}}",
    ).replace(
        f'{EXTRA_CALL_SPAN}:CheckedFunctionTarget{{definition:FunctionDefId(2),namespace:None,name:Name("lines")}},',
        "",
    )
    alias_span = ALIAS_FIELD_SPAN if callee_kind == "Field" else ALIAS_IDENT_SPAN
    alias_value = (
        f"StaticCallableAlias{{definition:{definition},method_call:{str(method_call).lower()},"
        'name:Name("render"),pure:true,'
        "signature:CallableType{effects:None,params:[],return_ty:Bool}}"
    )
    current = current.replace(
        "static_callable_aliases:{}",
        f"static_callable_aliases:{{{alias_span}:{alias_value}}}",
    )
    if callee_kind == "Field":
        owners.update({ALIAS_FIELD_SPAN: {"expr:Field"}, ALIAS_IDENT_SPAN: {"expr:Ident"}})
        exact_expression_rows = {
            3: (ALIAS_FIELD_SPAN, "Field", ""),
            4: (ALIAS_IDENT_SPAN, "Ident", "targets"),
        }
        exact_field_accesses = {3: (4, "call" if method_call else "native_execution")}
    else:
        owners.update({ALIAS_IDENT_SPAN: {"expr:Ident"}})
        exact_expression_rows = {3: (ALIAS_IDENT_SPAN, "Ident", "render")}
        exact_field_accesses = {}
    exact_owners = exact_call_owners(EXTRA_CALL_SPAN)
    exact_owners["exprs"].update(exact_expression_rows)
    exact_owners["field_accesses"] = exact_field_accesses
    exact_owners["expr_count"] = len(exact_owners["exprs"])
    return current, owners, exact_owners


def exact_qualified_call_publication():
    current, owners = exact_current_call_publication()
    current = current.replace(
        'namespace:None,name:Name("run")',
        'namespace:Some(Name("module.xsh")),name:Name("run")',
    ).replace(
        'kind:Constant(StmtId(12)),name:Name("not_prepared")',
        'kind:Module(Name("module.xsh")),name:Name("mod")',
    ).replace(
        "expressions:{},expression_namespaces:{ExprId(2):None}",
        "expressions:{ExprId(3):" + RUN_GLOBAL_BINDING + "},"
        "expression_namespaces:{ExprId(2):None,ExprId(3):None}",
    )
    owners.update(
        {
            "Span{source_id:SourceId(0),start:20,len:8}": {"expr:Field"},
            "Span{source_id:SourceId(0),start:20,len:3}": {"expr:Ident"},
        }
    )
    return current, owners, {
        "exprs": {
            0: (CALL_SPAN, "Call", ""),
            1: ("Span{source_id:SourceId(0),start:20,len:8}", "Field", ""),
            2: ("Span{source_id:SourceId(0),start:20,len:3}", "Ident", "mod"),
        },
        "calls": {0: 1},
        "field_accesses": {1: (2, "run")},
        "runs": {},
        "records": {},
        "aliases": {},
        "tails": {},
        "expr_count": 3,
        "record_field_count": 0,
    }


def exact_native_error_failure_publication(field="failure", return_type="Error"):
    current, owners = exact_current_call_publication()
    current = current.replace(
        f"statically_resolved_call_spans:{{{CALL_SPAN}}}",
        f"statically_resolved_call_spans:{{{CALL_SPAN},{ERROR_FAILURE_SPAN}}}",
    ).replace(
        f"expr_types:{{{CALL_SPAN}:Bool}}",
        f"expr_types:{{{CALL_SPAN}:Bool,{ERROR_FAILURE_SPAN}:{return_type}}}",
    )
    owners.update(
        {
            ERROR_FAILURE_SPAN: {"expr:Call"},
            ERROR_FAILURE_FIELD_SPAN: {"expr:Field"},
            ERROR_FAILURE_IDENT_SPAN: {"expr:Ident"},
        }
    )
    return current, owners, {
        "exprs": {
            0: (CALL_SPAN, "Call", ""),
            1: ("Span{source_id:SourceId(0),start:20,len:3}", "Ident", "run"),
            2: (ERROR_FAILURE_SPAN, "Call", ""),
            3: (ERROR_FAILURE_FIELD_SPAN, "Field", ""),
            4: (ERROR_FAILURE_IDENT_SPAN, "Ident", "error"),
        },
        "calls": {0: 1, 2: 3},
        "field_accesses": {3: (4, field)},
        "runs": {},
        "records": {},
        "aliases": {},
        "tails": {},
        "expr_count": 5,
        "record_field_count": 0,
    }


class PublicationAdapterTests(unittest.TestCase):
    def test_owner_index_rows_must_be_typed_unique_and_source_bounded(self):
        self.assertEqual(SOURCE_SIZES, {0: 100})
        self.assertEqual(SOURCE_OWNERS[CALL_SPAN], {"expr:Call"})
        self.assertEqual(TAIL_STATEMENT_OWNERS["StmtId(3)"], (TAIL_SPAN, "answer", "stmt:TailBareIdent"))
        self.assertEqual(STATEMENT_OWNERS["StmtId(4)"], (TAIL_CONST_SPAN, "stmt:Const"))

        reordered = OWNER_INDEX.replace(
            SPAN, "Span{len:6,start:10,source_id:SourceId(0)}"
        )
        self.assertEqual(parse_owner_index(reordered), (SOURCE_SIZES, SOURCE_OWNERS, TAIL_STATEMENT_OWNERS, STATEMENT_OWNERS))

        for malformed_span in (
            "Span{len:6,start:10,source_id:0}",
            "Span{len:6,start:ten,source_id:SourceId(0)}",
            "Span{len:6,start:10,source_id:SourceId(0),other:0}",
            "Span{len:6,start:10,start:10,source_id:SourceId(0)}",
        ):
            malformed_owner = OWNER_INDEX.replace(
                f"owner\t{SPAN}\tstmt:Match", f"owner\t{malformed_span}\tstmt:Match"
            )
            with self.subTest(malformed_span=malformed_span), self.assertRaisesRegex(
                AdapterError, "malformed span"
            ):
                parse_owner_index(malformed_owner)

        duplicate = OWNER_INDEX + f"owner\t{CALL_SPAN}\texpr:Call\n"
        with self.assertRaisesRegex(AdapterError, "duplicate owner row"):
            parse_owner_index(duplicate)

        outside = OWNER_INDEX.replace("source\tSourceId(0)\t100", "source\tSourceId(0)\t45")
        with self.assertRaisesRegex(AdapterError, "outside its source"):
            parse_owner_index(outside)

        missing_tail_owner = OWNER_INDEX.replace(
            f"owner\t{TAIL_SPAN}\tstmt:TailBareIdent\n", ""
        )
        with self.assertRaisesRegex(AdapterError, "no TailBareIdent node"):
            parse_owner_index(missing_tail_owner)

    def test_projection_removes_only_the_four_schema_fields_after_nonempty_legacy_check(self):
        legacy = project_legacy(LEGACY_PUBLICATION)
        current = project_publication(
            "current", CURRENT_PUBLICATION, SOURCE_SIZES, SOURCE_OWNERS
        )

        self.assertEqual(legacy, current)
        self.assertIn('opaque:Other{global_bindings:"keep"}', legacy)

    def test_projection_keeps_opaque_quoted_delimiters_and_rejects_root_corruption(self):
        opaque_field = r'opaque:Other{global_bindings:"brace } , { : \"quoted\" keep"}'
        publication = LEGACY_PUBLICATION.replace(
            'opaque:Other{global_bindings:"keep"}', opaque_field
        )
        self.assertIn(opaque_field, project_legacy(publication))

        with self.assertRaisesRegex(AdapterError, "unterminated Rust Debug value"):
            project_legacy(LEGACY_PUBLICATION[:-1])

        duplicate = LEGACY_PUBLICATION[:-1] + ",diagnostics:[]}"
        with self.assertRaisesRegex(AdapterError, "duplicate CheckOutput Debug field"):
            project_legacy(duplicate)

    def test_legacy_global_and_tail_fields_must_match_common_facts(self):
        missing_global = LEGACY_PUBLICATION.replace(
            '(None,Name("answer")):ExprId(2)', '(None,Name("answer")):ExprId(3)'
        )
        with self.assertRaisesRegex(AdapterError, "global bindings differ"):
            project_legacy(missing_global)

        missing_tail = LEGACY_PUBLICATION.replace(
            f"tail_bindings:{{{TAIL_SPAN}:{TAIL_EXPR}}}", "tail_bindings:{}"
        )
        with self.assertRaisesRegex(AdapterError, "tail bindings differ"):
            project_legacy(missing_tail)

        nonconstant_globals = LEGACY_PUBLICATION.replace(
            f"declarations:{{{BINDING}:{DECLARATION}}}",
            f"declarations:{{{BINDING}:{DECLARATION},"
            "BindingId(Import(UseStmtId(2))):BindingDeclaration{exported:false,global:true,"
            "kind:Module(Name(\"module\")),name:Name(\"module\"),namespace:None,"
            "span:Span{source_id:SourceId(0),start:30,len:6}},"
            "BindingId(Target(BindingTargetId(8))):BindingDeclaration{exported:false,global:true,"
            "kind:Runtime,name:Name(\"runtime\"),namespace:None,"
            "span:Span{source_id:SourceId(0),start:40,len:6}}}",
        )
        project_legacy(nonconstant_globals)

        wrong_declaration_span = LEGACY_PUBLICATION.replace(
            f"kind:Constant(StmtId(1)),name:Name(\"answer\"),namespace:None,span:{CONST_SPAN}",
            f"kind:Constant(StmtId(1)),name:Name(\"answer\"),namespace:None,span:{TAIL_CONST_SPAN}",
        )
        with self.assertRaisesRegex(AdapterError, "legacy constant has no matching AST declaration"):
            project_legacy(wrong_declaration_span)

    def test_nonempty_legacy_global_and_tail_spans_normalize_without_rewriting_common(self):
        reordered_constant_span = "Span{len:17,source_id:SourceId(0),start:0}"
        reordered_tail_span = "Span{len:6,start:40,source_id:SourceId(0)}"
        owner_index = OWNER_INDEX.replace(CONST_SPAN, reordered_constant_span)
        source_sizes, source_owners, tail_statement_owners, statement_owners = parse_owner_index(owner_index)
        legacy = LEGACY_PUBLICATION.replace(CONST_SPAN, reordered_constant_span).replace(
            f"tail_bindings:{{{TAIL_SPAN}:{TAIL_EXPR}}}",
            f"tail_bindings:{{{reordered_tail_span}:{TAIL_EXPR}}}",
        )

        projected = project_publication(
            "legacy", legacy, source_sizes, source_owners, tail_statement_owners, statement_owners
        )
        expected_common = project_legacy(LEGACY_PUBLICATION)
        self.assertEqual(projected, expected_common.replace(CONST_SPAN, reordered_constant_span))

    def test_current_fact_owner_joins_normalize_span_field_order(self):
        reordered_call_span = "Span{len:12,start:20,source_id:SourceId(0)}"
        reordered_match_span = "Span{start:10,len:6,source_id:SourceId(0)}"
        current = CURRENT_PUBLICATION.replace(
            f"selected_function_calls:{{{CALL_SPAN}:",
            f"selected_function_calls:{{{reordered_call_span}:",
        ).replace(
            f"statically_resolved_call_spans:{{{CALL_SPAN}}}",
            f"statically_resolved_call_spans:{{{reordered_call_span}}}",
        ).replace(
            "builtin_range_calls:{}",
            f"builtin_range_calls:{{{reordered_call_span}}}",
        ).replace(
            f"exhaustive_match_spans:{{{SPAN}}}",
            f"exhaustive_match_spans:{{{reordered_match_span}}}",
        )

        project_publication("current", current, SOURCE_SIZES, SOURCE_OWNERS)

    def test_selected_function_calls_reject_removed_added_and_wrong_owner_rows(self):
        missing = CURRENT_PUBLICATION.replace(
            f"selected_function_calls:{{{CALL_SPAN}:CheckedFunctionTarget{{definition:FunctionDefId(1),namespace:None,name:Name(\"run\")}}}}",
            "selected_function_calls:{}",
        )
        with self.assertRaisesRegex(AdapterError, "static call-site facts"):
            project_publication("current", missing, SOURCE_SIZES, SOURCE_OWNERS)

        extra_span = "Span{source_id:SourceId(0),start:40,len:12}"
        added = CURRENT_PUBLICATION.replace(
            f"selected_function_calls:{{{CALL_SPAN}:",
            f"selected_function_calls:{{{extra_span}:CheckedFunctionTarget{{definition:FunctionDefId(1),namespace:None,name:Name(\"run\")}},{CALL_SPAN}:",
        )
        with self.assertRaisesRegex(AdapterError, "static call-site facts"):
            project_publication("current", added, SOURCE_SIZES, SOURCE_OWNERS)

        wrong_owner = CURRENT_PUBLICATION.replace(
            'definition:FunctionDefId(1),namespace:None,name:Name("run")',
            'definition:FunctionDefId(1),namespace:None,name:Name("other")',
        )
        with self.assertRaisesRegex(AdapterError, "disagrees with its declaration"):
            project_publication("current", wrong_owner, SOURCE_SIZES, SOURCE_OWNERS)

        extra_target_field = CURRENT_PUBLICATION.replace(
            'CheckedFunctionTarget{definition:FunctionDefId(1)',
            'CheckedFunctionTarget{unexpected:true,definition:FunctionDefId(1)',
        )
        with self.assertRaisesRegex(AdapterError, "target has unexpected fields"):
            project_publication("current", extra_target_field, SOURCE_SIZES, SOURCE_OWNERS)

        outside_source = CURRENT_PUBLICATION.replace(CALL_SPAN, "Span{source_id:SourceId(1),start:20,len:12}")
        with self.assertRaisesRegex(AdapterError, "outside its source"):
            project_publication("current", outside_source, SOURCE_SIZES, SOURCE_OWNERS)

    def test_exact_call_proof_allows_nonstatic_stream_call_and_requires_one_call_id(self):
        current, owners = exact_current_call_publication(EXTRA_CALL_SPAN)
        project_publication(
            "current", current, SOURCE_SIZES, owners,
            candidate_exact_owners=exact_call_owners(EXTRA_CALL_SPAN),
        )

        wrong_target = current.replace(
            f"{EXTRA_CALL_SPAN}:CheckedFunctionTarget{{definition:FunctionDefId(2),namespace:None,name:Name(\"lines\")}}",
            f"{EXTRA_CALL_SPAN}:CheckedFunctionTarget{{definition:FunctionDefId(2),namespace:None,name:Name(\"other\")}}",
        )
        with self.assertRaisesRegex(AdapterError, "disagrees with its declaration"):
            project_publication(
                "current", wrong_target, SOURCE_SIZES, owners,
                candidate_exact_owners=exact_call_owners(EXTRA_CALL_SPAN),
            )

        same_signature_target = current.replace(
            f'{EXTRA_CALL_SPAN}:CheckedFunctionTarget{{definition:FunctionDefId(2),namespace:None,name:Name("lines")}}',
            f'{EXTRA_CALL_SPAN}:CheckedFunctionTarget{{definition:FunctionDefId(3),namespace:None,name:Name("alternate")}}',
        )
        with self.assertRaisesRegex(AdapterError, "callee identity disagrees"):
            project_publication(
                "current", same_signature_target, SOURCE_SIZES, owners,
                candidate_exact_owners=exact_call_owners(EXTRA_CALL_SPAN),
            )

        wrong_resolver_namespace = current.replace(
            "expression_namespaces:{ExprId(2):None,ExprId(4):None}",
            'expression_namespaces:{ExprId(2):Some(Name("other.xsh")),ExprId(4):None}',
        )
        with self.assertRaisesRegex(AdapterError, "callee identity disagrees"):
            project_publication(
                "current", wrong_resolver_namespace, SOURCE_SIZES, owners,
                candidate_exact_owners=exact_call_owners(EXTRA_CALL_SPAN),
            )

        run_declaration = (
            'CheckedFunctionDeclaration{namespace:None,name:Name("run"),internal:false,kind:Pure,'
            "signature:Some(CompactFunctionSig{params:[],parameter_schemas:[],return_ty:Bool,"
            "return_type_expr:TypeExprId(1),inferred_effects:false,effects:None}),body:"
            + CONST_SPAN
            + ",parameter_spans:[]}"
        )
        duplicate = "FunctionDefId(4):" + run_declaration
        ambiguous_target = current.replace(
            "FunctionDefId(1):" + run_declaration + ",FunctionDefId(2):",
            "FunctionDefId(1):" + run_declaration + "," + duplicate + ",FunctionDefId(2):",
        ).replace(
            f'{CALL_SPAN}:CheckedFunctionTarget{{definition:FunctionDefId(1),namespace:None,name:Name("run")}}',
            f'{CALL_SPAN}:CheckedFunctionTarget{{definition:FunctionDefId(4),namespace:None,name:Name("run")}}',
        )
        with self.assertRaisesRegex(AdapterError, "ambiguous checked declaration identity"):
            project_publication(
                "current", ambiguous_target, SOURCE_SIZES, owners,
                candidate_exact_owners=exact_call_owners(EXTRA_CALL_SPAN),
            )

        missing_callee = exact_call_owners(EXTRA_CALL_SPAN)
        missing_callee["calls"] = {0: 1}
        with self.assertRaisesRegex(AdapterError, "no exact callee link"):
            project_publication(
                "current", current, SOURCE_SIZES, owners,
                candidate_exact_owners=missing_callee,
            )


        invalid_callee = exact_call_owners(EXTRA_CALL_SPAN)
        invalid_callee["calls"][2] = 99
        with self.assertRaisesRegex(AdapterError, "invalid exact callee link"):
            project_publication(
                "current", current, SOURCE_SIZES, owners,
                candidate_exact_owners=invalid_callee,
            )

        missing_call_id = exact_call_owners(EXTRA_CALL_SPAN)
        missing_call_id["exprs"][0] = (CALL_SPAN, "Ident", "run")
        with self.assertRaisesRegex(AdapterError, "no unique exact AST identity"):
            project_publication(
                "current", current, SOURCE_SIZES, owners,
                candidate_exact_owners=missing_call_id,
            )

    def test_exact_qualified_call_uses_its_resolved_module_binding(self):
        current, owners, exact_owners = exact_qualified_call_publication()
        project_publication(
            "current", current, SOURCE_SIZES, owners,
            candidate_exact_owners=exact_owners,
        )

        wrong_module = current.replace(
            'kind:Module(Name("module.xsh")),name:Name("mod")',
            'kind:Module(Name("other.xsh")),name:Name("mod")',
        )
        with self.assertRaisesRegex(AdapterError, "module binding namespace disagrees"):
            project_publication(
                "current", wrong_module, SOURCE_SIZES, owners,
                candidate_exact_owners=exact_owners,
            )

    def test_static_callable_alias_routes_join_exact_callee_spans(self):
        for callee_kind, method_call in (("Field", False), ("Ident", False), ("Field", True)):
            with self.subTest(callee_kind=callee_kind, method_call=method_call):
                current, owners, exact_owners = exact_static_alias_publication(
                    callee_kind=callee_kind,
                    method_call=method_call,
                )
                projected = project_publication(
                    "current", current, SOURCE_SIZES, owners,
                    candidate_exact_owners=exact_owners,
                )
                self.assertIn("static_callable_aliases:{", projected)

        no_alias_fact = exact_static_alias_publication()
        no_alias_fact_current = no_alias_fact[0].replace(
            f"static_callable_aliases:{{{ALIAS_FIELD_SPAN}:"
            "StaticCallableAlias{definition:None,method_call:false,name:Name(\"render\"),pure:true,"
            "signature:CallableType{effects:None,params:[],return_ty:Bool}}}",
            "static_callable_aliases:{}",
        )
        with self.assertRaisesRegex(AdapterError, "static call-site facts"):
            project_publication(
                "current", no_alias_fact_current, SOURCE_SIZES, no_alias_fact[1],
                candidate_exact_owners=no_alias_fact[2],
            )

        wrong_callee = exact_static_alias_publication()
        wrong_callee[2]["calls"][2] = 1
        with self.assertRaisesRegex(AdapterError, "static call-site facts"):
            project_publication(
                "current", wrong_callee[0], SOURCE_SIZES, wrong_callee[1],
                candidate_exact_owners=wrong_callee[2],
            )

        wrong_method_flag = exact_static_alias_publication(method_call=True)
        wrong_method_flag_current = wrong_method_flag[0].replace(
            "method_call:true", "method_call:false"
        )
        with self.assertRaisesRegex(AdapterError, "false method flag"):
            project_publication(
                "current", wrong_method_flag_current, SOURCE_SIZES, wrong_method_flag[1],
                candidate_exact_owners=wrong_method_flag[2],
            )

        wrong_method_field = exact_static_alias_publication(method_call=True)
        wrong_method_field[2]["field_accesses"][3] = (4, "native_execution")
        with self.assertRaisesRegex(AdapterError, "wrong field"):
            project_publication(
                "current", wrong_method_field[0], SOURCE_SIZES, wrong_method_field[1],
                candidate_exact_owners=wrong_method_field[2],
            )

        malformed_method_flag = exact_static_alias_publication()
        malformed_method_flag_current = malformed_method_flag[0].replace(
            "method_call:false", "method_call:maybe"
        )
        with self.assertRaisesRegex(AdapterError, "malformed flags"):
            project_publication(
                "current", malformed_method_flag_current, SOURCE_SIZES, malformed_method_flag[1],
                candidate_exact_owners=malformed_method_flag[2],
            )

        malformed_signature = exact_static_alias_publication()
        malformed_signature_current = malformed_signature[0].replace(
            "signature:CallableType{effects:None,params:[],return_ty:Bool}",
            "signature:CallableType{effects:Bad,params:[],return_ty:Bool}",
        )
        with self.assertRaisesRegex(AdapterError, "malformed effects"):
            project_publication(
                "current", malformed_signature_current, SOURCE_SIZES, malformed_signature[1],
                candidate_exact_owners=malformed_signature[2],
            )

        missing_call_type = exact_static_alias_publication()
        missing_call_type_current = missing_call_type[0].replace(
            f",{EXTRA_CALL_SPAN}:Bool", ""
        )
        with self.assertRaisesRegex(AdapterError, "no checked type"):
            project_publication(
                "current", missing_call_type_current, SOURCE_SIZES, missing_call_type[1],
                candidate_exact_owners=missing_call_type[2],
            )

        module_contract_alias = exact_static_alias_publication(
            definition=f"Some({ALIAS_DEFINITION_SPAN})"
        )
        source_sizes = {**SOURCE_SIZES, 5: 100}
        project_publication(
            "current", module_contract_alias[0], source_sizes, module_contract_alias[1],
            candidate_exact_owners=module_contract_alias[2],
        )
        out_of_bounds_definition = module_contract_alias[0].replace(
            ALIAS_DEFINITION_SPAN,
            "Span{source_id:SourceId(5),start:90,len:20}",
        )
        with self.assertRaisesRegex(AdapterError, "outside its source"):
            project_publication(
                "current", out_of_bounds_definition, source_sizes, module_contract_alias[1],
                candidate_exact_owners=module_contract_alias[2],
            )

    def test_native_set_call_requires_exact_unshadowed_module_resolution(self):
        current, owners, exact_owners = exact_native_set_publication()
        project_publication(
            "current", current, SOURCE_SIZES, owners,
            candidate_exact_owners=exact_owners,
        )

        shadowed, shadowed_owners, shadowed_exact = exact_native_set_publication(shadowed=True)
        with self.assertRaisesRegex(AdapterError, "shadowed by a binding"):
            project_publication(
                "current", shadowed, SOURCE_SIZES, shadowed_owners,
                candidate_exact_owners=shadowed_exact,
            )

    def test_error_intrinsics_require_exact_call_shape_type_and_links(self):
        current, owners, exact_owners = exact_native_error_failure_publication()
        project_publication(
            "current", current, SOURCE_SIZES, owners,
            candidate_exact_owners=exact_owners,
        )

        fail_call = exact_native_error_failure_publication("fail", "Result(Unit,Error)")
        project_publication(
            "current", fail_call[0], SOURCE_SIZES, fail_call[1],
            candidate_exact_owners=fail_call[2],
        )

        wrong_type = current.replace(f"{ERROR_FAILURE_SPAN}:Error", f"{ERROR_FAILURE_SPAN}:Bool")
        wrong_fail_type = fail_call[0].replace(
            f"{ERROR_FAILURE_SPAN}:Result(Unit,Error)",
            f"{ERROR_FAILURE_SPAN}:Result(Unit,Bool)",
        )
        wrong_field = exact_native_error_failure_publication()[2]
        wrong_field["field_accesses"][3] = (4, "failed")
        wrong_case = exact_native_error_failure_publication()[2]
        wrong_case["field_accesses"][3] = (4, "Failure")
        wrong_name = exact_native_error_failure_publication()[2]
        wrong_name["exprs"][4] = (ERROR_FAILURE_IDENT_SPAN, "Ident", "err")
        wrong_name_case = exact_native_error_failure_publication()[2]
        wrong_name_case["exprs"][4] = (ERROR_FAILURE_IDENT_SPAN, "Ident", "Error")
        wrong_call_link = exact_native_error_failure_publication()[2]
        wrong_call_link["calls"][2] = 4
        wrong_field_link = exact_native_error_failure_publication()[2]
        wrong_field_link["field_accesses"][3] = (2, "failure")

        for publication, exact in (
            (wrong_type, exact_native_error_failure_publication()[2]),
            (wrong_fail_type, exact_native_error_failure_publication("fail", "Result(Unit,Error)")[2]),
            (current, wrong_field),
            (current, wrong_case),
            (current, wrong_name),
            (current, wrong_name_case),
            (current, wrong_call_link),
            (current, wrong_field_link),
        ):
            with self.subTest(exact=exact, publication=publication):
                with self.assertRaisesRegex(AdapterError, "static call-site facts"):
                    project_publication(
                        "current", publication, SOURCE_SIZES, owners,
                        candidate_exact_owners=exact,
                    )

        shadowed_error = current.replace(
            "ResolvedBindings{declarations:{",
            "ResolvedBindings{declarations:{BindingId(Target(BindingTargetId(99))):"
            'BindingDeclaration{exported:false,global:false,kind:Runtime,name:Name("error"),'
            f"namespace:None,span:{CONST_SPAN}}},",
            1,
        ).replace(
            "declaration_sites:{},expressions:{},expression_namespaces:{ExprId(2):None}",
            "declaration_sites:{},expressions:{ExprId(5):BindingId(Target(BindingTargetId(99)))},"
            "expression_namespaces:{ExprId(2):None,ExprId(5):None}",
        )
        project_publication(
            "current", shadowed_error, SOURCE_SIZES, owners,
            candidate_exact_owners=exact_native_error_failure_publication()[2],
        )

    def test_propagating_run_form_requires_exact_producer_and_true_flag(self):
        current, owners = exact_current_call_publication()
        current = current.replace(
            f"propagating_error_spans:{{{COMMAND_SPAN}}}",
            f"propagating_error_spans:{{{COMMAND_SPAN},{RUN_FORM_SPAN}}}",
        )
        exact_owners = exact_call_owners()
        exact_owners["runs"] = {0: (RUN_FORM_SPAN, True)}
        project_publication(
            "current", current, SOURCE_SIZES, owners,
            candidate_exact_owners=exact_owners,
        )

        exact_owners["runs"] = {0: (RUN_FORM_SPAN, False)}
        with self.assertRaisesRegex(AdapterError, "no exact propagating run"):
            project_publication(
                "current", current, SOURCE_SIZES, owners,
                candidate_exact_owners=exact_owners,
            )

    def test_exact_noncall_selection_uses_checked_function_reference_validation(self):
        current, owners = exact_current_call_publication()
        reference_span = "Span{source_id:SourceId(0),start:70,len:4}"
        owners[reference_span] = {"expr:Ident"}
        current = current.replace(
            f"selected_function_calls:{{{CALL_SPAN}:",
            f"selected_function_calls:{{{reference_span}:CheckedFunctionTarget{{definition:FunctionDefId(1),namespace:None,name:Name(\"run\")}},"
            f"{CALL_SPAN}:",
        ).replace(
            f"expr_types:{{{CALL_SPAN}:Bool}}",
            f"expr_types:{{{CALL_SPAN}:Bool,{reference_span}:Pure}}",
        ).replace(
            "expression_namespaces:{ExprId(2):None}",
            "expression_namespaces:{ExprId(2):None,ExprId(3):None}",
        )
        exact_owners = exact_call_owners()
        exact_owners["exprs"][2] = (reference_span, "Ident", "run")
        exact_owners["expr_count"] = 3
        project_publication(
            "current", current, SOURCE_SIZES, owners,
            candidate_exact_owners=exact_owners,
        )

        wrong_type = current.replace(f"{reference_span}:Pure", f"{reference_span}:Proc")
        with self.assertRaisesRegex(AdapterError, "checked type disagrees with its declaration kind"):
            project_publication(
                "current", wrong_type, SOURCE_SIZES, owners,
                candidate_exact_owners=exact_owners,
            )

    def test_control_flow_requires_complete_in_source_facts(self):
        incomplete = CURRENT_PUBLICATION.replace("complete:true", "complete:false")
        with self.assertRaisesRegex(AdapterError, "incomplete"):
            project_publication("current", incomplete, SOURCE_SIZES, SOURCE_OWNERS)

        outside_source = CURRENT_PUBLICATION.replace(
            f"exhaustive_match_spans:{{{SPAN}}}",
            "exhaustive_match_spans:{Span{source_id:SourceId(0),start:95,len:6}}",
        )
        with self.assertRaisesRegex(AdapterError, "outside its source"):
            project_publication("current", outside_source, SOURCE_SIZES, SOURCE_OWNERS)

        with self.assertRaisesRegex(AdapterError, "requires source sizes"):
            project_publication("current", CURRENT_PUBLICATION)

        with self.assertRaisesRegex(AdapterError, "requires source sizes and AST owners"):
            project_publication("current", CURRENT_PUBLICATION, SOURCE_SIZES)

    def test_control_flow_requires_exact_fields_typed_spans_and_owner_kinds(self):
        missing_category = CURRENT_PUBLICATION.replace(
            f"exhaustive_match_spans:{{{SPAN}}},", ""
        )
        with self.assertRaisesRegex(AdapterError, "fields differ: missing=.*exhaustive_match_spans"):
            project_publication("current", missing_category, SOURCE_SIZES, SOURCE_OWNERS)

        added_category = CURRENT_PUBLICATION.replace(
            "complete:true,", "complete:true,unexpected_spans:{},"
        )
        with self.assertRaisesRegex(AdapterError, "fields differ:.*unexpected_spans"):
            project_publication("current", added_category, SOURCE_SIZES, SOURCE_OWNERS)

        malformed = CURRENT_PUBLICATION.replace(
            f"exhaustive_match_spans:{{{SPAN}}}", "exhaustive_match_spans:{not_a_span}"
        )
        with self.assertRaisesRegex(AdapterError, "malformed exhaustive match spans span"):
            project_publication("current", malformed, SOURCE_SIZES, SOURCE_OWNERS)

        wrong_kind = CURRENT_PUBLICATION.replace(
            f"exhaustive_match_spans:{{{SPAN}}}",
            f"exhaustive_match_spans:{{{TAIL_SPAN}}}",
        )
        owners = {**SOURCE_OWNERS, TAIL_SPAN: {"stmt:TailBareIdent"}}
        with self.assertRaisesRegex(AdapterError, "no matching AST owner"):
            project_publication("current", wrong_kind, SOURCE_SIZES, owners)

    def test_fallible_range_and_repeat_stages_require_structured_stage_proof(self):
        for kind in ("Range", "Repeat"):
            with self.subTest(kind=kind):
                current, owners, exact_owners = exact_stage_publication(kind)
                project_publication(
                    "current", current, SOURCE_SIZES, owners,
                    candidate_exact_owners=exact_owners,
                )

        wrong_kind = exact_stage_publication("Map")
        with self.assertRaisesRegex(AdapterError, "no exact structured Range or Repeat stage"):
            project_publication(
                "current", wrong_kind[0], SOURCE_SIZES, wrong_kind[1],
                candidate_exact_owners=wrong_kind[2],
            )

        missing_stage = exact_stage_publication()
        missing_stage[2]["structured_stages"] = {}
        with self.assertRaisesRegex(AdapterError, "no exact structured Range or Repeat stage"):
            project_publication(
                "current", missing_stage[0], SOURCE_SIZES, missing_stage[1],
                candidate_exact_owners=missing_stage[2],
            )

        missing_type = exact_stage_publication(include_type=False)
        with self.assertRaisesRegex(AdapterError, "no checked stream stage"):
            project_publication(
                "current", missing_type[0], SOURCE_SIZES, missing_type[1],
                candidate_exact_owners=missing_type[2],
            )

    def test_propagating_error_command_span_joins_statement_position(self):
        command_span = "Span{source_id:SourceId(0),start:36,len:40}"
        current = CURRENT_PUBLICATION.replace(
            "propagating_error_spans:{}",
            f"propagating_error_spans:{{{command_span}}}",
        )
        owners = {**SOURCE_OWNERS, command_span: {"stmt:Command"}}

        project_publication("current", current, SOURCE_SIZES, owners)

        expression_error = CURRENT_PUBLICATION.replace(
            f"propagating_error_spans:{{{COMMAND_SPAN}}}",
            f"propagating_error_spans:{{{CALL_SPAN}}}",
        )
        project_publication("current", expression_error, SOURCE_SIZES, SOURCE_OWNERS)

        without_statement_position = current.replace(
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value}}",
            f"statement_positions:{{{SPAN}:Value}}",
        )
        with self.assertRaisesRegex(AdapterError, "no checked statement position"):
            project_publication("current", without_statement_position, SOURCE_SIZES, owners)

        unsupported_statement = {**SOURCE_OWNERS, command_span: {"stmt:LetExpr"}}
        with self.assertRaisesRegex(AdapterError, "no matching AST owner"):
            project_publication("current", current, SOURCE_SIZES, unsupported_statement)

    def test_current_cli_plan_none_field_selector_normalizes_only_its_typed_key(self):
        current_key = "((ExprId(2),None),false)"
        legacy_key = "(ExprId(2),false)"
        current = CURRENT_PUBLICATION

        legacy_projection = project_legacy(LEGACY_PUBLICATION)
        current_projection = project_publication(
            "current", current, SOURCE_SIZES, SOURCE_OWNERS
        )
        self.assertEqual(legacy_projection, current_projection)
        self.assertIn(f"{legacy_key}:Ok(Plan(\"payload\"))", current_projection)

        changed_payload = current.replace('Plan("payload")', 'Plan("changed")')
        self.assertNotEqual(
            current_projection,
            project_publication("current", changed_payload, SOURCE_SIZES, SOURCE_OWNERS),
        )

        field_key = '((ExprId(3),Some(Name("field"))),false)'
        field_selected = current.replace(
            f"data:{{{current_key}:Ok(Plan(\"payload\"))}}",
            f"data:{{{current_key}:Ok(Plan(\"payload\")),{field_key}:Err(error)}}",
        )
        with self.assertRaisesRegex(AdapterError, "Some field selector has no legacy comparison key"):
            project_publication("current", field_selected, SOURCE_SIZES, SOURCE_OWNERS)

        wrong_field_type = current.replace(
            current_key, '((ExprId(2),Some(Name(Symbol(7)))),false)'
        )
        with self.assertRaisesRegex(AdapterError, "malformed current CLI plan key"):
            project_publication("current", wrong_field_type, SOURCE_SIZES, SOURCE_OWNERS)

    def test_propagating_statement_command_projection_uses_statement_owner_and_position(self):
        legacy = project_legacy(LEGACY_PUBLICATION)
        current = project_publication(
            "current", CURRENT_PUBLICATION, SOURCE_SIZES, SOURCE_OWNERS
        )

        self.assertEqual(legacy, current)
        self.assertIn(f"propagating_statements:{{{SPAN}}}", current)

        without_statement_position = CURRENT_PUBLICATION.replace(
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value}}",
            f"statement_positions:{{{SPAN}:Value}}",
        )
        with self.assertRaisesRegex(AdapterError, "no checked statement position"):
            project_publication(
                "current", without_statement_position, SOURCE_SIZES, SOURCE_OWNERS
            )

        without_error_producer = CURRENT_PUBLICATION.replace(
            f"propagating_error_spans:{{{COMMAND_SPAN}}}",
            "propagating_error_spans:{}",
        )
        self.assertEqual(
            legacy,
            project_publication("current", without_error_producer, SOURCE_SIZES, SOURCE_OWNERS),
        )

        legacy_command = LEGACY_PUBLICATION.replace(
            f"propagating_statements:{{{SPAN}}}",
            f"propagating_statements:{{{SPAN},{COMMAND_SPAN}}}",
        )
        with self.assertRaisesRegex(AdapterError, "legacy propagating statements include a command"):
            project_legacy(legacy_command)

    def test_propagating_error_expr_statement_requires_exact_position_and_owner(self):
        current = CURRENT_PUBLICATION.replace(
            f"propagating_error_spans:{{{COMMAND_SPAN}}}",
            f"propagating_error_spans:{{{COMMAND_SPAN},{SERVICE_STOP_SPAN},{SERVICE_START_SPAN}}}",
        ).replace(
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value}}",
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value,{SERVICE_STOP_SPAN}:Value,{SERVICE_START_SPAN}:Value}}",
        )
        owners = {
            **SOURCE_OWNERS,
            SERVICE_STOP_SPAN: {"stmt:Expr"},
            SERVICE_START_SPAN: {"stmt:Expr"},
        }
        project_publication("current", current, {0: 600}, owners)

        without_position = current.replace(
            f",{SERVICE_STOP_SPAN}:Value",
            "",
        )
        with self.assertRaisesRegex(AdapterError, "no checked statement position"):
            project_publication("current", without_position, {0: 600}, owners)

        wrong_owner = {**owners, SERVICE_START_SPAN: {"stmt:Const"}}
        with self.assertRaisesRegex(AdapterError, "no matching AST owner"):
            project_publication("current", current, {0: 600}, wrong_owner)

        with self.assertRaisesRegex(AdapterError, "outside its source"):
            project_publication(
                "current", current, {0: 494}, owners
            )

    def test_propagating_assert_and_defer_require_exact_statement_id_join(self):
        for owner_kind in ("stmt:Assert", "stmt:AssertBare", "stmt:DeferExpr"):
            with self.subTest(owner_kind=owner_kind):
                current = CURRENT_PUBLICATION.replace(
                    f"propagating_error_spans:{{{COMMAND_SPAN}}}",
                    f"propagating_error_spans:{{{PROPAGATING_STATEMENT_SPAN}}}",
                ).replace(
                    f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value}}",
                    f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value,"
                    f"{PROPAGATING_STATEMENT_SPAN}:Value}}",
                )
                owners = {**SOURCE_OWNERS, PROPAGATING_STATEMENT_SPAN: {owner_kind}}
                statement_owners = {"StmtId(20)": (PROPAGATING_STATEMENT_SPAN, owner_kind)}

                project_publication(
                    "current", current, SOURCE_SIZES, owners,
                    statement_owners=statement_owners,
                )

                without_position = current.replace(
                    f",{PROPAGATING_STATEMENT_SPAN}:Value", ""
                )
                with self.assertRaisesRegex(AdapterError, "no checked statement position"):
                    project_publication(
                        "current", without_position, SOURCE_SIZES, owners,
                        statement_owners=statement_owners,
                    )

                for invalid_owners in (
                    None,
                    {"StmtId(20)": (PROPAGATING_STATEMENT_SPAN, "stmt:Const")},
                    {
                        "StmtId(20)": (PROPAGATING_STATEMENT_SPAN, owner_kind),
                        "StmtId(21)": (PROPAGATING_STATEMENT_SPAN, owner_kind),
                    },
                ):
                    with self.assertRaisesRegex(AdapterError, "no unique exact statement owner"):
                        project_publication(
                            "current", current, SOURCE_SIZES, owners,
                            statement_owners=invalid_owners,
                        )

    def test_run_form_error_does_not_inherit_command_owner_from_nearby_span(self):
        current = CURRENT_PUBLICATION.replace(
            f"propagating_error_spans:{{{COMMAND_SPAN}}}",
            f"propagating_error_spans:{{{CAPTURED_RUN_FORM_SPAN}}}",
        ).replace(
            f"propagating_statements:{{{SPAN},{COMMAND_SPAN}}}",
            f"propagating_statements:{{{SPAN},{COMMAND_SPAN},{CAPTURED_COMMAND_SPAN}}}",
        ).replace(
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value}}",
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value,{CAPTURED_COMMAND_SPAN}:Value}}",
        )
        owners = {**SOURCE_OWNERS, CAPTURED_COMMAND_SPAN: {"stmt:Command"}}

        with self.assertRaisesRegex(AdapterError, "no matching AST owner"):
            project_publication("current", current, {0: 40000}, owners)

    def test_cli_plan_map_order_does_not_depend_on_schema_hash_order(self):
        legacy_data = (
            '{(ExprId(2),false):Ok(Plan("first")),'
            '(ExprId(3),true):Err(PlanError("second"))}'
        )
        current_data = (
            '{((ExprId(3),None),true):Err(PlanError("second")),'
            '((ExprId(2),None),false):Ok(Plan("first"))}'
        )
        legacy = LEGACY_PUBLICATION.replace(
            '{(ExprId(2),false):Ok(Plan("payload"))}', legacy_data
        )
        current = CURRENT_PUBLICATION.replace(
            '{((ExprId(2),None),false):Ok(Plan("payload"))}', current_data
        )

        legacy_projection = project_legacy(legacy)
        current_projection = project_publication(
            "current", current, SOURCE_SIZES, SOURCE_OWNERS
        )

        self.assertEqual(legacy_projection, current_projection)
        self.assertIn('Ok(Plan("first"))', current_projection)
        self.assertIn('Err(PlanError("second"))', current_projection)

    def test_current_cli_plan_debug_keys_and_mutex_fields_are_strict(self):
        wrong_key = CURRENT_PUBLICATION.replace(
            "((ExprId(2),None),false)", "(ExprId(2),false)"
        )
        with self.assertRaisesRegex(AdapterError, "malformed current CLI plan key"):
            project_publication("current", wrong_key, SOURCE_SIZES, SOURCE_OWNERS)

        current_key = "((ExprId(2),None),false)"
        duplicate = CURRENT_PUBLICATION.replace(
            f"{current_key}:Ok(Plan(\"payload\"))",
            f"{current_key}:Ok(Plan(\"payload\")),"
            f"{current_key}:Err(error)",
        )
        with self.assertRaisesRegex(AdapterError, "duplicate normalized CLI plan key"):
            project_publication("current", duplicate, SOURCE_SIZES, SOURCE_OWNERS)

        unknown_field = CURRENT_PUBLICATION.replace(
            "poisoned:false", "poisoned:false,unexpected:false"
        )
        with self.assertRaisesRegex(AdapterError, "unexpected Mutex Debug fields"):
            project_publication("current", unknown_field, SOURCE_SIZES, SOURCE_OWNERS)

    def test_legacy_tail_fields_require_exact_statement_identity_and_name(self):
        missing_owner = {
            "StmtId(4)": (TAIL_SPAN, "answer", "stmt:TailBareIdent")
        }
        with self.assertRaisesRegex(AdapterError, "no statement owner or declaration"):
            project_legacy(LEGACY_PUBLICATION, missing_owner)

        wrong_name = {
            "StmtId(3)": (TAIL_SPAN, "other", "stmt:TailBareIdent")
        }
        with self.assertRaisesRegex(AdapterError, "disagrees with its binding declaration"):
            project_legacy(LEGACY_PUBLICATION, wrong_name)

        wrong_kind = {
            "StmtId(3)": (TAIL_SPAN, "answer", "stmt:Expr")
        }
        with self.assertRaisesRegex(AdapterError, "malformed legacy tail owner"):
            project_legacy(LEGACY_PUBLICATION, wrong_kind)

        removed_tail_fact = LEGACY_PUBLICATION.replace(
            "tails:{StmtId(3):" + TAIL_BINDING + ",StmtId(5):" + RUNTIME_BINDING
            + ",StmtId(13):" + UNPREPARED_TAIL_BINDING
            + ",StmtId(15):" + MISSING_VALUE_TAIL_BINDING + "}",
            "tails:{StmtId(5):" + RUNTIME_BINDING
            + ",StmtId(13):" + UNPREPARED_TAIL_BINDING
            + ",StmtId(15):" + MISSING_VALUE_TAIL_BINDING + "}",
        )
        with self.assertRaisesRegex(AdapterError, "tail bindings differ"):
            project_legacy(removed_tail_fact)

        unexpected_nonconstant_tail = LEGACY_PUBLICATION.replace(
            f"tail_bindings:{{{TAIL_SPAN}:{TAIL_EXPR}}}",
            f"tail_bindings:{{{TAIL_SPAN}:{TAIL_EXPR},{RUNTIME_TAIL_SPAN}:{EXPR}}}",
        )
        with self.assertRaisesRegex(AdapterError, "tail bindings differ"):
            project_legacy(unexpected_nonconstant_tail)

    def test_comparison_keeps_diagnostics_and_common_facts_exact(self):
        compare_publications(
            "diagnostic",
            LEGACY_PUBLICATION,
            SOURCE_SIZES,
            SOURCE_OWNERS,
            TAIL_STATEMENT_OWNERS,
            STATEMENT_OWNERS,
            "diagnostic",
            CURRENT_PUBLICATION,
            SOURCE_SIZES,
            SOURCE_OWNERS,
        )
        with self.assertRaisesRegex(AdapterError, "diagnostic mismatch"):
            compare_publications(
                "old", LEGACY_PUBLICATION, SOURCE_SIZES, SOURCE_OWNERS,
                TAIL_STATEMENT_OWNERS, STATEMENT_OWNERS,
                "new", CURRENT_PUBLICATION,
                SOURCE_SIZES, SOURCE_OWNERS
            )
        changed_common = CURRENT_PUBLICATION.replace('Name("run")', 'Name("changed")')
        with self.assertRaisesRegex(AdapterError, "common checked facts mismatch"):
            compare_publications(
                "diagnostic", LEGACY_PUBLICATION, SOURCE_SIZES, SOURCE_OWNERS,
                TAIL_STATEMENT_OWNERS, STATEMENT_OWNERS,
                "diagnostic", changed_common,
                SOURCE_SIZES, SOURCE_OWNERS
            )

    def test_comparison_passes_candidate_statement_owners(self):
        legacy = LEGACY_PUBLICATION.replace(
            f"propagating_statements:{{{SPAN}}}",
            f"propagating_statements:{{{SPAN},{PROPAGATING_STATEMENT_SPAN}}}",
        ).replace(
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value}}",
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value,"
            f"{PROPAGATING_STATEMENT_SPAN}:Value}}",
        )
        current = CURRENT_PUBLICATION.replace(
            f"propagating_statements:{{{SPAN},{COMMAND_SPAN}}}",
            f"propagating_statements:{{{SPAN},{PROPAGATING_STATEMENT_SPAN}}}",
        ).replace(
            f"propagating_error_spans:{{{COMMAND_SPAN}}}",
            f"propagating_error_spans:{{{PROPAGATING_STATEMENT_SPAN}}}",
        ).replace(
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value}}",
            f"statement_positions:{{{SPAN}:Value,{COMMAND_SPAN}:Value,"
            f"{PROPAGATING_STATEMENT_SPAN}:Value}}",
        )
        owners = {**SOURCE_OWNERS, PROPAGATING_STATEMENT_SPAN: {"stmt:Assert"}}
        candidate_statement_owners = {
            "StmtId(20)": (PROPAGATING_STATEMENT_SPAN, "stmt:Assert")
        }

        compare_publications(
            "diagnostic", legacy, SOURCE_SIZES, owners,
            TAIL_STATEMENT_OWNERS, STATEMENT_OWNERS,
            "diagnostic", current, SOURCE_SIZES, owners,
            candidate_statement_owners=candidate_statement_owners,
        )

    def test_projection_rejects_schema_shape_mismatch(self):
        with self.assertRaisesRegex(AdapterError, "matched 0 times"):
            project_publication("legacy", CURRENT_PUBLICATION)

    def test_propagating_statement_set_rejects_duplicate_spans(self):
        duplicate = CURRENT_PUBLICATION.replace(
            f"propagating_statements:{{{SPAN},{COMMAND_SPAN}}}",
            f"propagating_statements:{{{SPAN},{SPAN},{COMMAND_SPAN}}}",
        )
        with self.assertRaisesRegex(AdapterError, "duplicate Rust Debug set entry"):
            project_publication("current", duplicate, SOURCE_SIZES, SOURCE_OWNERS)


if __name__ == "__main__":
    unittest.main()
