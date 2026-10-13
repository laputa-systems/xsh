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
        self.assertEqual(77, len(result["owners"]))
        self.assertEqual(72, len(result["obligations"]))
        self.assertEqual([], coverage["finite_conformance"]["cases"])

    def test_constructor_add_remove_and_rename_drift(self) -> None:
        fixture = (ROOT / "formal/tests/inventory/fixtures/arena.rs").read_text()
        inventoried = CHECKER.extract_enum(fixture, "ArenaExprTag")
        self.assertEqual(["Null", "BoolFalse", "Call"], inventoried)
        self.assertEqual(
            ["Any", "List(Box<Type>)", "Result{ok:Box<Type>,error:Box<Type>}"],
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
        self.assertIn("Set=19|custom:Arc<Vec<MapKey>>", changed_schema)
        with self.assertRaisesRegex(CHECKER.InventoryError, "ir.lowered_value"):
            CHECKER.compare_owner_items("ir.lowered_value", inventoried_enum, changed_enum)
        with self.assertRaisesRegex(CHECKER.InventoryError, "ir.values"):
            CHECKER.compare_owner_items("ir.values", inventoried_schema, changed_schema)

    def test_named_raw_instruction_schema_rows_parse(self) -> None:
        fixture = (ROOT / "formal/tests/inventory/fixtures/instruction_schema.rs").read_text()
        rows = CHECKER.parse_instruction_schema(fixture)
        self.assertEqual(
            [
                "ValueEmpty=0|BuildValueRow:ValueView:Value|Empty()|0|Operands|FrameApply",
                "ValuePair=1|BuildValueRow:ValueView:Value|Pair(left:BuildExprId,right:Option<BuildExprId>)|EFFECT_HOST|EFFECT_TRACE|Scheduled|FrameControl:EagerAtom",
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
