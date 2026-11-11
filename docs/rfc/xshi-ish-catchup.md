# RFC: xshi ↔ ish catch-up inventory

Status: planning draft. Base revision `ff5726f6c4f84c34c5e7eb5ccb0d79cc9234f73c`.
Reference shell: `/home/josh/d/ish` at `6d73dd8aedf87119374ca70c1fcb36ab52cac8f5`
(2026-11-09). Contract: [SPEC-INTERACTIVE](../SPEC-INTERACTIVE.md).
No code is changed by this document.

## 0. Provenance and method

| Question | Answer | Evidence |
|---|---|---|
| Is the ish revision xshi was ported from recorded? | No. `b1f6c6f21` is titled only `ish`; its body is empty. | `git log -1 --format='%B' b1f6c6f21` (output: `ish`) |
| What is recorded? | The port number and its test strategy; "checked against the built `ish`". | `git show b1f6c6f21:plan-ish-parity.md:1-60` |
| Which ish tests were adapted? | `crates/xshi/src/interactive/ported_tests.rs` names ish's `tests/integration.rs`, not `tests/pty.rs`. | `crates/xshi/src/interactive/ported_tests.rs:1` |
| Why no PTY parity today? | The replay driver is absent; 193 goldens per OS remain as reference only. | `docs/SPEC-INTERACTIVE.md:20-24`, `docs/TESTING.md:815-817` |
| What harness did ish use? | `laputa-ptytest` 0.2; this workspace pins 0.1.1 (parity lane owns that upgrade). | `/home/josh/d/ish/Cargo.toml:38`, `Cargo.toml:137` |
| What harness does XSH itself use? | `laputa-ptytest` through `tests/core_compat_extra/pty.rs`, including btop. | `tests/core_compat_extra/pty.rs:1-16`, `dev/gates.xsh:580` |
| Scope excluded from (A) | `tests/integration.rs` and `benches/fs/fs_bench.rs` do not import ptytest; the bench is not a test. | `rg -l ptytest --type rust` result set |

Consequence: "covered" below means an executable xshi test exists. `covered†`
means only a retained golden scenario exists (recorded from ish, compared before
the driver was removed); it is reference evidence, not an executable test. This
distinction is the main uncertainty the restored parity driver removes.

Classification legend, applied per test:

| Class | Meaning |
|---|---|
| covered | xshi implements the behavior and an executable xshi test pins it. |
| covered† | xshi implements it; a same-named golden exists; no executable test. |
| untested | xshi implements it; no xshi test and no golden. |
| missing | No xshi implementation, or the implemented behavior is structurally absent. |
| deviation | Deliberate difference documented in SPEC-INTERACTIVE §1, §8, or §13, or in the port's design notes (`plan-ish-parity.md`, commit `b1f6c6f21`). |

Fixture shorthand `FX:NAME` = `tests/fixtures/interactive-parity/{linux,macos}/NAME.txt`.
All `tests/pty.rs:L` citations are `/home/josh/d/ish/tests/pty.rs:L`.

Citation convention: paths beginning `crates/ish-fs/`, `crates/fdu-`,
`tests/pty.rs`, `tests/fdu_`, `tests/external_signals.rs`,
`tests/history_search_paste.rs`, `src/builtin.rs`, `src/shell.rs`,
`src/editor.rs`, `src/ls.rs`, `src/denv.rs`, `src/external.rs`, `src/fdu*.rs`,
`src/fs_command.rs`, `src/rmrf.rs`, `src/sync_command.rs`, `src/history.rs`,
and `src/diagnostics.rs` are ish paths under `/home/josh/d/ish`; every other
path is this repository. Bare `app.rs`, `repl.rs`, `render.rs`, `line.rs`,
`input.rs`, `complete.rs`, `alias.rs`, `config.rs`, `prompt.rs`, `listing.rs`,
`session.rs`, `signal.rs`, `term.rs`, `z.rs` denote
`crates/xshi/src/interactive/`; bare `prims.rs`, `control.rs`, `xattr.rs`
denote `src/modules/fs/`; `fs.rs` denotes `src/modules/fs.rs`; `mutate.rs`
denotes `crates/xsh-root/src/`; bare `linux.rs` and `macos.rs` in §3 denote
`crates/xsh-root/src/`; `btop_*.xsh` and `tui_*.xsh` denote `core/lib/`.
A leading `:L` inside a cell continues the file named earlier in that cell.
In §3, ish-fs module paths (`platform/...`, `engine/...`, `sync.rs`,
`policy.rs`, `roots.rs`) are relative to `/home/josh/d/ish/crates/ish-fs/src`,
and `fdu-*/src/...` paths are relative to `/home/josh/d/ish/crates`.

## 1. (A) ish laputa-ptytest inventory

### 1.1 `tests/pty.rs` (202 tests)

#### Native file commands and fdu

| Test (line) | Class | xshi evidence |
|---|---|---|
| `native_fs_copy_and_move_lists_keep_successes_and_continue_after_failures` (777) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` has no `cp`/`mv` |
| `native_fs_exact_targets_and_into_directories_preserve_real_bytes` (805) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_multi_source_into_trailing_directory` (835) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_redirections_prevent_failed_transfers_and_capture_stderr` (868) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_parent_placement_accepts_command_alias_expanded_name_and_argument_substitution` (894) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_standalone_negation_runs_in_the_shell_process` (910) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_static_dynamic_and_nested_fork_contexts_refuse_before_redirections` (950) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_quick_no_progress_copy_stays_quiet` (980) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_ctrl_c_during_copy_returns_130_to_a_usable_prompt` (992) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_ctrl_z_is_ignored_during_copy_and_external_job_control_still_works` (1015) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_sigterm_and_sighup_cancel_and_restore_the_terminal` (1055) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_broken_stderr_cancels_bounded_work_and_keeps_the_prompt_usable` (1085) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_fs_rsync_interactive` (1824) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` (no `rsync`) |
| `native_fs_fdu_interactive` (1851) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` (no `fdu`) |
| `rmrf_removes_a_directory` (2950) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` (no `rmrf`) |
| `native_handlers_refuse_child_execution_before_target_redirections` (4358) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `native_copy_uses_current_store_and_prefix_tmpdir_values` (5084) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |

#### Prompt, basic commands, line editing

| Test (line) | Class | xshi evidence |
|---|---|---|
| `prompt_appears_on_startup` (1106) | covered† | `crates/xshi/src/interactive/prompt.rs:65`; FX:`prompt_appears_on_startup` |
| `echo_command` (1114) | covered | `crates/xshi/src/interactive/app.rs:2978-3000` internal echo; `app.rs:3414` |
| `prompt_does_not_share_a_line_with_unterminated_external_output` (1125) | covered† | `crates/xshi/src/interactive/repl.rs:905`; FX:`prompt_does_not_share_a_line_with_unterminated_external_output` |
| `broken_interpreter_reports_bad_interpreter` (1145) | covered† | `crates/xshi/src/interactive/app.rs:1920`; FX:`broken_interpreter_reports_bad_interpreter` |
| `pwd_builtin` (1164) | covered | `crates/xshi/src/interactive/app.rs:2973` (internal pwd) |
| `cd_and_pwd` (1176) | covered† | `crates/xshi/src/interactive/app.rs:627`; FX:`cd_and_pwd` |
| `exit_with_ctrl_d` (1186) | covered† | `crates/xshi/src/interactive/repl.rs:971`; FX:`exit_with_ctrl_d` |
| `exit_command` (1196) | covered† | `crates/xshi/src/interactive/app.rs:295`; FX:`exit_command` |
| `ctrl_c_cancels_input` (1204) | covered | `crates/xshi/src/interactive/repl.rs:970`; FX:`ctrl_c_cancels_input` |
| `sigint_during_editing_cancels_the_line` (1219) | covered | `crates/xshi/src/interactive/signal.rs:43`; `crates/xshi/tests/terminal_storm/mod.rs:466` |
| `line_editing_backspace` (1243) | covered | `crates/xshi/src/interactive/line.rs:84`; FX:`line_editing_backspace` |
| `line_editing_up_down_navigate_wrapped_input` (1257) | covered | `crates/xshi/src/interactive/line.rs:142-163`; `render.rs:1185-1276` |
| `line_editing_ctrl_u` (1279) | covered | `crates/xshi/src/interactive/line.rs:326`; FX:`line_editing_ctrl_u` |
| `line_editing_ctrl_w` (1305) | covered | `crates/xshi/src/interactive/line.rs:335`; FX:`line_editing_ctrl_w` |
| `line_editing_ctrl_delete` (1323) | covered | `crates/xshi/src/interactive/input.rs:601-618`; FX:`line_editing_ctrl_delete` |
| `line_editing_ctrl_k_and_ctrl_y` (1344) | covered | `crates/xshi/src/interactive/line.rs:318`, `:372`; FX:`line_editing_ctrl_k_and_ctrl_y` |

#### Shell language and builtins

| Test (line) | Class | xshi evidence |
|---|---|---|
| `pipeline` (1365) | covered† | `crates/xshi/src/interactive/app.rs:1079`; FX:`pipeline` |
| `and_or_list` (1373) | covered† | `crates/xshi/src/interactive/app.rs:937`; FX:`and_or_list` |
| `or_list` (1381) | covered† | `crates/xshi/src/interactive/app.rs:937`; FX:`or_list` |
| `redirect_output` (1392) | covered† | `crates/xshi/src/interactive/app.rs:3081`; FX:`redirect_output` |
| `l_lists_files` (1405) | covered† | `crates/xshi/src/interactive/listing.rs:5`; FX:`l_lists_files` |
| `l_wraps_long_names_at_narrow_terminal_width` (1422) | missing | `crates/xshi/src/interactive/listing.rs:5-63` has no width/wrap input |
| `set_and_echo_var` (1450) | covered† | `crates/xshi/src/interactive/app.rs:531`; FX:`set_and_echo_var` |
| `exported_var_reaches_external_commands` (1462) | covered | `crates/xshi/src/interactive/app.rs:1585`; FX:`exported_var_reaches_external_commands` |
| `set_var_reaches_external_commands` (1474) | covered† | `crates/xshi/src/interactive/app.rs:1611`; FX:`set_var_reaches_external_commands` |
| `set_var_joins_multiple_value_words` (1486) | covered | `crates/xshi/src/interactive/app.rs:551`; FX:`set_var_joins_multiple_value_words` |
| `set_no_args_lists_env_vars` (1498) | covered† | `crates/xshi/src/interactive/app.rs:531-542`; FX:`set_no_args_lists_env_vars` |
| `set_option_and_positional_forms_are_refused_without_target_effects` (1506) | deviation | SPEC-INTERACTIVE:47 (options accepted and ignored); `app.rs:544-550` |
| `exec_and_trap_refusals_preserve_the_shell_and_target_files` (1534) | deviation | SPEC-INTERACTIVE:48-50 (`exec` exists, `trap` does not); `app.rs:304` |
| `builtin_lookup_completion_and_explicit_paths_share_the_policy` (1568) | covered | `crates/xshi/src/interactive/ported_tests.rs:1018`; `crates/xshi/src/interactive/builtin.rs:29-41` |
| `approved_builtin_classes_remain_available` (1610) | deviation | SPEC-INTERACTIVE:48-53 (`source`, `eval`, `exec`, `bg` are additions) |
| `unset_removes_var_from_children` (1624) | covered† | `crates/xshi/src/interactive/session.rs:230`; FX:`unset_variable` |
| `unset_removes_os_env_var_set_by_set` (1637) | covered† | `crates/xshi/src/interactive/session.rs:230`; FX:`unset_removes_os_env_var_set_by_set` |
| `unset_in_same_line_not_inherited_by_children` (1650) | covered† | `crates/xshi/src/interactive/session.rs:230`; FX:`unset_in_same_line_not_inherited_by_children` |
| `unset_removes_ambient_environment_from_children` (1661) | covered | `crates/xshi/src/interactive/session.rs:230` |
| `unset_removes_ambient_environment_in_pipeline_children` (1690) | covered | `crates/xshi/src/interactive/session.rs:230`; `app.rs:1109` |
| `compound_list_export_reaches_child` (1708) | covered | `crates/xshi/src/interactive/app.rs:1585`; FX:`compound_list_export_reaches_child` |
| `store_home_drives_interactive_cd` (1718) | covered† | `crates/xshi/src/interactive/app.rs:466`; FX:`store_home_drives_interactive_cd` |
| `unset_oldpwd_blocks_interactive_cd_minus` (1726) | covered† | `crates/xshi/src/interactive/app.rs:466`; FX:`unset_oldpwd_blocks_interactive_cd_minus` |
| `prefix_assignment_reaches_child_but_does_not_persist` (1738) | covered | `crates/xshi/src/interactive/app.rs:3544`; FX:`prefix_assignment_reaches_child_but_does_not_persist` |
| `set_path_affects_command_lookup` (1753) | covered† | `crates/xshi/src/interactive/path.rs:1`; FX:`set_path_affects_command_lookup` |
| `which_reflects_exported_path` (1765) | covered† | `crates/xshi/src/interactive/builtin.rs:68`; FX:`which_reflects_exported_path` |
| `which_uses_store_path_not_os_env` (1777) | covered | `crates/xshi/src/interactive/builtin.rs:68`; `app.rs:1619-1625` |
| `command_lookup_classifies_ish_commands_with_empty_path` (1797) | covered† | `crates/xshi/src/interactive/builtin.rs:45-75`; FX:`command_lookup_classifies_ish_commands_with_empty_path` |
| `tilde_expansion` (1861) | covered† | `crates/xshi/src/interactive/app.rs:2555`; FX:`tilde_expansion` |

#### History search and pager

| Test (line) | Class | xshi evidence |
|---|---|---|
| `history_up_arrow` (1873) | covered† | `crates/xshi/src/interactive/repl.rs:990`; FX:`history_up_arrow` |
| `history_up_narrow_repaint_clears_wrapped_rows` (1903) | covered | `crates/xshi/src/interactive/render.rs:1285`; FX:`history_up_narrow_repaint_clears_wrapped_rows` |
| `history_ctrl_r_search` (1943) | covered | `crates/xshi/src/interactive/repl.rs:1625`; `crates/xshi/src/interactive/history/tests.rs:131-165`; FX:`history_ctrl_r_search` |
| `history_ctrl_r_includes_later_global_entries` (1964) | deviation | SPEC-INTERACTIVE:265-267 keeps peer entries out of Ctrl-R; FX:`history_ctrl_r_ignores_later_global_entries` |
| `history_ctrl_r_escape_cancels` (1991) | covered† | `crates/xshi/src/interactive/repl.rs:1640`; FX:`history_ctrl_r_escape_cancels` |
| `history_ctrl_r_narrow_repaint_does_not_stack_rows` (2012) | covered | `crates/xshi/src/interactive/render.rs:1285`; FX:`history_ctrl_r_narrow_repaint_does_not_stack_rows` |
| `history_search_selection_preserves_cursor_after_typing` (2042) | covered† | `crates/xshi/src/interactive/repl.rs:1656-1671`; FX:`history_search_selection_preserves_cursor_after_typing` |
| `history_accept_reanchors_prompt_before_typing` (2132) | covered† | `crates/xshi/src/interactive/repl.rs:1643`; FX:`history_accept_reanchors_prompt_before_typing` |
| `history_ctrl_r_near_bottom_keeps_pager_stable` (2208) | covered | `crates/xshi/src/interactive/render.rs:1313`; FX:`history_ctrl_r_near_bottom_keeps_pager_stable` |
| `history_ctrl_r_scrolls_when_selection_passes_last_visible_entry` (2244) | covered† | `crates/xshi/src/interactive/repl.rs:1648-1655`; FX:`history_ctrl_r_scrolls_when_selection_passes_last_visible_entry` |
| `history_ctrl_r_near_bottom_query_edits_do_not_stack_headers` (2276) | covered | `crates/xshi/src/interactive/render.rs:1278`; FX:`history_ctrl_r_near_bottom_query_edits_do_not_stack_headers` |

#### Completion

| Test (line) | Class | xshi evidence |
|---|---|---|
| `tab_completion_files` (2407) | covered | `crates/xshi/src/interactive/complete.rs:1196-1266`; FX:`tab_completion_files` |
| `tab_completion_shows_grid` (2425) | covered | `crates/xshi/src/interactive/complete.rs:1206`; `render.rs:1370` |
| `tab_completion_first_tab_has_no_selection_second_tab_selects_first` (2443) | covered | `crates/xshi/src/interactive/ported_tests.rs:606` |
| `tab_completion_narrow_repaint_does_not_stack_rows` (2474) | covered† | `crates/xshi/src/interactive/render.rs:1370`; FX:`tab_completion_narrow_repaint_does_not_stack_rows` |
| `tab_completion_directory` (2509) | covered | `crates/xshi/src/interactive/complete.rs:1196`; FX:`tab_completion_directory` |
| `tab_completion_escape_restores_typed_prefix` (2523) | covered† | `crates/xshi/src/interactive/repl.rs:1554`; FX:`tab_completion_escape_restores_typed_prefix` |
| `tab_completion_narrowing_does_not_autoaccept` (2545) | covered† | `crates/xshi/src/interactive/complete.rs:1218`; FX:`tab_completion_narrowing_does_not_autoaccept` |
| `tab_completion_with_wide_dir_name_restores_prompt_cursor` (2569) | covered† | `crates/xshi/src/interactive/render.rs:1185-1276`; FX:`tab_completion_with_wide_dir_name_restores_prompt_cursor` |
| `completion_resize_rerenders_grid` (2687) | covered† | `crates/xshi/src/interactive/repl.rs:1554`; FX:`completion_resize_rerenders_grid` |
| `normal_resize_reanchors_wrapped_prompt` (2714) | covered | `crates/xshi/src/interactive/render.rs:1140`; FX:`normal_resize_reanchors_wrapped_prompt` |
| `history_resize_rerenders_pager` (2754) | covered | `crates/xshi/src/interactive/render.rs:1285`; FX:`history_resize_rerenders_pager` |

#### Aliases, lookup, job basics

| Test (line) | Class | xshi evidence |
|---|---|---|
| `alias_expansion` (2778) | covered | `crates/xshi/src/interactive/alias.rs:140-176`; FX:`alias_expansion` |
| `alias_self_referencing_no_reexpand` (2790) | covered | `crates/xshi/src/interactive/alias.rs:153` |
| `alias_self_referencing_from_config` (2826) | covered | `crates/xshi/src/interactive/config.rs:280`; FX:`alias_self_referencing_from_config` |
| `alias_self_referencing_exec` (2869) | covered | `crates/xshi/src/interactive/alias.rs:153`; `app.rs:304` |
| `alias_list` (2887) | covered† | `crates/xshi/src/interactive/app.rs:557`; FX:`alias_list` |
| `alias_with_command_substitution` (2896) | covered | `crates/xshi/src/interactive/alias.rs:164` |
| `alias_preserves_quoted_word` (2917) | covered | `crates/xshi/src/interactive/alias.rs:164`; FX:`alias_preserves_quoted_word` |
| `which_builtin` (2929) | covered† | `crates/xshi/src/interactive/builtin.rs:63`; FX:`which_builtin` |
| `which_external` (2962) | covered† | `crates/xshi/src/interactive/builtin.rs:68`; FX:`which_external` |
| `which_alias` (2973) | covered† | `crates/xshi/src/interactive/builtin.rs:58`; FX:`which_alias` |
| `which_same_name_alias_shows_underlying_binary` (2985) | missing | `crates/xshi/src/interactive/builtin.rs:58-61` returns the alias line only |
| `error_status_colors_prompt` (3002) | covered† | `crates/xshi/src/interactive/prompt.rs:65`; FX:`error_status_colors_prompt` |
| `nonexistent_command` (3017) | covered† | `crates/xshi/src/interactive/app.rs:1920`; FX:`nonexistent_command` |
| `script_mode_refused` (3028) | deviation | SPEC-INTERACTIVE:43 (`-c COMMAND` is one command line); `crates/xshi/tests/cli.rs:48` |
| `source_is_refused` (3044) | deviation | SPEC-INTERACTIVE:49 (`source`/`.` implemented); `app.rs:1584` |
| `expanded_targets_obey_ish_execution_policy` (3055) | covered† | `crates/xshi/src/interactive/app.rs:249`; FX:`expanded_targets_obey_ish_execution_policy` |
| `ctrl_l_clears_screen` (3175) | covered† | `crates/xshi/src/interactive/repl.rs:984`; FX:`ctrl_l_clears_screen` |
| `multiline_continuation` (3200) | covered† | `crates/xshi/src/interactive/repl.rs:1059-1063`; FX:`multiline_continuation` |
| `multiline_preserves_unicode_quotes_and_backslash_newlines` (3218) | untested | `crates/xshi/src/interactive/line.rs:253` |
| `interactive_final_history_replay_restores_the_recorded_directory` (3241) | untested | `crates/xshi/src/interactive/history/store.rs:246` records cwd; no replay test |
| `multiline_completion_on_continuation_line` (3259) | covered† | `crates/xshi/src/interactive/repl.rs:1053`; FX:`multiline_completion_on_continuation_line` |
| `dir_picker_narrow_repaint_does_not_stack_rows` (3278) | covered | `crates/xshi/src/interactive/render.rs:1362`; FX:`dir_picker_narrow_repaint_does_not_stack_rows` |
| `config_file_loaded` (3310) | covered | `crates/xshi/src/interactive/config.rs:280`; FX:`config_file_loaded` |
| `prompt_shows_cwd` (3327) | covered | `crates/xshi/src/interactive/prompt.rs:187`; FX:`prompt_shows_cwd` |
| `cd_minus_goes_back` (3341) | covered† | `crates/xshi/src/interactive/app.rs:466`; FX:`cd_minus_goes_back` |
| `cd_tilde_subdir` (3352) | covered† | `crates/xshi/src/interactive/app.rs:499`; FX:`cd_tilde_subdir` |
| `implicit_cd_quoted_path` (3366) | covered† | `crates/xshi/src/interactive/app.rs:660`; FX:`implicit_cd_quoted_path` |
| `path_binary_wins_over_implicit_cd` (3378) | covered† | `crates/xshi/src/interactive/app.rs:660`; FX:`path_binary_wins_over_implicit_cd` |
| `l_tilde_subdir` (3394) | covered† | `crates/xshi/src/interactive/listing.rs:5`; FX:`l_tilde_subdir` |
| `unset_variable` (3407) | covered† | `crates/xshi/src/interactive/session.rs:230`; FX:`unset_variable` |
| `glob_expansion` (3418) | covered† | `crates/xshi/src/interactive/shell/glob.rs:1`; FX:`glob_expansion` |
| `l_glob_expansion` (3430) | covered† | `crates/xshi/src/interactive/shell/glob.rs:1`; FX:`l_glob_expansion` |
| `l_hides_dotfiles` (3456) | missing | `crates/xshi/src/interactive/listing.rs:121-124` only skips `.`/`..`, so `l` shows dotfiles |
| `ll_shows_dotfiles` (3470) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` has no `ll` |
| `quoted_string_preserves_spaces` (3484) | covered† | `crates/xshi/src/interactive/shell/lex.rs:128`; FX:`quoted_string_preserves_spaces` |
| `single_quotes_no_expansion` (3495) | covered† | `crates/xshi/src/interactive/shell/lex.rs:128`; FX:`single_quotes_no_expansion` |

#### History storage, denv, jobs and terminal

| Test (line) | Class | xshi evidence |
|---|---|---|
| `history_persisted_across_commands` (3504) | covered | `crates/xshi/src/interactive/history/tests.rs:205`; FX:`history_persisted_across_commands` |
| `history_survives_abrupt_termination_and_restart` (3519) | covered | `crates/xshi/src/interactive/history/tests.rs:499` |
| `history_recovers_partial_append_after_abrupt_termination` (3542) | covered | `crates/xshi/src/interactive/history/tests.rs:617` |
| `history_shared_shell_processes_keep_all_commands` (3573) | covered | `crates/xshi/src/interactive/history/tests.rs:1194` |
| `history_write_failure_is_visible_and_command_still_runs` (3616) | covered | `crates/xshi/src/interactive/history/tests.rs:1054` |
| `history_read_only_commands_preserve_another_writer` (3638) | covered | `crates/xshi/src/interactive/history/tests.rs:639` |
| `history_reset_reopens_storage_in_existing_shell_processes` (3655) | covered | `crates/xshi/src/interactive/history/tests.rs:1251`, `:1270` |
| `history_readers_keep_startup_log_after_home_changes` (3680) | covered | `crates/xshi/src/interactive/history/tests.rs:452` |
| `history_reset_refuses_pipeline_and_command_substitution` (3703) | untested | `crates/xshi/src/interactive/app.rs:1652` |
| `history_up_ignores_later_global_entries` (3720) | deviation | SPEC-INTERACTIVE:265-267; FX:`history_up_ignores_later_global_entries` |
| `history_help` (3745) | covered† | `crates/xshi/src/interactive/app.rs:1627`; FX:`history_help` |
| `history_autosuggest_ignores_later_global_entries` (3755) | deviation | SPEC-INTERACTIVE:265-267; FX:`history_autosuggest_ignores_later_global_entries` |
| `true_and_false_builtins` (3781) | covered | `crates/xshi/src/interactive/app.rs:3414`; FX:`true_and_false_builtins` |
| `denv_loads_allowed_envrc_on_cd` (3862) | covered | `crates/xshi/src/interactive/denv.rs:1072`; FX:`denv_loads_allowed_envrc_on_cd` |
| `denv_loads_dotenv_on_cd_only_after_allow` (3876) | deviation | SPEC-INTERACTIVE:406-410 (`.env` needs no trust); FX:`denv_loads_dotenv_on_cd_without_allow` |
| `denv_unloads_on_leave` (3897) | covered | `crates/xshi/src/interactive/denv.rs:1072`; FX:`denv_unloads_on_leave` |
| `denv_exit_command_inside_project_removes_active_record` (3924) | deviation | xshi keeps restore state in memory (no `active_<pid>` file): `crates/xshi/src/interactive/denv.rs:361`, `crates/xshi/src/interactive/denv.rs:1072` |
| `denv_ctrl_d_inside_project_removes_active_record` (3940) | deviation | same in-memory state: `crates/xshi/src/interactive/denv.rs:361` |
| `denv_allow_applies_env` (3955) | covered | `crates/xshi/src/interactive/denv.rs:1093`; FX:`denv_allow_applies_env` |
| `denv_deny_removes_env_and_marks_dirty` (3976) | covered | `crates/xshi/src/interactive/denv.rs:1093`; FX:`denv_deny_removes_env_and_marks_dirty` |
| `denv_startup_loads_allowed_dotenv_in_initial_cwd` (3991) | covered | `crates/xshi/src/interactive/denv.rs:1072`; FX:`denv_startup_loads_dotenv_in_initial_cwd` |
| `denv_startup_loads_allowed_envrc_in_initial_cwd` (4009) | covered | `crates/xshi/src/interactive/denv.rs:1072`; FX:`denv_startup_loads_allowed_envrc_in_initial_cwd` |
| `denv_dotenv_overrides_envrc` (4023) | covered | `crates/xshi/src/interactive/denv.rs:1072`; FX:`denv_dotenv_overrides_envrc` |
| `denv_reload_after_reallow_picks_up_envrc_edit` (4047) | covered | `crates/xshi/src/interactive/denv.rs:1052`; FX:`denv_reload_after_reallow_picks_up_envrc_edit` |
| `denv_edit_envrc_invalidates_trust` (4070) | covered | `crates/xshi/src/interactive/denv.rs:1052`; FX:`denv_edit_envrc_invalidates_trust` |
| `denv_restores_preexisting_var_on_leave` (4096) | covered | `crates/xshi/src/interactive/denv.rs:1072`; FX:`denv_restores_preexisting_var_on_leave` |
| `denv_path_add_relative_dir` (4122) | covered | `crates/xshi/src/interactive/denv.rs:1072`; FX:`denv_path_add_relative_dir` |
| `denv_dotenv_helper_rejects_the_entire_candidate` (4144) | untested | `crates/xshi/src/interactive/denv_stdlib.sh:179` (`dotenv`) |
| `denv_automatic_dotenv_loads_without_a_helper` (4163) | covered | `crates/xshi/src/interactive/denv.rs:1072` |
| `denv_automatic_dotenv_keeps_shell_text_as_data_and_revokes_edits` (4177) | covered | `crates/xshi/src/interactive/denv.rs:913` (dotenv parser) |
| `denv_rejects_parent_helpers_after_parent_revocation_or_edit` (4202) | untested | `crates/xshi/src/interactive/denv_stdlib.sh:160` (`source_up`); trust check `crates/xshi/src/interactive/denv.rs:1052` |
| `denv_rejects_changed_shared_file_helpers` (4237) | untested | `crates/xshi/src/interactive/denv_stdlib.sh:153` (`source_env`); trust check `crates/xshi/src/interactive/denv.rs:1052` |
| `denv_allow_without_any_env_file_fails` (4257) | untested | `crates/xshi/src/interactive/app.rs:1692` |
| `ctrl_c_interrupts_running_child` (4268) | covered† | `crates/xshi/src/interactive/app.rs:1560`; FX:`ctrl_c_interrupts_running_child` |
| `ctrl_d_with_stopped_job_warns_then_exits` (4281) | covered† | `crates/xshi/src/interactive/repl.rs:1879` (SPEC §12); FX:`ctrl_d_with_a_suspended_job_warns_before_forcing` |
| `exit_twice_with_stopped_job_warns_then_exits` (4298) | deviation | SPEC-INTERACTIVE:38-41 (second `exit` forces, then SIGTERM+SIGCONT); `app.rs:615-621` |
| `confirmed_exit_with_stopped_job_keeps_the_requested_status` (4318) | deviation | SPEC-INTERACTIVE:38-41 |
| `pipeline_suspend_and_resume_retains_every_stage` (4337) | untested | `crates/xshi/src/interactive/app.rs:1560` (one job slot) |
| `external_input_redirections_preserve_foreground_terminal_ownership` (4378) | untested | `crates/xshi/src/interactive/app.rs:2230` |
| `ordinary_exit_148_does_not_create_a_suspended_job` (4388) | untested | `crates/xshi/src/interactive/app.rs:1560` |
| `occupied_job_slot_restores_new_job_terminal_modes_before_continuing` (4396) | untested | `crates/xshi/src/interactive/app.rs:1899` |
| `stopped_pipeline_discards_the_remaining_list` (4420) | deviation | SPEC-INTERACTIVE:36-38 (`fg` continues the rest of the list) |
| `job_suspend_and_resume` (4438) | covered† | `crates/xshi/src/interactive/app.rs:1560`; FX:`job_suspend_and_resume` |
| `pipeline_suspend_and_resume_keeps_job_owned` (4478) | untested | `crates/xshi/src/interactive/app.rs:1560` |
| `killed_child_in_raw_mode_does_not_leak_into_the_next_command` (4550) | covered | `crates/xshi/tests/terminal_storm/mod.rs:466`; `crates/xshi/src/interactive/term.rs:64` |
| `stopped_child_in_raw_mode_restores_baseline_then_fg_restores_its_own_mode` (4559) | untested | `crates/xshi/src/interactive/app.rs:1750` |
| `foreground_terminal_mode_changes_on_stdout_apply_to_stdin` (4571) | untested | `crates/xshi/src/interactive/app.rs:1899` |
| `git_log_pages_on_a_converged_terminal` (4584) | untested | `crates/xshi/src/interactive/app.rs:1920` |
| `single_command_sees_one_terminal_for_all_standard_streams` (4623) | untested | `crates/xshi/src/interactive/app.rs:1920` |
| `output_beyond_the_capture_limit_still_reaches_the_terminal_and_the_shell_continues` (4643) | untested | `crates/xshi/src/interactive/app.rs:1166` |
| `bracketed_paste_over_limit_rejected` (4655) | covered | `crates/xshi/src/interactive/input.rs:637`; FX:`bracketed_paste_over_limit_rejected` |
| `bracketed_paste_large_markdown_rejected` (4689) | covered | `crates/xshi/src/interactive/input.rs:637` |
| `bracketed_paste_under_limit_accepted` (4720) | covered | `crates/xshi/src/interactive/input.rs:619`; FX:`bracketed_paste_under_limit_accepted` |
| `bracketed_paste_exactly_at_limit_accepted` (4744) | covered | `crates/xshi/src/interactive/input.rs:637`; FX:`bracketed_paste_exactly_at_limit_accepted` |
| `interactive_final_typeahead_after_enter_survives_the_next_prompt` (4768) | untested | `crates/xshi/src/interactive/repl.rs:518` |
| `interactive_final_sigterm_interrupts_oversized_paste_drain` (4778) | untested | `crates/xshi/src/interactive/term.rs:44` |
| `interactive_final_failed_shell_builtins_update_question_status` (4797) | untested | `crates/xshi/src/interactive/app.rs:392` (status path) |
| `interactive_final_z_matches_quoted_directory_history` (4820) | covered | `crates/xshi/src/interactive/z.rs:112-135` |
| `external_commands_run_in_the_directory_a_list_cd_selected` (4842) | untested | `crates/xshi/src/interactive/app.rs:937` |
| `sigterm_and_sighup_restore_the_terminal_and_exit_with_the_signal_status` (4866) | covered | `crates/xshi/src/interactive/term.rs:12-77`; `crates/xshi/tests/terminal_storm/mod.rs:466` |
| `fatal_crash_signals_restore_the_terminal_and_exit_with_the_signal_status` (4884) | covered | `crates/xshi/tests/terminal_storm/mod.rs:466` |
| `second_ctrl_z_is_refused_and_the_first_job_stays_resumable` (4915) | untested | `crates/xshi/src/interactive/app.rs:1560` |
| `state_builtin_in_a_list_and_undocumented_syntax_are_refused_without_running_anything` (4940) | untested | `crates/xshi/src/interactive/app.rs:3222-3240` |
| `ctrl_p_dump_is_private_and_omits_typed_text` (4959) | untested | `crates/xshi/src/interactive/repl.rs:248` |
| `native_listing_and_standalone_cd_preserve_private_use_and_raw_path_bytes` (4984) | missing | `crates/xshi/src/interactive/listing.rs:119` uses `to_string_lossy` |
| `prompt_identity_displays_unicode_and_replaces_only_invalid_display_bytes` (5074) | untested | `crates/xshi/src/interactive/prompt.rs:35` |
| `osc133_marks_prompt_input_execution_and_completion` (5144) | missing | `crates/xshi/src/interactive/repl.rs:303-372` emits only OSC 7 |
| `osc133_reports_command_status` (5155) | missing | `crates/xshi/src/interactive/repl.rs:303-372` emits no OSC 133 |
| `osc133_cancelled_input_has_no_execution_marker` (5164) | missing | `crates/xshi/src/interactive/repl.rs:303-372` emits no OSC 133 |
| `osc133_refused_input_has_no_execution_marker` (5180) | missing | `crates/xshi/src/interactive/repl.rs:303-372` emits no OSC 133 |
| `osc133_resize_does_not_duplicate_markers` (5190) | missing | `crates/xshi/src/interactive/repl.rs:303-372` emits no OSC 133 |
| `osc133_redirected_output_keeps_markers_off_command_file` (5208) | missing | `crates/xshi/src/interactive/repl.rs:303-372` emits no OSC 133 |
| `osc133_continuation_does_not_emit_extra_execution_markers` (5220) | missing | `crates/xshi/src/interactive/repl.rs:303-372` emits no OSC 133 |
| `osc133_suspended_job_and_fg_emit_completion_markers` (5232) | missing | `crates/xshi/src/interactive/repl.rs:303-372` emits no OSC 133 |
| `delegated_ish_c_emits_no_osc133_markers` (5264) | missing | `crates/xshi/src/interactive/repl.rs:303-372` emits no OSC 133 |
| `osc7_working_directory_emitted_to_interactive_terminal` (5277) | untested | `crates/xshi/src/interactive/repl.rs:305` |
| `resize_storm_keeps_shell_responsive` (5285) | untested | `crates/xshi/src/interactive/signal.rs:12` |
| `repeated_stop_resume_preserves_command_boundaries` (5299) | untested | `crates/xshi/src/interactive/app.rs:1560` |
| `converged_command_reads_from_dev_tty` (5325) | untested | `crates/xshi/src/interactive/app.rs:1920` |
| `alternate_screen_sequences_pass_through_relay` (5336) | untested | `crates/xshi/src/interactive/term.rs:140` |
| `osc52_copy_scrollback_emits_base64_session_log` (5347) | untested | `crates/xshi/src/interactive/app.rs:342` |
| `shell_sighup_at_prompt_exits_and_restores_terminal` (5364) | covered | `crates/xshi/src/interactive/term.rs:16`; `crates/xshi/tests/terminal_storm/mod.rs:473` |

### 1.2 Other ptytest files

`tests/external_signals.rs` has one real test; `external_signal_probe` (line 38)
is an `#[ignore]` external helper, not a test.

| Test (line) | Class | xshi evidence |
|---|---|---|
| `external_commands_restore_terminal_signals_in_every_context` (54) | covered | `crates/xshi/tests/terminal_storm/mod.rs:466` restores inherited dispositions; `crates/xshi/src/interactive/app.rs:1920` |

`tests/fdu_pty.rs`:

| Test (line) | Class | xshi evidence |
|---|---|---|
| `startup_shows_a_minimal_title_and_the_entry_count_and_restores_the_terminal` (10) | missing | no `fdu`: `crates/xshi/src/interactive/builtin.rs:6-20` |
| `repeated_exclusions_hide_exact_root_relative_paths_in_the_browser` (36) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `directory_with_excluded_contents_is_deleted_around_the_excluded_entry` (49) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `delete_confirmation_shows_root_and_entry_counts_and_leaves_unconfirmed_marks` (74) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `help_screen_opens_and_closes` (92) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `read_only_mode_disables_deletion` (106) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `minus_toggles_between_side_by_side_and_stacked_panes` (121) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `s_cycles_between_size_name_and_date_order` (151) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `unicode_filter_input_and_backspace_preserve_complete_characters` (167) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `profile_large_layout_fixture` (189) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `rapid_input_handoff_burst_does_not_drop_queued_quit` (222) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `sigterm_terminates_shell_running_fdu` (267) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |

`tests/fdu_snapshots.rs` (all missing, same evidence):

| Test (line) | Class | xshi evidence |
|---|---|---|
| `ready_listing` (121) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `marked_pane_beside_the_list` (128) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `stacked_panes` (141) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `narrow_terminal_shows_one_pane` (155) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `inside_a_marked_directory` (166) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `help_overlay` (177) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |
| `filter_prompt_and_applied_filter` (188) | missing | `crates/xshi/src/interactive/builtin.rs:6-20` |

`tests/history_search_paste.rs`:

| Test (line) | Class | xshi evidence |
|---|---|---|
| `pasted_history_query_filters_and_executes_selected_command` (68) | covered | `crates/xshi/src/interactive/input.rs:619`; `repl.rs:1625` |
| `pasted_history_query_edits_at_cursor_and_restores_candidates_after_delete` (89) | covered | `crates/xshi/src/interactive/repl.rs:1656-1682` |
| `pasted_history_query_resets_selection_after_narrowing` (109) | covered | `crates/xshi/src/interactive/repl.rs:1651` |

### 1.3 Counts

| Class | pty.rs | other ptytest | total |
|---|---:|---:|---:|
| covered | 69 | 4 | 73 |
| covered† | 57 | 0 | 57 |
| untested | 31 | 0 | 31 |
| missing | 31 | 19 | 50 |
| deviation | 14 | 0 | 14 |
| **total** | **202** | **23** | **225** |

## 2. (B) ish builtins and editor features

### 2.1 ish-owned builtins (`ISH_BUILTINS`)

`/home/josh/d/ish/src/builtin.rs:165-185` is the authoritative table.

| Builtin | ish implementation (LOC) | xshi counterpart |
|---|---|---|
| `cp` | `src/fs_command.rs` (654) + `/home/josh/d/ish/crates/ish-fs`; dispatch `src/external.rs:26-45` | none (`crates/xshi/src/interactive/builtin.rs:6-20`) |
| `mv` | same as `cp`; `src/external.rs:26-45` | none |
| `rsync` | `src/sync_command.rs` (312) + `/home/josh/d/ish/crates/ish-fs/src/sync.rs` (1122); `src/external.rs:46-61` | none |
| `rmrf` | `src/rmrf.rs` (876); `src/shell.rs:436-459` | none |
| `fdu` | `src/fdu.rs` (433) + `src/fdu_browser.rs` (3039) + `/home/josh/d/ish/crates/fdu-tui` (2609); `src/shell.rs:461-496` | none |
| `denv` | `src/denv.rs` (2932); `src/shell.rs:502-507` | `crates/xshi/src/interactive/denv.rs` (1117) |
| `z` | `src/frecency.rs` (242); `src/shell.rs:510-522` | `crates/xshi/src/interactive/z.rs` (135) |
| `l` | `src/ls.rs` (819); `src/external.rs:62-67` | `crates/xshi/src/interactive/listing.rs` (335) |
| `ll` | `src/ls.rs` (819), `show_hidden=true`; `src/builtin.rs:177` | none |
| `c` | `src/external.rs:68-71` | `crates/xshi/src/interactive/app.rs:1664` |
| `w`/`which`/`type` | `src/builtin.rs:182-184`, `src/shell.rs:529+`, `src/path.rs` (280) | `crates/xshi/src/interactive/builtin.rs:45-75` |
| `cd` | `src/shell.rs:396-427` | `crates/xshi/src/interactive/app.rs:466` |
| `exit` | `src/shell.rs:393-395` | `crates/xshi/src/interactive/app.rs:295` |
| `fg` | `src/shell.rs:511-518` + `src/job.rs` (90) | `crates/xshi/src/interactive/app.rs:1560` |
| `alias` | `src/shell.rs:392`, `:732+`; `src/alias.rs` (105) | `crates/xshi/src/interactive/alias.rs` (176) |
| `history` | `src/history.rs` (1674) + `src/history/store.rs` (603); `src/shell.rs:428-430` | `crates/xshi/src/interactive/history.rs` (1608) + `crates/xshi/src/interactive/history/store.rs` (512) |
| `copy-scrollback` | `src/shell.rs:431-434`, OSC 52 at `src/shell.rs:370` | `crates/xshi/src/interactive/app.rs:342` |
| `ish-dump` | `src/shell.rs:435`, `src/diagnostics.rs` (282) | `crates/xshi/src/interactive/repl.rs:240` as `xshi-dump` |

### 2.2 epsh builtins exposed by ish

`/home/josh/d/ish/src/builtin.rs:242-258` lists `echo`, `printf`, `pwd`, `true`,
`false`, `:`, `test`, `[`, `kill`, `export`, `unset`, `set`, `command`; the
implementations live in `/home/josh/d/ish/crates/epsh/src/builtins.rs` (1410).

| Group | xshi counterpart |
|---|---|
| `echo`, `pwd`, `true`, `false` | internal: `crates/xshi/src/interactive/app.rs:2965-3000` |
| `export`, `unset`, `set` | `crates/xshi/src/interactive/app.rs:1585-1617`, `app.rs:531` |
| `printf`, `test`, `[`, `kill`, `command`, `:` | not internal; `:` is `Noop` (`app.rs:3228`), the rest resolve on `PATH` (`crates/xshi/src/interactive/builtin.rs:23-27`, SPEC-INTERACTIVE:372-374) |

### 2.3 Editor features (README "Interactive use")

| Feature (README) | ish | xshi |
|---|---|---|
| Up/Down wrapped row; history at boundary | `src/editor.rs:790-827` | `crates/xshi/src/interactive/repl.rs:990-1021` |
| Ctrl+R history search | `src/editor.rs:785`, `:1522-1582` | `crates/xshi/src/interactive/repl.rs:985`, `:1625-1702` |
| Tab completion | `src/editor.rs:870`, `:1401-1424` | `crates/xshi/src/interactive/repl.rs:1053`, `:1536-1559` |
| Ctrl+A / Ctrl+E | `src/editor.rs:777-778` | `crates/xshi/src/interactive/repl.rs:977-978` |
| Ctrl+W / Ctrl+Delete | `src/editor.rs:781`, `:868` | `crates/xshi/src/interactive/repl.rs:981`, `:1046` |
| Alt+D | `src/editor.rs:782` | `crates/xshi/src/interactive/repl.rs:982` |
| Ctrl+U / Ctrl+K | `src/editor.rs:779-780` | `crates/xshi/src/interactive/repl.rs:979-980` |
| Ctrl+Y | `src/editor.rs:783` | `crates/xshi/src/interactive/repl.rs:983` |
| **Ctrl+_ / Alt+R undo/redo** | `src/editor.rs:767-768`, state `:188-270` | **none** (`crates/xshi/src/interactive/repl.rs:970-1070` has no undo state) |
| Ctrl+Backspace recent dirs | `src/editor.rs:866`, `:387-405` | `crates/xshi/src/interactive/repl.rs:1041`, `crates/xshi/src/interactive/render.rs:929` |
| Left/Right | `src/editor.rs:847-862` | `crates/xshi/src/interactive/repl.rs:1024-1037` |
| Ctrl+Left/Ctrl+Right word motion | `src/editor.rs:828-846` | `crates/xshi/src/interactive/repl.rs:1022-1023` |
| **Ctrl+Right accepts suggestion word** | `src/editor.rs:829-846` (`suggestion_word_accept_len`, `:833`) | **none** (`crates/xshi/src/interactive/repl.rs:1023` only moves) |
| Right accepts whole suggestion | `src/editor.rs:848-861` | `crates/xshi/src/interactive/repl.rs:1027-1037` |
| Ctrl+C / Ctrl+D | `src/editor.rs:769-776` | `crates/xshi/src/interactive/repl.rs:970-976` |
| Ctrl+L | `src/editor.rs:784` | `crates/xshi/src/interactive/repl.rs:984` |
| Ctrl+Z suspends | `src/job.rs` (90), `src/foreground.rs` (785) | `crates/xshi/src/interactive/signal.rs:24`, `crates/xshi/src/interactive/app.rs:1750` |
| Ctrl+P layout dump | `src/editor.rs:134` | `crates/xshi/src/interactive/repl.rs:443` |
| Autosuggestion threshold/ghost | `src/editor.rs:104-107`, `:676-688` | `crates/xshi/src/interactive/repl.rs:842`, `crates/xshi/src/interactive/render.rs:544` |
| Bracketed paste limit | `src/input.rs` (832), limit in `src/editor.rs` path | `crates/xshi/src/interactive/input.rs:5` (8192) |
| Prompt shortening / git / denv mark | `src/prompt.rs` (580) | `crates/xshi/src/interactive/prompt.rs` (352) |
| History pager rendering | `src/render.rs` (1345) | `crates/xshi/src/interactive/render.rs` (1389) |
| Completion grid | `src/complete.rs` (1138) | `crates/xshi/src/interactive/complete.rs` (1362) |

## 3. (C) Facilities map

### 3.1 fdu TUI against XSH's TUI

ish's fdu is four crates plus a browser: `fdu-core` (921), `fdu-scan` (2212),
`fdu-delete` (1201), `fdu-posix` (1348), `fdu-tui` (2609). XSH already owns a
production TUI stack: `core/lib/btop_terminal.xsh` (153),
`btop_input.xsh` (356), `btop_surface.xsh` (270), `btop_render.xsh` (625),
`btop_ui.xsh` (736), `btop_model.xsh` (595), plus the Rust cell layer
`src/modules/tui.rs` (288) and `src/modules/unix/tty.rs` (763). It is exercised
by `tests/core_compat_extra/pty.rs` (1436) and `core/tests/test-btop-*.xsh`.

| ish component | What it provides | XSH facility today | Concrete gap to grow |
|---|---|---|---|
| `fdu-tui/src/terminal.rs:1` (562) | raw mode, alternate screen, key decoder, 30 ms escape timeout, resize intent, unconsumed input hand-off, panic restore | `btop_terminal.xsh:100-153` (raw/alt screen/defer restore), `btop_input.xsh:304` (decoder), `unix.tty_*` (`SPEC:6903-6911`) | No public hand-off of unconsumed bytes to a parent shell; no panic/signal restore contract independent of `defer`; resize arrives only as a polled size (`core/btop.xsh:36-145`) |
| `fdu-tui/src/screen.rs:178` (361) | cell grid, wide-glyph erasure, style, diff to bytes | `btop_surface.xsh:50-268` (`blank`, `from_lines`, `prepare`, `complete`), `src/modules/tui.rs:68-191` (cell width/clip) | `btop_surface` is row-string based, not a cell/overlay model; modal overlays and wide-glyph erase are untested there (`crates/fdu-tui/src/screen.rs:244`) |
| `fdu-tui/src/lscolors.rs:1` (314) | parse `LS_COLORS`, color names by kind | `core/dircolors.xsh:1` (230) can emit `LS_COLORS`; no reusable parser | No shared `LS_COLORS` parser/colorizer library for XSH apps |
| `fdu-tui/src/lib.rs:201` (1372) | two panes, focus, mark/range/all, tree flatten, sort/size modes, filter, help modal, delete confirm, status strips | `btop_ui.xsh:169-700` (sort, filter, identity, tree collapse, selection), `btop_render.xsh:59-87` (`panel`, `split_panel`), `btop_surface.xsh:181-214` (`selected_row`, `viewport`, `input_field`) | No marked-items pane model, no mark/range semantics, no destructive-action confirmation strip, no per-row state glyphs, no file-tree model |
| `fdu-core/src/lib.rs:1` (921) | `Tree`, `NodeId`, `NodeRecord` (allocated/apparent, type, mtime, nlink, mount/alias flags), deterministic ordering | `btop_model.xsh:1` (595) is process/CPU/network oriented; `core/lib/system_report.xsh:1` and `fs.walk` streams exist | No file-tree/index type in XSH; no size aggregation with `M`/`A`/`@`/`*` annotations |
| `fdu-scan/src/indexed.rs:1` (1200), `fdu-scan/src/exclusions.rs:1` (123), `fdu-scan/src/linux/platform.rs:1` (182), `fdu-scan/src/macos/attributes.rs:1` (225) | incremental indexed scan, exclusions, access-error state, mount identity, parallel workers | `fs.walk` (`src/modules/fs.rs:1399-1432`), `fs.files`/`fs.dirs` (`SPEC:5983`, `6497-6513`), `core/lib/system_report_collect.xsh:1` | No size-annotated incremental scan, no scan-exclusion model, no partial-scan/access-error stream, no bounded worker pool |
| `fdu-delete/src/lib.rs:1` (1201) | per-entry deletion plan, skipped/incomplete reporting | `core/rm.xsh:1` (346), `fs.remove`, `fs.rmdir` | No plan/outcome model; no partial-tree reporting |
| `fdu-posix/src/quarantine.rs:1` (1219) | versioned quarantine sessions, root lock, intent records, crash recovery | `fs.rename_at`/`fs.rename_exchange` (`src/modules/fs/control.rs:41`), `fs.sync_path` (`control.rs:10`), `xsh-root` (`crates/xsh-root/src/lib.rs:36-121`) | No journaled stage/recover primitive, no mount/ancestry validation, no receipt spool |

XSH-side strengths that reduce the port: kernel-anchored opens
(`crates/xsh-root/src/linux.rs:26-99`, `macos.rs:8-88`), raw-mode restore and
cleanup (`btop_terminal.xsh:90-153`), synchronized output
(`btop_terminal.xsh:149-153`), and PTY tests for TUI lifecycle
(`tests/core_compat_extra/pty.rs:692-961`).

Delivery constraint: DESIGN.md requires userspace growth in XSH and only narrow
host primitives in Rust (`docs/DESIGN.md:20-45`). The TUI itself therefore
belongs in `core/lib/` (genericized from the btop-prefixed libraries), while a
size-indexed scan may justify a narrow Rust host primitive because `fs.walk`
already lives in Rust (`src/modules/fs.rs:1399`).

### 3.2 ish-fs against XSH's filesystem layer

ish-fs is 17065 lines over all files (17046 Rust) under
`/home/josh/d/ish/crates/ish-fs`. XSH's substrate is
`src/modules/fs.rs` (3471) + `src/modules/fs/prims.rs` (991) +
`src/modules/fs/control.rs` (126) + `src/modules/fs/xattr.rs` (119) + `crates/xsh-root/src/lib.rs:1` (995) +
`src/modules/unix/tty.rs` (763, terminal only). Userspace GNU commands already
exist: `core/cp.xsh` (751), `core/mv.xsh` (445), `core/rm.xsh` (346).

| ish-fs module (LOC) | Technique | XSH today | Gap |
|---|---|---|---|
| `platform/linux.rs:403` (1409) | `FICLONE`, `copy_file_range`, `fallocate`, `openat`/`O_NOFOLLOW`, `statx` mount id, short-read/cancel checks | `prims.rs:628-880` already does clone → `copy_file_range` → `pread`/`pwrite`, sparse extents (`prims.rs:533-563`), and reports `clone`/`copy_file_range`/`read_write`; `control.rs:41` does `RENAME_EXCHANGE`; `fs.rs:773` does `RENAME_NOREPLACE` | No `statx` mount identity, no hardlink-group transfer, no cancellation/partial-progress accounting, no short-source/cancel tests |
| `platform/macos.rs:57` (1260) | `clonefile`/`fclonefileat`, `copyfile` with cancellation, resource forks, `O_EVTONLY` staging dirs | `prims.rs:795-801` returns `ENOTSUP` for clone on non-Linux; `xsh-root/src/macos.rs` does confined opens only | macOS clone/copyfile/resource-fork path is absent |
| `engine/transfer.rs:225` (1297) | private staging, atomic publication, metadata apply, hardlink groups, durable receipts, direct-copy fallback | `fs.copy_file` creates/truncates the destination in place (`prims.rs:706-715`); `rooted_write_atomic` (`fs.rs:733-784`) covers single-file atomic write; `core/cp.xsh:57-136` applies xattrs/times/mode | No directory staging, no atomic replace of a tree, no hardlink grouping, no receipt/version revalidation, no crash-safe fallback policy |
| `engine/records.rs:1` (660), `engine/retirement.rs:1` (240), `engine/anchored.rs:1` (147), `engine/mod.rs:1` (679), `engine/dry_run.rs:1` (368) | intent records, anchored parent walks, retirement, dry-run | `xsh-root` anchors opens (`Root::open_dir`, `mutate.rs:112-346`); no records | No journal, no per-entry retirement, no dry-run engine |
| `sync.rs:75` (1122) | quick-check (kind/size/mtime), staged update, `--delete`, parallel transfers, cancellation cleanup | single-file copy only (`src/modules/fs/prims.rs:628`); `fs.walk` (`src/modules/fs.rs:1399`) exists | No local sync engine, quick-check, staged update, or `--delete`; each caller would orchestrate walk plus copy |
| `policy.rs:1` (495) | collision policy matrix, `--into`/merge/ancestry/duplicate preflight | ad-hoc policy in `core/cp.xsh:16`/`core/mv.xsh:391`; no shared decision model | No reusable collision/preflight model |
| `roots.rs:204` (212), `platform/mod.rs:1` (664), `platform/metadata.rs:1` (46) | root identity, mount identity, metadata capture/apply incl. ACL/xattr/flags/times | `fs.stat`/`set_owner`/`set_times`/`chmod` (`prims.rs:51-343`), `fs.xattr_*` (`xattr.rs:12`), `xsh-root` metadata (`mutate.rs:281-310`) | No `Metadata` record that captures and applies ACLs, flags, resource forks, or ownership normalization; `SPEC:6596-6618` says `fs.copy_file` "sets no other metadata" |
| `engine/tests.rs:1` (860), `engine/repair_tests.rs:1` (848), `engine/quarantine_tests.rs:1` (244) | crash/fault injection | none; the copy seam is `src/modules/fs/prims.rs:628` | Needs a test seam for staged publication |

Split of responsibilities:

| Belongs in XSH's core filesystem layer (scripts benefit) | Shell-only UX (xshi) |
|---|---|
| Metadata capture/apply record (mode, owner, times, xattrs, ACL bytes, flags) and safe normalization | Option grammar (`--into`, `--merge`, `--replace`, `-n`, `-j`, `--dry-run`, `--durable`, `-v`) |
| Hardlink-group preservation during a tree transfer | Operand mapping (`SOURCE... DIR`, trailing slash, basename rules) and diagnostics wording |
| Staged publication + `RENAME_NOREPLACE`/`RENAME_EXCHANGE` replacement | Progress rendering and `--no-progress` timing |
| Journaled quarantine with recovery (records, locks, mount/ancestry checks) | Ignore `SIGTSTP`, map Ctrl+C to status 130, restore terminal |
| Mount-boundary policy (`statx` mount id, `--cross-file-systems`) | Refuse native fs commands in pipelines/substitutions before redirections |
| Bounded worker pool, cancellation flag, byte/hole progress | Whole-line command restrictions, alias/`command` interaction |
| `--delete` and quick-check for local sync | `rsync` operand/`--contents` semantics and status codes |

Existing XSH userspace already proves the substrate can support `core/cp.xsh`
(reflink/sparse/xattr/times, `core/cp.xsh:467`) and `core/mv.xsh`
(`renameat2` exchange, `core/mv.xsh:169`, `:239`). The missing layer is the
shared, crash-safe transfer engine, not the option parsing.

## 4. (D) Implementation slices

Ownership is file-disjoint per slice. S1 pre-declares empty modules and owns
the shared registry, so S2/S3/S6 never edit `src/modules/fs.rs`. The parity
driver restoration is out of scope (another lane).

| # | Slice | Owned paths (new unless noted) | ish tests it must make pass |
|---|---|---|---|
| S1 | FS core: metadata record + hardlink groups + cancellation/progress types | `src/modules/fs/meta.rs`, `src/modules/fs/transfer.rs` (new), `src/modules/fs.rs` (registration only), `crates/xsh-root/src/mutate.rs`, `crates/xsh-fs/Cargo.toml` if a crate is added | `/home/josh/d/ish/crates/ish-fs/tests/preservation.rs:223-336`, `/home/josh/d/ish/crates/ish-fs/tests/transfer.rs:77-109`, `/home/josh/d/ish/crates/ish-fs/tests/native_linux.rs:346-503`, `/home/josh/d/ish/crates/ish-fs/tests/native_macos.rs:792-1065` |
| S2 | FS core: staged publication, journal, crash recovery, collision policy | `src/modules/fs/stage.rs`, `src/modules/fs/control.rs`, `src/modules/fs/policy.rs` | `/home/josh/d/ish/crates/ish-fs/tests/publication.rs:69-202`, `/home/josh/d/ish/crates/ish-fs/tests/interruption.rs:118-180`, `/home/josh/d/ish/crates/ish-fs/tests/policy.rs:84-462`, `/home/josh/d/ish/crates/fdu-posix/tests/recovery.rs:79-406` |
| S3 | FS core: worker pool, mount boundary, durable sync, dry-run | `src/modules/fs/engine.rs`, `src/linux/mount.rs` or `src/modules/fs/mounts.rs` | `/home/josh/d/ish/crates/ish-fs/tests/resources.rs:125-331`, `/home/josh/d/ish/crates/ish-fs/tests/native_boundaries.rs:85-527`, `/home/josh/d/ish/crates/ish-fs/tests/sync_adversarial.rs:18-235`, `/home/josh/d/ish/crates/ish-fs/tests/native_linux.rs:530-589` |
| S4 | Builtins `cp`/`mv`/`rsync`/`rmrf` on S1-S3 | `crates/xshi/src/interactive/fs_command.rs`, `crates/xshi/src/interactive/sync_command.rs`, `crates/xshi/src/interactive/rmrf.rs`, `crates/xshi/src/interactive/builtin.rs`, `crates/xshi/src/interactive/app.rs` (dispatch only) | `/home/josh/d/ish/tests/pty.rs:777-1105`, `:1824`, `:2950`, `:4358`, `:5084`; `/home/josh/d/ish/crates/ish-fs/examples/fs_bench.rs` as perf check |
| S5 | TUI growth: genericize btop TUI, add browser/mark/modal model | `core/lib/tui_surface.xsh`, `core/lib/tui_input.xsh`, `core/lib/tui_terminal.xsh`, `core/lib/tui_browser.xsh`, `core/lib/btop_*.xsh` (imports only), `core/tests/test-tui-*.xsh` | `fdu-tui` unit tests (37); `tests/core_compat_extra/pty.rs:660-961`; `core/tests/test-btop-surface.xsh`, `test-btop-input.xsh` |
| S6 | fdu on S5: scan model, browser, summary, delete | `core/fdu.xsh`, `core/lib/fdu_*.xsh`, `src/modules/fs/scan.rs` (if a Rust scan primitive is chosen), `core/tests/test-fdu-*.xsh` | `/home/josh/d/ish/tests/fdu_pty.rs:10-276`, `/home/josh/d/ish/tests/fdu_snapshots.rs:121-200`, `/home/josh/d/ish/crates/fdu-scan/tests/indexed_policy.rs:200-216`, `/home/josh/d/ish/crates/fdu-delete/tests/browser_plan.rs:30-109`, `/home/josh/d/ish/tests/pty.rs:1851` |
| S7 | Editor: undo/redo, suggestion word accept, recent dirs | `crates/xshi/src/interactive/repl.rs`, `line.rs`, `render.rs` | `/home/josh/d/ish/tests/pty.rs:1243-1360`, `:1903-2012`, `:3278`, `/home/josh/d/ish/src/editor/tests.rs` undo cases |
| S8 | denv helper trust + copy-scrollback / OSC tests | `crates/xshi/src/interactive/denv.rs`, `crates/xshi/src/interactive/denv_stdlib.sh`, `crates/xshi/tests/pty_local.rs` (new) | `/home/josh/d/ish/tests/pty.rs:3862-4257`, `:5347`, `:5277` |

Sequencing: S1 → S2 → S3 → S4; S5 → S6; S7 and S8 are independent after S1.
S6 depends on S1 only if the scan primitive reuses the metadata record.

## Decisions (owner, 2026-11-11)

- **ish is canonical for everything.** xshi is outdated; every behavior ish
  has, xshi matches, and the deviations listed in SPEC-INTERACTIVE §1 and
  elsewhere are retired as each slice reaches them: `l` hides dotfiles with
  `ll` showing them, OSC 133 markers, ish's history visibility, `.env` behind
  `denv allow`, and the rest. SPEC-INTERACTIVE is rewritten to describe the
  ish behavior. xshi-only additions that leave ish-shaped input alone (XSH
  source at the prompt) are kept unless a slice finds a conflict, which goes
  back to the owner.
- **fdu is an XSH app** (`core/fdu.xsh`) on generic `tui_*` libraries extracted
  from btop's; xshi runs it as a builtin.
- **The fdu scan is a narrow Rust primitive** beside `fs.walk`, carrying ish's
  size-indexed parallel scan; model, browser and delete logic stay in XSH.
- **Staged publication and crash recovery stay internal** to XSH's
  filesystem layer as a Rust API of the `xsh` crate that xshi's
  `cp`/`mv`/`rsync`/`rmrf` call, with no new script-visible contract yet.
- **Harness:** the parity lane owns the move to `laputa-ptytest` 0.4.
- **`-c` runs in process** (owner, 2026-11-11). ish execs `/bin/sh -c`; on
  Laputa `/bin/sh` is xshi, so xshi evaluates the command itself. This is the
  only platform-forced deviation and SPEC-INTERACTIVE records it with that
  reason.
- **Syntax ish refuses is refused** (owner, 2026-11-11): `eval`, `exec`,
  trailing `&`, `bg`, `if`, `${VAR:-x}` and the other undocumented constructs
  ish rejects get ish's refusals and messages. XSH source at the prompt (§5)
  stays as the one xshi addition.
- **`fg` after a stopped AND-OR list drops the rest of the list**, as ish does.
- **Deferred to later slices:** pipeline job control (a multi-child job handle
  in XSH's process layer), raw non-UTF-8 path bytes in shell words, and ish's
  capture-PTY job cases.
- **macOS** stays owner-deferred; Linux first.

## 5. (E) Open questions for the owner

1. **fdu delivery vehicle.** XSH app (`core/fdu.xsh`, reusing genericized
   `tui_*` libraries) or an xshi Rust builtin that calls the XSH renderer?
   DESIGN.md points to the former; the shell needs a pending-input hand-off
   (`/home/josh/d/ish/src/shell.rs:461-496`) either way.
2. **Rust vs XSH scan.** May a size-indexed parallel scanner be a narrow host
   primitive (like `fs.walk` at `src/modules/fs.rs:1399`), or must the scan be
   XSH code over `fs.walk`?
3. **Journal in the substrate.** Should staged publication/quarantine be
   exposed to scripts (`fs.stage`, `fs.recover`) or remain internal to the
   shell's fs commands? Owner direction says grow the internal filesystem
   layer; exposing it benefits scripts but adds a public contract.
4. **macOS scope.** Is clonefile/copyfile/resource-fork parity in this campaign,
   or Linux first? XSH's current clone path is Linux-only
   (`src/modules/fs/prims.rs:795-801`).
5. **`l`/`ll` contract.** ish's `l` hides dotfiles and `ll` shows them
   (`/home/josh/d/ish/src/ls.rs:1-16`); xshi's SPEC calls `l` `ls -plAhG` and
   shows dotfiles (`SPEC-INTERACTIVE:376-378`). Is the SPEC the intended
   contract, or is this a parity bug to fix by adding `ll`?
6. **OSC 133.** ish emits prompt/command markers (`tests/pty.rs:5144-5264`);
   xshi emits only OSC 7. Add OSC 133, or record its absence as a deviation?
7. **History visibility.** SPEC §8 deliberately hides peer entries from
   Up/Ctrl-R/autosuggest until the next shell (`SPEC-INTERACTIVE:265-267`).
   Confirm this stays a deviation from ish's `history_ctrl_r_includes_later_global_entries`.
8. **denv `.env` trust.** SPEC §13 loads `.env` without trust; ish requires
   `denv allow` (`tests/pty.rs:3876`). Confirm the xshi contract.
9. **Harness alignment.** ish is on `laputa-ptytest` 0.2 and this workspace is
   on 0.1.1 (`Cargo.toml:137`); confirm the parity lane owns the upgrade and
   that this RFC's slices do not edit the harness.

## 6. Sources

- ish: `/home/josh/d/ish/README.md`, `docs/filesystem.md`, `src/builtin.rs`,
  `src/shell.rs`, `src/external.rs`, `src/editor.rs`, `src/ls.rs`,
  `src/denv.rs`, `crates/ish-fs/src/**`, `crates/fdu-*/src/**`,
  `tests/pty.rs`, `tests/fdu_pty.rs`, `tests/fdu_snapshots.rs`,
  `tests/external_signals.rs`, `tests/history_search_paste.rs`.
- xshi: `crates/xshi/src/interactive/**`, `crates/xshi/tests/**`,
  `docs/SPEC-INTERACTIVE.md`, `docs/TESTING.md`.
- XSH substrate: `src/modules/fs.rs`, `src/modules/fs/{prims,control,xattr}.rs`,
  `crates/xsh-root/src/**`, `crates/xsh-archive/src/**`,
  `src/modules/tui.rs`, `src/modules/unix/tty.rs`, `core/lib/btop_*.xsh`,
  `core/{cp,mv,rm,dircolors}.xsh`, `docs/{BTOP,DESIGN,ARCHITECTURE}.md`,
  `tests/core_compat_extra/pty.rs`.
- Port provenance: `git show b1f6c6f21:plan-ish-parity.md`,
  `crates/xshi/src/interactive/ported_tests.rs:1`.
