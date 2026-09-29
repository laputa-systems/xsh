# xshi ↔ ish parity

Goal (from `ish.md`): everyday `xshi` use is indistinguishable from `~/d/ish` —
same keystrokes, screen behavior, command results, and history. `xshi` stays an
interactive frontend over XSH's lowering, runtime, stdlib, and terminal/process
facilities; it does not import ish or epsh and does not shell out for
shell-owned work. This file is the working checklist; `docs/SPEC-INTERACTIVE.md`
remains the contract and is updated as behavior lands.

## Decisions

- Checklist lives here, not in `plan.md` (which holds the finished
  `system-report` plan).
- Shell-shaped input wins where lexically unambiguous: `export NAME=v`,
  `set NAME v…`, `set -e`, `source f`, `alias name cmd…`. XSH-shaped input
  (`export proc/let/type…`, `let`, `use`, …) stays XSH.
- Presentation matches ish exactly: prompt, colors, live command-word coloring,
  OSC 7, repaint sequence. Only names, paths, and `ish:` → `xshi:` message
  prefixes differ. `ish-dump`/Ctrl+P become `xshi-dump` writing
  `~/.cache/xshi/dump-<hex>`.
- History adopts ish's storage at `~/.local/share/xshi/history` (records v1/v2,
  binary cache, reset marker, lock). No reader/migrator for the previous xshi
  record or `XH` cache formats: clean break, single user.
- No xshi-side compatibility shims: replaced behavior is deleted.
- `xshi`-only extensions kept where they do not change ish-shaped input:
  XSH source at the prompt, `bg`, trailing `&`, `z`, `:`, `history [N]`,
  core utilities on PATH.

## Test strategy

- Scenarios live in `tests/runtime/interactive/parity.rs` and run the real
  `xshi` binary in a PTY through `laputa-ptytest` (dev-dependency; the same
  harness ish's PTY suite uses, with a `vt100` screen model and event-driven
  waits). Each scenario records per-step frames: visible rows, cursor
  row/column, plus explicit persistent effects.
- Golden frames under `tests/fixtures/interactive-parity/<os>/` were recorded
  from ish on that operating system. Ordinary runs compare xshi to the goldens and need no ish install.
- `XSHI_PARITY_ISH_BIN=/path/to/ish` additionally runs the same scenario against
  ish and requires ish == golden == xshi (drift detection).
  `XSHI_PARITY_RECORD=1` with that variable rewrites goldens from ish.
- Normalization is explicit and limited to: HOME/scratch paths, the binary name
  in message prefixes, and the session id / timestamps in history records.
- Pure logic (line buffer, input decoding, scoring, layout math, history
  storage, prompt shortening) is ported as `cargo test -p xshi` unit tests.
- Shell-semantics checks that need no terminal use `xshi -c` from native
  `xsht` tests or the Rust integration harness.

## Phases

1. Harness + checklist  2. History  3. Line editing / input / render / prompt
4. Completion  5. Shell semantics, builtins, statuses  6. Jobs, signals,
terminal restoration  7. Benchmarks + Linux gate

## Status

Every phase has landed on macOS; the Linux gate is recorded at the end of this
section.

- Harness: `tests/runtime/interactive/parity.rs` with 193 differential scenarios
  (`parity/scenarios.rs`: the adapted `ish` tests; `parity/extended.rs`: inputs
  `ish`'s own suite never sends — Unicode, word motions, kill ring, multiline,
  autosuggest, completion contexts and ordering, ssh hosts, history ranking,
  cwd boost, layout dumps, redirection order) and second-shell (`spawn_peer`)
  scenarios for cross-session history. Goldens: `tests/fixtures/interactive-parity/`.
- `xshi`-only tests: `tests/runtime/interactive.rs` (piped session semantics,
  `$` completion, `fg` continuing lists, forced exit, `eval`/`exec`, redirection
  order, history round trip and recovery) and `crates/xshi/tests/cli.rs`.
- History rewritten around a durable log plus cache (`history/store.rs`):
  bounded shared/exclusive locking, disk-to-disk compaction, incremental sync
  with partial-line and replacement handling, reset generations, quarantine on
  `history rebuild`; 62 tests against real files
  (`interactive::history::tests`).
- UI stack ported from `ish`; shell semantics, builtins, pipelines, redirections
  (left-to-right via `ProcessRedirection::ChildDup`), job control, denv, CLI.
- Docs: `docs/SPEC-INTERACTIVE.md` rewritten; `ARCHITECTURE`, `TEST-MAP`,
  `SPEC-OS`, `BENCHMARKING` updated.

Benchmarks against the preceding revision (`docs/BENCHMARKING.md`): prompt
render 66 → 29 ns, history search render and dynamic-name session flat;
completion navigation (19 µs → 0.6 ms) and the `cd`/`l`/complete workflow
(2.7 → 10.7 ms) regressed because the old directory-snapshot cache is gone.

Verification (macOS ARM64, `Dockerfile.test` aarch64-musl image):

- `cargo test -p xshi`: 288 lib + 288 bin + 5 CLI + 1 stdlib tests, both hosts.
- `runtime::interactive::`: 193 differential scenarios plus 37 `xshi`-only tests,
  no `#[ignore]`, both hosts. With `XSHI_PARITY_ISH_BIN` set, `ish` == golden ==
  `xshi` for every scenario on macOS and on Linux (goldens under
  `tests/fixtures/interactive-parity/{macos,linux}/`).
- `cargo test --lib runtime::process` (new `ChildDup` ordering test), the rest of
  `runtime::` and the native corpus: the failures that remain also fail on the
  untouched `HEAD` tree (`system-report` collectors, `fs.xsh`, the readlink
  fixture, corpus formatting of the `system-report` files, and on Linux two
  `netlink` unit tests, the net descriptor test, and the `linux_priv` feature
  target's missing `serde_json`).
- `Dockerfile.test` gained an empty `libutil.a` per musl target so the PTY
  harness links.

No formatter, linter, or autofixer was run.

## Deliberate deviations from ish

Where `ish`'s README documents behavior its implementation lacks, `xshi`
implements the README (checked against the built `ish`):

- `&>`/`&>>`/`|&`: `ish` treats `&>` as a background marker and rejects `|&`.
- `**` recursion, no-match glob error, quoted glob characters literal: `ish`
  passes patterns through and globs inside quotes.
- `$NAME` Tab completion: `ish` completes files.
- `fg` continues a list stopped mid-way; second `exit` forces past a suspended
  job (and `SIGTERM`+`SIGCONT` reaches the job): `ish` clears the warning before
  reading the line, so only Ctrl-D forces.

Other differences:

- History is safe under concurrent shells: the ported store lost entries when
  shells compacted around one another, skipped records appended between a
  shell's sync and its own append, parsed torn records as commands, stalled on
  invalid UTF-8, and could panic on a 64 KiB multibyte command. Loading no
  longer writes a cache.
- `ssh` remote-path completion passes the host as one argument and never builds
  a local shell string.
- `-c COMMAND` is the command flag; `--config PATH` replaces `ish`'s `-c PATH`.
- `eval` and `exec` are implemented (`exec` runs the command, then leaves);
  shell functions, `trap`, `readonly`, `shift`, `local`, and `init.sh` are not;
  `set -e`-style options are accepted and ignored.
- `xshi`-only: XSH source at the prompt, `bg`, trailing `&`, `z`, `:`.
- State lives in `xshi`'s namespace (`~/.config/xshi`, `~/.local/share/xshi`,
  `~/.cache/xshi`), the layout dump builtin is `xshi-dump`, and messages use the
  `xshi:` prefix. `~/.local/share/xshi/history.bin` written by the previous
  `xshi` (format `XH`) is unreadable to the new code and reported as corrupt
  until `history rebuild`; nothing of `ish`'s files is ever touched.

## Source-test → equivalent-test checklist

`[ ]` open · `[x]` equivalent passing · `[~]` intentionally not applicable
(reason in the row). Kinds: `unit` (in `src/`), `int` (`tests/integration.rs`),
`pty` (`tests/pty.rs`).


### `src/config.rs` (1)

- [x] unit `config_path_uses_non_utf8_home` → `config::tests::config_path_uses_non_utf8_home`

### `src/frecency.rs` (4)

- [x] unit `extract_cd_simple` → `z::tests::extract_cd_simple`
- [x] unit `extract_cd_from_command_list` → `z::tests::extract_cd_from_command_list`
- [x] unit `extract_cd_no_match` → `z::tests::extract_cd_no_match`
- [x] unit `extract_cd_dash` → `z::tests::extract_cd_dash`

### `src/denv.rs` (16)

- [x] unit `parse_dotenv_skips_empty_and_comments` → `denv::tests::parse_dotenv_skips_empty_and_comments`
- [x] unit `parse_dotenv_plain_and_export_prefix` → `denv::tests::parse_dotenv_plain_and_export_prefix`
- [x] unit `parse_dotenv_double_quotes_unescape_common_sequences` → `denv::tests::parse_dotenv_double_quotes_unescape_common_sequences`
- [x] unit `parse_dotenv_single_quotes_are_literal` → `denv::tests::parse_dotenv_single_quotes_are_literal`
- [x] unit `parse_env_null_skips_shell_internal_vars` → `denv::tests::parse_env_null_skips_shell_internal_vars`
- [x] unit `diff_sorted_env_reports_sets_and_unsets` → `denv::tests::diff_sorted_env_reports_sets_and_unsets`
- [~] unit `escape_roundtrip_preserves_newlines_and_backslashes` → xshi keeps denv restore state in memory (no `active_<pid>` file to escape)
- [x] unit `push_sh_escaped_handles_single_quotes` → `denv::tests::push_sh_escaped_handles_single_quotes`
- [x] unit `parse_denv_state_supports_spaces_in_dir` → `denv::tests::parse_denv_state_supports_spaces_in_dir`
- [~] unit `apply_bash_output_bench_counts_directives` → tested a benchmark-only parser for the old export/unset text protocol; xshi diffs `env -0` snapshots instead
- [x] unit `first_line_uses_bash_detects_substring` → `denv::tests::first_line_uses_bash_detects_substring`
- [x] unit `eval_env_uses_epsh_for_non_bash_envrc` → `denv::tests::sh_envrc_is_evaluated_by_sh` (a real `sh` replaces the embedded epsh)
- [x] unit `eval_env_bash_uses_explicit_store_environment` → `denv::tests::bash_envrc_sees_only_the_explicit_environment`
- [x] unit `eval_env_epsh_restores_process_env_after_failure` → `denv::tests::failing_envrc_reports_a_failed_evaluation_and_changes_nothing`
- [~] unit `restore_process_env_preserves_non_utf8_entries` → there is no process-environment mirror: the session environment is already a byte map (`session.env`)
- [x] unit `state_var_fast_path_ok_uses_cached_dir` → `denv::tests::state_fast_path_uses_the_cached_directory`

### `src/complete.rs` (16)

- [x] unit `host_flag` → `complete::tests::host_flag`
- [~] unit `shell_escape_no_quotes` → remote completion no longer builds a local shell string; see `complete::tests::single_quote_wraps_and_escapes_embedded_quotes`
- [x] unit `shell_escape_with_quotes` → `complete::tests::single_quote_wraps_and_escapes_embedded_quotes`
- [x] unit `split_path_no_slash` → `complete::tests::split_path_no_slash`
- [x] unit `split_path_with_dir` → `complete::tests::split_path_with_dir`
- [x] unit `grid_computation` → `complete::tests::grid_computation`
- [x] unit `candidate_completion_prefers_prefix_matches` → `complete::tests::candidate_completion_prefers_prefix_matches`
- [x] unit `partial_path_resolves_this_repo` → `complete::tests::partial_path_resolves_this_repo`
- [x] unit `partial_path_two_levels` → `complete::tests::partial_path_two_levels`
- [x] unit `partial_path_complete_finds_entries` → `complete::tests::partial_path_complete_finds_entries`
- [x] unit `partial_path_existing_dir_returns_empty` → `complete::tests::partial_path_existing_dir_returns_empty`
- [x] unit `partial_path_nonexistent_returns_empty` → `complete::tests::partial_path_nonexistent_returns_empty`
- [x] unit `contains_icase_basic` → `complete::tests::contains_icase_basic`
- [x] unit `substring_fallback_finds_toml` → `complete::tests::substring_fallback_finds_toml`
- [x] unit `prefix_match_preferred_over_substring` → `complete::tests::prefix_match_preferred_over_substring`
- [x] unit `partial_path_absolute` → `complete::tests::partial_path_absolute`

### `src/input.rs` (6)

- [x] unit `modifier_parsing` → `input::tests::modifier_parsing`
- [x] unit `csi_delete_with_ctrl_modifier_maps_to_ctrl_delete` → `input::tests::csi_delete_with_ctrl_modifier_maps_to_ctrl_delete`
- [x] unit `csi_delete_with_caret_suffix_maps_to_ctrl_delete` → `input::tests::csi_delete_with_caret_suffix_maps_to_ctrl_delete`
- [x] unit `paste_start_end_resets_state` → `input::tests::paste_start_end_resets_state`
- [x] unit `paste_accumulation_stops_at_limit` → `input::tests::paste_accumulation_stops_at_limit`
- [x] unit `blocking_poll_waits_for_signal` → `input::tests::blocking_poll_waits_for_signal`

### `src/history.rs` (16, plus the store tests in `history/tests.rs`)

- [x] unit `subsequence` → `history::tests::subsequence`
- [x] unit `subsequence_no_match` → `history::tests::subsequence_no_match`
- [x] unit `history_path_uses_non_utf8_home` → `history::tests::history_path_uses_non_utf8_home`
- [x] unit `recency_breaks_ties_within_same_tier` → `history::tests::recency_breaks_ties_within_same_tier`
- [x] unit `prefix_tier_beats_boundary_substring` → `history::tests::prefix_tier_beats_boundary_substring`
- [x] unit `boundary_substring_tier_beats_plain_substring` → `history::tests::boundary_substring_tier_beats_plain_substring`
- [x] unit `substring_tier_beats_subsequence_fallback` → `history::tests::substring_tier_beats_subsequence_fallback`
- [x] unit `search_into_sorts_before_limit` → `history::tests::search_into_sorts_before_limit`
- [x] unit `cwd_weight_prefers_ancestor_entries` → `history::tests::cwd_weight_prefers_ancestor_entries`
- [x] unit `cwd_metadata_round_trips_in_text_and_cache` → `history::tests::record_round_trips_metadata_with_awkward_cwd` and `history::tests::cache_round_trips_metadata_and_pre_1998_timestamps`
- [x] unit `legacy_history_has_unknown_cwd` → `history::tests::legacy_history_has_unknown_cwd`
- [x] unit `parallel_vecs_sync_after_add` → `history::tests::parallel_vecs_sync_after_add`
- [x] unit `timestamps_are_set` → `history::tests::timestamps_are_set`
- [~] unit `v4_round_trip` — the v1–v4 cache generations are not read (clean break); `history::tests::cache_with_any_structural_damage_is_rejected` pins that an older magic is unreadable and `cache_layout_is_the_ish_v5_format` pins the v5 layout
- [x] unit `structured_log_load_preserves_metadata` → `history::tests::loading_deduplicates_keeping_the_latest_use`
- [~] unit `render_history_file_strips_metadata` — `render_history_file` served ish's external `history | cmd`; `history` is an internal builtin here, so the function was removed (`history_command_forms` covers the output)

### `src/render.rs` (17)

- [x] unit `prompt_info_no_wrap` → `render::tests::prompt_info_no_wrap`
- [x] unit `prompt_info_partial_wrap` → `render::tests::prompt_info_partial_wrap`
- [x] unit `prompt_info_exact_boundary` → `render::tests::prompt_info_exact_boundary`
- [x] unit `prompt_info_exact_boundary_with_suggestion` → `render::tests::prompt_info_exact_boundary_with_suggestion`
- [x] unit `prompt_info_two_full_rows` → `render::tests::prompt_info_two_full_rows`
- [x] unit `multiline_exact_boundary` → `render::tests::multiline_exact_boundary`
- [x] unit `multiline_no_force_wrap` → `render::tests::multiline_no_force_wrap`
- [x] unit `multiline_trailing_newline_keeps_cursor_on_empty_continuation` → `render::tests::multiline_trailing_newline_keeps_cursor_on_empty_continuation`
- [x] unit `multiline_cursor_accounts_for_wrapped_first_segment` → `render::tests::multiline_cursor_accounts_for_wrapped_first_segment`
- [x] unit `history_pager_tracks_wrapped_query_cursor_row` → `render::tests::history_pager_tracks_wrapped_query_cursor_row`
- [x] unit `history_pager_clears_from_top_on_wrapped_rerender` → `render::tests::history_pager_clears_from_top_on_wrapped_rerender`
- [x] unit `smart_restore_prefers_anchor_when_query_stays_on_first_row` → `render::tests::smart_restore_prefers_anchor_when_query_stays_on_first_row`
- [x] unit `smart_restore_prefers_end_when_cursor_is_deeper_in_region` → `render::tests::smart_restore_prefers_end_when_cursor_is_deeper_in_region`
- [x] unit `history_row_groups_contiguous_match_highlights` → `render::tests::history_row_groups_contiguous_match_highlights`
- [x] unit `dir_picker_places_cursor_after_header` → `render::tests::dir_picker_places_cursor_after_header`
- [x] unit `completion_grid_does_not_pad_last_column` → `render::tests::completion_grid_does_not_pad_last_column`
- [x] unit `completion_grid_truncates_entry_to_terminal_width` → `render::tests::completion_grid_truncates_entry_to_terminal_width`

### `src/prompt.rs` (6)

- [x] unit `pwd_shortens_middle` → `prompt::tests::pwd_shortens_middle`
- [x] unit `pwd_home_exactly` → `prompt::tests::pwd_home_exactly`
- [x] unit `pwd_preserves_dot` → `prompt::tests::pwd_preserves_dot`
- [x] unit `pwd_root` → `prompt::tests::pwd_root`
- [x] unit `pwd_outside_home` → `prompt::tests::pwd_outside_home`
- [x] unit `pwd_no_false_tilde` → `prompt::tests::pwd_no_false_tilde`

### `src/line.rs` (10)

- [x] unit `char_widths` → `line::tests::char_widths`
- [x] unit `str_widths` → `line::tests::str_widths`
- [x] unit `display_with_fullwidth` → `line::tests::display_with_fullwidth`
- [x] unit `insert_and_cursor` → `line::tests::insert_and_cursor`
- [x] unit `set_with_cursor_position` → `line::tests::set_with_cursor_position`
- [x] unit `delete_back` → `line::tests::delete_back`
- [x] unit `move_word` → `line::tests::move_word`
- [x] unit `kill_word_back` → `line::tests::kill_word_back`
- [x] unit `kill_word_forward` → `line::tests::kill_word_forward`
- [x] unit `kill_to_start_and_yank` → `line::tests::kill_to_start_and_yank`

### `tests/integration.rs` (127)

- [x] int `line_buffer_insert_middle` → `ported_tests::line_buffer_insert_middle`
- [x] int `line_buffer_delete_forward` → `ported_tests::line_buffer_delete_forward`
- [x] int `line_buffer_utf8_handling` → `ported_tests::line_buffer_utf8_handling`
- [x] int `line_buffer_word_operations_complex` → `ported_tests::line_buffer_word_operations_complex`
- [x] int `line_buffer_kill_word_forward` → `ported_tests::line_buffer_kill_word_forward`
- [x] int `line_buffer_kill_yank_cycle` → `ported_tests::line_buffer_kill_yank_cycle`
- [x] int `line_buffer_kill_to_end` → `ported_tests::line_buffer_kill_to_end`
- [x] int `line_buffer_empty_operations` → `ported_tests::line_buffer_empty_operations`
- [x] int `line_buffer_set_resets_cursor` → `ported_tests::line_buffer_set_resets_cursor`
- [x] int `line_buffer_insert_str` → `ported_tests::line_buffer_insert_str`
- [x] int `line_buffer_grid_navigation_preserves_terminal_column` → `ported_tests::line_buffer_grid_navigation_preserves_terminal_column`
- [x] int `line_buffer_grid_navigation_ignores_prompt_only_rows` → `ported_tests::line_buffer_grid_navigation_ignores_prompt_only_rows`
- [x] int `history_dedup_on_add` → `ported_tests::history_dedup_on_add`
- [x] int `history_prefix_search_recency` → `ported_tests::history_prefix_search_recency`
- [x] int `history_fuzzy_search_ordering` → `ported_tests::history_fuzzy_search_ordering`
- [x] int `history_fuzzy_case_insensitive` → `ported_tests::history_fuzzy_case_insensitive`
- [x] int `history_fuzzy_empty_query_returns_all` → `ported_tests::history_fuzzy_empty_query_returns_all`
- [x] int `history_fuzzy_match_positions_correct` → `ported_tests::history_fuzzy_match_positions_correct`
- [x] int `scoring_contiguous_beats_scattered` → `ported_tests::scoring_contiguous_beats_scattered`
- [x] int `scoring_word_boundary_preferred` → `ported_tests::scoring_word_boundary_preferred`
- [x] int `scoring_exact_command_name` → `ported_tests::scoring_exact_command_name`
- [x] int `scoring_path_components` → `ported_tests::scoring_path_components`
- [x] int `scoring_flag_matching` → `ported_tests::scoring_flag_matching`
- [x] int `scoring_pwd_bonus` → `ported_tests::scoring_pwd_bonus`
- [x] int `scoring_git_workflow` → `ported_tests::scoring_git_workflow`
- [x] int `scoring_docker_compose` → `ported_tests::scoring_docker_compose`
- [x] int `scoring_equal_quality_uses_recency` → `ported_tests::scoring_equal_quality_uses_recency`
- [x] int `scoring_long_path_not_destroyed_by_gaps` → `ported_tests::scoring_long_path_not_destroyed_by_gaps`
- [x] int `scoring_npm_scripts` → `ported_tests::scoring_npm_scripts`
- [x] int `optimal_alignment_finds_tight_window` → `ported_tests::optimal_alignment_finds_tight_window`
- [x] int `optimal_alignment_prefers_contiguous_suffix` → `ported_tests::optimal_alignment_prefers_contiguous_suffix`
- [x] int `optimal_alignment_single_char` → `ported_tests::optimal_alignment_single_char`
- [x] int `optimal_alignment_full_string_match` → `ported_tests::optimal_alignment_full_string_match`
- [x] int `optimal_alignment_real_world_path` → `ported_tests::optimal_alignment_real_world_path`
- [x] int `scoring_first_match_bonus` → `ported_tests::scoring_first_match_bonus`
- [x] int `scoring_first_match_with_optimal_alignment` → `ported_tests::scoring_first_match_with_optimal_alignment`
- [x] int `scoring_into_matches_search` → `ported_tests::scoring_into_matches_search`
- [x] int `scoring_into_respects_limit_without_changing_order` → `ported_tests::scoring_into_respects_limit_without_changing_order`
- [x] int `history_add_whitespace_only_ignored` → `ported_tests::history_add_whitespace_only_ignored`
- [x] int `grid_single_entry` → `ported_tests::grid_single_entry`
- [x] int `grid_fits_multiple_columns` → `ported_tests::grid_fits_multiple_columns`
- [x] int `grid_narrow_terminal_forces_single_column` → `ported_tests::grid_narrow_terminal_forces_single_column`
- [x] int `grid_empty_entries` → `ported_tests::grid_empty_entries`
- [x] int `completion_state_navigation_wraps` → `ported_tests::completion_state_navigation_wraps`
- [x] int `completion_state_left_right_wrap` → `ported_tests::completion_state_left_right_wrap`
- [x] int `comp_entry_display_name_dir_suffix` → `ported_tests::comp_entry_display_name_dir_suffix`
- [x] int `comp_entry_display_name_file` → `ported_tests::comp_entry_display_name_file`
- [x] int `complete_path_finds_files` → `ported_tests::complete_path_finds_files`
- [x] int `complete_path_dirs_only` → `ported_tests::complete_path_dirs_only`
- [x] int `complete_path_hidden_files` → `ported_tests::complete_path_hidden_files`
- [x] int `complete_path_nonexistent_dir` → `ported_tests::complete_path_nonexistent_dir`
- [x] int `prompt_shorten_deep_path` → `ported_tests::prompt_shorten_deep_path`
- [x] int `prompt_shorten_single_component` → `ported_tests::prompt_shorten_single_component`
- [x] int `prompt_shorten_empty_home` → `ported_tests::prompt_shorten_empty_home`
- [x] int `prompt_shorten_home_prefix_not_subdir` → `ported_tests::prompt_shorten_home_prefix_not_subdir`
- [x] int `input_modifier_combinations` → `ported_tests::input_modifier_combinations`
- [x] int `line_buffer_boundary_conditions` → `ported_tests::line_buffer_boundary_conditions`
- [x] int `history_subsequence_no_infinite_loop` → `ported_tests::history_subsequence_no_infinite_loop`
- [x] int `prompt_shorten_pwd_adversarial` → `ported_tests::prompt_shorten_pwd_adversarial`
- [x] int `config_load_all_paths` → `ported_tests::config_load_all_paths`
- [x] int `prompt_display_len_no_ansi` → `ported_tests::prompt_display_len_no_ansi`
- [x] int `prompt_display_len_with_ansi` → `ported_tests::prompt_display_len_with_ansi`
- [x] int `prompt_display_len_multiple_escapes` → `ported_tests::prompt_display_len_multiple_escapes`
- [x] int `prompt_display_len_utf8` → `ported_tests::prompt_display_len_utf8`
- [x] int `prompt_render_status_colors` → `ported_tests::prompt_render_status_colors`
- [x] int `prompt_render_dirty_indicator` → `ported_tests::prompt_render_dirty_indicator`
- [x] int `prompt_invalidate_git` → `ported_tests::prompt_invalidate_git`
- [x] int `prompt_default_impl` → `ported_tests::prompt_default_impl`
- [x] int `prompt_git_branch_in_git_repo` → `ported_tests::prompt_git_branch_in_git_repo`
- [x] int `prompt_render_multiple` → `ported_tests::prompt_render_multiple`
- [x] int `builtin_is_builtin_known` → `ported_tests::builtin_is_builtin_known`
- [x] int `builtin_is_builtin_unknown` → `ported_tests::builtin_is_builtin_unknown`
- [x] int `alias_set_get` → `ported_tests::alias_set_get`
- [x] int `alias_override` → `ported_tests::alias_override`
- [x] int `alias_iter` → `ported_tests::alias_iter`
- [x] int `alias_default_impl` → `ported_tests::alias_default_impl`
- [x] int `history_load_empty` → `ported_tests::history_load_empty`
- [x] int `history_add_with_newlines` → `ported_tests::history_add_with_newlines`
- [x] int `history_add_dedup_preserves_order` → `ported_tests::history_add_dedup_preserves_order`
- [x] int `history_get_and_len` → `ported_tests::history_get_and_len`
- [x] int `history_from_entries_empty` → `ported_tests::history_from_entries_empty`
- [x] int `history_prefix_search_no_match` → `ported_tests::history_prefix_search_no_match`
- [x] int `history_fuzzy_match_positions_empty_query` → `ported_tests::history_fuzzy_match_positions_empty_query`
- [x] int `ls_list_dir_cwd` → `ported_tests::ls_list_dir_cwd`
- [x] int `ls_list_dir_file` → `ported_tests::ls_list_dir_file`
- [x] int `ls_list_dir_nonexistent` → `ported_tests::ls_list_dir_nonexistent`
- [x] int `ls_list_dir_tempdir` → `ported_tests::ls_list_dir_tempdir`
- [x] int `ls_list_dir_empty` → `ported_tests::ls_list_dir_empty`
- [x] int `ls_list_dir_symlink` → `ported_tests::ls_list_dir_symlink`
- [x] int `line_buffer_default_impl` → `ported_tests::line_buffer_default_impl`
- [x] int `line_buffer_display_len` → `ported_tests::line_buffer_display_len`
- [x] int `line_buffer_move_word_at_boundaries` → `ported_tests::line_buffer_move_word_at_boundaries`
- [x] int `line_buffer_kill_to_end_at_end` → `ported_tests::line_buffer_kill_to_end_at_end`
- [x] int `line_buffer_kill_to_start_at_start` → `ported_tests::line_buffer_kill_to_start_at_start`
- [x] int `line_buffer_kill_word_back_at_start` → `ported_tests::line_buffer_kill_word_back_at_start`
- [x] int `line_buffer_yank_empty_kill_ring` → `ported_tests::line_buffer_yank_empty_kill_ring`
- [x] int `line_buffer_word_movement_with_whitespace` → `ported_tests::line_buffer_word_movement_with_whitespace`
- [x] int `completion_state_selected_entry` → `ported_tests::completion_state_selected_entry`
- [x] int `completion_state_selected_entry_out_of_bounds` → `ported_tests::completion_state_selected_entry_out_of_bounds`
- [x] int `completion_move_with_zero_rows` → `ported_tests::completion_move_with_zero_rows`
- [x] int `completion_navigation_single_entry` → `ported_tests::completion_navigation_single_entry`
- [x] int `completion_move_right_wraps_to_first_col` → `ported_tests::completion_move_right_wraps_to_first_col`
- [x] int `completion_move_left_wraps_to_last_col` → `ported_tests::completion_move_left_wraps_to_last_col`
- [x] int `comp_entry_display_link` → `ported_tests::comp_entry_display_link`
- [x] int `comp_entry_display_exec` → `ported_tests::comp_entry_display_exec`
- [x] int `input_key_event_constructors` → `ported_tests::input_key_event_constructors`
- [x] int `input_modifiers_none` → `ported_tests::input_modifiers_none`
- [x] int `input_modifiers_default` → `ported_tests::input_modifiers_default`
- [x] int `input_modifier_from_param_zero` → `ported_tests::input_modifier_from_param_zero`
- [x] int `prompt_shorten_pwd_single_char_components` → `ported_tests::prompt_shorten_pwd_single_char_components`
- [x] int `prompt_shorten_pwd_dotfiles_in_middle` → `ported_tests::prompt_shorten_pwd_dotfiles_in_middle`
- [x] int `history_file_io` → `ported_tests::history_file_io`
- [x] int `history_reset_invalidates_existing_shell_cache` → `ported_tests::history_reset_invalidates_existing_shell_cache`
- [x] int `history_up_arrow_uses_session_start_boundary` → `ported_tests::history_up_arrow_uses_session_start_boundary`
- [x] int `history_ctrl_r_uses_session_start_boundary` → `ported_tests::history_ctrl_r_uses_session_start_boundary`
- [x] int `ls_list_dir_with_executable` → `ported_tests::ls_list_dir_with_executable`
- [x] int `ls_list_dir_with_symlink_to_dir` → `ported_tests::ls_list_dir_with_symlink_to_dir`
- [x] int `ls_list_symlink_to_dir_as_file` → `ported_tests::ls_list_symlink_to_dir_as_file`
- [x] int `ls_list_dir_sorts_case_insensitive` → `ported_tests::ls_list_dir_sorts_case_insensitive`
- [x] int `prompt_git_branch_detects_repo` → `ported_tests::prompt_git_branch_detects_repo`
- [x] int `prompt_git_branch_detached_head` → `ported_tests::prompt_git_branch_detached_head`
- [x] int `prompt_git_branch_no_repo` → `ported_tests::prompt_git_branch_no_repo`
- [x] int `prompt_git_cache_no_repo_reuse` → `ported_tests::prompt_git_cache_no_repo_reuse`
- [x] int `prompt_git_invalidate_clears_cache` → `ported_tests::prompt_git_invalidate_clears_cache`
- [x] int `prompt_git_bare_ref` → `ported_tests::prompt_git_bare_ref`
- [x] int `prompt_git_worktree_gitdir_file` → `ported_tests::prompt_git_worktree_gitdir_file`
- [x] int `complete_path_symlink` → `ported_tests::complete_path_symlink`

### `tests/pty.rs` (113)

- [x] pty `prompt_appears_on_startup` → `parity::scenarios::prompt_appears_on_startup`
- [x] pty `echo_command` → `parity::scenarios::echo_command`
- [x] pty `prompt_does_not_share_a_line_with_unterminated_external_output` → `parity::scenarios::prompt_does_not_share_a_line_with_unterminated_external_output`
- [x] pty `broken_interpreter_reports_bad_interpreter` → `parity::scenarios::broken_interpreter_reports_bad_interpreter`
- [x] pty `pwd_builtin` → `parity::scenarios::pwd_builtin`
- [x] pty `cd_and_pwd` → `parity::scenarios::cd_and_pwd`
- [x] pty `exit_with_ctrl_d` → `parity::scenarios::exit_with_ctrl_d`
- [x] pty `exit_command` → `parity::scenarios::exit_command`
- [x] pty `ctrl_c_cancels_input` → `parity::scenarios::ctrl_c_cancels_input`
- [x] pty `line_editing_backspace` → `parity::scenarios::line_editing_backspace`
- [x] pty `line_editing_up_down_navigate_wrapped_input` → `parity::scenarios::line_editing_up_down_navigate_wrapped_input`
- [x] pty `line_editing_ctrl_u` → `parity::scenarios::line_editing_ctrl_u`
- [x] pty `line_editing_ctrl_w` → `parity::scenarios::line_editing_ctrl_w`
- [x] pty `line_editing_ctrl_delete` → `parity::scenarios::line_editing_ctrl_delete`
- [x] pty `line_editing_ctrl_k_and_ctrl_y` → `parity::scenarios::line_editing_ctrl_k_and_ctrl_y`
- [x] pty `pipeline` → `parity::scenarios::pipeline`
- [x] pty `and_or_list` → `parity::scenarios::and_or_list`
- [x] pty `or_list` → `parity::scenarios::or_list`
- [x] pty `redirect_output` → `parity::scenarios::redirect_output`
- [x] pty `l_lists_files` → `parity::scenarios::l_lists_files`
- [x] pty `set_and_echo_var` → `parity::scenarios::set_and_echo_var`
- [x] pty `exported_var_reaches_external_commands` → `parity::scenarios::exported_var_reaches_external_commands`
- [x] pty `set_var_reaches_external_commands` → `parity::scenarios::set_var_reaches_external_commands`
- [x] pty `set_var_joins_multiple_value_words` → `parity::scenarios::set_var_joins_multiple_value_words`
- [x] pty `set_no_args_lists_env_vars` → `parity::scenarios::set_no_args_lists_env_vars`
- [x] pty `set_option_forms_fall_through_to_epsh` → `parity::scenarios::set_option_forms_fall_through_to_epsh`
- [x] pty `unset_removes_var_from_children` → `parity::scenarios::unset_removes_var_from_children`
- [x] pty `unset_removes_os_env_var_set_by_set` → `parity::scenarios::unset_removes_os_env_var_set_by_set`
- [x] pty `unset_in_same_line_not_inherited_by_children` → `parity::scenarios::unset_in_same_line_not_inherited_by_children`
- [x] pty `unset_removes_ambient_environment_from_children` → `parity::scenarios::unset_removes_ambient_environment_from_children`
- [x] pty `unset_removes_ambient_environment_in_pipeline_children` → `parity::scenarios::unset_removes_ambient_environment_in_pipeline_children`
- [x] pty `compound_list_export_reaches_child` → `parity::scenarios::compound_list_export_reaches_child`
- [x] pty `store_home_drives_interactive_cd` → `parity::scenarios::store_home_drives_interactive_cd`
- [x] pty `unset_oldpwd_blocks_interactive_cd_minus` → `parity::scenarios::unset_oldpwd_blocks_interactive_cd_minus`
- [x] pty `prefix_assignment_reaches_child_but_does_not_persist` → `parity::scenarios::prefix_assignment_reaches_child_but_does_not_persist`
- [x] pty `set_path_affects_command_lookup` → `parity::scenarios::set_path_affects_command_lookup`
- [x] pty `which_reflects_exported_path` → `parity::scenarios::which_reflects_exported_path`
- [x] pty `which_uses_store_path_not_os_env` → `parity::scenarios::which_uses_store_path_not_os_env`
- [x] pty `tilde_expansion` → `parity::scenarios::tilde_expansion`
- [x] pty `history_up_arrow` → `parity::scenarios::history_up_arrow`
- [x] pty `history_up_narrow_repaint_clears_wrapped_rows` → `parity::scenarios::history_up_narrow_repaint_clears_wrapped_rows`
- [x] pty `history_ctrl_r_search` → `parity::scenarios::history_ctrl_r_search`
- [x] pty `history_ctrl_r_ignores_later_global_entries` → `parity::scenarios::history_ctrl_r_ignores_later_global_entries`
- [x] pty `history_ctrl_r_escape_cancels` → `parity::scenarios::history_ctrl_r_escape_cancels`
- [x] pty `history_ctrl_r_narrow_repaint_does_not_stack_rows` → `parity::scenarios::history_ctrl_r_narrow_repaint_does_not_stack_rows`
- [x] pty `history_search_selection_preserves_cursor_after_typing` → `parity::scenarios::history_search_selection_preserves_cursor_after_typing`
- [x] pty `history_accept_reanchors_prompt_before_typing` → `parity::scenarios::history_accept_reanchors_prompt_before_typing`
- [x] pty `history_ctrl_r_near_bottom_keeps_pager_stable` → `parity::scenarios::history_ctrl_r_near_bottom_keeps_pager_stable`
- [x] pty `history_ctrl_r_scrolls_when_selection_passes_last_visible_entry` → `parity::scenarios::history_ctrl_r_scrolls_when_selection_passes_last_visible_entry`
- [x] pty `history_ctrl_r_near_bottom_query_edits_do_not_stack_headers` → `parity::scenarios::history_ctrl_r_near_bottom_query_edits_do_not_stack_headers`
- [x] pty `tab_completion_files` → `parity::scenarios::tab_completion_files`
- [x] pty `tab_completion_shows_grid` → `parity::scenarios::tab_completion_shows_grid`
- [x] pty `tab_completion_first_tab_has_no_selection_second_tab_selects_first` → `parity::scenarios::tab_completion_first_tab_has_no_selection_second_tab_selects_first`
- [x] pty `tab_completion_narrow_repaint_does_not_stack_rows` → `parity::scenarios::tab_completion_narrow_repaint_does_not_stack_rows`
- [x] pty `tab_completion_directory` → `parity::scenarios::tab_completion_directory`
- [x] pty `tab_completion_escape_restores_typed_prefix` → `parity::scenarios::tab_completion_escape_restores_typed_prefix`
- [x] pty `tab_completion_narrowing_does_not_autoaccept` → `parity::scenarios::tab_completion_narrowing_does_not_autoaccept`
- [x] pty `tab_completion_with_wide_dir_name_restores_prompt_cursor` → `parity::scenarios::tab_completion_with_wide_dir_name_restores_prompt_cursor`
- [x] pty `completion_resize_rerenders_grid` → `parity::scenarios::completion_resize_rerenders_grid`
- [x] pty `normal_resize_reanchors_wrapped_prompt` → `parity::scenarios::normal_resize_reanchors_wrapped_prompt`
- [x] pty `history_resize_rerenders_pager` → `parity::scenarios::history_resize_rerenders_pager`
- [x] pty `alias_expansion` → `parity::scenarios::alias_expansion`
- [x] pty `alias_self_referencing_no_reexpand` → `parity::scenarios::alias_self_referencing_no_reexpand`
- [x] pty `alias_self_referencing_from_config` → `parity::scenarios::alias_self_referencing_from_config`
- [x] pty `alias_self_referencing_exec` → `parity::scenarios::alias_self_referencing_exec`
- [x] pty `alias_list` → `parity::scenarios::alias_list`
- [x] pty `alias_with_command_substitution` → `parity::scenarios::alias_with_command_substitution`
- [x] pty `alias_preserves_quoted_word` → `parity::scenarios::alias_preserves_quoted_word`
- [x] pty `which_builtin` → `parity::scenarios::which_builtin`
- [x] pty `which_external` → `parity::scenarios::which_external`
- [x] pty `which_alias` → `parity::scenarios::which_alias`
- [x] pty `error_status_colors_prompt` → `parity::scenarios::error_status_colors_prompt`
- [x] pty `nonexistent_command` → `parity::scenarios::nonexistent_command`
- [x] pty `script_mode_refused` → `crates/xshi/tests/cli.rs::script_mode_is_refused_like_the_reference_shell`
- [x] pty `source_nonexistent_error` → `parity::scenarios::source_nonexistent_error`
- [x] pty `ctrl_l_clears_screen` → `parity::scenarios::ctrl_l_clears_screen`
- [x] pty `multiline_continuation` → `parity::scenarios::multiline_continuation`
- [x] pty `multiline_completion_on_continuation_line` → `parity::scenarios::multiline_completion_on_continuation_line`
- [x] pty `dir_picker_narrow_repaint_does_not_stack_rows` → `parity::scenarios::dir_picker_narrow_repaint_does_not_stack_rows`
- [x] pty `config_file_loaded` → `parity::scenarios::config_file_loaded`
- [x] pty `prompt_shows_cwd` → `parity::scenarios::prompt_shows_cwd`
- [x] pty `cd_minus_goes_back` → `parity::scenarios::cd_minus_goes_back`
- [x] pty `cd_tilde_subdir` → `parity::scenarios::cd_tilde_subdir`
- [x] pty `implicit_cd_quoted_path` → `parity::scenarios::implicit_cd_quoted_path`
- [x] pty `l_tilde_subdir` → `parity::scenarios::l_tilde_subdir`
- [x] pty `unset_variable` → `parity::scenarios::unset_variable`
- [x] pty `glob_expansion` → `parity::scenarios::glob_expansion`
- [x] pty `l_glob_expansion` → `parity::scenarios::l_glob_expansion`
- [x] pty `quoted_string_preserves_spaces` → `parity::scenarios::quoted_string_preserves_spaces`
- [x] pty `single_quotes_no_expansion` → `parity::scenarios::single_quotes_no_expansion`
- [x] pty `history_persisted_across_commands` → `parity::scenarios::history_persisted_across_commands`
- [x] pty `history_help` → `parity::scenarios::history_help`
- [x] pty `history_autosuggest_ignores_later_global_entries` → `parity::scenarios::history_autosuggest_ignores_later_global_entries`
- [x] pty `true_and_false_builtins` → `parity::scenarios::true_and_false_builtins`
- [x] pty `denv_loads_allowed_envrc_on_cd` → `parity::scenarios::denv_loads_allowed_envrc_on_cd`
- [x] pty `denv_loads_dotenv_on_cd_without_allow` → `parity::scenarios::denv_loads_dotenv_on_cd_without_allow`
- [x] pty `denv_unloads_on_leave` → `parity::scenarios::denv_unloads_on_leave`
- [x] pty `denv_allow_applies_env` → `parity::scenarios::denv_allow_applies_env`
- [x] pty `denv_deny_removes_env_and_marks_dirty` → `parity::scenarios::denv_deny_removes_env_and_marks_dirty`
- [x] pty `denv_startup_loads_dotenv_in_initial_cwd` → `parity::scenarios::denv_startup_loads_dotenv_in_initial_cwd`
- [x] pty `denv_startup_loads_allowed_envrc_in_initial_cwd` → `parity::scenarios::denv_startup_loads_allowed_envrc_in_initial_cwd`
- [x] pty `denv_dotenv_overrides_envrc` → `parity::scenarios::denv_dotenv_overrides_envrc`
- [x] pty `denv_reload_after_reallow_picks_up_envrc_edit` → `parity::scenarios::denv_reload_after_reallow_picks_up_envrc_edit`
- [x] pty `denv_edit_envrc_invalidates_trust` → `parity::scenarios::denv_edit_envrc_invalidates_trust`
- [x] pty `denv_restores_preexisting_var_on_leave` → `parity::scenarios::denv_restores_preexisting_var_on_leave`
- [x] pty `denv_path_add_relative_dir` → `parity::scenarios::denv_path_add_relative_dir`
- [x] pty `denv_dotenv_helper_loads_env_file` → `parity::scenarios::denv_dotenv_helper_loads_env_file`
- [x] pty `denv_allow_requires_envrc` → `parity::scenarios::denv_allow_requires_envrc`
- [x] pty `job_suspend_and_resume` → `parity::scenarios::job_suspend_and_resume`
- [x] pty `bracketed_paste_over_limit_rejected` → `parity::scenarios::bracketed_paste_over_limit_rejected`
- [x] pty `bracketed_paste_agents_md_rejected` → `parity::scenarios::bracketed_paste_large_document_rejected` (the reference file is not in this repository; a generated large document stands in)
- [x] pty `bracketed_paste_under_limit_accepted` → `parity::scenarios::bracketed_paste_under_limit_accepted`
- [x] pty `bracketed_paste_exactly_at_limit_accepted` → `parity::scenarios::bracketed_paste_exactly_at_limit_accepted`
