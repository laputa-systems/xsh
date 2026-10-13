from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
CHECKER_PATH = ROOT / "formal/scripts/check-inventory.py"
SPEC = importlib.util.spec_from_file_location("check_inventory", CHECKER_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load the inventory checker")
CHECKER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CHECKER
SPEC.loader.exec_module(CHECKER)


class InventoryCheckerTests(unittest.TestCase):
    def test_current_owner_sets_match_provisional_inventory(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        result = CHECKER.validate_coverage(ROOT, coverage)
        self.assertEqual(set(CHECKER.OWNER_SPECS), set(result["owners"]))
        self.assertEqual(89, len(result["owners"]))
        self.assertEqual(72, len(result["obligations"]))
        self.assertEqual([], coverage["finite_conformance"]["cases"])

    def test_constructor_add_remove_and_rename_drift(self) -> None:
        fixture = (ROOT / "formal/tests/inventory/fixtures/arena.rs").read_text()
        inventoried = CHECKER.extract_enum(fixture, "ArenaExprTag")
        self.assertEqual(["Null", "BoolFalse", "Call"], inventoried)
        self.assertEqual(
            [
                'Any',
                'List(tokens:["Box","<","Type",">"])',
                'Result{ok:tokens:["Box","<","Type",">"],error:tokens:["Box","<","Type",">"]}',
            ],
            CHECKER.extract_enum(fixture, "Type"),
        )
        changed_sources = (
            fixture.replace("    Call,\n", "    Call,\n    Retry,\n"),
            fixture.replace("    BoolFalse,\n", ""),
            fixture.replace("    Call,\n", "    Invoke,\n"),
        )
        for changed in changed_sources:
            with self.subTest(changed=changed):
                live = CHECKER.extract_enum(changed, "ArenaExprTag")
                with self.assertRaises(CHECKER.InventoryError):
                    CHECKER.compare_owner_items("syntax.arena.expr", inventoried, live)

    def test_enum_payload_schema_drift(self) -> None:
        fixture = (ROOT / "formal/tests/inventory/fixtures/arena.rs").read_text()
        inventoried = CHECKER.extract_enum(fixture, "Type")
        changed = fixture.replace("error: Box<Type>", "error: Vec<Type>")
        live = CHECKER.extract_enum(changed, "Type")
        with self.assertRaises(CHECKER.InventoryError):
            CHECKER.compare_owner_items("types.fixture", inventoried, live)

    def test_token_fingerprint_preserves_boundaries_in_payloads_and_bodies(self) -> None:
        left = ["ab", "c"]
        right = ["a", "bc"]
        self.assertEqual("".join(left), "".join(right))
        self.assertNotEqual(CHECKER.token_fingerprint(left), CHECKER.token_fingerprint(right))

        left_schema = CHECKER.extract_enum(
            "enum Payload { Item(payload!(ab c)) }", "Payload"
        )
        right_schema = CHECKER.extract_enum(
            "enum Payload { Item(payload!(a bc)) }", "Payload"
        )
        self.assertNotEqual(left_schema, right_schema)
        with self.assertRaisesRegex(CHECKER.InventoryError, "owner drift in types.fixture"):
            CHECKER.compare_owner_items("types.fixture", left_schema, right_schema)

        left_body = CHECKER.function_body(
            "fn body() { macro_rules! m { (ab c) => {}; } }", "body"
        )
        right_body = CHECKER.function_body(
            "fn body() { macro_rules! m { (a bc) => {}; } }", "body"
        )
        self.assertNotEqual(left_body, right_body)
        with self.assertRaisesRegex(CHECKER.InventoryError, "owner drift in body.fixture"):
            CHECKER.compare_owner_items("body.fixture", [left_body], [right_body])

    def test_lowered_value_variant_requires_exactly_one_codec_route(self) -> None:
        source = (ROOT / "src/runtime/eval.rs").read_text()
        changed = source.replace(
            "    ResultErr(Box<Value>),",
            "    ResultErr(Box<Value>),\n    Unrouted,",
        )
        self.assertNotEqual(source, changed)
        enum_items = CHECKER.extract_enum(changed, "LoweredValue")
        schema_source = (ROOT / "src/runtime/eval/indexed/full.rs").read_text()
        schema_items = CHECKER.parse_lowered_value_schema(schema_source)
        with self.assertRaisesRegex(CHECKER.InventoryError, "codec routes differ"):
            CHECKER.reconcile_lowered_value_routes(enum_items, schema_items)

    def test_lowered_value_codec_rejects_duplicate_routes(self) -> None:
        source = (ROOT / "src/runtime/eval.rs").read_text()
        variants = CHECKER.extract_enum(source, "LoweredValue")
        schema_source = (ROOT / "src/runtime/eval/indexed/full.rs").read_text()
        schema_items = CHECKER.parse_lowered_value_schema(schema_source)
        with self.assertRaisesRegex(CHECKER.InventoryError, "duplicate LoweredValue codec routes"):
            CHECKER.reconcile_lowered_value_routes(variants, schema_items + [schema_items[0]])

    def test_set_custom_codec_requires_one_encode_decode_and_verify_route(self) -> None:
        source = (ROOT / "src/runtime/eval/indexed/full.rs").read_text()
        missing_verify = source.replace(
            "            verify { decode_set_payload(decoder, &mut payload)?; }\n",
            "",
        )
        self.assertNotEqual(source, missing_verify)
        with self.assertRaisesRegex(CHECKER.InventoryError, "requires exactly one encode, decode, and verify"):
            CHECKER.parse_lowered_value_schema(missing_verify)
        duplicate_verify = source.replace(
            "            verify { decode_set_payload(decoder, &mut payload)?; }",
            "            verify { decode_set_payload(decoder, &mut payload)?; }\n"
            "            verify { decode_set_payload(decoder, &mut payload)?; }",
        )
        self.assertNotEqual(source, duplicate_verify)
        with self.assertRaisesRegex(CHECKER.InventoryError, "duplicate custom lowered value procedure verify"):
            CHECKER.parse_lowered_value_schema(duplicate_verify)

    def test_type_tag_payload_buckets_reject_omitted_and_duplicate_routes(self) -> None:
        source = (ROOT / "src/runtime/eval/indexed/semantic.rs").read_text()
        original = CHECKER.parse_indexed_type_pool_schema(source)
        old = "Self::List | Self::Stream | Self::Optional | Self::Set"
        omitted = source.replace(old, "Self::List | Self::Stream | Self::Optional")
        self.assertNotEqual(source, omitted)
        with self.assertRaisesRegex(CHECKER.InventoryError, "owner drift in types.indexed_pool_schema"):
            CHECKER.compare_owner_items(
                "types.indexed_pool_schema",
                original,
                CHECKER.parse_indexed_type_pool_schema(omitted),
            )
        duplicated = source.replace(old, old + " | Self::Set")
        self.assertNotEqual(source, duplicated)
        with self.assertRaisesRegex(CHECKER.InventoryError, "duplicate TypeTag::has_one_type"):
            CHECKER.parse_indexed_type_pool_schema(duplicated)

    def test_runtime_value_literal_validation_and_operation_owners_are_live(self) -> None:
        cases = (
            ("src/runtime/value.rs", "Value", "RunError(Box<RunError>)", "RunError(Arc<RunError>)", "runtime.value"),
            ("src/runtime/value.rs", "ResultValue", "Err(Box<Value>)", "Err(Box<RuntimeError>)", "runtime.result_value"),
            ("src/sema/constants.rs", "LiteralConstant", "Float(u64)", "Float(f64)", "constants.literal"),
            ("src/sema/validated.rs", "Validation", "Range(IntRange)", "Range(i64)", "types.validation"),
            ("crates/xsh-registry/src/runtime_op.rs", "RuntimeOp", "FsTempDirIn", "FsTempDirOut", "types.runtime_op"),
        )
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        for relative, declaration, before, after, owner in cases:
            with self.subTest(owner=owner):
                source = (ROOT / relative).read_text()
                changed = source.replace(before, after)
                self.assertNotEqual(source, changed)
                inventoried = coverage["core_inventory"][owner]["items"]
                live = CHECKER.extract_enum(changed, declaration)
                with self.assertRaisesRegex(CHECKER.InventoryError, f"owner drift in {owner}"):
                    CHECKER.compare_owner_items(owner, inventoried, live)

    def test_callable_and_stream_rules_detect_contract_drift(self) -> None:
        callable_source = (ROOT / "src/sema/types.rs").read_text()
        callable_items = CHECKER.parse_callable_contract(callable_source)
        changed_callable = callable_source.replace("defaulted: false", "defaulted: true", 1)
        self.assertNotEqual(callable_source, changed_callable)
        with self.assertRaisesRegex(CHECKER.InventoryError, "owner drift in types.callable_contract"):
            CHECKER.compare_owner_items(
                "types.callable_contract",
                callable_items,
                CHECKER.parse_callable_contract(changed_callable),
            )

        stream_source = (ROOT / "crates/xsh-registry/src/stream_parameters.rs").read_text()
        stream_items = CHECKER.parse_stream_parameters(stream_source)
        changed_stream = stream_source.replace('"map"\n            | "where"', '"where"', 1)
        self.assertNotEqual(stream_source, changed_stream)
        with self.assertRaisesRegex(CHECKER.InventoryError, "owner drift in stream.parameters"):
            CHECKER.compare_owner_items(
                "stream.parameters",
                stream_items,
                CHECKER.parse_stream_parameters(changed_stream),
            )

        encoding_source = (ROOT / "src/runtime/eval/indexed/semantic.rs").read_text()
        encoding_items = CHECKER.parse_callable_encoding(encoding_source)
        changed_encoding = encoding_source.replace(
            "const PARAM_REST: u32 = 1 << 1;",
            "const PARAM_REST: u32 = 1 << 2;",
        )
        self.assertNotEqual(encoding_source, changed_encoding)
        with self.assertRaisesRegex(CHECKER.InventoryError, "owner drift in types.callable_encoding"):
            CHECKER.compare_owner_items(
                "types.callable_encoding",
                encoding_items,
                CHECKER.parse_callable_encoding(changed_encoding),
            )

    def test_indexed_ids_and_locations_detect_representation_drift(self) -> None:
        id_source = (ROOT / "src/runtime/eval/indexed.rs").read_text()
        id_items = CHECKER.parse_indexed_id_layout(id_source)
        changed_ids = id_source.replace(
            "const IR_NONE: u32 = u32::MAX;",
            "const IR_NONE: u32 = 0;",
        )
        self.assertNotEqual(id_source, changed_ids)
        with self.assertRaisesRegex(CHECKER.InventoryError, "owner drift in ir.indexed_ids"):
            CHECKER.compare_owner_items(
                "ir.indexed_ids", id_items, CHECKER.parse_indexed_id_layout(changed_ids)
            )

        source_map = (ROOT / "src/source.rs").read_text()
        location_items = CHECKER.parse_source_location_layout(source_map)
        changed_span = source_map.replace(
            "pub const fn end(self) -> usize {\n        self.start as usize + self.len as usize",
            "pub const fn end(self) -> usize {\n        self.start as usize + self.len as usize + 1",
        )
        self.assertNotEqual(source_map, changed_span)
        with self.assertRaisesRegex(CHECKER.InventoryError, "owner drift in source.locations"):
            CHECKER.compare_owner_items(
                "source.locations",
                location_items,
                CHECKER.parse_source_location_layout(changed_span),
            )

    def test_inventory_derivation_and_boundary_are_pinned(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["core_inventory"]["runtime.value"]["boundary"] = "everything"
        with self.assertRaisesRegex(CHECKER.InventoryError, "stale boundary metadata"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_lowered_set_payload_is_derived_from_live_enum(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        inventoried_enum = coverage["core_inventory"]["ir.lowered_value"]["items"]
        inventoried_schema = coverage["core_inventory"]["ir.values"]["items"]
        source = (ROOT / "src/runtime/eval.rs").read_text()
        changed = source.replace(
            "Set(Arc<BTreeSet<MapKey>>)", "Set(Arc<Vec<MapKey>>)"
        )
        self.assertNotEqual(source, changed)
        changed_enum = CHECKER.extract_enum(changed, "LoweredValue")
        schema_source = (ROOT / "src/runtime/eval/indexed/full.rs").read_text()
        changed_schema = CHECKER.reconcile_lowered_value_routes(
            changed_enum, CHECKER.parse_lowered_value_schema(schema_source)
        )
        self.assertTrue(
            any(
                item.startswith('Set=19|custom:tokens:["Arc","<","Vec"')
                for item in changed_schema
            )
        )
        with self.assertRaisesRegex(CHECKER.InventoryError, "ir.lowered_value"):
            CHECKER.compare_owner_items("ir.lowered_value", inventoried_enum, changed_enum)
        with self.assertRaisesRegex(CHECKER.InventoryError, "ir.values"):
            CHECKER.compare_owner_items("ir.values", inventoried_schema, changed_schema)

    def test_named_raw_instruction_schema_rows_parse(self) -> None:
        fixture = (ROOT / "formal/tests/inventory/fixtures/instruction_schema.rs").read_text()
        rows = CHECKER.parse_instruction_schema(fixture)
        self.assertEqual(
            [
                'ValueEmpty=0|BuildValueRow:ValueView:Value|Empty()|tokens:["0"]|Operands|FrameApply',
                'ValuePair=1|BuildValueRow:ValueView:Value|Pair(left:tokens:["BuildExprId"],right:tokens:["Option","<","BuildExprId",">"])'
                '|tokens:["EFFECT_HOST","|","EFFECT_TRACE"]|Scheduled|FrameControl:EagerAtom',
            ],
            rows,
        )

    def test_builtin_type_name_table_is_inventoried(self) -> None:
        fixture = (ROOT / "formal/tests/inventory/fixtures/builtin_type_names.rs").read_text()
        self.assertEqual(
            ['Any:"Any"', 'UInt:"UInt"'],
            CHECKER.parse_builtin_type_names(fixture),
        )

    def test_missing_obligation_entry_fails(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        plan = (ROOT / "xsh-lean.md").read_text()
        with self.assertRaisesRegex(CHECKER.InventoryError, "plan obligation entries differ"):
            CHECKER.validate_obligation_entries(plan, coverage["universal_theorems"][:-1])

    def test_missing_owner_and_duplicate_items_fail(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        del coverage["core_inventory"]["types.semantic"]
        with self.assertRaisesRegex(CHECKER.InventoryError, "owner entries differ"):
            CHECKER.validate_coverage(ROOT, coverage)
        with self.assertRaisesRegex(CHECKER.InventoryError, "duplicate fixture entries"):
            CHECKER.compare_owner_items("fixture", ["Any", "Any"], ["Any"])

    def test_provisional_source_cannot_be_marked_frozen_here(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["source_state"] = "frozen-post-consolidation"
        with self.assertRaisesRegex(CHECKER.InventoryError, "requires reconciliation"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_duplicate_json_keys_fail(self) -> None:
        fixture = ROOT / "formal/tests/inventory/fixtures/duplicate-key.json"
        with self.assertRaisesRegex(CHECKER.InventoryError, "duplicate JSON object key"):
            CHECKER.load_json(fixture)

    def test_malformed_schema_type_fails(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["schema"] = True
        with self.assertRaisesRegex(CHECKER.InventoryError, "unsupported coverage schema"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_duplicate_obligation_rows_fail(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        obligations = coverage["universal_theorems"]
        with self.assertRaisesRegex(CHECKER.InventoryError, "duplicate obligation id"):
            CHECKER.validate_obligation_entries(
                (ROOT / "xsh-lean.md").read_text(), obligations + [obligations[0]]
            )


if __name__ == "__main__":
    unittest.main()
