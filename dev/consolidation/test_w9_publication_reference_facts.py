import unittest
from pathlib import Path

from w9_publication_adapter import AdapterError
from w9_publication_reference_facts import validate_function_reference_targets


IDENT_SPAN = "Span{source_id:SourceId(0),start:10,len:6}"
PARENT_SPAN = "Span{source_id:SourceId(0),start:8,len:12}"
TAIL_SPAN = "Span{source_id:SourceId(0),start:30,len:6}"
RETAINED_DECLARATION_BODY = "Span{len:290,source_id:SourceId(2),start:6423}"
REORDERED_IDENT_SPAN = "Span{len:6,source_id:SourceId(0),start:10}"
REORDERED_CALL_SPAN = "Span{len:8,source_id:SourceId(1),start:50}"
TOUR_USAGE_SPAN = "Span{source_id:SourceId(0),start:374,len:5}"

PURE_SIGNATURE = (
    "Some(CompactFunctionSig { params: [], parameter_schemas: [], return_ty: Int, "
    "return_type_expr: TypeExprId(1), inferred_effects: true, effects: None })"
)
PURE_DECLARATION = (
    "CheckedFunctionDeclaration { namespace: None, name: Name(\"render\"), internal: false, "
    f"kind: Pure, signature: {PURE_SIGNATURE}, "
    f"body: {RETAINED_DECLARATION_BODY}, parameter_spans: [] }}"
)
TARGET = (
    "CheckedFunctionTarget { definition: FunctionDefId(1), namespace: None, "
    "name: Name(\"render\") }"
)
TOUR_USAGE_TARGET = (
    "CheckedFunctionTarget { definition: FunctionDefId(1), namespace: None, "
    "name: Name(\"usage\") }"
)
TOUR_USAGE_DECLARATION = (
    'CheckedFunctionDeclaration{body:Span{len:27,source_id:SourceId(0),start:220},'
    'internal:false,kind:Pure,name:Name("usage"),namespace:None,'
    'parameter_spans:[Span{source_id:SourceId(0),start:211,len:7}],'
    'signature:Some(CompactFunctionSig{params:[CallableParamType{name:Name("d"),'
    'ty:Record({Name("mount"):Str,Name("size"):Int,Name("used"):Int}),'
    'defaulted:false,rest:false}],parameter_schemas:[Some(SchemaExpectation{'
    'instances:[SchemaInstance{definition:TypeDefId(1),arguments:[]}],children:{'
    'Field(Name("mount")):SchemaExpectation{children:{},instances:[]},'
    'Field(Name("size")):SchemaExpectation{children:{},instances:[]},'
    'Field(Name("used")):SchemaExpectation{children:{},instances:[]}}})],'
    'return_ty:Int,return_type_expr:TypeExprId(7),inferred_effects:false,effects:None})}'
)


def _fields(*, selected_span=IDENT_SPAN, selected=TARGET, declaration=PURE_DECLARATION, types=None,
            static_spans="{}"):
    return {
        "selected_function_calls": f"{{{selected_span}: {selected}}}",
        "statically_resolved_call_spans": static_spans,
        "standard_call_spans": "{}",
        "api_calls": "{}",
        "module_commands": "{}",
        "record_constructor_instances": "{}",
        "message_payload_constructors": "{}",
        "error_constructors": "{}",
        "checked_declarations": (
            "CheckedDeclarations { functions: {FunctionDefId(1): "
            + declaration
            + "} }"
        ),
        "expr_types": types if types is not None else f"{{{IDENT_SPAN}: Pure}}",
    }


def _owners(span=IDENT_SPAN, kind="expr:Ident"):
    return {span: {kind}}


def _direct_owners():
    return {
        "exprs": {0: (IDENT_SPAN, "Ident", "render")},
        "records": {},
        "aliases": {},
        "expr_count": 1,
        "record_field_count": 0,
        "tails": {},
    }


def _validate(fields, owners=None, exact_owners=None, sizes=None):
    validate_function_reference_targets(
        fields=fields,
        source_sizes={0: 128, 1: 100, 2: 10000} if sizes is None else sizes,
        source_owners=_owners() if owners is None else owners,
        exact_owners=_direct_owners() if exact_owners is None else exact_owners,
    )


class FunctionReferenceFactTests(unittest.TestCase):
    def test_retained_declaration_accepts_reordered_source_span_fields(self):
        _validate(_fields())

    def test_direct_identifier_uses_exact_name_and_checked_pure_type(self):
        _validate(_fields())

    def test_untyped_direct_identifier_uses_checked_declared_function_kind(self):
        _validate(_fields(types="{}"))

    def test_tour_record_1162_untyped_usage_has_checked_pure_declaration(self):
        source = (
            Path(__file__).resolve().parents[2] / "docs/snippets/tour/18-inference.xsh"
        ).read_bytes()
        self.assertEqual(source[374:379], b"usage")
        self.assertIn(b"sort-by(desc: true, block: usage)", source)
        exact_owners = {
            "exprs": {0: (TOUR_USAGE_SPAN, "Ident", "usage")},
            "records": {},
            "aliases": {},
            "expr_count": 1,
            "record_field_count": 0,
            "tails": {},
        }
        fields = _fields(
            selected_span=TOUR_USAGE_SPAN,
            selected=TOUR_USAGE_TARGET,
            declaration=TOUR_USAGE_DECLARATION,
            types="{}",
        )
        _validate(
            fields,
            owners={TOUR_USAGE_SPAN: {"expr:Ident"}},
            exact_owners=exact_owners,
            sizes={0: 732},
        )

    def test_selected_reference_and_static_call_partition_normalizes_span_keys(self):
        selected = (
            f"{{{REORDERED_IDENT_SPAN}: {TARGET}, "
            f"{REORDERED_CALL_SPAN}: {TARGET}}}"
        )
        fields = _fields()
        fields["selected_function_calls"] = selected
        fields["statically_resolved_call_spans"] = f"{{{REORDERED_CALL_SPAN}}}"
        fields["expr_types"] = (
            f"{{{REORDERED_IDENT_SPAN}: Pure, {REORDERED_CALL_SPAN}: Bool}}"
        )
        _validate(
            fields,
            owners={IDENT_SPAN: {"expr:Ident"}, REORDERED_CALL_SPAN: {"expr:Call"}},
            sizes={0: 128, 1: 100, 2: 10000},
        )

    def test_static_function_call_does_not_require_reference_owner_rows(self):
        _validate(
            _fields(static_spans=f"{{{IDENT_SPAN}}}"),
            owners={},
            exact_owners={},
        )

    def test_record_shorthand_uses_its_exact_parent_field(self):
        exact_owners = {
            "exprs": {
                0: (PARENT_SPAN, "Record", ""),
                1: (IDENT_SPAN, "Ident", "render"),
            },
            "records": {
                0: {
                    "span": PARENT_SPAN,
                    "count": 1,
                    "fields": {0: ("shorthand", "render", IDENT_SPAN, None, None)},
                }
            },
            "aliases": {(0, 0): (1, IDENT_SPAN, "render")},
            "expr_count": 2,
            "record_field_count": 1,
            "tails": {},
        }
        fields = _fields(types=f"{{{PARENT_SPAN}: Record({{Name(\"render\"): Pure}})}}")
        _validate(
            fields,
            owners={PARENT_SPAN: {"expr:Record"}, IDENT_SPAN: {"expr:Ident"}},
            exact_owners=exact_owners,
        )

    def test_tail_reference_uses_its_statement_type(self):
        exact_owners = {
            "exprs": {},
            "records": {},
            "aliases": {},
            "expr_count": 0,
            "record_field_count": 0,
            "tails": {0: (TAIL_SPAN, "render", "stmt:TailBareIdent")},
        }
        fields = _fields(selected_span=TAIL_SPAN, types=f"{{{TAIL_SPAN}: Pure}}")
        _validate(
            fields,
            owners={TAIL_SPAN: {"stmt:TailBareIdent"}},
            exact_owners=exact_owners,
        )

    def test_callable_fit_fact_allows_defaulted_target_parameter_and_narrow_result(self):
        signature = (
            "Some(CompactFunctionSig { params: [CallableParamType { name: Name(\"item\"), "
            "ty: Int, defaulted: true, rest: false }], parameter_schemas: [None], "
            "return_ty: Result(Unit, ErrorVariant { family: Name(\"Fs\"), "
            "variant: Name(\"NotFound\") }), return_type_expr: TypeExprId(1), "
            "inferred_effects: true, effects: None })"
        )
        declaration = (
            "CheckedFunctionDeclaration { namespace: None, name: Name(\"render\"), "
            "internal: false, kind: Pure, signature: "
            + signature
            + ", body: Span{source_id:SourceId(0),start:0,len:50}, parameter_spans: "
            + f"[{IDENT_SPAN}] }}"
        )
        callable_type = (
            "Callable(TypedCallable { pure: true, sig: CallableType { params: "
            "[CallableParamType { name: Name(\"item\"), ty: Int, defaulted: false, "
            "rest: false }], return_ty: Result(Unit, ErrorFamily(Name(\"Fs\"))), effects: None } })"
        )
        fields = _fields(declaration=declaration, types=f"{{{IDENT_SPAN}: {callable_type}}}")
        _validate(fields)

    def test_untyped_reference_still_requires_checked_declaration_signature(self):
        declaration = PURE_DECLARATION.replace(
            f"signature: {PURE_SIGNATURE}", "signature: None"
        )
        with self.assertRaises(AdapterError):
            _validate(_fields(declaration=declaration, types="{}"))

    def test_rejects_target_that_names_a_different_declaration(self):
        target = TARGET.replace('Name("render")', 'Name("other")')
        with self.assertRaises(AdapterError):
            _validate(_fields(selected=target))

    def test_rejects_target_without_a_checked_declaration_signature(self):
        declaration = PURE_DECLARATION.replace(
            f"signature: {PURE_SIGNATURE}", "signature: None"
        )
        with self.assertRaises(AdapterError):
            _validate(_fields(declaration=declaration))

    def test_rejects_target_without_a_checked_declaration(self):
        fields = _fields(types="{}")
        fields["checked_declarations"] = "CheckedDeclarations { functions: {} }"
        with self.assertRaises(AdapterError):
            _validate(fields)

    def test_rejects_present_invalid_checked_type(self):
        with self.assertRaises(AdapterError):
            _validate(_fields(types=f"{{{IDENT_SPAN}: Invalid}}"))

    def test_rejects_present_checked_type_with_wrong_function_kind(self):
        with self.assertRaises(AdapterError):
            _validate(_fields(types=f"{{{IDENT_SPAN}: Proc}}"))

    def test_rejects_target_with_a_different_namespace(self):
        target = TARGET.replace("namespace: None", 'namespace: Some(Name("module"))')
        with self.assertRaises(AdapterError):
            _validate(_fields(selected=target))

    def test_rejects_selected_span_without_exact_ast_identity(self):
        with self.assertRaises(AdapterError):
            _validate(
                _fields(),
                exact_owners={
                    "exprs": {},
                    "records": {},
                    "aliases": {},
                    "expr_count": 0,
                    "record_field_count": 0,
                    "tails": {},
                },
            )

    def test_rejects_wrong_checked_callable_kind(self):
        callable_type = (
            "Callable(TypedCallable { pure: false, sig: CallableType { params: [], "
            "return_ty: Int, effects: None } })"
        )
        with self.assertRaises(AdapterError):
            _validate(_fields(types=f"{{{IDENT_SPAN}: {callable_type}}}"))

    def test_rejects_missing_record_shorthand_alias(self):
        exact_owners = {
            "exprs": {0: (PARENT_SPAN, "Record", ""), 1: (IDENT_SPAN, "Ident", "render")},
            "records": {
                0: {
                    "span": PARENT_SPAN,
                    "count": 1,
                    "fields": {0: ("shorthand", "render", IDENT_SPAN, None, None)},
                }
            },
            "aliases": {},
            "expr_count": 2,
            "record_field_count": 1,
            "tails": {},
        }
        fields = _fields(types=f"{{{PARENT_SPAN}: Record({{Name(\"render\"): Pure}})}}")
        with self.assertRaises(AdapterError):
            _validate(
                fields,
                owners={PARENT_SPAN: {"expr:Record"}, IDENT_SPAN: {"expr:Ident"}},
                exact_owners=exact_owners,
            )


if __name__ == "__main__":
    unittest.main()
