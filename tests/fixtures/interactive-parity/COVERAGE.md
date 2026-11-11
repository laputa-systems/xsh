# ish PTY test parity coverage

Generated from the `#[test]` lists in `/home/josh/d/ish/tests/`. A scenario name
means `crates/xshi/tests/parity/` records it from ish and requires xshi to
match. `known-gap` entries are listed in `known-gaps.txt` with the owning code.

## tests/pty.rs (202)

| ish test | xshi parity scenario |
|---|---|
| `native_fs_copy_and_move_lists_keep_successes_and_continue_after_failures` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_exact_targets_and_into_directories_preserve_real_bytes` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_multi_source_into_trailing_directory` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_redirections_prevent_failed_transfers_and_capture_stderr` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_parent_placement_accepts_command_alias_expanded_name_and_argument_substitution` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_standalone_negation_runs_in_the_shell_process` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_static_dynamic_and_nested_fork_contexts_refuse_before_redirections` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_quick_no_progress_copy_stays_quiet` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_ctrl_c_during_copy_returns_130_to_a_usable_prompt` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_ctrl_z_is_ignored_during_copy_and_external_job_control_still_works` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_sigterm_and_sighup_cancel_and_restore_the_terminal` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_broken_stderr_cancels_bounded_work_and_keeps_the_prompt_usable` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `prompt_appears_on_startup` | `prompt_appears_on_startup` |
| `echo_command` | `echo_command` |
| `prompt_does_not_share_a_line_with_unterminated_external_output` | `prompt_does_not_share_a_line_with_unterminated_external_output` |
| `broken_interpreter_reports_bad_interpreter` | `broken_interpreter_reports_bad_interpreter` |
| `pwd_builtin` | `pwd_builtin` |
| `cd_and_pwd` | `cd_and_pwd` |
| `exit_with_ctrl_d` | `exit_with_ctrl_d` |
| `exit_command` | `exit_command` |
| `ctrl_c_cancels_input` | `ctrl_c_cancels_input` |
| `sigint_during_editing_cancels_the_line` | `sigint_during_editing_cancels_the_line` |
| `line_editing_backspace` | `line_editing_backspace` |
| `line_editing_up_down_navigate_wrapped_input` | `line_editing_up_down_navigate_wrapped_input` |
| `line_editing_ctrl_u` | `line_editing_ctrl_u` |
| `line_editing_ctrl_w` | `line_editing_ctrl_w` |
| `line_editing_ctrl_delete` | `line_editing_ctrl_delete` |
| `line_editing_ctrl_k_and_ctrl_y` | `line_editing_ctrl_k_and_ctrl_y` |
| `pipeline` | `pipeline` |
| `and_or_list` | `and_or_list` |
| `or_list` | `or_list` |
| `redirect_output` | `redirect_output` |
| `l_lists_files` | `l_lists_files` |
| `l_wraps_long_names_at_narrow_terminal_width` | `l_wraps_long_names_at_narrow_terminal_width` |
| `set_and_echo_var` | `set_and_echo_var` |
| `exported_var_reaches_external_commands` | `exported_var_reaches_external_commands` |
| `set_var_reaches_external_commands` | `set_var_reaches_external_commands` |
| `set_var_joins_multiple_value_words` | `set_var_joins_multiple_value_words` |
| `set_no_args_lists_env_vars` | `set_no_args_lists_env_vars` |
| `set_option_and_positional_forms_are_refused_without_target_effects` | `set_refuses_option_and_positional_forms_without_redirection_effects` |
| `exec_and_trap_refusals_preserve_the_shell_and_target_files` | `unsupported_builtins_are_refused_without_target_effects` |
| `builtin_lookup_completion_and_explicit_paths_share_the_policy` | known-gap: epsh `command` builtin absent (interactive/app.rs) (`builtin_lookup_completion_and_explicit_paths_share_the_policy`) |
| `approved_builtin_classes_remain_available` | known-gap: epsh `command` builtin absent (interactive/app.rs) (`approved_builtin_classes_remain_available`) |
| `unset_removes_var_from_children` | `unset_removes_var_from_children` |
| `unset_removes_os_env_var_set_by_set` | `unset_removes_os_env_var_set_by_set` |
| `unset_in_same_line_not_inherited_by_children` | `unset_in_same_line_not_inherited_by_children` |
| `unset_removes_ambient_environment_from_children` | `unset_removes_ambient_environment_from_children` |
| `unset_removes_ambient_environment_in_pipeline_children` | `unset_removes_ambient_environment_in_pipeline_children` |
| `compound_list_export_reaches_child` | `compound_list_export_reaches_child` |
| `store_home_drives_interactive_cd` | `store_home_drives_interactive_cd` |
| `unset_oldpwd_blocks_interactive_cd_minus` | `unset_oldpwd_blocks_interactive_cd_minus` |
| `prefix_assignment_reaches_child_but_does_not_persist` | `prefix_assignment_reaches_child_but_does_not_persist` |
| `set_path_affects_command_lookup` | `set_path_affects_command_lookup` |
| `which_reflects_exported_path` | `which_reflects_exported_path` |
| `which_uses_store_path_not_os_env` | `which_uses_store_path_not_os_env` |
| `command_lookup_classifies_ish_commands_with_empty_path` | Excluded: S4 native fs lane (asserts `cp`/`mv` classification) |
| `native_fs_rsync_interactive` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `native_fs_fdu_interactive` | Excluded: S6 fdu lane (`fdu_pty.rs`/`fdu_snapshots.rs` owner) |
| `tilde_expansion` | `tilde_expansion` |
| `history_up_arrow` | `history_up_arrow` |
| `history_up_narrow_repaint_clears_wrapped_rows` | `history_up_narrow_repaint_clears_wrapped_rows` |
| `history_ctrl_r_search` | `history_ctrl_r_search` |
| `history_ctrl_r_includes_later_global_entries` | `history_ctrl_r_includes_later_global_entries` |
| `history_ctrl_r_escape_cancels` | `history_ctrl_r_escape_cancels` |
| `history_ctrl_r_narrow_repaint_does_not_stack_rows` | `history_ctrl_r_narrow_repaint_does_not_stack_rows` |
| `history_search_selection_preserves_cursor_after_typing` | `history_search_selection_preserves_cursor_after_typing` |
| `history_accept_reanchors_prompt_before_typing` | `history_accept_reanchors_prompt_before_typing` |
| `history_ctrl_r_near_bottom_keeps_pager_stable` | `history_ctrl_r_near_bottom_keeps_pager_stable` |
| `history_ctrl_r_scrolls_when_selection_passes_last_visible_entry` | `history_ctrl_r_scrolls_when_selection_passes_last_visible_entry` |
| `history_ctrl_r_near_bottom_query_edits_do_not_stack_headers` | `history_ctrl_r_near_bottom_query_edits_do_not_stack_headers` |
| `tab_completion_files` | `tab_completion_files` |
| `tab_completion_shows_grid` | `tab_completion_shows_grid` |
| `tab_completion_first_tab_has_no_selection_second_tab_selects_first` | `tab_completion_first_tab_has_no_selection_second_tab_selects_first` |
| `tab_completion_narrow_repaint_does_not_stack_rows` | `tab_completion_narrow_repaint_does_not_stack_rows` |
| `tab_completion_directory` | `tab_completion_directory` |
| `tab_completion_escape_restores_typed_prefix` | `tab_completion_escape_restores_typed_prefix` |
| `tab_completion_narrowing_does_not_autoaccept` | `tab_completion_narrowing_does_not_autoaccept` |
| `tab_completion_with_wide_dir_name_restores_prompt_cursor` | `tab_completion_with_wide_dir_name_restores_prompt_cursor` |
| `completion_resize_rerenders_grid` | `completion_resize_rerenders_grid` |
| `normal_resize_reanchors_wrapped_prompt` | `normal_resize_reanchors_wrapped_prompt` |
| `history_resize_rerenders_pager` | `history_resize_rerenders_pager` |
| `alias_expansion` | `alias_expansion` |
| `alias_self_referencing_no_reexpand` | `alias_self_referencing_no_reexpand` |
| `alias_self_referencing_from_config` | `alias_self_referencing_from_config` |
| `alias_self_referencing_exec` | `alias_self_referencing_exec` |
| `alias_list` | `alias_list` |
| `alias_with_command_substitution` | `alias_with_command_substitution` |
| `alias_preserves_quoted_word` | `alias_preserves_quoted_word` |
| `which_builtin` | `which_builtin` |
| `rmrf_removes_a_directory` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `which_external` | `which_external` |
| `which_alias` | `which_alias` |
| `which_same_name_alias_shows_underlying_binary` | `which_same_name_alias_shows_underlying_binary` |
| `error_status_colors_prompt` | `error_status_colors_prompt` |
| `nonexistent_command` | `nonexistent_command` |
| `script_mode_refused` | `script_mode_refused` |
| `source_is_refused` | `source_is_refused` |
| `expanded_targets_obey_ish_execution_policy` | known-gap: dynamic target refusal absent (interactive/app.rs) (`expanded_targets_obey_ish_execution_policy`) |
| `ctrl_l_clears_screen` | `ctrl_l_clears_screen` |
| `multiline_continuation` | `multiline_continuation` |
| `multiline_preserves_unicode_quotes_and_backslash_newlines` | `multiline_preserves_unicode_quotes_and_backslash_newlines` |
| `interactive_final_history_replay_restores_the_recorded_directory` | known-gap: absolute cd target is not quoted (interactive/app.rs); plain-directory case in `interactive_final_history_replay_in_a_plain_directory` (`interactive_final_history_replay_restores_the_recorded_directory`) |
| `multiline_completion_on_continuation_line` | `multiline_completion_on_continuation_line` |
| `dir_picker_narrow_repaint_does_not_stack_rows` | `dir_picker_narrow_repaint_does_not_stack_rows` |
| `config_file_loaded` | `config_file_loaded` |
| `prompt_shows_cwd` | `prompt_shows_cwd` |
| `cd_minus_goes_back` | `cd_minus_goes_back` |
| `cd_tilde_subdir` | `cd_tilde_subdir` |
| `implicit_cd_quoted_path` | `implicit_cd_quoted_path` |
| `path_binary_wins_over_implicit_cd` | known-gap: implicit cd precedes PATH lookup (interactive/app.rs) (`path_binary_wins_over_implicit_cd`) |
| `l_tilde_subdir` | `l_tilde_subdir` |
| `unset_variable` | `unset_variable` |
| `glob_expansion` | `glob_expansion` |
| `l_glob_expansion` | `l_glob_expansion` |
| `l_hides_dotfiles` | `l_hides_dotfiles` |
| `ll_shows_dotfiles` | `ll_shows_dotfiles` |
| `quoted_string_preserves_spaces` | `quoted_string_preserves_spaces` |
| `single_quotes_no_expansion` | `single_quotes_no_expansion` |
| `history_persisted_across_commands` | `history_persisted_across_commands` |
| `history_survives_abrupt_termination_and_restart` | `history_survives_abrupt_termination_and_restart` |
| `history_recovers_partial_append_after_abrupt_termination` | `history_recovers_partial_append_after_abrupt_termination` |
| `history_shared_shell_processes_keep_all_commands` | `history_shared_shell_processes_keep_all_commands` |
| `history_write_failure_is_visible_and_command_still_runs` | `history_write_failure_is_visible_and_command_still_runs` |
| `history_read_only_commands_preserve_another_writer` | `history_read_only_commands_preserve_another_writer` |
| `history_reset_reopens_storage_in_existing_shell_processes` | `history_reset_reaches_running_shells` |
| `history_readers_keep_startup_log_after_home_changes` | `history_readers_keep_startup_log_after_home_changes` |
| `history_reset_refuses_pipeline_and_command_substitution` | `history_reset_refuses_pipeline_and_command_substitution` |
| `history_up_ignores_later_global_entries` | `history_up_ignores_later_global_entries` |
| `history_help` | `history_help` |
| `history_autosuggest_ignores_later_global_entries` | `history_autosuggest_ignores_later_global_entries` |
| `true_and_false_builtins` | `true_and_false_builtins` |
| `denv_loads_allowed_envrc_on_cd` | `denv_loads_allowed_envrc_on_cd` |
| `denv_loads_dotenv_on_cd_only_after_allow` | `denv_dotenv_loads_only_after_allow` |
| `denv_unloads_on_leave` | `denv_unloads_on_leave` |
| `denv_exit_command_inside_project_removes_active_record` | `denv_exit_removes_the_active_record` |
| `denv_ctrl_d_inside_project_removes_active_record` | `denv_ctrl_d_removes_the_active_record` |
| `denv_allow_applies_env` | `denv_allow_applies_env` |
| `denv_deny_removes_env_and_marks_dirty` | `denv_deny_removes_env_and_marks_dirty` |
| `denv_startup_loads_allowed_dotenv_in_initial_cwd` | `denv_startup_loads_allowed_dotenv_in_initial_cwd` |
| `denv_startup_loads_allowed_envrc_in_initial_cwd` | `denv_startup_loads_allowed_envrc_in_initial_cwd` |
| `denv_dotenv_overrides_envrc` | `denv_dotenv_overrides_envrc` |
| `denv_reload_after_reallow_picks_up_envrc_edit` | `denv_reload_after_reallow_picks_up_envrc_edit` |
| `denv_edit_envrc_invalidates_trust` | `denv_edit_envrc_invalidates_trust` |
| `denv_restores_preexisting_var_on_leave` | `denv_restores_preexisting_var_on_leave` |
| `denv_path_add_relative_dir` | `denv_path_add_relative_dir` |
| `denv_dotenv_helper_rejects_the_entire_candidate` | `denv_dotenv_helper_rejects_the_entire_candidate` |
| `denv_automatic_dotenv_loads_without_a_helper` | denv_loads_dotenv_on_cd_without_allow + denv_startup_loads_dotenv_in_initial_cwd |
| `denv_automatic_dotenv_keeps_shell_text_as_data_and_revokes_edits` | `denv_dotenv_keeps_text_as_data_and_revokes_edits` |
| `denv_rejects_parent_helpers_after_parent_revocation_or_edit` | `denv_rejects_parent_helpers_after_parent_revocation_or_edit` |
| `denv_rejects_changed_shared_file_helpers` | `denv_source_env_is_rejected` |
| `denv_allow_without_any_env_file_fails` | `denv_allow_without_any_env_file_fails` |
| `ctrl_c_interrupts_running_child` | `ctrl_c_interrupts_running_child` |
| `ctrl_d_with_stopped_job_warns_then_exits` | `ctrl_d_with_a_suspended_job_warns_before_forcing` |
| `exit_twice_with_stopped_job_warns_then_exits` | `exit_twice_with_a_stopped_job_exits` |
| `confirmed_exit_with_stopped_job_keeps_the_requested_status` | `confirmed_exit_with_a_stopped_job_keeps_the_requested_status` |
| `pipeline_suspend_and_resume_retains_every_stage` | `pipeline_suspend_and_resume_retains_every_stage` |
| `native_handlers_refuse_child_execution_before_target_redirections` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `external_input_redirections_preserve_foreground_terminal_ownership` | `external_input_redirections_preserve_foreground_terminal_ownership` + known-gap `closed_input_redirection_keeps_terminal_ownership` (`0<&-` parses as background) |
| `ordinary_exit_148_does_not_create_a_suspended_job` | `ordinary_exit_148_does_not_create_a_suspended_job` |
| `occupied_job_slot_restores_new_job_terminal_modes_before_continuing` | `occupied_job_slot_restores_new_job_terminal_modes_before_continuing` |
| `stopped_pipeline_discards_the_remaining_list` | `stopped_pipeline_discards_the_remaining_list` |
| `job_suspend_and_resume` | `job_suspend_and_resume` |
| `pipeline_suspend_and_resume_keeps_job_owned` | `pipeline_suspend_and_resume_keeps_job_owned` |
| `killed_child_in_raw_mode_does_not_leak_into_the_next_command` | `killed_child_in_raw_mode_does_not_leak_into_the_next_command` |
| `stopped_child_in_raw_mode_restores_baseline_then_fg_restores_its_own_mode` | `stopped_child_in_raw_mode_restores_baseline_then_fg_restores_its_own_mode` |
| `foreground_terminal_mode_changes_on_stdout_apply_to_stdin` | `foreground_terminal_mode_changes_on_stdout_apply_to_stdin` |
| `git_log_pages_on_a_converged_terminal` | `git_log_pages_on_a_converged_terminal` |
| `single_command_sees_one_terminal_for_all_standard_streams` | `single_command_sees_one_terminal_for_all_standard_streams` |
| `output_beyond_the_capture_limit_still_reaches_the_terminal_and_the_shell_continues` | `output_beyond_the_capture_limit_still_reaches_the_terminal_and_the_shell_continues` |
| `bracketed_paste_over_limit_rejected` | `bracketed_paste_over_limit_rejected` |
| `bracketed_paste_large_markdown_rejected` | `bracketed_paste_large_document_rejected` |
| `bracketed_paste_under_limit_accepted` | `bracketed_paste_under_limit_accepted` |
| `bracketed_paste_exactly_at_limit_accepted` | `bracketed_paste_exactly_at_limit_accepted` |
| `interactive_final_typeahead_after_enter_survives_the_next_prompt` | `interactive_final_typeahead_after_enter_survives_the_next_prompt` |
| `interactive_final_sigterm_interrupts_oversized_paste_drain` | `interactive_final_sigterm_interrupts_oversized_paste_drain` |
| `interactive_final_failed_shell_builtins_update_question_status` | `failed_builtins_update_question_status` |
| `interactive_final_z_matches_quoted_directory_history` | known-gap: z ignores quoted directory history (interactive/z.rs) (`interactive_final_z_matches_quoted_directory_history`) |
| `external_commands_run_in_the_directory_a_list_cd_selected` | `external_commands_run_in_the_directory_a_list_cd_selected` |
| `sigterm_and_sighup_restore_the_terminal_and_exit_with_the_signal_status` | `sigterm_and_sighup_restore_the_terminal_and_exit_with_the_signal_status` |
| `fatal_crash_signals_restore_the_terminal_and_exit_with_the_signal_status` | `fatal_crash_signals_restore_the_terminal_and_exit_with_the_signal_status` |
| `second_ctrl_z_is_refused_and_the_first_job_stays_resumable` | `second_ctrl_z_is_refused_and_the_first_job_stays_resumable` |
| `state_builtin_in_a_list_and_undocumented_syntax_are_refused_without_running_anything` | `state_builtin_in_a_list_and_undocumented_syntax_are_refused_without_running_anything` |
| `ctrl_p_dump_is_private_and_omits_typed_text` | `ctrl_p_dump_is_private_and_omits_typed_text` |
| `native_listing_and_standalone_cd_preserve_private_use_and_raw_path_bytes` | Owner-deferred: raw non-UTF-8 path bytes |
| `prompt_identity_displays_unicode_and_replaces_only_invalid_display_bytes` | known-gap: non-UTF-8 command substitution refused (interactive/app.rs); Unicode and control cases in `prompt_identity_displays_unicode_user` (`prompt_identity_displays_unicode_and_replaces_only_invalid_display_bytes`) |
| `native_copy_uses_current_store_and_prefix_tmpdir_values` | Excluded: S4 native fs lane (`cp`/`mv`/`rsync`/`rmrf`) |
| `osc133_marks_prompt_input_execution_and_completion` | `osc133_marks_prompt_input_execution_and_completion` |
| `osc133_reports_command_status` | `osc133_reports_command_status` |
| `osc133_cancelled_input_has_no_execution_marker` | `osc133_cancelled_input_has_no_execution_marker` |
| `osc133_refused_input_has_no_execution_marker` | `osc133_refused_input_has_no_execution_marker` |
| `osc133_resize_does_not_duplicate_markers` | `osc133_resize_does_not_duplicate_markers` |
| `osc133_redirected_output_keeps_markers_off_command_file` | `osc133_redirected_output_keeps_markers_off_command_file` |
| `osc133_continuation_does_not_emit_extra_execution_markers` | `osc133_continuation_does_not_emit_extra_execution_markers` |
| `osc133_suspended_job_and_fg_emit_completion_markers` | `osc133_suspended_job_and_fg_emit_completion_markers` |
| `delegated_ish_c_emits_no_osc133_markers` | `delegated_ish_c_emits_no_osc133_markers` |
| `osc7_working_directory_emitted_to_interactive_terminal` | `osc7_working_directory_emitted_to_interactive_terminal` |
| `resize_storm_keeps_shell_responsive` | `resize_storm_keeps_shell_responsive` |
| `repeated_stop_resume_preserves_command_boundaries` | `repeated_stop_resume_preserves_command_boundaries` |
| `converged_command_reads_from_dev_tty` | `converged_command_reads_from_dev_tty` |
| `alternate_screen_sequences_pass_through_relay` | `alternate_screen_sequences_pass_through_relay` |
| `osc52_copy_scrollback_emits_base64_session_log` | `osc52_copy_scrollback_emits_base64_session_log` |
| `shell_sighup_at_prompt_exits_and_restores_terminal` | `shell_sighup_at_prompt_exits_and_restores_terminal` |

## tests/history_search_paste.rs (3)

| ish test | xshi parity scenario |
|---|---|
| `pasted_history_query_filters_and_executes_selected_command` | `pasted_history_query_filters_and_executes_selected_command` |
| `pasted_history_query_edits_at_cursor_and_restores_candidates_after_delete` | `pasted_history_query_edits_at_cursor_and_restores_candidates_after_delete` |
| `pasted_history_query_resets_selection_after_narrowing` | `pasted_history_query_resets_selection_after_narrowing` |

## tests/external_signals.rs (2)

| ish test | xshi parity scenario |
|---|---|
| `external_commands_restore_terminal_signals_in_every_context` | `external_commands_restore_terminal_signals_in_every_context` |
| `external_signal_probe` (`#[ignore]` helper) | Not a test: executed as an external command by the test above |

## tests/prompt_git_refresh.rs (2)

| ish test | xshi parity scenario |
|---|---|
| `prompt_refreshes_negative_git_lookup_after_repository_creation` | `prompt_refreshes_negative_git_lookup_after_repository_creation` |
| `prompt_refreshes_negative_git_lookup_after_ancestor_repository_creation` | `prompt_refreshes_negative_git_lookup_after_ancestor_repository_creation` |

## tests/prompt_identity.rs (2)

| ish test | xshi parity scenario |
|---|---|
| `prompt_identity_replaces_controls_on_construction` | `prompt_identity_displays_unicode_user` (control replacement through the PTY) |
| `prompt_identity_replaces_controls_after_update` | `prompt_identity_displays_unicode_user` (identity updated by `USER=...`) |

## tests/login_shell.rs (7)

| ish test | xshi coverage |
|---|---|
| `command_mode_runs_the_string_with_bin_sh` | `command_mode_matches_the_reference_shell` |
| `command_mode_propagates_the_child_status` | `command_mode_matches_the_reference_shell` |
| `command_mode_connects_stdin_and_stdout` | `command_mode_matches_the_reference_shell` |
| `command_mode_requires_a_command` | `command_mode_matches_the_reference_shell` (both refuse; xshi usage status is 2, ish 1, in `crates/xshi/src/app.rs`) |
| `config_flag_requires_a_path` | `command_mode_matches_the_reference_shell` (same status difference) |
| `a_command_that_names_a_path_is_not_read_as_config` | `command_mode_matches_the_reference_shell` |
| `a_bare_argument_is_still_refused` | `script_mode_refused` (`crates/xshi/tests/cli.rs::script_mode_is_refused_like_the_reference_shell`) |

## tests/history_import.rs (1)

| ish test | xshi coverage |
|---|---|
| `legacy_fish_import_refuses_existing_log_without_changing_files` | Not applicable: ish-only `scripts/import-fish-history`; xshi ships no fish importer and no legacy fish log |

## tests/denv_environment.rs (3)

| ish test | xshi coverage |
|---|---|
| `dotenv_sessions_restore_exact_bindings_without_host_environment_changes` | `denv_restores_unexported_and_empty_values_on_leave`, `denv_restores_preexisting_var_on_leave`; raw non-UTF-8 env bytes owner-deferred |
| `readonly_delta_rejects_every_assignment_before_installing_active_state` | `denv.rs` unit tests `candidate_must_complete_its_failure_channel`, `apply_diff`; xshi has no `readonly` variables |
| `evaluators_use_selected_raw_environment_and_restore_it_after_failure` | `denv_envrc_sees_the_selected_environment`, `denv_dotenv_keeps_text_as_data_and_revokes_edits`; raw non-UTF-8 env bytes owner-deferred |

## tests/integration.rs (140)

These are in-process unit tests, not PTY fixtures. xshi ports them under
`crates/xshi/src/interactive/ported_tests.rs` and `history/tests.rs` with the
same names unless a mapping is given.

| ish test | xshi unit test |
|---|---|
| `line_buffer_insert_middle` | `ported_tests.rs::line_buffer_insert_middle` |
| `line_buffer_delete_forward` | `ported_tests.rs::line_buffer_delete_forward` |
| `line_buffer_utf8_handling` | `ported_tests.rs::line_buffer_utf8_handling` |
| `line_buffer_word_operations_complex` | `ported_tests.rs::line_buffer_word_operations_complex` |
| `line_buffer_kill_word_forward` | `ported_tests.rs::line_buffer_kill_word_forward` |
| `line_buffer_kill_yank_cycle` | `ported_tests.rs::line_buffer_kill_yank_cycle` |
| `line_buffer_kill_to_end` | `ported_tests.rs::line_buffer_kill_to_end` |
| `line_buffer_empty_operations` | `ported_tests.rs::line_buffer_empty_operations` |
| `line_buffer_set_resets_cursor` | `ported_tests.rs::line_buffer_set_resets_cursor` |
| `line_buffer_insert_str` | `ported_tests.rs::line_buffer_insert_str` |
| `line_buffer_grid_navigation_preserves_terminal_column` | `ported_tests.rs::line_buffer_grid_navigation_preserves_terminal_column` |
| `line_buffer_grid_navigation_ignores_prompt_only_rows` | `ported_tests.rs::line_buffer_grid_navigation_ignores_prompt_only_rows` |
| `history_dedup_on_add` | `ported_tests.rs::history_dedup_on_add` |
| `history_prefix_search_recency` | `ported_tests.rs::history_prefix_search_recency` |
| `history_fuzzy_search_ordering` | `ported_tests.rs::history_fuzzy_search_ordering` |
| `history_fuzzy_case_insensitive` | `ported_tests.rs::history_fuzzy_case_insensitive` |
| `history_fuzzy_empty_query_returns_all` | `ported_tests.rs::history_fuzzy_empty_query_returns_all` |
| `history_fuzzy_match_positions_correct` | `ported_tests.rs::history_fuzzy_match_positions_correct` |
| `scoring_contiguous_beats_scattered` | `ported_tests.rs::scoring_contiguous_beats_scattered` |
| `scoring_word_boundary_preferred` | `ported_tests.rs::scoring_word_boundary_preferred` |
| `scoring_exact_command_name` | `ported_tests.rs::scoring_exact_command_name` |
| `scoring_path_components` | `ported_tests.rs::scoring_path_components` |
| `scoring_flag_matching` | `ported_tests.rs::scoring_flag_matching` |
| `scoring_pwd_bonus` | `ported_tests.rs::scoring_pwd_bonus` |
| `scoring_git_workflow` | `ported_tests.rs::scoring_git_workflow` |
| `scoring_docker_compose` | `ported_tests.rs::scoring_docker_compose` |
| `scoring_equal_quality_uses_recency` | `ported_tests.rs::scoring_equal_quality_uses_recency` |
| `scoring_long_path_not_destroyed_by_gaps` | `ported_tests.rs::scoring_long_path_not_destroyed_by_gaps` |
| `scoring_npm_scripts` | `ported_tests.rs::scoring_npm_scripts` |
| `optimal_alignment_finds_tight_window` | `ported_tests.rs::optimal_alignment_finds_tight_window` |
| `optimal_alignment_prefers_contiguous_suffix` | `ported_tests.rs::optimal_alignment_prefers_contiguous_suffix` |
| `optimal_alignment_single_char` | `ported_tests.rs::optimal_alignment_single_char` |
| `optimal_alignment_full_string_match` | `ported_tests.rs::optimal_alignment_full_string_match` |
| `optimal_alignment_real_world_path` | `ported_tests.rs::optimal_alignment_real_world_path` |
| `scoring_first_match_bonus` | `ported_tests.rs::scoring_first_match_bonus` |
| `scoring_first_match_with_optimal_alignment` | `ported_tests.rs::scoring_first_match_with_optimal_alignment` |
| `scoring_into_matches_search` | `ported_tests.rs::scoring_into_matches_search` |
| `scoring_into_respects_limit_without_changing_order` | `ported_tests.rs::scoring_into_respects_limit_without_changing_order` |
| `history_add_whitespace_only_ignored` | `ported_tests.rs::history_add_whitespace_only_ignored` |
| `grid_single_entry` | `ported_tests.rs::grid_single_entry` |
| `grid_fits_multiple_columns` | `ported_tests.rs::grid_fits_multiple_columns` |
| `grid_narrow_terminal_forces_single_column` | `ported_tests.rs::grid_narrow_terminal_forces_single_column` |
| `grid_empty_entries` | `ported_tests.rs::grid_empty_entries` |
| `completion_state_navigation_wraps` | `ported_tests.rs::completion_state_navigation_wraps` |
| `completion_state_left_right_wrap` | `ported_tests.rs::completion_state_left_right_wrap` |
| `comp_entry_display_name_dir_suffix` | `ported_tests.rs::comp_entry_display_name_dir_suffix` |
| `comp_entry_display_name_file` | `ported_tests.rs::comp_entry_display_name_file` |
| `complete_path_finds_files` | `ported_tests.rs::complete_path_finds_files` |
| `complete_path_dirs_only` | `ported_tests.rs::complete_path_dirs_only` |
| `complete_path_hidden_files` | `ported_tests.rs::complete_path_hidden_files` |
| `complete_path_nonexistent_dir` | `ported_tests.rs::complete_path_nonexistent_dir` |
| `prompt_shorten_deep_path` | `ported_tests.rs::prompt_shorten_deep_path` |
| `prompt_shorten_single_component` | `ported_tests.rs::prompt_shorten_single_component` |
| `prompt_shorten_empty_home` | `ported_tests.rs::prompt_shorten_empty_home` |
| `prompt_shorten_home_prefix_not_subdir` | `ported_tests.rs::prompt_shorten_home_prefix_not_subdir` |
| `input_modifier_combinations` | `ported_tests.rs::input_modifier_combinations` |
| `line_buffer_boundary_conditions` | `ported_tests.rs::line_buffer_boundary_conditions` |
| `history_subsequence_no_infinite_loop` | `ported_tests.rs::history_subsequence_no_infinite_loop` |
| `prompt_shorten_pwd_adversarial` | `ported_tests.rs::prompt_shorten_pwd_adversarial` |
| `config_load_all_paths` | `ported_tests.rs::config_load_all_paths` |
| `prompt_display_len_no_ansi` | `ported_tests.rs::prompt_display_len_no_ansi` |
| `prompt_display_len_with_ansi` | `ported_tests.rs::prompt_display_len_with_ansi` |
| `prompt_display_len_multiple_escapes` | `ported_tests.rs::prompt_display_len_multiple_escapes` |
| `prompt_display_len_utf8` | `ported_tests.rs::prompt_display_len_utf8` |
| `prompt_render_status_colors` | `ported_tests.rs::prompt_render_status_colors` |
| `prompt_render_dirty_indicator` | `ported_tests.rs::prompt_render_dirty_indicator` |
| `prompt_invalidate_git` | `ported_tests.rs::prompt_invalidate_git` |
| `prompt_default_impl` | `ported_tests.rs::prompt_default_impl` |
| `builtin_is_builtin_known` | `ported_tests.rs::builtin_is_builtin_known` |
| `builtin_is_builtin_unknown` | `ported_tests.rs::builtin_is_builtin_unknown` |
| `alias_set_get` | `ported_tests.rs::alias_set_get` |
| `alias_override` | `ported_tests.rs::alias_override` |
| `alias_iter` | `ported_tests.rs::alias_iter` |
| `alias_default_impl` | `ported_tests.rs::alias_default_impl` |
| `history_load_empty` | `ported_tests.rs::history_load_empty` |
| `history_add_keeps_newlines_verbatim` | `history/tests.rs::commands_are_stored_verbatim_and_display_on_one_row` |
| `history_add_dedup_preserves_order` | `ported_tests.rs::history_add_dedup_preserves_order` |
| `history_get_and_len` | `ported_tests.rs::history_get_and_len` |
| `history_from_entries_empty` | `ported_tests.rs::history_from_entries_empty` |
| `history_prefix_search_no_match` | `ported_tests.rs::history_prefix_search_no_match` |
| `history_fuzzy_match_positions_empty_query` | `ported_tests.rs::history_fuzzy_match_positions_empty_query` |
| `ls_list_dir_cwd` | `ported_tests.rs::ls_list_dir_cwd` |
| `ls_list_dir_file` | `ported_tests.rs::ls_list_dir_file` |
| `ls_list_dir_nonexistent` | `ported_tests.rs::ls_list_dir_nonexistent` |
| `ls_list_dir_tempdir` | `ported_tests.rs::ls_list_dir_tempdir` |
| `ls_list_dir_empty` | `ported_tests.rs::ls_list_dir_empty` |
| `ls_list_dir_symlink` | `ported_tests.rs::ls_list_dir_symlink` |
| `line_buffer_default_impl` | `ported_tests.rs::line_buffer_default_impl` |
| `line_buffer_display_len` | `ported_tests.rs::line_buffer_display_len` |
| `line_buffer_move_word_at_boundaries` | `ported_tests.rs::line_buffer_move_word_at_boundaries` |
| `line_buffer_kill_to_end_at_end` | `ported_tests.rs::line_buffer_kill_to_end_at_end` |
| `line_buffer_kill_to_start_at_start` | `ported_tests.rs::line_buffer_kill_to_start_at_start` |
| `line_buffer_kill_word_back_at_start` | `ported_tests.rs::line_buffer_kill_word_back_at_start` |
| `line_buffer_yank_empty_kill_ring` | `ported_tests.rs::line_buffer_yank_empty_kill_ring` |
| `line_buffer_word_movement_with_whitespace` | `ported_tests.rs::line_buffer_word_movement_with_whitespace` |
| `completion_state_selected_entry` | `ported_tests.rs::completion_state_selected_entry` |
| `completion_state_selected_entry_out_of_bounds` | `ported_tests.rs::completion_state_selected_entry_out_of_bounds` |
| `completion_move_with_zero_rows` | `ported_tests.rs::completion_move_with_zero_rows` |
| `completion_navigation_single_entry` | `ported_tests.rs::completion_navigation_single_entry` |
| `completion_move_right_wraps_to_first_col` | `ported_tests.rs::completion_move_right_wraps_to_first_col` |
| `completion_move_left_wraps_to_last_col` | `ported_tests.rs::completion_move_left_wraps_to_last_col` |
| `comp_entry_display_link` | `ported_tests.rs::comp_entry_display_link` |
| `comp_entry_display_exec` | `ported_tests.rs::comp_entry_display_exec` |
| `input_key_event_constructors` | `ported_tests.rs::input_key_event_constructors` |
| `input_modifiers_none` | `ported_tests.rs::input_modifiers_none` |
| `input_modifiers_default` | `ported_tests.rs::input_modifiers_default` |
| `input_modifier_from_param_zero` | `ported_tests.rs::input_modifier_from_param_zero` |
| `prompt_shorten_pwd_single_char_components` | `ported_tests.rs::prompt_shorten_pwd_single_char_components` |
| `prompt_shorten_pwd_dotfiles_in_middle` | `ported_tests.rs::prompt_shorten_pwd_dotfiles_in_middle` |
| `history_file_io` | `ported_tests.rs::history_file_io` |
| `history_migrates_legacy_once_and_retires_originals` | `history/tests.rs::migrates_cache_and_text_and_retires_originals` |
| `history_corrupt_legacy_cache_reports_migration_error` | `history/tests.rs::unsupported_or_corrupt_log_headers_are_preserved` |
| `history_concurrent_writers_preserve_all_appends_and_compaction` | `history/tests.rs::simultaneous_open_migrates_once_and_appends_without_lost_occurrences` |
| `history_ranking_uses_recorded_directory_after_match_quality` | `history/tests.rs::match_quality_precedes_exact_directory_and_frequency` |
| `history_usage_ranks_repeated_commands_without_duplicate_results` | `history/tests.rs::recent_usage_beats_stale_popularity_but_frequency_matters_when_recent` |
| `history_match_positions_count_unicode_characters` | `history/tests.rs::unicode_scalar_lowercasing_and_long_subsequence_queries_keep_positions` |
| `history_external_repeats_preserve_session_navigation_order` | `history/tests.rs::session_recall_skips_replaced_occurrences` |
| `history_append_reports_storage_failure` | `history/tests.rs::append_failure_does_not_update_memory_or_usage` |
| `history_reset_invalidates_existing_shell_cache` | `ported_tests.rs::history_reset_invalidates_existing_shell_cache` |
| `history_up_arrow_uses_session_start_boundary` | `ported_tests.rs::history_up_arrow_uses_session_start_boundary` |
| `history_ctrl_r_searches_later_global_entries` | `ported_tests.rs::history_ctrl_r_includes_later_global_entries` |
| `ls_list_dir_with_executable` | `ported_tests.rs::ls_list_dir_with_executable` |
| `ls_list_dir_with_symlink_to_dir` | `ported_tests.rs::ls_list_dir_with_symlink_to_dir` |
| `ls_list_symlink_to_dir_as_file` | `ported_tests.rs::ls_list_symlink_to_dir_as_file` |
| `ls_list_dir_sorts_case_insensitive` | `ported_tests.rs::ls_list_dir_sorts_case_insensitive` |
| `prompt_git_branch_detects_repo` | `ported_tests.rs::prompt_git_branch_detects_repo` |
| `prompt_git_branch_detached_head` | `ported_tests.rs::prompt_git_branch_detached_head` |
| `prompt_git_branch_not_shown_outside_repo` | `ported_tests.rs::prompt_git_branch_no_repo` |
| `prompt_git_invalidate_clears_cache` | `ported_tests.rs::prompt_git_invalidate_clears_cache` |
| `prompt_git_bare_ref` | `ported_tests.rs::prompt_git_bare_ref` |
| `prompt_git_worktree_gitdir_file` | `ported_tests.rs::prompt_git_worktree_gitdir_file` |
| `complete_path_symlink` | `ported_tests.rs::complete_path_symlink` |
| `builtin_registry_drives_names_and_execution_classes` | `ported_tests.rs::builtin_is_builtin_known`, `builtin_is_builtin_unknown` |
| `every_builtin_has_help` | `crates/xshi/src/interactive/builtin.rs` help table; no parity subject |
| `fdu_help_and_rejections_and_summary` | Excluded: S4 native fs / S6 fdu lane |
| `rsync_help_and_option_rejections` | Excluded: S4 native fs / S6 fdu lane |
| `rsync_operand_mapping_and_trailing_slash_invariance` | Excluded: S4 native fs / S6 fdu lane |
| `rsync_incremental_quick_check_skip` | Excluded: S4 native fs / S6 fdu lane |
| `rsync_delete_and_dry_run` | Excluded: S4 native fs / S6 fdu lane |
| `rsync_metadata_and_permissions_preserved` | Excluded: S4 native fs / S6 fdu lane |
