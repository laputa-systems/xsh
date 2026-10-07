type EnvResult = {status: Int, stdout: Str, stderr: Str}

proc env_run(ctx: TestContext, args: List[Str], unset_probe: Bool = false) [fs, process, error] -> Result[EnvResult] {
  let root = test.temp_dir(ctx, name: "env-argv")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/env.xsh"
  let argv = [ctx.xsh_bin.display(), "--", script.display()].extend(args)
  let command = if unset_probe {
    process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", PATH: "/bin:/usr/bin", XSH_TEST_UNSET: "present"}, b"", stdout, stderr)
  } else {
    process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", PATH: "/bin:/usr/bin"}, b"", stdout, stderr)
  }
  let status = process.run(command)?
  {status: status.exit_code()?, stdout: stdout.read_text()?, stderr: stderr.read_text()?}
}

test test_env_assignment_runs_command { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- XSH_MODULE_PATH=ok ${ctx.xsh_bin} \
    fp"{ctx.core_dir}/printenv.xsh" -- XSH_MODULE_PATH
  assert output.trim() == "ok"
}

test test_env_split_string_runs_command { |ctx|
  let script = fp"{ctx.core_dir}/printenv.xsh"
  let command = f"XSH_MODULE_PATH=split {ctx.xsh_bin} {script} -- XSH_MODULE_PATH"
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- "-S" $command
  assert output.trim() == "split"
}

test test_env_split_string_as_single_shebang_arg_runs_command { |ctx|
  let script = fp"{ctx.core_dir}/printenv.xsh"
  let command = f"-S XSH_MODULE_PATH=split {ctx.xsh_bin} {script} -- XSH_MODULE_PATH"
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- $command
  assert output.trim() == "split"
}

test test_env_uses_direct_xsh_shebang { |ctx|
  assert fp"{ctx.core_dir}/env.xsh".read_text()?.starts_with("#!/bin/xsh")
}

test test_env_split_string_preserves_quoted_argument { |ctx|
  let command = f"-S {ctx.xsh_bin} {ctx.core_dir}/printf.xsh -- %s \"hello world\""
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- $command

  assert output == "hello world"
}

test test_env_chdir_runs_command_in_selected_directory { |ctx|
  let directory = test.temp_dir(ctx, name: "env-cwd")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- -C ${directory} pwd

  assert output.trim() == directory.display()
}

test test_env_null_environment_list_uses_nul_separators { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" -- -0

  assert "PATH=" in output
  assert "\0" in output
  assert ! output.ends_with("\n")
}

test test_env_forwards_non_utf8_command_arguments { |ctx|
  let root = test.temp_dir(ctx, name: "env-raw-argv")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let raw_format_stdout = fp"{root}/raw-format-stdout"
  let raw_format_stderr = fp"{root}/raw-format-stderr"
  let printf = fp"{ctx.core_dir}/printf.xsh"
  let env_script = fp"{ctx.core_dir}/env.xsh"
  let invalid = Path.parse_bytes(b"\xff")?
  let argv: List[Path] = [ctx.xsh_bin, p"--", env_script, ctx.xsh_bin, p"--", printf, p"%s", invalid]
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", stdout, stderr))?
  let raw_format = Path.parse_bytes(b"Swer \xff")?
  let raw_format_argv: List[Path] = [ctx.xsh_bin, p"--", env_script, ctx.xsh_bin, p"--", printf, raw_format]
  let raw_format_status = process.run(process.command_argv(ctx.xsh_bin, raw_format_argv, root, {LC_ALL: "C"}, b"", raw_format_stdout, raw_format_stderr))?

  assert status.exit_code()? == 0
  assert stdout.read_bytes()? == b"\xff"
  assert stderr.read_text()? == ""
  assert raw_format_status.exit_code()? == 0
  assert raw_format_stdout.read_bytes()? == b"Swer \xff"
  assert raw_format_stderr.read_text()? == ""
}

test test_env_split_string_rejects_unsupported_variable_syntax { |ctx|
  let missing_brace = env_run(ctx, [r"-Secho ${FOO"])?
  let default_value = env_run(ctx, [r"-Secho ${FOO:-value}"])?
  let digit_name = env_run(ctx, [r"-Secho ${1FOO}"])?
  let unexpected_character = env_run(ctx, [r"-Secho ${FOO?}"])?
  let unbraced = env_run(ctx, ["-Secho $TEST_VAR_12345"])?

  assert missing_brace.status == 125
  assert missing_brace.stdout == ""
  assert missing_brace.stderr == r"env: only ${VARNAME} expansion is supported, error at: ${FOO" + "\n"
  assert default_value.status == 125
  assert default_value.stderr == r"env: only ${VARNAME} expansion is supported, error at: ${FOO:-value}" + "\n"
  assert digit_name.status == 125
  assert digit_name.stderr == r"env: only ${VARNAME} expansion is supported, error at: ${1FOO}" + "\n"
  assert unexpected_character.status == 125
  assert unexpected_character.stderr == r"env: only ${VARNAME} expansion is supported, error at: ${FOO?}" + "\n"
  assert unbraced.status == 125
  assert unbraced.stderr == r"env: only ${VARNAME} expansion is supported, error at: $TEST_VAR_12345" + "\n"
}

test test_env_split_string_double_quoted_escape_reports_backslash { |ctx|
  let result = env_run(ctx, ["-S\"\\c\""])?

  assert result.status == 125
  assert result.stdout == ""
  assert result.stderr == "env: '\\c' must not appear in double-quoted -S string\n"
}

test test_env_split_string_empty_executable_reports_not_found { |ctx|
  for split in ["-S''", "-S\"\""] {
    let result = env_run(ctx, [split])?

    assert result.status == 127
    assert result.stdout == ""
    assert result.stderr == "env: '': No such file or directory\n"
  }
}

test test_env_rejects_invalid_unset_names { |ctx|
  let empty = env_run(ctx, ["-u", ""])?
  let equals = env_run(ctx, ["-u", "a=b"])?
  let short_inline = env_run(ctx, ["-u="])?
  let long_inline = env_run(ctx, ["--unset="])?

  assert empty.status == 125
  assert empty.stderr == "env: cannot unset '': Invalid argument\n"
  assert equals.status == 125
  assert equals.stderr == "env: cannot unset 'a=b': Invalid argument\n"
  assert short_inline.status == 125
  assert short_inline.stderr == "env: cannot unset '=': Invalid argument\n"
  assert long_inline.status == 125
  assert long_inline.stderr == "env: cannot unset '': Invalid argument\n"
}

test test_env_prints_assignment_environment { |ctx|
  let result = env_run(ctx, ["-i", "MOTOR=idle", "GEARBOX=locked"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "GEARBOX=locked\nMOTOR=idle\n"
  assert result.stderr == ""
}

test test_env_unset_removes_variable_from_environment_output { |ctx|
  let result = env_run(ctx, ["-u", "XSH_TEST_UNSET"], true)?

  assert result.status == 0, result.stderr
  assert "XSH_TEST_UNSET=" not in result.stdout
}

test test_env_null_output_uses_nul_separators { |ctx|
  let result = env_run(ctx, ["-i", "-0", "A=one", "B=two"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "A=one\0B=two\0"
  assert result.stderr == ""
}

test test_env_replaces_child_environment_with_assignments { |ctx|
  let result = env_run(ctx, ["-i", "VALUE=kept", "/usr/bin/env"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "VALUE=kept\n"
  assert result.stderr == ""
}

test test_env_accepts_unicode_environment_names { |ctx|
  let result = env_run(ctx, ["-i", "🎯_VAR=Hello 🌍", "/usr/bin/env"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "🎯_VAR=Hello 🌍\n"
  assert result.stderr == ""
}

test test_env_uses_path_from_replacement_environment { |ctx|
  let result = env_run(ctx, ["-i", "PATH=/bin", "echo", "found"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "found\n"
  assert result.stderr == ""
}

test test_env_reparses_options_in_split_string { |ctx|
  let result = env_run(ctx, ["-S", "-i VALUE=split /usr/bin/env"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "VALUE=split\n"
  assert result.stderr == ""
}

test test_env_split_string_after_ignore_option { |ctx|
  let result = env_run(ctx, ["-i", "-SX=\"Y\\_Z=W\" /usr/bin/env"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "X=Y Z=W\n"
  assert result.stderr == ""
}

test test_env_verbose_split_string_reports_gnu_trace { |ctx|
  let result = env_run(ctx, ["-vS /bin/echo split"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "split\n"
  assert "split -S:" in result.stderr
  assert "executing: /bin/echo\n" in result.stderr
}

test test_env_rejects_empty_assignment_name { |ctx|
  let result = env_run(ctx, ["-i", "=zap"])?

  assert result.status == 125
  assert result.stderr == "env: cannot set '': Invalid argument\n"
}

test test_env_split_string_invalid_escape_uses_gnu_message { |ctx|
  let result = env_run(ctx, [r"-S\|echo"])?

  assert result.status == 125
  assert result.stderr == "env: invalid sequence '\\|' in -S\n"
}

test test_env_split_string_single_quote_escapes { |ctx|
  let result = env_run(ctx, ["-S /usr/bin/printf %s:%s 'foo\\'bar' 'a\\\\b'"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "foo'bar:a\\b"
}

test test_env_split_string_unquoted_backslash_c_ends_word { |ctx|
  let result = env_run(ctx, ["-S /usr/bin/printf %s foo\\c ignored"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "foo"
}

test test_env_reports_invalid_change_directory { |ctx|
  let result = env_run(ctx, ["-C", "missing-directory", "/bin/echo"])?

  assert result.status == 125
  assert "cannot change directory to 'missing-directory'" in result.stderr
}

test test_env_hints_for_shebang_command_arguments { |ctx|
  let result = env_run(ctx, ["missing command"])?

  assert result.status == 127
  assert "No such file or directory" in result.stderr
  assert "use -[v]S to pass options in shebang lines" in result.stderr
}

test test_env_help_lists_options { |ctx|
  let result = env_run(ctx, ["--help"])?

  assert result.status == 0, result.stderr
  assert "Options:" in result.stdout
  assert "--unset" in result.stdout
  assert "--ignore-signal" in result.stdout
}

test test_env_ignored_signal_survives_exec { |ctx|
  let result = env_run(ctx, ["--ignore-signal=USR1", "/bin/sh", "-c", r"kill -USR1 $$; printf survived"])?

  assert result.status == 0, result.stderr
  assert result.stdout == "survived"
  assert result.stderr == ""
}

test test_env_lists_signal_actions_and_restores_defaults { |ctx|
  let result = env_run(ctx, ["--ignore-signal=USR1,USR2", "--list-signal-handling", "/bin/true"])?
  let restored = env_run(ctx, ["--ignore-signal=USR1,USR2", "--default-signal=USR1", "--list-signal-handling", "/bin/true"])?

  assert result.status == 0, result.stderr
  assert "USR1" in result.stderr
  assert "USR2" in result.stderr
  assert "IGNORE" in result.stderr
  assert restored.status == 0, restored.stderr
  assert "USR1" not in restored.stderr
  assert "USR2" in restored.stderr
}

test test_env_resets_all_signal_actions { |ctx|
  let ignored = env_run(ctx, ["--ignore-signal", "--list-signal-handling", "/bin/true"])?
  let restored = env_run(ctx, ["--ignore-signal", "--default-signal", "--list-signal-handling", "/bin/true"])?

  assert ignored.status == 0, ignored.stderr
  assert "IGNORE" in ignored.stderr
  assert "KILL" not in ignored.stderr
  assert restored.status == 0, restored.stderr
  assert restored.stderr == ""
}

test test_env_accepts_empty_optional_signal_list { |ctx|
  let result = env_run(ctx, ["--ignore-signal=", "/bin/true"])?

  assert result.status == 0, result.stderr
  assert result.stderr == ""
}

test test_env_reports_invalid_signal_names { |ctx|
  let result = env_run(ctx, ["--ignore-signal=NOSUCH", "/bin/true"])?
  let without_command = env_run(ctx, ["--ignore-signal=NOSUCH"])?

  assert result.status == 125
  assert "'NOSUCH': invalid signal" in result.stderr
  assert without_command.status == 125
  assert "'NOSUCH': invalid signal" in without_command.stderr
}

test test_env_reports_uncatchable_signal_actions { |ctx|
  let result = env_run(ctx, ["--ignore-signal=KILL", "/bin/true"])?

  assert result.status == 125
  assert result.stderr == "env: failed to set signal action for signal 9: Invalid argument\n"
}
