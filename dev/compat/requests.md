# Lane Requests Backlog

What merged lanes asked the integrator for, kept here because lane reports do not
outlive the session. Nothing below is applied unless it says so. Exclusion
candidates are claims by the lane: verify each against GNU 9.12 source
(`/home/user/ref/gnu-coreutils/src/*.c` after `run-gnu.sh prepare`) before adding
it to `exclusions.json`, as was done for `basename -h/-V` and `tee -h`. Categories:
see `check_exclusions.py`.

## From `trivial` (merged; slice 30/343 -> 280/343)

**Integrator edits**

- Alias `[` -> `test` in `aliases.json`: if added, delete `core/[.xsh` (stage.py rejects an alias that collides with an applet). `core/[.xsh` and `core/test.xsh` are thin entries over `core/lib/testexpr.xsh` today.
- `gaps.json` has no entries for these utilities; the lane wrote none.
- Unfinished: sleep `test_sleep_stops_after_sigbus` and `test_sleep_stops_after_sigsegv` fail, uninvestigated. `xsht lint` style warnings remain.

**Runtime blockers** (record, do not work around)

- `src/entrypoints/xsh.rs:124` swallows a first `--` after the script path. Blocks echo `test_double_hyphens`, `test_double_hyphens_at_start`, `test_flag_like_arguments_which_are_no_flags`, `test_backslash_n_last_char_in_last_argument`; test `test_some_literals`; sleep `test_invalid_time_interval`, `test_negative_interval`, `test_invalid_duration` (cases 1-3), `test_valid_hex_duration::case_2_negative_zero`.
- Stdout flushing / EPIPE / SIGPIPE: yes `test_simple`, `test_args`, `test_long_output`, `test_long_odd_output`, `test_long_input`, `test_long_line_exceeds_pipe_capacity`, `test_piped_to_dev_full`; true/false `test_full`; tty `test_stdout_fail`, `test_write_error`. `yes` is bounded to 32 MiB of whole lines meanwhile.
- Non-UTF-8: echo `non_utf_8`; yes `test_non_utf8`; test `test_invalid_utf8_integer_compare`; printenv `test_non_utf8_value`, `test_non_utf8_env_vars` (`env.list` errors on such values).

**New native APIs wanted**

- `fs.stat` following symlinks with dev/ino/nanosecond times (native-fs now provides `fs.stat(path, follow_symlinks=...)`; wire `test -ef`, currently failing explicitly): test `test_file_is_itself`, `test_hard_link_is_same_file`, `test_same_device_inode`.
- `user.groups(name)` (getgrouplist): id `test_id_single_user`, `test_id_multiple_users`, `test_id_multiple_users_non_existing`; groups `test_groups_username`, `test_groups_username_multiple`.
- `user.login_name()` (getlogin) for logname; `system.hostid()` for hostid (reads `/etc/hostid` or returns 0 today); ordered `getgroups` in `unix.id`; online CPU count for `nproc --all` (uses `cpu.count()` today); u64 integers (nproc clamp is a special case).

**Bug in `core/lib/gnu.xsh`**: `gnu.quote` prints `''$'\t'` where GNU 9.12 prints `'\t'` (control characters inside a quoted name). Also prints `'$'\302\263''` for `'³'` in the C locale (found by text-a1).

**Exclusion candidates**

- uutils-extension: test `test_parenthesized_string_comparison` (GNU 9.12 also rejects `( ( != ) )`); test `diagnostics::*` (7 tests, feat_diagnostics); id `test_id_pretty_print_password_record`, `test_id_pretty_print_suid_binary` (`-p`/`-P`); sleep `test_sleep_when_input_has_only_whitespace_then_error::case_2_only_tab`, `::case_3_only_newline`, `test_sleep_when_input_has_trailing_whitespace_then_error::case_2_mixed_newlines_spaces_tabs` (they expect `$'\t'` quoting; recheck after the `gnu.quote` fix).
- clap-wording: arch `test_arch_help`; logname `test_help`; echo `full_version_argument`; test `test_bracket_syntax_version`; sleep `test_sleep_no_argument`; printenv `test_invalid_option_exit_code`.
- Environment (not exclusions by themselves): root-dependent test `test_file_is_not_readable`, `test_file_is_not_writable`, `test_file_not_owned_by_euid`, `test_file_not_owned_by_egid`, id `test_id_name`; logname `test_normal`, `test_output_format` need a login session or `CI=true`.

## From `text-a1` (merged; slice 43/361 -> 239/361; final O_APPEND octal-parse fix was untested against uutils at the lane's end, covered by the merge run)

**Runtime blockers**

- Stdout flushed only at exit and write errors invisible: cat `test_broken_pipe`, `test_cat_broken_pipe_nonzero_and_message`, `test_dev_full_show_all`, `test_piped_to_dev_full`, `test_version_help_dev_full`, `test_uchild_when_no_capture_reading_from_infinite_source`; tac `test_failed_write_is_reported`; head `test_write_to_dev_full`, `test_verbose_header_write_error_long_filename`; tail `test_failed_write_is_reported`, `test_failed_write_is_reported_on_seekable_input`, `test_when_output_closed_then_no_broken_pipe`, all `test_follow_*` and `test_retry*`; tee `test_pipe_error_*`, `test_space_error_exit*`, `test_tee_no_more_writeable_2`, `test_tee_output_not_buffered`, `test_tee_continues_after_short_read`.
- `io.stdin_bytes` reads all of stdin; no incremental read: head `test_validate_stdin_offset_lines`, `test_validate_stdin_offset_bytes`, tee/cat live-stdin tests. Stdin and non-seekable files are read whole by the applets.
- No native FIFO read (`read_bytes` on a FIFO returns nothing): cat `test_fifo_symlink`.
- Non-UTF-8 argv: cat `test_cat_non_utf8_paths`; head `test_head_non_utf8_paths`; tac `test_tac_non_utf8_paths`, `test_non_utf8_separator`, `test_non_utf8_regex_separator`; tail `test_obsolete_encoding_unix`.
- The cli parser trims leading dashes from option names, so `---presume-input-pipe` is pre-filtered in `core/lib/textio_a1.xsh`. Regex applies only to `Str`, so `tac -r` rejects non-UTF-8 input.
- Running as root defeats tail `test_permission_denied*` and tee `test_readonly`.

**Exclusion candidates**

- uutils-extension: head `test_all_but_last_*_huge_count_does_not_panic` (`-n=-N`); tail `test_small_file` (`-n -10` as one argument), `test_tail_obsolete_blocks` (bare `-b`), `test_tail_obsolete_f_flag` (`+f`); tac `test_null_separator` (empty separator; GNU errors) and the regex tests `test_regex`, `test_regex_before`, `test_regex_bare_anchors`, `test_regex_or_operator`, `test_regular_start_anchor`, `test_regular_end_anchor`, `test_unescaped_middle_anchor` (assert uutils regex-crate semantics; check whether GNU `tac -r` agrees before excluding).
- clap-wording: the three head `test_snippet_*`, tail `test_snippet_points_at_the_unknown_unit`, tail `test_follow_invalid_pid`; tail `test_args_sleep_interval_when_illegal_argument_then_usage_error` cases and head `test_head_invalid_num` fail only because uutils expects `'³'` unquoted plus a Try-line.

**Not investigated:** cat `test_write_fast_*`, `test_closes_file_descriptors`; tail `test_bytes_for_funny_unix_files`. `rev` also takes `-0`, `-h`, `-V` (not a uutils utility); `tee` dropped `--input`.

## From `native-fs` (merged)

- Done by the integrator: `make docs`.
- Optional: make `fs.copy` a wrapper over `fs.copy_file`; switch `gnu.errno` to the new `failure.errno`.
- Untested: the macOS paths (`rename_noreplace`, reflink, sparse seek) are written but never run. `follow_symlinks: false` on `fs.chmod` fails with `EOPNOTSUPP` for a symlink (Linux has no no-follow chmod).

## From `sysreport-extract` (merged)

- Point `tests/xsh/system-report*.xsh` at the `sys_*` modules, then delete the thin compatibility wrappers in `core/lib/system_report_*.xsh`.
- Mention the `sys_*` modules in `core/README.md`.
- Remaining collectors, in order: `devices`, `sensors`, `network`, `cpu`, `memory`, `kernel`, `processes`, `power`, `firmware`, `cgroups`.
