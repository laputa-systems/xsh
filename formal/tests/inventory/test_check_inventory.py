from __future__ import annotations

import importlib.util
import subprocess
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
    def git_object(self, revision: str) -> str:
        return subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "--verify", "--end-of-options", revision],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def git_source_identity(self, revision: str = "HEAD") -> dict[str, str]:
        commit = self.git_object(f"{revision}^{{commit}}")
        tree = self.git_object(f"{commit}^{{tree}}")
        return {"commit": commit, "tree": tree}

    def cargo_coverage_with_case(self) -> dict[str, object]:
        coverage = self.frozen_coverage_with_case()
        case = coverage["finite_conformance"]["cases"][0]
        test_id = "runtime::eval::indexed::full::tests::rejects_bad_tag"
        case["test_id"] = test_id
        case["command"] = [
            "cargo",
            "test",
            "--profile",
            "verification",
            "--target",
            "x86_64-unknown-linux-musl",
            "-p",
            "xsh",
            "--lib",
            test_id,
            "--",
            "--exact",
        ]
        case["binary_sha256s"] = {"cargo": "e" * 64, "cargo-test:xsh:lib": "f" * 64}
        case["profile"] = "verification"
        stdout = (
            "running 1 test\n"
            f"test {test_id} ... ok\n"
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "
            "12 filtered out; finished in 0.01s\n"
        )
        case["expected_result"] = {
            "test_id": test_id,
            "exit_code": 0,
            "passed": 1,
            "failed": 0,
            "skipped": 0,
            "filtered": 12,
        }
        case["actual_result"] = {"exit_code": 0, "stdout": stdout, "stderr": ""}
        return coverage

    def frozen_coverage_with_case(self) -> dict[str, object]:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["source_state"] = "frozen-post-consolidation"
        coverage["source_identity"] = self.git_source_identity()
        coverage["finite_conformance"]["status"] = "passed"
        coverage["finite_conformance"]["cases"] = [
            {
                "name": "float-equality-source-witnesses",
                "test_id": "tests/xsh/float-equality.xsh::test_float_runtime_equality_uses_ieee_values",
                "evidence_class": "RUST_CONFORMANCE",
                "binary_sha256s": {"xsht": "c" * 64, "xsh": "d" * 64},
                "profile": "release",
                "target": "x86_64-unknown-linux-musl",
                "command": [
                    "target/x86_64-unknown-linux-musl/release/xsht",
                    "test",
                    "--exact",
                    "tests/xsh/float-equality.xsh::test_float_runtime_equality_uses_ieee_values",
                ],
                "expected_result": {
                    "test_id": "tests/xsh/float-equality.xsh::test_float_runtime_equality_uses_ieee_values",
                    "exit_code": 0,
                    "passed": 1,
                    "failed": 0,
                    "skipped": 0,
                    "filtered": None,
                },
                "actual_result": {
                    "exit_code": 0,
                    "stdout": (
                        "running 1 tests\n"
                        "tests/xsh/float-equality.xsh::test_float_runtime_equality_uses_ieee_values "
                        "... ok 0ms\n"
                        "test result: ok. 1 passed; 0 failed; 0 skipped\n"
                    ),
                    "stderr": "",
                },
                "status": "passed",
            }
        ]
        return coverage

    def test_current_owner_sets_match_provisional_inventory(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        result = CHECKER.validate_coverage(ROOT, coverage)
        self.assertEqual(set(CHECKER.OWNER_SPECS), set(result["owners"]))
        self.assertEqual(89, len(result["owners"]))
        self.assertEqual(
            {root["id"] for root in coverage["fragment_roots"]},
            result["fragment_roots"],
        )
        self.assertEqual(82, len(result["fragment_roots"]))
        self.assertEqual(72, len(result["obligations"]))
        self.assertEqual("provisional-during-consolidation", coverage["source_state"])
        self.assertIsNone(coverage["source_identity"])
        self.assertEqual([], coverage["finite_conformance"]["cases"])

    def test_root_import_reaches_every_project_module(self) -> None:
        formal_root = ROOT / "formal"
        module_sources = CHECKER.lean_module_sources(formal_root)
        reachable = CHECKER.reachable_lean_modules(module_sources)
        self.assertEqual(set(module_sources), reachable)

    def test_fragment_roots_cover_every_theorem_module(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        roots = coverage["fragment_roots"]
        module_sources = CHECKER.lean_module_sources(ROOT / "formal")
        reachable = CHECKER.reachable_lean_modules(module_sources)
        root_ids = CHECKER.validate_fragment_roots(roots, module_sources, reachable)
        self.assertEqual(len(roots), len(root_ids))
        self.assertTrue(
            any(
                "pending_integer_frames_reject_noninteger_mutations_fragment"
                in root["declaration"]
                for root in roots
            )
        )
        self.assertTrue(
            any(
                "float_sign_bit_mutation_changes_machine_payload_fragment"
                in root["declaration"]
                for root in roots
            )
        )
        base_machine_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Proofs.BaseMachine"
        }
        self.assertTrue(
            {
                "Xsh.Proofs.BaseMachine.resumeHostRequest_preserved_fragment",
                "Xsh.Proofs.BaseMachine.time_now_is_a_typed_host_request_fragment",
                "Xsh.Proofs.BaseMachine.time_now_accepts_only_an_integer_host_response_fragment",
                "Xsh.Proofs.BaseMachine.boolean_response_type_mutation_is_rejected_fragment",
                "Xsh.Proofs.BaseMachine.host_requests_are_observed_in_machine_order_fragment",
            }.issubset(base_machine_roots)
        )
        host_trace_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Semantics.BaseHostTrace"
        }
        self.assertEqual(
            {
                "Xsh.Semantics.BaseHostTrace.host_response_transition_preserves_wellFormed",
                "Xsh.Semantics.BaseHostTrace.scripted_two_request_trace_is_valid",
                "Xsh.Semantics.BaseHostTrace.mismatched_response_is_labeled_and_rejected",
            },
            host_trace_roots,
        )
        host_trace_proof_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Proofs.BaseHostTrace"
        }
        self.assertEqual(
            {
                "Xsh.Proofs.BaseHostTrace.runBaseHostFuel_trace",
                "Xsh.Proofs.BaseHostTrace.runBaseHostFuel_preserves_wellFormed",
                "Xsh.Proofs.BaseHostTrace.omitted_response_event_is_not_a_trace",
            },
            host_trace_proof_roots,
        )
        runner_law_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Proofs.BaseHostRunnerLaws"
        }
        self.assertEqual(
            {
                "Xsh.Proofs.BaseHostRunnerLaws.runBaseHostFuel_terminal_extension",
                "Xsh.Proofs.BaseHostRunnerLaws.runBaseHostFuel_waiting_without_response",
                "Xsh.Proofs.BaseHostRunnerLaws.response_suffix_changes_waiting_result",
                "Xsh.Proofs.BaseHostRunnerLaws.response_order_changes_waiting_result",
            },
            runner_law_roots,
        )
        raw_roots = {
            root["id"] for root in roots
            if root["module"].startswith("Xsh.IR.Raw.")
        }
        self.assertEqual(
            {
                "fragment.Xsh.IR.Raw.decoded_row_preserves_raw_fields",
                "fragment.Xsh.IR.Raw.RawRange.resolve_sound",
                "fragment.Xsh.IR.Raw.tag_data_mismatch_row_read_fails",
                "fragment.Xsh.IR.Raw.PoolId.decode_encode",
                "fragment.Xsh.IR.Raw.readWords_success_length",
                "fragment.Xsh.IR.Raw.readPayload_success_length_and_bounds",
                "fragment.Xsh.IR.Raw.out_of_bounds_payload_read_is_rejected",
            },
            raw_roots,
        )
        effect_roots = {
            root["declaration"] for root in roots
            if root["module"] in {
                "Xsh.Effects.Lattice",
                "Xsh.Effects.Solver",
                "Xsh.Effects.Lexical",
                "Xsh.Effects.ExpressionBridge",
            }
        }
        self.assertEqual(
            {
                "Xsh.Semantics.Effects.closure_is_least_closed",
                "Xsh.Semantics.Effects.Solver.solve_least_closed",
                "Xsh.Semantics.Effects.nested_without_only_restricts",
                "Xsh.Semantics.Effects.pull_sequence_preserves_creation_and_body_order",
                "Xsh.Semantics.Effects.pull_sequence_charge_keeps_origin_and_body_site",
                "Xsh.Semantics.Effects.pull_sequence_charges_are_permitted",
                "Xsh.Semantics.Effects.pull_observation_pairs_preserve_emission_order",
                "Xsh.Semantics.Effects.pull_observation_pair_keeps_origin_body_site_and_effect",
                "Xsh.Semantics.Effects.pull_observation_pair_is_permitted_when_aligned",
                "Xsh.Semantics.Effects.pull_observation_charges_match_pull_sequence",
                "Xsh.Semantics.Effects.checked_consumer_obligation_is_permitted_at_its_site",
                "Xsh.Semantics.Effects.permitsCreationBool_correct",
                "Xsh.Semantics.Effects.checkProducerCall_success_implies_creation_permission",
                "Xsh.Semantics.Effects.checkProducerCall_success_records_its_lexical_site",
                "Xsh.Semantics.Effects.checked_producer_body_obligation_is_permitted_at_creation",
                "Xsh.Semantics.Effects.outside_creation_accepts_the_classified_producer_fragment",
                "Xsh.Semantics.Effects.alias_preserves_the_creation_handle_inside_without",
                "Xsh.Semantics.Effects.restricted_consumption_does_not_recharge_a_checked_alias_fragment",
                "Xsh.Semantics.Effects.without_net_rejects_a_network_producer_creation_fragment",
                "Xsh.Semantics.Effects.delegated_child_retains_outer_delegate_child_and_body_sites_fragment",
                "Xsh.Semantics.Effects.delegated_child_is_checked_at_its_own_call_context_fragment",
                "Xsh.Semantics.Effects.conditional_source_retains_both_tagged_producer_alternatives_fragment",
                "Xsh.Semantics.Effects.conditional_blocked_arm_mutation_is_rejected_fragment",
                "Xsh.Semantics.Effects.missing_alias_binding_mutation_is_rejected_fragment",
                "Xsh.Semantics.Effects.unknown_source_fails_closed_fragment",
            },
            effect_roots,
        )

    def test_missing_fragment_module_root_fails(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["fragment_roots"] = [
            root
            for root in coverage["fragment_roots"]
            if root["module"] != "Xsh.Values.Scalars"
        ]
        with self.assertRaisesRegex(CHECKER.InventoryError, "missing fragment roots"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_duplicate_fragment_root_fails(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["fragment_roots"].append(coverage["fragment_roots"][0].copy())
        with self.assertRaisesRegex(CHECKER.InventoryError, "duplicate fragment root id"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_fragment_root_must_name_a_live_qualified_theorem(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["fragment_roots"][0]["declaration"] = "Xsh.Foundation.missing_root"
        with self.assertRaisesRegex(CHECKER.InventoryError, "not a public theorem"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_unreachable_theorem_root_module_fails(self) -> None:
        module_sources = CHECKER.lean_module_sources(ROOT / "formal")
        reachable = CHECKER.reachable_lean_modules(module_sources)
        reachable.remove("Xsh.Values.Scalars")
        roots = [
            {
                "id": "fragment.scalar.round-trip",
                "module": "Xsh.Values.Scalars",
                "declaration": "Xsh.Values.decode_encode_int",
                "evidence_class": "SPEC_PROVED",
                "status": "fragment",
                "scope": "Scalar representation only.",
                "boundary": "No full-language or Rust conformance claim.",
            }
        ]
        with self.assertRaisesRegex(CHECKER.InventoryError, "unreachable Lean theorem modules"):
            CHECKER.validate_fragment_roots(roots, module_sources, reachable)

    def test_fragment_cannot_close_a_full_obligation(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["universal_theorems"][0]["proof_root_id"] = coverage["fragment_roots"][0]["id"]
        coverage["universal_theorems"][0]["status"] = "proved"
        with self.assertRaisesRegex(CHECKER.InventoryError, "cannot close theorem obligation"):
            CHECKER.validate_coverage(ROOT, coverage)

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

    def test_enum_body_search_stops_at_declaration_end(self) -> None:
        sources = (
            "enum Target; enum Other { A }",
            "mod outer { enum Target } enum Other { A }",
        )
        for source in sources:
            with self.subTest(source=source):
                with self.assertRaisesRegex(
                    CHECKER.InventoryError, "unsupported enum declaration for Target"
                ):
                    CHECKER.extract_enum(source, "Target")

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

    def test_frozen_source_identity_and_named_case_are_accepted(self) -> None:
        coverage = self.frozen_coverage_with_case()
        CHECKER.validate_coverage(ROOT, coverage)

    def test_ancestor_commit_is_a_valid_frozen_source(self) -> None:
        coverage = self.frozen_coverage_with_case()
        coverage["source_identity"] = self.git_source_identity("HEAD^")
        CHECKER.validate_coverage(ROOT, coverage)

    def test_provisional_source_rejects_finite_conformance_cases(self) -> None:
        coverage = self.frozen_coverage_with_case()
        coverage["source_state"] = "provisional-during-consolidation"
        coverage["source_identity"] = None
        with self.assertRaisesRegex(CHECKER.InventoryError, "require a frozen source identity"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_frozen_source_requires_exact_commit_and_tree(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["source_state"] = "frozen-post-consolidation"
        with self.assertRaisesRegex(CHECKER.InventoryError, "requires exact commit and tree"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_unknown_frozen_source_commit_fails(self) -> None:
        coverage = self.frozen_coverage_with_case()
        coverage["source_identity"]["commit"] = "0" * 40
        with self.assertRaisesRegex(CHECKER.InventoryError, "commit does not exist"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_frozen_source_tree_must_match_commit_root_tree(self) -> None:
        coverage = self.frozen_coverage_with_case()
        coverage["source_identity"]["commit"] = self.git_object("HEAD^{commit}")
        parent_commit = self.git_object("HEAD^")
        coverage["source_identity"]["tree"] = self.git_object(f"{parent_commit}^{{tree}}")
        with self.assertRaisesRegex(CHECKER.InventoryError, "does not match the commit root tree"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_frozen_source_commit_and_tree_must_be_full_object_ids(self) -> None:
        for field in ("commit", "tree"):
            with self.subTest(field=field):
                coverage = self.frozen_coverage_with_case()
                coverage["source_identity"][field] = coverage["source_identity"][field][:-1]
                with self.assertRaisesRegex(CHECKER.InventoryError, "full lowercase Git object id"):
                    CHECKER.validate_coverage(ROOT, coverage)

    def test_provisional_source_cannot_carry_frozen_identity(self) -> None:
        coverage = CHECKER.load_json(ROOT / "formal/coverage.json")
        coverage["source_identity"] = {"commit": "a" * 40, "tree": "b" * 40}
        with self.assertRaisesRegex(CHECKER.InventoryError, "cannot carry a frozen source identity"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_duplicate_finite_conformance_case_names_fail(self) -> None:
        coverage = self.frozen_coverage_with_case()
        cases = coverage["finite_conformance"]["cases"]
        cases.append(cases[0].copy())
        with self.assertRaisesRegex(CHECKER.InventoryError, "duplicate finite conformance case name"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_finite_conformance_requires_rust_evidence_class(self) -> None:
        coverage = self.frozen_coverage_with_case()
        coverage["finite_conformance"]["cases"][0]["evidence_class"] = "SPEC_PROVED"
        with self.assertRaisesRegex(CHECKER.InventoryError, "must use RUST_CONFORMANCE"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_finite_conformance_requires_binary_identity(self) -> None:
        coverage = self.frozen_coverage_with_case()
        coverage["finite_conformance"]["cases"][0]["binary_sha256s"]["xsht"] = ""
        with self.assertRaisesRegex(CHECKER.InventoryError, "binary SHA-256 per role"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_xsht_cases_require_both_sibling_binary_roles(self) -> None:
        coverage = self.frozen_coverage_with_case()
        del coverage["finite_conformance"]["cases"][0]["binary_sha256s"]["xsh"]
        with self.assertRaisesRegex(CHECKER.InventoryError, "binary roles must be"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_finite_conformance_requires_binary_profile_and_target(self) -> None:
        for field in ("profile", "target"):
            with self.subTest(field=field):
                coverage = self.frozen_coverage_with_case()
                coverage["finite_conformance"]["cases"][0][field] = " "
                with self.assertRaisesRegex(CHECKER.InventoryError, f"binary {field}"):
                    CHECKER.validate_coverage(ROOT, coverage)

    def test_failed_or_mismatched_finite_conformance_cases_fail(self) -> None:
        coverage = self.frozen_coverage_with_case()
        coverage["finite_conformance"]["cases"][0]["status"] = "failed"
        with self.assertRaisesRegex(CHECKER.InventoryError, "must have passed status"):
            CHECKER.validate_coverage(ROOT, coverage)

        coverage = self.frozen_coverage_with_case()
        coverage["finite_conformance"]["cases"][0]["actual_result"]["exit_code"] = 1
        with self.assertRaisesRegex(CHECKER.InventoryError, "result does not match expectation"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_finite_case_test_id_must_match_supported_argv(self) -> None:
        coverage = self.frozen_coverage_with_case()
        coverage["finite_conformance"]["cases"][0]["command"] = ["true"]
        with self.assertRaisesRegex(CHECKER.InventoryError, "unsupported evidence runner"):
            CHECKER.validate_coverage(ROOT, coverage)

        coverage = self.frozen_coverage_with_case()
        coverage["finite_conformance"]["cases"][0]["test_id"] = "some-other-test"
        with self.assertRaisesRegex(CHECKER.InventoryError, "test identifier does not match argv"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_supported_xsht_command_rejects_unrecognized_arguments(self) -> None:
        coverage = self.frozen_coverage_with_case()
        coverage["finite_conformance"]["cases"][0]["command"].append("--help")
        with self.assertRaisesRegex(CHECKER.InventoryError, "exact xsht test command"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_xsht_case_requires_one_named_passing_test(self) -> None:
        coverage = self.frozen_coverage_with_case()
        case = coverage["finite_conformance"]["cases"][0]
        test_id = case["test_id"]
        outcomes = [
            (
                "running 0 tests\n"
                "test result: ok. 0 passed; 0 failed; 0 skipped\n",
                "no matching tests",
            ),
            (
                "running 2 tests\n"
                f"{test_id} ... ok 0ms\n"
                "tests/xsh/other.xsh::test_other ... ok 0ms\n"
                "test result: ok. 2 passed; 0 failed; 0 skipped\n",
                "multiple tests",
            ),
            (
                "running 1 tests\n"
                "tests/xsh/other.xsh::test_other ... ok 0ms\n"
                "test result: ok. 1 passed; 0 failed; 0 skipped\n",
                "wrong test id",
            ),
            (
                "running 1 tests\n"
                f"{test_id} ... skipped: unavailable 0ms\n"
                "test result: ok. 0 passed; 0 failed; 1 skipped\n",
                "skipped test",
            ),
        ]
        for stdout, label in outcomes:
            with self.subTest(label=label):
                case["actual_result"] = {"exit_code": 0, "stdout": stdout, "stderr": ""}
                with self.assertRaises(CHECKER.InventoryError):
                    CHECKER.validate_coverage(ROOT, coverage)

    def test_cargo_case_requires_one_named_passing_test(self) -> None:
        coverage = self.cargo_coverage_with_case()
        case = coverage["finite_conformance"]["cases"][0]
        test_id = case["test_id"]
        outcomes = [
            (
                "running 0 tests\n"
                "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; "
                "0 filtered out; finished in 0.00s\n",
                "no matching tests",
            ),
            (
                "running 2 tests\n"
                f"test {test_id} ... ok\n"
                "test extra::test ... ok\n"
                "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; "
                "1 filtered out; finished in 0.01s\n",
                "multiple tests",
            ),
            (
                "running 1 test\n"
                f"test {test_id} ... ignored\n"
                "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; "
                "1 filtered out; finished in 0.00s\n",
                "ignored test",
            ),
            (
                "running 1 test\n"
                "test another::test ... ok\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "
                "1 filtered out; finished in 0.01s\n",
                "wrong test id",
            ),
        ]
        for stdout, label in outcomes:
            with self.subTest(label=label):
                case["actual_result"] = {"exit_code": 0, "stdout": stdout, "stderr": ""}
                with self.assertRaises(CHECKER.InventoryError):
                    CHECKER.validate_coverage(ROOT, coverage)

    def test_passed_case_requires_captured_successful_result(self) -> None:
        coverage = self.frozen_coverage_with_case()
        case = coverage["finite_conformance"]["cases"][0]
        case["actual_result"] = None
        with self.assertRaisesRegex(CHECKER.InventoryError, "actual result must record"):
            CHECKER.validate_coverage(ROOT, coverage)

        coverage = self.frozen_coverage_with_case()
        case = coverage["finite_conformance"]["cases"][0]
        case["actual_result"]["exit_code"] = 1
        with self.assertRaisesRegex(CHECKER.InventoryError, "result does not match expectation"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_expected_result_must_be_normalized_oracle(self) -> None:
        coverage = self.frozen_coverage_with_case()
        case = coverage["finite_conformance"]["cases"][0]
        case["expected_result"] = {
            "exit_code": 0,
            "stdout": case["actual_result"]["stdout"],
            "stderr": "",
        }
        with self.assertRaisesRegex(CHECKER.InventoryError, "normalized test outcome"):
            CHECKER.validate_coverage(ROOT, coverage)

    def test_runner_duration_does_not_change_normalized_outcome(self) -> None:
        xsht_coverage = self.frozen_coverage_with_case()
        xsht_case = xsht_coverage["finite_conformance"]["cases"][0]
        xsht_case["actual_result"]["stdout"] = xsht_case["actual_result"]["stdout"].replace(
            "ok 0ms", "ok 48ms"
        )
        CHECKER.validate_coverage(ROOT, xsht_coverage)

        cargo_coverage = self.cargo_coverage_with_case()
        cargo_case = cargo_coverage["finite_conformance"]["cases"][0]
        cargo_case["actual_result"]["stdout"] = cargo_case["actual_result"]["stdout"].replace(
            "finished in 0.01s", "finished in 91.47s"
        )
        CHECKER.validate_coverage(ROOT, cargo_coverage)

        cargo_case["actual_result"]["stdout"] = cargo_case["actual_result"]["stdout"].replace(
            "12 filtered out", "13 filtered out"
        )
        with self.assertRaisesRegex(CHECKER.InventoryError, "result does not match expectation"):
            CHECKER.validate_coverage(ROOT, cargo_coverage)

        xsht_case["actual_result"]["stdout"] += "unexpected output\n"
        with self.assertRaisesRegex(CHECKER.InventoryError, "unexpected xsht result output"):
            CHECKER.validate_coverage(ROOT, xsht_coverage)

        invalid_duration_coverage = self.cargo_coverage_with_case()
        invalid_duration_case = invalid_duration_coverage["finite_conformance"]["cases"][0]
        invalid_duration_case["actual_result"]["stdout"] = invalid_duration_case["actual_result"][
            "stdout"
        ].replace("finished in 0.01s", "finished in soon")
        with self.assertRaisesRegex(CHECKER.InventoryError, "invalid cargo test result transcript"):
            CHECKER.validate_coverage(ROOT, invalid_duration_coverage)

    def test_cargo_case_identifies_runner_and_package_test_artifact(self) -> None:
        coverage = self.cargo_coverage_with_case()
        case = coverage["finite_conformance"]["cases"][0]
        CHECKER.validate_coverage(ROOT, coverage)

        del case["binary_sha256s"]["cargo-test:xsh:lib"]
        with self.assertRaisesRegex(CHECKER.InventoryError, "binary roles must be"):
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
