proc env_run(ctx: TestContext, args: List[Str]) [process, error] -> Result[Str] {
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" @args
}

type EnvResult = {status: Int, stdout: Str, stderr: Str}

proc env_result(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[EnvResult] {
  let root = test.temp_dir(ctx, name: "env-result")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/env.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_env_assignment_runs_command { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" XSH_MODULE_PATH=ok ${ctx.xsh_bin} \
    fp"{ctx.core_dir}/printenv.xsh" XSH_MODULE_PATH ?
  assert output.trim() == "ok"
}

test test_env_split_string_runs_command { |ctx|
  let script = fp"{ctx.core_dir}/printenv.xsh"
  let command = f"XSH_MODULE_PATH=split {ctx.xsh_bin} {script} XSH_MODULE_PATH"
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" "-S" $command ?
  assert output.trim() == "split"
}

test test_env_split_string_as_single_shebang_arg_runs_command { |ctx|
  let script = fp"{ctx.core_dir}/printenv.xsh"
  let command = f"-S XSH_MODULE_PATH=split {ctx.xsh_bin} {script} XSH_MODULE_PATH"
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" $command ?
  assert output.trim() == "split"
}

test test_env_uses_direct_xsh_shebang { |ctx|
  assert fp"{ctx.core_dir}/env.xsh".read_text()?.starts_with("#!/bin/xsh")
}

test test_env_split_string_shell_quoting_escapes_and_variables { |ctx|
  let script = fp"{ctx.core_dir}/printf.xsh"
  let command = ctx.xsh_bin.display() + " " + script.display() + " x%sx\\n $" + "{TEST_VAR}"
  let output = run.text TEST_VAR=value ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" "-S" $command ?
  assert output == "xvaluex\n"

  let quoted = ctx.xsh_bin.display() + " " + script.display() + " %s '\\q'"
  assert env_run(ctx, ["-S", quoted])? == "\\q"
}

test test_env_split_string_option_forms_and_short_cluster { |ctx|
  let command = ctx.xsh_bin.display() + " " + fp"{ctx.core_dir}/printf.xsh".display() + " x%sx\\n A B"
  assert env_run(ctx, ["-S" + command])? == "xAx\nxBx\n"
  assert env_run(ctx, ["--split-string=" + command])? == "xAx\nxBx\n"
  assert env_run(ctx, ["-vS" + command])? == "xAx\nxBx\n"
}

test test_env_split_string_rejects_invalid_expansions_and_quotes { |ctx|
  let script = fp"{ctx.core_dir}/env.xsh"
  let err = test.temp_path(ctx, name: "env-split.err")
  let bad_escape = run.status ${ctx.xsh_bin} $script "-S" "echo \\q" 2> $err
  assert bad_escape.exit_code()? == 125
  assert "invalid sequence '\\q' in -S" in err.read_text()?

  let bad_expansion = run.status ${ctx.xsh_bin} $script "-S" ("echo " + "$" + "TEST_VAR") 2> $err
  assert bad_expansion.exit_code()? == 125
  assert r"only ${VARNAME} expansion is supported" in err.read_text()?
}

test test_env_chdir_null_output_and_statuses { |ctx|
  let root = test.temp_dir(ctx, name: "env-cwd")?
  assert env_run(ctx, ["-C", root.display(), "pwd"])?.trim() == root.display()

  let listing = env_run(ctx, ["-0", "XSH_TEST_VALUE=ready"])?
  assert listing.ends_with("\0")
  assert "XSH_TEST_VALUE=ready\0" in listing

  let err = test.temp_path(ctx, name: "env-null.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/env.xsh" "-0" "true" 2> $err
  assert status.exit_code()? == 125
  assert "cannot specify --null (-0) with command" in err.read_text()?
}

test test_env_rejects_unavailable_environment_replacement_explicitly { |ctx|
  let script = fp"{ctx.core_dir}/env.xsh"
  let err = test.temp_path(ctx, name: "env-option.err")
  let ignore = run.status ${ctx.xsh_bin} $script "-i" "true" 2> $err
  assert ignore.exit_code()? == 125
  assert "cannot replace the inherited environment" in err.read_text()?

  let unset = run.status ${ctx.xsh_bin} $script "-u" "HOME" "true" 2> $err
  assert unset.exit_code()? == 125
  assert "cannot remove variables from a child environment" in err.read_text()?
}

test test_env_ignore_and_unset_filter_a_printed_environment { |ctx|
  let ignored = env_run(ctx, ["-i", "XSH_TEST_VALUE=ready"])?
  assert ignored.trim() == "XSH_TEST_VALUE=ready"

  let filtered = env_run(ctx, ["-i", "-u", "HOME", "XSH_TEST_VALUE=ready"])?
  assert filtered.trim() == "XSH_TEST_VALUE=ready"

  let empty = env_run(ctx, ["-i"])?
  assert empty == ""

  let old_style = env_run(ctx, ["-", "XSH_TEST_VALUE=ready"])?
  assert old_style == "XSH_TEST_VALUE=ready\n"
}

test test_env_signal_actions_and_assignment_run_together { |ctx|
  let result = env_result(ctx, ["--ignore-signal=PIPE", "XSH_TEST_VALUE=ready", ctx.xsh_bin.display(), fp"{ctx.core_dir}/printenv.xsh".display(), "XSH_TEST_VALUE"])?
  assert result.status == 0
  assert result.stdout == "ready\n"

  let ignore_all = env_result(ctx, ["--ignore-signal", "true"])?
  let default_all = env_result(ctx, ["--default-signal", "true"])?
  assert ignore_all.status == 0, ignore_all.stderr
  assert default_all.status == 0, default_all.stderr
}

test test_env_signal_names_validate_before_listing { |ctx|
  let invalid = env_result(ctx, ["--ignore-signal=banana"])?
  assert invalid.status == 125
  assert "'banana': invalid signal" in invalid.stderr

  let stop = env_result(ctx, ["--ignore-signal=SToP", "true"])?
  assert stop.status == 125
  assert "failed to set signal action for signal 19" in stop.stderr

  let block = env_result(ctx, ["--block-signal=__ALL__", "true"])?
  assert block.status == 125
  assert "'__ALL__': invalid signal" in block.stderr
}

test test_env_debug_keeps_unicode_next_to_control_bytes { |ctx|
  let payload = "🎯\u{b}"
  let result = env_result(ctx, ["-vv", ctx.xsh_bin.display(), fp"{ctx.core_dir}/printf.xsh".display(), "%s", payload])?
  assert result.status == 0, result.stderr
  assert result.stdout == payload
  assert "🎯\\x0B" in result.stderr, result.stderr
}

test test_env_runtime_limitations_are_explicit { |ctx|
  let argv0 = env_result(ctx, ["--argv0", "custom", "true"])?
  assert argv0.status == 125
  assert "cannot set a child process argv[0]" in argv0.stderr

  let block = env_result(ctx, ["--block-signal=PIPE", "true"])?
  assert block.status == 125
  assert "cannot block signals for an exec'd child" in block.stderr
}
