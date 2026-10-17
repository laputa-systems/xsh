#!/usr/bin/env bash
set -Eeuo pipefail
harness=/target/x86_64-unknown-linux-musl/debug/build/xsh/692c8c084af82ba2/out/xsh-692c8c084af82ba2
selectors=(
  sema::check::method::host_obligations::tests::method_dispatch_preserves_receiver_resolution_error_categories
  sema::check::method::host_obligations::tests::host_candidate_trials_preserve_resolution_error_categories
  sema::check::infer_param::host_anchor_tests::host_receiver_domains_preserve_record_map_and_signed_unsigned_alternatives
  sema::constraints::tests::constraint_trials_restore_cells_rows_provenance_and_fresh_id_ownership
  sema::constraints::tests::constraint_trials_restore_an_in_flight_journal_on_panic
  sema::constraints::tests::constraint_trial_work_follows_touched_cells_instead_of_unrelated_wide_state
  sema::check::method::host_obligations::tests::host_result_context_does_not_choose_overloads_within_one_receiver_domain
  sema::constraints::tests::failed_unification_rolls_back_rank_binding_and_path_compression
  sema::constraints::tests::failed_unification_after_alias_walk_keeps_probes_independent
  sema::constraints::tests::row_constraint_probes_keep_clones_independent
  runtime::eval::indexed::full::tests::inferred_indirect_host_anchor_executes_after_frontend_drop_on_both_routes
  sema::check::infer_param::host_anchor_tests::inferred_host_obligations_publish_indirect_chain_and_reordered_facts
  sema::check::infer_param::host_anchor_tests::inferred_host_obligations_reject_actual_ambiguity_without_caller_evidence
  sema::check::method::host_obligations::tests::host_obligation_dependencies_preserve_depth_and_foreign_variable_errors
  modules::signature::tests::api_spec_adapter_exactly_mirrors_registry
  modules::signature::tests::every_registry_method_effect_is_enforced_by_the_checker
  modules::signature::tests::a_required_label_is_declared_only_where_the_checker_enforces_it
  sema::builtin_templates::tests::dynamic_list_receiver_cannot_acquire_a_concrete_item_proof
  sema::builtin_templates::tests::nested_result_and_nominal_error_domains_survive_receiver_instantiation
  sema::builtin_templates::tests::success_expectation_preserves_a_declared_result_envelope
  sema::builtin_templates::tests::wider_destinations_preserve_concrete_builtin_result_domains
  sema::builtin_templates::tests::concrete_inert_receiver_domains_do_not_become_fresh_holes
  runtime::eval::indexed::full::tests::builtin_templates_execute_checked_calls_after_frontend_drop_on_both_routes
  runtime::eval::lower::checked_constructor_fact_tests::checker_rejects_generic_list_body_before_lowering
  runtime::eval::indexed::full::tests::inferred_host_replace_executes_after_frontend_drop_on_both_routes
  runtime::eval::indexed::full::tests::inferred_host_nested_split_executes_after_frontend_drop_on_both_routes
  sema::check::infer_param::host_anchor_tests::inferred_host_facts_follow_late_parameter_constraints
  sema::check::infer_param::host_anchor_tests::inferred_host_anchors_preserve_ambiguity_arguments_effects_and_annotations
  runtime::eval::indexed::full::tests::user_schemes_execute_after_frontend_drop_on_both_routes
  sema::check::module_driver::tests::repeated_generic_calls_add_call_facts_without_increasing_final_checker_body_entries
  sema::check::module_driver::tests::recursive_inferred_parameters_share_provisional_types_in_both_orders
  sema::check::module_driver::tests::recursive_parameters_are_solved_without_caller_evidence
  frontend_stats::tests::frontend_stats_reports_final_checker_body_entries
  sema::constraints::tests::ground_record_resolution_preserves_depth_errors
  sema::constraints::tests::record_resolution_preserves_foreign_variable_errors
  sema::check::module_driver::tests::captured_mutable_bindings_are_not_generalized
)
for selector in "${selectors[@]}"; do
  "$harness" --exact "$selector" --nocapture > "/scratch/logs/${selector##*::}-final.log" 2>&1
  grep -q "1 passed; 0 failed;" "/scratch/logs/${selector##*::}-final.log"
done
