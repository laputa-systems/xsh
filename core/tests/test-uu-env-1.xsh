##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_env.rs.

use support.uu as uu

proc signal_target(s: uu.Scene, args: List[Str], signal: Str, alive: Bool) [fs, process, env, error, time] -> Result[Unit, Error] {
  let command = uu.command(s, "env", args.extend(["sleep", "1000"]))?
  let child = spawn command?
  defer child.cancel(signal: "KILL", kill_after: 0ms)?
  time.sleep(500ms)?
  process.kill(child.pid, signal: signal)?
  time.sleep(100ms)?
  let done = process.wait_timeout([child], 0ms)?
  let running = done == null
  assert running == alive
  Ok()
}

# Start with SIGPIPE ignored, so env must restore its default in the command.
proc default_pipe(s: uu.Scene, option: Str) [fs, process, env, error] -> Result[Unit, Error] {
  let argv = uu.argv(s, "env", [Path(option), p"sh", p"-c", p"trap - PIPE; seq 999999 2>err | head -n1 > out"])?
  let words = [p"sh", p"-c", Path(r"""trap '' PIPE; exec "$@"; """), p"env-parent"].extend(argv)
  let status = process.run(process.command_argv(p"/bin/sh", words, s.root, {}, b"", uu.at(s, "stdout"), uu.at(s, "stderr")))?
  assert status.exit_code()? == 0
  uu.file_is(s, "out", "1\n")
  uu.file_is(s, "err", "")
  Ok()
}

# origin: uutils test_env::test_invalid_arg
test test_uu_env_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--definitely-invalid"])?
  uu.fails_with_code(r1, 125)
}

# origin: uutils test_env::test_flags_after_command
test test_uu_env_flags_after_command { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["echo", "-u=v"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is(r1, "-u=v\n")
  let r2 = uu.invoke(s, "env", ["printf", "%s-%s", "-Sfoo bar"])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  uu.stdout_is(r2, "-Sfoo bar-")
  let r3 = uu.invoke(s, "env", ["-i", "--", "-u=v"])?
  uu.succeeds(r3)
  uu.no_stderr(r3)
  uu.stdout_is(r3, "-u=v\n")
  let r4 = uu.invoke(s, "env", ["-C", "..", "echo", "-u=v"])?
  uu.succeeds(r4)
  uu.no_stderr(r4)
  uu.stdout_is(r4, "-u=v\n")
  let r5 = uu.invoke(s, "env", ["-C..", "echo", "-u=v"])?
  uu.succeeds(r5)
  uu.no_stderr(r5)
  uu.stdout_is(r5, "-u=v\n")
  let r6 = uu.invoke(s, "env", ["-iC", "..", "echo", "-u=v"])?
  uu.succeeds(r6)
  uu.no_stderr(r6)
  uu.stdout_is(r6, "-u=v\n")
  let r7 = uu.invoke(s, "env", ["-iC..", "echo", "-u=v"])?
  uu.succeeds(r7)
  uu.no_stderr(r7)
  uu.stdout_is(r7, "-u=v\n")
}

# origin: uutils test_env::test_env_help
test test_uu_env_env_help { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--help"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_contains(r1, "Mandatory arguments to long options are mandatory for short options too.")
}

# origin: uutils test_env::test_env_permissions
test test_uu_env_env_permissions { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "env", "empty", "empty")?
  let r1 = uu.invoke(s, "env", ["./empty"])?
  uu.fails_with_code(r1, 126)
  uu.stderr_is(r1, "env: './empty': Permission denied\n")
}

# origin: uutils test_env::test_split_string_into_args_one_argument_no_quotes
test test_uu_env_split_string_into_args_one_argument_no_quotes { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-S echo hello world"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "hello world\n")
}

# origin: uutils test_env::test_split_string_into_args_one_argument
test test_uu_env_split_string_into_args_one_argument { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-S echo \"hello world\""])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "hello world\n")
}

# origin: uutils test_env::test_split_string_into_args_s_escaping_challenge
test test_uu_env_split_string_into_args_s_escaping_challenge { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-S echo \"hello \\\"great\\\" world\""])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "hello \"great\" world\n")
}

# origin: uutils test_env::test_split_string_into_args_s_whitespace_handling
test test_uu_env_split_string_into_args_s_whitespace_handling { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-Sprintf x%sx\\n A \t B \u{b}\u{c}\r\n"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "xAx\nxBx\n")
}



# origin: uutils test_env::test_split_string_option_forms_match_gnu_required_argument_handling
test test_uu_env_split_string_option_forms_match_gnu_required_argument_handling { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-S", "printf x:%s\\n one two"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "x:one\nx:two\n")
  let r2 = uu.invoke(s, "env", ["--split-string", "printf x:%s\\n one two"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "x:one\nx:two\n")
  let r3 = uu.invoke(s, "env", ["--split-string=printf x:%s\\n one two"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "x:one\nx:two\n")
}

# origin: uutils test_env::test_split_string_single_quotes_keep_unknown_backslash_sequences_literal
test test_uu_env_split_string_single_quotes_keep_unknown_backslash_sequences_literal { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-Sprintf %s '\\x'"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\\x")
  let r2 = uu.invoke(s, "env", ["-Sprintf %s '\\a'"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "\\a")
  let r3 = uu.invoke(s, "env", ["-Sprintf %s '\\`'"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "\\`")
  let r4 = uu.invoke(s, "env", ["-Sprintf %s '\\q'"])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "\\q")
  let r5 = uu.invoke(s, "env", ["-Sprintf %s '\\|'"])?
  uu.succeeds(r5)
  uu.stdout_is(r5, "\\|")
  let r6 = uu.invoke(s, "env", ["-Sprintf %s '\\9'"])?
  uu.succeeds(r6)
  uu.stdout_is(r6, "\\9")
}

# origin: uutils test_env::test_split_string_backslash_a_behavior_matches_gnu_quoting_context
test test_uu_env_split_string_backslash_a_behavior_matches_gnu_quoting_context { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-Sprintf %s '\\a'"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\\a")
  let r2 = uu.invoke(s, "env", ["-Sprintf %s \"\\a\""])?
  uu.fails_with_code(r2, 125)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "invalid sequence '\\a' in -S")
  let r3 = uu.invoke(s, "env", ["-Sprintf %s \\a"])?
  uu.fails_with_code(r3, 125)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "invalid sequence '\\a' in -S")
}

# origin: uutils test_env::test_env_with_empty_executable_single_quotes
test test_uu_env_env_with_empty_executable_single_quotes { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-S''"])?
  uu.fails_with_code(r1, 127)
  uu.no_stdout(r1)
  uu.stderr_is(r1, "env: '': No such file or directory\n")
}

# origin: uutils test_env::test_env_with_empty_executable_double_quotes
test test_uu_env_env_with_empty_executable_double_quotes { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-S\"\""])?
  uu.fails_with_code(r1, 127)
  uu.no_stdout(r1)
  uu.stderr_is(r1, "env: '': No such file or directory\n")
}

# origin: uutils test_env::test_env_overwrite_arg0
test test_uu_env_env_overwrite_arg0 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--argv0", "hijacked", "sh", "-c", r"""echo $0"""])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "hijacked\n")
}

# origin: uutils test_env::test_env_arg_argv0_overwrite
test test_uu_env_env_arg_argv0_overwrite { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--argv0", "dirname", "--argv0", "hijacked", "sh", "-c", r"""echo $0"""])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "hijacked\n")
  let r2 = uu.invoke(s, "env", ["-a", "dirname", "-a", "hijacked", "sh", "-c", r"""echo $0"""])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "hijacked\n")
  let r3 = uu.invoke(s, "env", ["--argv0", "dirname", "-a", "hijacked", "sh", "-c", r"""echo $0"""])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "hijacked\n")
  let r4 = uu.invoke(s, "env", ["-a", "dirname", "--argv0", "hijacked", "sh", "-c", r"""echo $0"""])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "hijacked\n")
}

# origin: uutils test_env::test_env_arg_argv0_overwrite_mixed_with_string_args
test test_uu_env_env_arg_argv0_overwrite_mixed_with_string_args { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["-S--argv0 dirname", "--argv0", "hijacked", "sh", "-c", r"""echo $0"""])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "hijacked\n")
  let r2 = uu.invoke(s, "env", ["-a", "dirname", r"""-S-a hijacked sh -c 'echo $0'"""])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "hijacked\n")
  let r3 = uu.invoke(s, "env", [r"""-S--argv0 dirname -a hijacked sh -c 'echo $0'"""])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "hijacked\n")
  let r4 = uu.invoke(s, "env", ["-S-a dirname", r"""-S--argv0 hijacked sh -c 'echo $0'"""])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "hijacked\n")
  let r5 = uu.invoke(s, "env", ["-a", "sleep", "-S-a dirname", "-a", "hijacked", "sh", "-c", r"""echo $0"""])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "hijacked\n")
}

# origin: uutils test_env::test_env_arg_ignore_signal_invalid_signals
test test_uu_env_env_arg_ignore_signal_invalid_signals { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--ignore-signal=banana"])?
  uu.fails_with_code(r1, 125)
  uu.stderr_contains(r1, "env: 'banana': invalid signal")
  let r2 = uu.invoke(s, "env", ["--ignore-signal=SIGbanana"])?
  uu.fails_with_code(r2, 125)
  uu.stderr_contains(r2, "env: 'SIGbanana': invalid signal")
  let r3 = uu.invoke(s, "env", ["--ignore-signal=exit"])?
  uu.fails_with_code(r3, 125)
  uu.stderr_contains(r3, "env: 'exit': invalid signal")
  let r4 = uu.invoke(s, "env", ["--ignore-signal=SIGexit"])?
  uu.fails_with_code(r4, 125)
  uu.stderr_contains(r4, "env: 'SIGexit': invalid signal")
}

# origin: uutils test_env::test_env_arg_ignore_signal_empty
test test_uu_env_env_arg_ignore_signal_empty { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--ignore-signal=", "echo", "hello"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_contains(r1, "hello")
}

# origin: uutils test_env::test_env_block_signal_flag
test test_uu_env_env_block_signal_flag { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--block-signal", "true"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
}

# origin: uutils test_env::test_env_block_realtime_signal
test test_uu_env_env_block_realtime_signal { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--block-signal=SIGRTMIN+7", "true"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  let r2 = uu.invoke(s, "env", ["--block-signal=SIGRTMAX-7", "true"])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
}

# origin: uutils test_env::test_emoji_env_vars
test test_uu_env_emoji_env_vars { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["🎯_VAR=Hello 🌍", "printenv", "🎯_VAR"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "Hello 🌍")
}

# origin: uutils test_env::test_shebang_error
test test_uu_env_shebang_error { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["'-v '"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "use -[v]S to pass options in shebang lines")
}

# origin: uutils test_env::test_simple_braced_variable
test test_uu_env_simple_braced_variable { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", [r"""-Secho ${TEST_VAR_12345}"""], vars: {"TEST_VAR_12345": "value"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "value\n")
}

# origin: uutils test_env::test_braced_variable_error_missing_closing_brace
test test_uu_env_braced_variable_error_missing_closing_brace { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", [r"""-Secho ${FOO"""])?
  uu.fails_with_code(r1, 125)
  uu.stderr_contains(r1, r"""only ${VARNAME} expansion is supported, error at: ${FOO""")
}

# origin: uutils test_env::test_braced_variable_error_rejects_default_syntax
test test_uu_env_braced_variable_error_rejects_default_syntax { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", [r"""-Secho ${FOO:-value}"""])?
  uu.fails_with_code(r1, 125)
  uu.stderr_contains(r1, r"""only ${VARNAME} expansion is supported""")
}

# origin: uutils test_env::test_braced_variable_error_starts_with_digit
test test_uu_env_braced_variable_error_starts_with_digit { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", [r"""-Secho ${1FOO}"""])?
  uu.fails_with_code(r1, 125)
  uu.stderr_contains(r1, r"""only ${VARNAME} expansion is supported, error at: ${1FOO}""")
}

# origin: uutils test_env::test_braced_variable_error_unexpected_character
test test_uu_env_braced_variable_error_unexpected_character { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", [r"""-Secho ${FOO?}"""])?
  uu.fails_with_code(r1, 125)
  uu.stderr_contains(r1, r"""only ${VARNAME} expansion is supported""")
}

# origin: uutils test_env::test_env_disallow_double_underscore_all
test test_uu_env_env_disallow_double_underscore_all { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--ignore-signal=__ALL__", "true"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid signal")
  let r2 = uu.invoke(s, "env", ["--default-signal=__ALL__", "true"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "invalid signal")
  let r3 = uu.invoke(s, "env", ["--block-signal=__ALL__", "true"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "invalid signal")
}

# origin: uutils test_env::test_echo
test test_uu_env_echo { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "env", ["echo", "FOO-bar"])?
  uu.succeeds(r)
  assert r.stdout.utf8()?.trim() == "FOO-bar"
}

# origin: uutils test_env::test_unset_invalid_variables
test test_uu_env_unset_invalid_variables { |ctx|
  let s = uu.scene(ctx)?
  for variable in ["", "a=b"] {
    let r = uu.invoke(s, "env", ["-u", variable])?
    uu.fails(r)
    uu.stderr_only(r, f"env: cannot unset '{variable}': Invalid argument\n")
  }
}

# origin: uutils test_env::test_single_name_value_pair
test test_uu_env_single_name_value_pair { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "env", ["FOO=bar"])?
  uu.succeeds(r)
  # The upstream iterator's boolean is discarded, so only status is asserted.
  let matching = [line for line in r.stdout.utf8()?.lines() if line == "FOO=bar"]
}

# origin: uutils test_env::test_multiple_name_value_pairs
test test_uu_env_multiple_name_value_pairs { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "env", ["FOO=bar", "ABC=xyz"])?
  uu.succeeds(r)
  let matching = [line for line in r.stdout.utf8()?.lines() if line in ["FOO=bar", "ABC=xyz"]]
  assert matching.len() == 2
}

# origin: uutils test_env::test_ignore_environment
test test_uu_env_ignore_environment { |ctx|
  let s = uu.scene(ctx)?
  for option in ["-i", "-"] {
    let r = uu.invoke(s, "env", [option])?
    uu.succeeds(r)
    uu.no_stdout(r)
  }
}

# origin: uutils test_env::test_null_delimiter
test test_uu_env_null_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "env", ["-i", "--null", "FOO=bar", "ABC=xyz"])?
  uu.succeeds(r)
  let variables = r.stdout.utf8()?.split("\0") |> sort
  assert variables == ["", "ABC=xyz", "FOO=bar"]
}

# origin: uutils test_env::test_unset_variable
test test_uu_env_unset_variable { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "env", ["-u", "HOME"], vars: {HOME: "FOO"})?
  uu.succeeds(r)
  assert [line for line in r.stdout.utf8()?.lines() if line.starts_with("HOME=")].is_empty()
}

# origin: uutils test_env::test_fail_null_with_program
test test_uu_env_fail_null_with_program { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "env", ["--null", "cd"])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot specify --null (-0) with command")
}

# origin: uutils test_env::test_change_directory
test test_uu_env_change_directory { |ctx|
  let s = uu.scene(ctx)?
  let directory = test.temp_dir(ctx, name: "destination")?
  assert directory != s.root
  let r = uu.invoke(s, "env", ["--chdir", directory.display(), "pwd"])?
  uu.succeeds(r)
  assert r.stdout.utf8()?.trim() == directory.display()
}

# origin: uutils test_env::test_fail_change_directory
test test_uu_env_fail_change_directory { |ctx|
  let s = uu.scene(ctx)?
  assert ! uu.at(s, "some_nonexistent_path").exists()?
  let r = uu.invoke(s, "env", ["--chdir", "some_nonexistent_path", "pwd"])?
  uu.fails(r)
  uu.stderr_contains(r, "env: cannot change directory to ")
}

# origin: uutils test_env::test_env_arg_ignore_signal_special_signals
test test_uu_env_env_arg_ignore_signal_special_signals { |ctx|
  let s = uu.scene(ctx)?
  for pair in [{name: "stop", number: 19}, {name: "kill", number: 9}, {name: "SToP", number: 19}, {name: "SIGKILL", number: 9}] {
    let r = uu.invoke(s, "env", [f"--ignore-signal={pair.name}", "echo", "hello"])?
    uu.fails_with_code(r, 125)
    uu.stderr_contains(r, f"env: failed to set signal action for signal {pair.number}: Invalid argument")
  }
}

# origin: uutils test_env::test_env_arg_ignore_signal_valid_signals
test test_uu_env_env_arg_ignore_signal_valid_signals { |ctx|
  let s = uu.scene(ctx)?
  signal_target(s, ["--ignore-signal=int"], "INT", true)?
  signal_target(s, ["--ignore-signal=usr2"], "USR2", true)?
  signal_target(s, ["--ignore-signal=int,usr2"], "USR1", false)?
}

# origin: uutils test_env::test_env_arg_ignore_signal_all_signals
test test_uu_env_env_arg_ignore_signal_all_signals { |ctx|
  let s = uu.scene(ctx)?
  signal_target(s, ["--ignore-signal"], "INT", true)?
}

# origin: uutils test_env::test_env_default_signal_pipe
test test_uu_env_env_default_signal_pipe { |ctx|
  let s = uu.scene(ctx)?
  default_pipe(s, "--default-signal=PIPE")?
}

# origin: uutils test_env::test_env_default_signal_all_signals
test test_uu_env_env_default_signal_all_signals { |ctx|
  let s = uu.scene(ctx)?
  default_pipe(s, "--default-signal")?
}

# origin: uutils test_env::test_env_list_signal_handling_reports_ignore
test test_uu_env_env_list_signal_handling_reports_ignore { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "env", ["--ignore-signal=INT", "--list-signal-handling", "true"])?
  uu.succeeds(r)
  let stderr = r.stderr.utf8()?
  assert "INT" in stderr and "IGNORE" in stderr
}

# origin: uutils test_env::disallow_equals_sign_on_short_unset_option
test test_uu_env_disallow_equals_sign_on_short_unset_option { |ctx|
  let s = uu.scene(ctx)?
  let equals = uu.invoke(s, "env", ["-u="])?
  uu.fails_with_code(equals, 125)
  uu.stderr_contains(equals, "env: cannot unset '=': Invalid argument")
  let name = uu.invoke(s, "env", ["-u=A1B2C3"])?
  uu.fails_with_code(name, 125)
  uu.stderr_contains(name, "env: cannot unset '=A1B2C3': Invalid argument")
  let split = uu.invoke(s, "env", ["--split-string=A1B=2C3="])?
  uu.succeeds(split)
  let empty = uu.invoke(s, "env", ["--unset="])?
  uu.fails_with_code(empty, 125)
  uu.stderr_contains(empty, "env: cannot unset '': Invalid argument")
}

# origin: uutils test_env::test_simulation_of_terminal_false
test test_uu_env_simulation_of_terminal_false { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "env", "is_a_tty.sh", "is_a_tty.sh")?
  let r = uu.invoke(s, "env", ["sh", "is_a_tty.sh"])?
  uu.succeeds(r)
  uu.stdout_is(r, "stdin is not a tty\nstdout is not a tty\nstderr is not a tty\n")
  uu.stderr_is(r, "This is an error message.\n")
}

# origin: uutils test_env::test_reject_shell_style_variable_expansions
test test_uu_env_reject_shell_style_variable_expansions { |ctx|
  let s = uu.scene(ctx)?
  for split in [r"-Secho $TEST_VAR_12345", r"-Secho ${TEST_VAR_12345:fallback}", r"-Secho ${TEST_VAR_12345:-fallback}", r"-Secho ${TEST_VAR_12345-default}"] {
    let r = uu.invoke(s, "env", [split], vars: {TEST_VAR_12345: "value"})?
    uu.fails_with_code(r, 125)
    uu.stderr_contains(r, r"only ${VARNAME} expansion is supported")
  }
}

# origin: uutils test_env::test_non_utf8_env_vars
test test_uu_env_non_utf8_env_vars { |ctx|
  let s = uu.scene(ctx)?
  let value = Path.parse_bytes(b"hello\x80world")?
  let r = uu.invoke(s, "env", [], vars: {NON_UTF8_VAR: value})?
  uu.succeeds(r)
  assert b"NON_UTF8_VAR='hello'$'\\200''world'" in r.stdout
}

# origin: uutils test_env::test_split_string_into_args_long_option_whitespace_handling
test test_uu_env_split_string_into_args_long_option_whitespace_handling { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "env", ["--split-string printf x%sx\\n A \t B \u{b}\u{c}\r\n"])?
  uu.fails_with_code(r1, 125)
  uu.no_stdout(r1)
  uu.stderr_is(r1, "env: unrecognized option '--split-string printf x%sx\\n A \t B \u{b}\u{c}\r\n'\nTry 'env --help' for more information.\n")
}
