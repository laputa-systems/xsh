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
        self.assertEqual(91, len(result["owners"]))
        self.assertEqual(
            {root["id"] for root in coverage["fragment_roots"]},
            result["fragment_roots"],
        )
        self.assertEqual(685, len(result["fragment_roots"]))
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
        integer_ops_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Values.IntegerOps"
        }
        self.assertEqual(
            {
                "Xsh.Values.IntegerOps.negative_division_truncates_toward_zero",
                "Xsh.Values.IntegerOps.checkedInt_total",
                "Xsh.Values.IntegerOps.addition_total",
                "Xsh.Values.IntegerOps.subtraction_total",
                "Xsh.Values.IntegerOps.multiplication_total",
                "Xsh.Values.IntegerOps.division_total",
                "Xsh.Values.IntegerOps.remainder_total",
                "Xsh.Values.IntegerOps.checked_add_rejects_wraparound",
                "Xsh.Values.IntegerOps.euclidean_remainder_mutation_changes_sign",
            },
            integer_ops_roots,
        )
        list_access_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Semantics.ListAccess"
        }
        self.assertEqual(
            {
                "Xsh.Semantics.ListAccess.nonnegative_literal_reads_its_position",
                "Xsh.Semantics.ListAccess.negative_literal_reads_from_the_end",
                "Xsh.Semantics.ListAccess.negative_zero_literal_is_zero",
                "Xsh.Semantics.ListAccess.too_negative_literal_is_rejected",
                "Xsh.Semantics.ListAccess.computed_negative_index_is_rejected",
                "Xsh.Semantics.ListAccess.negative_assignment_literal_does_not_count_from_end",
                "Xsh.Semantics.ListAccess.normalized_slice_bounds_are_clamped",
                "Xsh.Semantics.ListAccess.out_of_range_slice_bounds_clamp_to_the_list",
                "Xsh.Semantics.ListAccess.negative_slice_bounds_count_from_the_end",
                "Xsh.Semantics.ListAccess.reversed_slice_bounds_produce_an_empty_list",
                "Xsh.Semantics.ListAccess.computed_negative_from_end_mutation_is_detectable",
            },
            list_access_roots,
        )
        json_compatibility_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Json.Compatibility"
        }
        self.assertEqual(
            {
                "Xsh.Json.Compatibility.null_projects_to_json_null",
                "Xsh.Json.Compatibility.scalar_projection_preserves_value",
                "Xsh.Json.Compatibility.finite_float_is_accepted",
                "Xsh.Json.Compatibility.nonfinite_float_is_rejected",
                "Xsh.Json.Compatibility.list_projection_preserves_order",
                "Xsh.Json.Compatibility.record_projection_sorts_fields",
                "Xsh.Json.Compatibility.map_projection_requires_string_keys",
                "Xsh.Json.Compatibility.map_projection_checks_values_recursively",
                "Xsh.Json.Compatibility.set_projects_as_key_ordered_array",
                "Xsh.Json.Compatibility.str_enum_projects_to_wire_spelling",
                "Xsh.Json.Compatibility.nested_incompatible_value_rejects_root",
                "Xsh.Json.Compatibility.path_requires_explicit_json_conversion",
                "Xsh.Json.Compatibility.failure_publishes_no_partial_tree",
                "Xsh.Json.Compatibility.nonfinite_acceptance_mutation_is_detected",
            },
            json_compatibility_roots,
        )
        json_encoding_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Json.Encoding"
        }
        self.assertEqual(
            {
                "Xsh.Json.Encoding.null_token",
                "Xsh.Json.Encoding.boolean_tokens",
                "Xsh.Json.Encoding.integer_token",
                "Xsh.Json.Encoding.quote_is_escaped",
                "Xsh.Json.Encoding.backslash_is_escaped",
                "Xsh.Json.Encoding.control_characters_are_escaped",
                "Xsh.Json.Encoding.noncontrol_unicode_is_preserved",
                "Xsh.Json.Encoding.empty_array_is_compact",
                "Xsh.Json.Encoding.array_order_is_preserved",
                "Xsh.Json.Encoding.object_fields_are_sorted",
                "Xsh.Json.Encoding.nested_shape_and_order_are_preserved",
                "Xsh.Json.Encoding.raw_control_and_quote_mutation_are_distinguished",
            },
            json_encoding_roots,
        )
        expected_batch_roots = {
            "Xsh.Enums.WireConversion": {
                "Xsh.Enums.WireConversion.source_mapping_builds_from_finite_declarations_and_rejects_duplicates",
                "Xsh.Enums.WireConversion.exact_wire_spelling_converts_to_its_source_variant",
                "Xsh.Enums.WireConversion.unknown_wire_value_rejects_instead_of_defaulting",
                "Xsh.Enums.WireConversion.optional_enum_preserves_null_and_checks_present_values",
                "Xsh.Enums.WireConversion.enum_values_require_exact_module_and_declared_variant",
                "Xsh.Enums.WireConversion.record_conversion_preserves_extras_and_field_order",
                "Xsh.Enums.WireConversion.map_conversion_visits_every_value_and_preserves_key_order",
                "Xsh.Enums.WireConversion.nested_recursive_failures_report_field_index_and_key",
                "Xsh.Enums.WireConversion.list_conversion_preserves_order_and_spends_recursive_fuel",
                "Xsh.Enums.WireConversion.require_wire_publishes_only_converted_nested_value",
                "Xsh.Enums.WireConversion.rejected_wire_conversion_retains_candidate_and_nested_blame",
                "Xsh.Enums.WireConversion.rejected_conversion_exposes_no_converted_value",
            },
            "Xsh.Operators.BooleanShortCircuit": {
                "Xsh.Operators.BooleanShortCircuit.left_evaluated_once",
                "Xsh.Operators.BooleanShortCircuit.false_and_skips_right",
                "Xsh.Operators.BooleanShortCircuit.true_and_evaluates_right_once",
                "Xsh.Operators.BooleanShortCircuit.true_or_skips_right",
                "Xsh.Operators.BooleanShortCircuit.false_or_evaluates_right_once",
                "Xsh.Operators.BooleanShortCircuit.and_left_failure_propagates",
                "Xsh.Operators.BooleanShortCircuit.or_left_failure_propagates",
                "Xsh.Operators.BooleanShortCircuit.and_reached_right_failure_propagates",
                "Xsh.Operators.BooleanShortCircuit.or_reached_right_failure_propagates",
                "Xsh.Operators.BooleanShortCircuit.operand_effects_preserve_left_to_right_trace",
                "Xsh.Operators.BooleanShortCircuit.nested_boolean_evaluation_is_left_to_right",
                "Xsh.Operators.BooleanShortCircuit.eager_rhs_mutation_is_detectable",
            },
            "Xsh.Bytes.Access": {
                "Xsh.Bytes.Access.unsigned_lookup_matches_list",
                "Xsh.Bytes.Access.negative_lookup_is_absent",
                "Xsh.Bytes.Access.past_end_lookup_is_absent",
                "Xsh.Bytes.Access.zero_offset_reads_head",
                "Xsh.Bytes.Access.last_and_end_offsets_are_distinct",
                "Xsh.Bytes.Access.omitted_length_returns_suffix",
                "Xsh.Bytes.Access.exact_length_slice_selects_requested_bytes",
                "Xsh.Bytes.Access.zero_length_slice_is_empty",
                "Xsh.Bytes.Access.slice_length_is_clamped",
                "Xsh.Bytes.Access.invalid_slice_offsets_are_rejected",
                "Xsh.Bytes.Access.negative_slice_length_is_rejected",
                "Xsh.Bytes.Access.raw_ff_byte_does_not_require_utf8_decoding",
            },
            "Xsh.ByteCodecs.Integer": {
                "Xsh.ByteCodecs.Integer.valid_widths",
                "Xsh.ByteCodecs.Integer.invalid_width_rejection",
                "Xsh.ByteCodecs.Integer.negative_pack_rejection",
                "Xsh.ByteCodecs.Integer.pack_boundary_acceptance_and_overflow_rejection",
                "Xsh.ByteCodecs.Integer.little_and_big_endian_byte_order",
                "Xsh.ByteCodecs.Integer.pack_acceptance_matches_width_range",
                "Xsh.ByteCodecs.Integer.fit_range_pack_unpack_round_trip",
                "Xsh.ByteCodecs.Integer.lower_width_unpack_zero_extends",
                "Xsh.ByteCodecs.Integer.width_eight_signed_decode",
                "Xsh.ByteCodecs.Integer.invalid_offset_rejection",
                "Xsh.ByteCodecs.Integer.short_input_rejection",
                "Xsh.ByteCodecs.Integer.endian_swap_mutation_changes_value",
            },
            "Xsh.Lists.Splice": {
                "Xsh.Lists.Splice.written_order_trace",
                "Xsh.Lists.Splice.element_evaluated_once",
                "Xsh.Lists.Splice.splice_source_evaluated_once",
                "Xsh.Lists.Splice.splice_expands_in_place",
                "Xsh.Lists.Splice.unmarked_list_remains_nested",
                "Xsh.Lists.Splice.empty_splice_identity",
                "Xsh.Lists.Splice.duplicates_are_retained",
                "Xsh.Lists.Splice.exact_result_and_length",
                "Xsh.Lists.Splice.element_failure_stops_the_suffix",
                "Xsh.Lists.Splice.splice_failure_stops_the_suffix",
                "Xsh.Lists.Splice.failure_publishes_no_partial_list",
                "Xsh.Lists.Splice.hoisting_splice_changes_an_effectful_witness",
            },
            "Xsh.Sets.Algebra": {
                "Xsh.Sets.Algebra.union_commutativity",
                "Xsh.Sets.Algebra.union_associativity",
                "Xsh.Sets.Algebra.union_idempotence",
                "Xsh.Sets.Algebra.union_empty_left",
                "Xsh.Sets.Algebra.union_empty_right",
                "Xsh.Sets.Algebra.intersection_commutativity",
                "Xsh.Sets.Algebra.intersection_associativity",
                "Xsh.Sets.Algebra.intersection_idempotence",
                "Xsh.Sets.Algebra.intersection_empty_annihilation",
                "Xsh.Sets.Algebra.difference_self_empty",
                "Xsh.Sets.Algebra.difference_empty_identity",
                "Xsh.Sets.Algebra.difference_union_mutation_witness",
            },
            "Xsh.Patterns.ListMatcher": {
                "Xsh.Patterns.ListMatcher.wildcard_matches_without_captures",
                "Xsh.Patterns.ListMatcher.binder_capture_publishes_value",
                "Xsh.Patterns.ListMatcher.scalar_literal_matches_equal_value",
                "Xsh.Patterns.ListMatcher.scalar_literal_mismatch_is_rejected",
                "Xsh.Patterns.ListMatcher.exact_length_list_matches_each_element",
                "Xsh.Patterns.ListMatcher.exact_length_list_rejects_wrong_length",
                "Xsh.Patterns.ListMatcher.exact_length_is_checked_before_element_callbacks",
                "Xsh.Patterns.ListMatcher.prefix_rest_captures_suffix_after_prefix",
                "Xsh.Patterns.ListMatcher.empty_prefix_and_rest_capture_edges",
                "Xsh.Patterns.ListMatcher.nested_patterns_match_recursive_values",
                "Xsh.Patterns.ListMatcher.captures_follow_pattern_order",
                "Xsh.Patterns.ListMatcher.late_miss_discards_prior_capture_mutation_witness",
            },
        }
        for module, expected in expected_batch_roots.items():
            self.assertEqual(
                expected,
                {root["declaration"] for root in roots if root["module"] == module},
            )
        list_execution_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Semantics.ListExecution"
        }
        self.assertEqual(
            {
                "Xsh.Semantics.ListExecution.outer_source_is_first_event",
                "Xsh.Semantics.ListExecution.dependent_inner_source_runs_once_per_outer_binding",
                "Xsh.Semantics.ListExecution.inner_source_precedes_filter_and_projection_per_outer_binding",
                "Xsh.Semantics.ListExecution.filters_run_in_written_order",
                "Xsh.Semantics.ListExecution.false_filter_skips_later_filters_and_projection",
                "Xsh.Semantics.ListExecution.projection_runs_once_per_survivor",
                "Xsh.Semantics.ListExecution.output_follows_loop_nesting_order",
                "Xsh.Semantics.ListExecution.empty_outer_skips_inner_source_filters_and_projection",
                "Xsh.Semantics.ListExecution.outer_source_failure_stops_without_partial_publication",
                "Xsh.Semantics.ListExecution.inner_source_failure_stops_later_outer_bindings",
                "Xsh.Semantics.ListExecution.filter_failure_stops_later_filters_and_projection",
                "Xsh.Semantics.ListExecution.projection_failure_stops_without_partial_publication",
                "Xsh.Semantics.ListExecution.hoisted_inner_source_mutation_changes_trace_and_output",
                "Xsh.Semantics.ListExecution.rejecting_all_bindings_returns_empty_list",
            },
            list_execution_roots,
        )
        iteration_snapshot_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Iteration.Snapshot"
        }
        self.assertEqual(
            {
                "Xsh.Iteration.Snapshot.source_evaluates_once",
                "Xsh.Iteration.Snapshot.list_iterates_in_order",
                "Xsh.Iteration.Snapshot.map_iterates_in_key_order",
                "Xsh.Iteration.Snapshot.str_iterates_unicode_scalars",
                "Xsh.Iteration.Snapshot.bytes_iterate_unsigned_values",
                "Xsh.Iteration.Snapshot.result_error_skips_body",
                "Xsh.Iteration.Snapshot.result_ok_iterates_wrapped_snapshot",
                "Xsh.Iteration.Snapshot.source_rebinding_does_not_change_snapshot",
                "Xsh.Iteration.Snapshot.captured_body_observes_item_value",
                "Xsh.Iteration.Snapshot.item_binding_is_immutable",
                "Xsh.Iteration.Snapshot.live_mutation_changes_iteration",
                "Xsh.Iteration.Snapshot.snapshot_rejects_live_mutation",
            },
            iteration_snapshot_roots,
        )
        duration_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Values.DurationOps"
        }
        self.assertEqual(
            {
                "Xsh.Values.DurationOps.addition_returns_in_range_sum",
                "Xsh.Values.DurationOps.addition_rejects_out_of_range_sum",
                "Xsh.Values.DurationOps.subtraction_returns_ordered_difference",
                "Xsh.Values.DurationOps.subtraction_rejects_larger_right_operand",
                "Xsh.Values.DurationOps.multiplication_returns_in_range_product",
                "Xsh.Values.DurationOps.multiplication_rejects_out_of_range_product",
                "Xsh.Values.DurationOps.multiplication_rejects_negative_factor",
                "Xsh.Values.DurationOps.division_returns_quotient_for_positive_divisor",
                "Xsh.Values.DurationOps.division_rejects_nonpositive_divisor",
                "Xsh.Values.DurationOps.ratio_returns_validated_count",
                "Xsh.Values.DurationOps.ratio_rejects_zero_denominator",
                "Xsh.Values.DurationOps.ratio_overflow_iff_validator_rejects",
                "Xsh.Values.DurationOps.ratio_rejects_interval_count_overflow",
                "Xsh.Values.DurationOps.duration_max_ratio_overflows_int",
                "Xsh.Values.DurationOps.time_millis_clamp_exact_saturation_laws",
                "Xsh.Values.DurationOps.time_seconds_one_is_one_thousand_milliseconds",
                "Xsh.Values.DurationOps.time_seconds_clamp_exact_saturation_laws",
                "Xsh.Values.DurationOps.checked_add_rejects_wrapping_mutation",
            },
            duration_roots,
        )
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
                "fragment.Xsh.IR.Raw.DecoderSoundness.decode_success_respects_tag_payload_schema",
                "fragment.Xsh.IR.Raw.DecoderSoundness.successful_words_row_payload_has_length_and_bounds",
                "fragment.Xsh.IR.Raw.DecoderSoundness.unaligned_tables_are_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.aligned_row_index_and_reference_metadata_errors_are_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.out_of_range_reference_is_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.out_of_bounds_payload_start_is_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.payload_range_past_table_end_is_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.unknown_tag_is_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.singleton_tag_with_wrong_payload_length_is_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.immediate_tag_with_nonzero_payload_length_is_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.valid_singleton_payload_is_accepted",
                "fragment.Xsh.IR.Raw.DecoderSoundness.immediate_payload_guard_mutation_is_detected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.FiniteSchema.finite_schema_decode_encode",
                "fragment.Xsh.IR.Raw.DecoderSoundness.FiniteSchema.finite_schema_success_has_declared_width",
                "fragment.Xsh.IR.Raw.DecoderSoundness.FiniteSchema.finite_schema_acceptance_iff",
                "fragment.Xsh.IR.Raw.DecoderSoundness.FiniteSchema.finite_schema_unknown_ordinal_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.FiniteSchema.finite_schema_payload_width_mismatch_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.FiniteSchema.finite_schema_reserved_ordinal_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.FiniteSchema.finite_schema_out_of_range_ordinal_rejected",
                "fragment.Xsh.IR.Raw.DecoderSoundness.FiniteSchema.finite_schema_accepts_declared_sample_row",
                "fragment.Xsh.IR.Raw.DecoderSoundness.FiniteSchema.finite_schema_rejects_wrong_width_mutation_accepts",
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
                "Xsh.Effects.Capture",
                "Xsh.Effects.BoundFailureWitnesses",
                "Xsh.Effects.SolverEdgeOrder",
                "Xsh.Effects.CheckerBridge",
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
                "Xsh.Semantics.Effects.capture_effect_set_removes_error",
                "Xsh.Semantics.Effects.capture_effect_set_preserves_other_effect",
                "Xsh.Semantics.Effects.capture_effect_set_idempotent",
                "Xsh.Semantics.Effects.capture_commutes_with_effect_closure",
                "Xsh.Semantics.Effects.ofList_permutation_invariant",
                "Xsh.Semantics.Effects.ofList_duplicate_invariant",
                "Xsh.Semantics.Effects.captured_ofList_permutation_invariant",
                "Xsh.Semantics.Effects.captured_ofList_duplicate_invariant",
                "Xsh.Semantics.Effects.capture_rejects_network_under_empty_bound",
                "Xsh.Semantics.Effects.mutation_dropping_named_effects_is_observable",
                "Xsh.Semantics.Effects.capture_rejects_unresolved_under_empty_bound",
                "Xsh.Semantics.Effects.mutation_dropping_unresolved_is_observable",
                "Xsh.Semantics.Effects.capture_keeps_unknown_contract_unresolved",
                "Xsh.Semantics.Effects.classified_failure_retains_site_and_effect",
                "Xsh.Semantics.Effects.classified_failure_reason_is_sound",
                "Xsh.Semantics.Effects.rejected_classified_obligation_retains_site_and_effect",
                "Xsh.Semantics.Effects.missing_clause_network_witness_retains_obligation",
                "Xsh.Semantics.Effects.without_net_network_witness_retains_obligation",
                "Xsh.Semantics.Effects.mutation_dropping_failure_site_is_detected",
                "Xsh.Semantics.Effects.mutation_dropping_failure_effect_is_detected",
                "Xsh.Semantics.Effects.Solver.reorderGraph_preserves_direct",
                "Xsh.Semantics.Effects.Solver.reorderGraph_preserves_declared_contract",
                "Xsh.Semantics.Effects.Solver.edgesContribution_permutation_invariant",
                "Xsh.Semantics.Effects.Solver.nodeEquation_edgePermutation",
                "Xsh.Semantics.Effects.Solver.step_edgePermutation",
                "Xsh.Semantics.Effects.Solver.solve_edgePermutation",
                "Xsh.Semantics.Effects.Solver.reordered_edge_witness_preserves_solver_answer",
                "Xsh.Semantics.Effects.Solver.first_edge_only_mutation_changes_answer",
                "Xsh.Semantics.Effects.CheckerBridge.resolved_network_summary_projects_at_body_site",
                "Xsh.Semantics.Effects.CheckerBridge.unresolved_summary_does_not_become_known_empty",
                "Xsh.Semantics.Effects.CheckerBridge.unresolved_missing_and_opaque_sources_fail_closed",
                "Xsh.Semantics.Effects.CheckerBridge.successful_summary_call_records_lexical_creation",
                "Xsh.Semantics.Effects.CheckerBridge.successful_summary_call_pays_each_body_obligation",
                "Xsh.Semantics.Effects.CheckerBridge.alias_source_returns_the_checked_binding",
                "Xsh.Semantics.Effects.CheckerBridge.restricted_alias_keeps_the_original_creation_observation",
                "Xsh.Semantics.Effects.CheckerBridge.consumer_site_recheck_mutation_changes_alias_acceptance",
                "Xsh.Semantics.Effects.CheckerBridge.delegation_preserves_ordered_parent_child_body_provenance",
                "Xsh.Semantics.Effects.CheckerBridge.delegation_rejects_child_blocked_at_its_own_creation",
                "Xsh.Semantics.Effects.CheckerBridge.conditional_source_retains_both_tagged_arms",
                "Xsh.Semantics.Effects.CheckerBridge.conditional_source_rejects_a_blocked_arm",
            },
            effect_roots,
        )
        nominal_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Validation.Nominal"
        }
        self.assertEqual(
            {
                "Xsh.Validation.Nominal.constructor_introduction_retains_exact_provenance",
                "Xsh.Validation.Nominal.require_introduction_retains_exact_provenance",
                "Xsh.Validation.Nominal.erasure_preserves_underlying_value",
                "Xsh.Validation.Nominal.package_declaration_ids_are_distinct",
                "Xsh.Validation.Nominal.nominal_witnesses_keep_identity_before_erasure",
                "Xsh.Validation.Nominal.nominal_witnesses_erase_to_the_same_value",
                "Xsh.Validation.Nominal.erasure_is_not_injective",
                "Xsh.Validation.Nominal.overlapping_nominal_union_keeps_alternatives_distinct",
                "Xsh.Validation.Nominal.id_collapse_mutation_is_detected",
                "Xsh.Validation.Nominal.provenance_erasure_mutation_is_detected",
            },
            nominal_roots,
        )
        self.assertTrue(
            {
                "Xsh.Validation.Nominal.id_collapse_mutation_is_detected",
                "Xsh.Validation.Nominal.provenance_erasure_mutation_is_detected",
            }.issubset(nominal_roots)
        )
        identity_storage_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Validation.IdentityStorage"
        }
        self.assertEqual(
            {
                "Xsh.Validation.IdentityStorage.same_validation_identity_assigns",
                "Xsh.Validation.IdentityStorage.distinct_validations_do_not_assign",
                "Xsh.Validation.IdentityStorage.validated_list_has_its_base_view",
                "Xsh.Validation.IdentityStorage.nested_validated_element_has_base_view",
                "Xsh.Validation.IdentityStorage.validated_set_element_widening_is_rejected",
                "Xsh.Validation.IdentityStorage.validated_map_key_widening_is_rejected",
                "Xsh.Validation.IdentityStorage.validated_map_value_has_base_view",
                "Xsh.Validation.IdentityStorage.validated_result_field_has_base_view",
                "Xsh.Validation.IdentityStorage.optional_injection_preserves_assignability",
                "Xsh.Validation.IdentityStorage.validated_path_injects_optional",
                "Xsh.Validation.IdentityStorage.optional_storage_widening_and_any_injection_are_rejected",
                "Xsh.Validation.IdentityStorage.collection_parameter_invariance_remains",
                "Xsh.Validation.IdentityStorage.concrete_type_fits_any",
                "Xsh.Validation.IdentityStorage.any_does_not_fit_path",
                "Xsh.Validation.IdentityStorage.persistent_view_update_preserves_narrow_binding",
                "Xsh.Validation.IdentityStorage.persistent_path_update_commits_and_preserves_source",
                "Xsh.Validation.IdentityStorage.persistent_update_commits_selected_payload",
                "Xsh.Validation.IdentityStorage.widened_binding_preserves_payload",
                "Xsh.Validation.IdentityStorage.path_update_preserves_resource_identity",
                "Xsh.Validation.IdentityStorage.persistent_resource_update_witness",
                "Xsh.Validation.IdentityStorage.changed_resource_identity_mutation_is_detected",
                "Xsh.Validation.IdentityStorage.disabled_persistent_update_mutation_is_detected",
            },
            identity_storage_roots,
        )
        relpath_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Validation.RelPath"
        }
        self.assertEqual(
            {
                "Xsh.Validation.RelPath.empty_component_preserves_depth",
                "Xsh.Validation.RelPath.dot_component_preserves_depth",
                "Xsh.Validation.RelPath.parent_at_start_is_rejected",
                "Xsh.Validation.RelPath.parent_with_depth_decreases",
                "Xsh.Validation.RelPath.ordinary_component_increases_depth",
                "Xsh.Validation.RelPath.empty_path_is_rejected",
                "Xsh.Validation.RelPath.absolute_path_is_rejected",
                "Xsh.Validation.RelPath.dot_path_is_accepted",
                "Xsh.Validation.RelPath.empty_and_dot_components_are_accepted",
                "Xsh.Validation.RelPath.cancelling_parent_is_accepted",
                "Xsh.Validation.RelPath.cancelling_parent_to_start_is_accepted",
                "Xsh.Validation.RelPath.escaping_parent_is_rejected",
                "Xsh.Validation.RelPath.non_utf8_name_is_accepted",
                "Xsh.Validation.RelPath.nul_byte_is_rejected",
                "Xsh.Validation.RelPath.introduce_erases_exactly",
                "Xsh.Validation.RelPath.require_accepts",
                "Xsh.Validation.RelPath.require_rejects",
                "Xsh.Validation.RelPath.require_rejected_iff_invalid",
                "Xsh.Validation.RelPath.require_success_preserves_bytes",
                "Xsh.Validation.RelPath.require_success_establishes_boundary",
                "Xsh.Validation.RelPath.treating_parent_as_name_is_detected",
            },
            relpath_roots,
        )
        path_fields_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Paths.Fields"
        }
        self.assertEqual(
            {
                "Xsh.Paths.Fields.name_reads_final_component_after_trailing_separators",
                "Xsh.Paths.Fields.root_name_posix_basename_and_mutation_witness",
                "Xsh.Paths.Fields.posix_basename_ignores_trailing_separators",
                "Xsh.Paths.Fields.posix_dirname_ignores_trailing_separators",
                "Xsh.Paths.Fields.posix_dirname_of_bare_name_is_dot",
                "Xsh.Paths.Fields.posix_dirname_of_root_is_root",
                "Xsh.Paths.Fields.parent_preserves_written_components",
                "Xsh.Paths.Fields.parent_of_single_relative_name_is_dot",
                "Xsh.Paths.Fields.parent_of_root_is_root",
                "Xsh.Paths.Fields.leading_dot_name_has_no_extension",
                "Xsh.Paths.Fields.trailing_dot_has_empty_extension",
                "Xsh.Paths.Fields.extension_uses_final_nonleading_dot",
            },
            path_fields_roots,
        )
        loop_control_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Loops.ControlFlow"
        }
        self.assertEqual(
            {
                "Xsh.Loops.ControlFlow.normal_step_advances_to_next_iteration",
                "Xsh.Loops.ControlFlow.continue_targets_next_iteration",
                "Xsh.Loops.ControlFlow.break_unit_completes_loop",
                "Xsh.Loops.ControlFlow.break_value_completes_loop",
                "Xsh.Loops.ControlFlow.return_propagates_through_loop",
                "Xsh.Loops.ControlFlow.failure_propagates_through_loop",
                "Xsh.Loops.ControlFlow.inner_break_targets_inner_loop",
                "Xsh.Loops.ControlFlow.inner_continue_targets_inner_loop",
                "Xsh.Loops.ControlFlow.repeat_evaluates_count_once_with_mutation_witness",
                "Xsh.Loops.ControlFlow.zero_range_runs_no_body",
                "Xsh.Loops.ControlFlow.range_item_count_matches_absolute_count",
                "Xsh.Loops.ControlFlow.continue_then_break_has_exact_trace",
            },
            loop_control_roots,
        )
        path_normalize_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Paths.Normalize"
        }
        self.assertEqual(
            {
                "Xsh.Paths.Normalize.absolute_root_is_retained",
                "Xsh.Paths.Normalize.empty_relative_path_normalizes_to_dot",
                "Xsh.Paths.Normalize.empty_and_dot_components_are_removed",
                "Xsh.Paths.Normalize.ordinary_component_before_parent_is_cancelled",
                "Xsh.Paths.Normalize.leading_and_consecutive_relative_parents_are_retained",
                "Xsh.Paths.Normalize.absolute_parents_clamp_at_root",
                "Xsh.Paths.Normalize.absolute_normalization_renders_root",
                "Xsh.Paths.Normalize.repeated_normalization_matches_single_result_for_example",
                "Xsh.Paths.Normalize.normalized_components_remove_empty_and_dot_for_example",
                "Xsh.Paths.Normalize.ordinary_component_step_preserves_bytes",
                "Xsh.Paths.Normalize.relative_parent_stack_mutation_witness",
                "Xsh.Paths.Normalize.absolute_parent_stack_mutation_witness",
            },
            path_normalize_roots,
        )
        recursive_require_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Validation.RecursiveRequire"
        }
        self.assertEqual(
            {
                "Xsh.Validation.RecursiveRequire.recursive_check_uses_predecessor_fuel",
                "Xsh.Validation.RecursiveRequire.require_success_preserves_erased_data_and_provenance",
                "Xsh.Validation.RecursiveRequire.rejected_require_is_recoverable_and_nonmutating",
                "Xsh.Validation.RecursiveRequire.accepted_nested_record_list_and_optional_witness",
                "Xsh.Validation.RecursiveRequire.optional_null_is_accepted",
                "Xsh.Validation.RecursiveRequire.missing_optional_field_is_rejected",
                "Xsh.Validation.RecursiveRequire.unique_record_keys_with_extra_field_are_accepted",
                "Xsh.Validation.RecursiveRequire.duplicate_record_key_with_valid_first_value_is_rejected",
                "Xsh.Validation.RecursiveRequire.require_rejects",
                "Xsh.Validation.RecursiveRequire.duplicate_key_guard_detects_first_match_mutation",
                "Xsh.Validation.RecursiveRequire.shallow_only_mutation_misses_nested_bad_item",
            },
            recursive_require_roots,
        )
        bounded_refinement_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Validation.BoundedRefinements"
        }
        bounded_refinement_evidence = [
            root for root in roots
            if root["module"] == "Xsh.Validation.BoundedRefinements"
        ]
        self.assertEqual(
            {
                "Xsh.Validation.BoundedRefinements.widen_preserves_exact_payload",
                "Xsh.Validation.BoundedRefinements.widening_composes",
                "Xsh.Validation.BoundedRefinements.widen_composition",
                "Xsh.Validation.BoundedRefinements.narrow_rejects_outside_smaller",
                "Xsh.Validation.BoundedRefinements.zero_to_ten_widens_to_byte",
                "Xsh.Validation.BoundedRefinements.zero_to_ten_widening_witness_preserves_ten",
                "Xsh.Validation.BoundedRefinements.byte_is_nested_in_zero_to_256",
                "Xsh.Validation.BoundedRefinements.narrowing_256_to_byte_rejects",
                "Xsh.Validation.BoundedRefinements.dropping_high_bound_mutation_is_detected",
            },
            bounded_refinement_roots,
        )
        for root in bounded_refinement_evidence:
            self.assertIn("independent mathematical relation", root["scope"])
            self.assertIn("not XSH assignability", root["scope"])
            self.assertIn(
                "Does not model XSH assignability or prove VAL-WIDEN",
                root["boundary"],
            )
        widen_obligation = next(
            obligation for obligation in coverage["universal_theorems"]
            if obligation["id"] == "VAL-WIDEN"
        )
        self.assertEqual("planned", widen_obligation["status"])
        nonempty_filtering_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Validation.NonEmptyFiltering"
        }
        self.assertEqual(
            {
                "Xsh.Validation.NonEmptyFiltering.filter_is_nonempty_iff_survivor",
                "Xsh.Validation.NonEmptyFiltering.filterThenRequire_accepts_iff_survivor",
                "Xsh.Validation.NonEmptyFiltering.filterThenRequire_success_erases_exactly",
                "Xsh.Validation.NonEmptyFiltering.all_false_filter_rejects",
                "Xsh.Validation.NonEmptyFiltering.removing_nonempty_guard_is_detected",
            },
            nonempty_filtering_roots,
        )
        argv_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Semantics.Argv"
        }
        self.assertEqual(
            {
                "Xsh.Semantics.Argv.ordinary_word_is_one_argument",
                "Xsh.Semantics.Argv.word_with_embedded_space_remains_one_argument",
                "Xsh.Semantics.Argv.wildcard_word_remains_literal",
                "Xsh.Semantics.Argv.splice_preserves_items_and_order",
                "Xsh.Semantics.Argv.splice_items_keep_boundaries_in_source_order",
                "Xsh.Semantics.Argv.empty_splice_is_identity",
                "Xsh.Semantics.Argv.expand_argv_append",
                "Xsh.Semantics.Argv.mixed_words_preserve_source_order",
                "Xsh.Semantics.Argv.whitespace_split_mutation_changes_argv",
            },
            argv_roots,
        )
        primitive_classification_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Effects.PrimitiveClassification"
        }
        self.assertEqual(
            {
                "Xsh.Semantics.Effects.file_read_requires_filesystem",
                "Xsh.Semantics.Effects.run_requires_process",
                "Xsh.Semantics.Effects.time_now_requires_time",
                "Xsh.Semantics.Effects.socket_connect_requires_process_and_network",
                "Xsh.Semantics.Effects.print_requires_no_effects",
                "Xsh.Semantics.Effects.io_bound_permits_run",
                "Xsh.Semantics.Effects.without_net_rejects_socket_connect",
                "Xsh.Semantics.Effects.file_read_fs_to_process_mutation_changes_permission",
                "Xsh.Semantics.Effects.socket_connect_dropped_net_mutation_changes_permission",
            },
            primitive_classification_roots,
        )
        ordered_set_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Values.OrderedSet"
        }
        self.assertEqual(
            {
                "Xsh.Values.OrderedSet.ofList_membership",
                "Xsh.Values.OrderedSet.toList_nodup",
                "Xsh.Values.OrderedSet.toList_key_ordered",
                "Xsh.Values.OrderedSet.insert_existing_identity",
                "Xsh.Values.OrderedSet.remove_absent_identity",
                "Xsh.Values.OrderedSet.union_membership_iff",
                "Xsh.Values.OrderedSet.intersection_membership_iff",
                "Xsh.Values.OrderedSet.difference_membership_iff",
                "Xsh.Values.OrderedSet.ofList_permutation_preserves_iteration",
                "Xsh.Values.OrderedSet.ofList_duplicate_append_preserves_iteration",
                "Xsh.Values.OrderedSet.insertion_order_mutation_detected",
            },
            ordered_set_roots,
        )
        collect_block_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Collect.Block"
        }
        self.assertEqual(
            {
                "Xsh.Collect.Block.single_yield_appends_one_item",
                "Xsh.Collect.Block.delegated_list_keeps_its_order",
                "Xsh.Collect.Block.yields_follow_execution_order",
                "Xsh.Collect.Block.normal_exit_publishes_the_accumulator",
                "Xsh.Collect.Block.empty_normal_exit_publishes_an_empty_list",
                "Xsh.Collect.Block.nested_collect_uses_an_independent_accumulator",
                "Xsh.Collect.Block.err_is_an_ordinary_collected_item",
                "Xsh.Collect.Block.escaping_break_discards_the_partial_list",
                "Xsh.Collect.Block.escaping_continue_discards_the_partial_list",
                "Xsh.Collect.Block.escaping_return_discards_the_partial_list",
                "Xsh.Collect.Block.escaping_failure_discards_the_partial_list",
                "Xsh.Collect.Block.reversing_delegated_items_changes_the_list",
            },
            collect_block_roots,
        )
        match_selection_roots = {
            root["declaration"] for root in roots
            if root["module"] == "Xsh.Matches.Selection"
        }
        self.assertEqual(
            {
                "Xsh.Matches.Selection.subject_expression_is_evaluated_once",
                "Xsh.Matches.Selection.pattern_tests_follow_arm_order",
                "Xsh.Matches.Selection.pattern_miss_skips_guard",
                "Xsh.Matches.Selection.pattern_miss_advances_to_next_arm",
                "Xsh.Matches.Selection.successful_pattern_precedes_guard",
                "Xsh.Matches.Selection.false_guard_advances_to_next_arm",
                "Xsh.Matches.Selection.true_guard_selects_current_arm",
                "Xsh.Matches.Selection.selected_arm_skips_later_tests",
                "Xsh.Matches.Selection.else_runs_after_prior_rejections",
                "Xsh.Matches.Selection.exhausted_arms_produce_no_arm_fault",
                "Xsh.Matches.Selection.captures_are_published_only_after_pattern_success",
                "Xsh.Matches.Selection.reordered_else_mutation_changes_selection",
            },
            match_selection_roots,
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

    def test_comment_only_declaration_cannot_satisfy_a_fragment_root(self) -> None:
        fixture = ROOT / "formal/tests/inventory/fixtures/commented_theorem.lean.txt"
        self.assertEqual(
            {"Xsh.Fixture.real_theorem"},
            CHECKER.lean_theorem_declarations(fixture.read_text(encoding="utf-8")),
        )
        module_sources = {"Xsh.Fixture": fixture}
        roots = [
            {
                "id": "fragment.fixture.comment",
                "module": "Xsh.Fixture",
                "declaration": "Xsh.Fixture.not_a_theorem",
                "evidence_class": "SPEC_PROVED",
                "status": "fragment",
                "scope": "No proof scope.",
                "boundary": "No theorem exists in this module.",
            }
        ]
        with self.assertRaisesRegex(CHECKER.InventoryError, "not a public theorem"):
            CHECKER.validate_fragment_roots(roots, module_sources, {"Xsh.Fixture"})

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
