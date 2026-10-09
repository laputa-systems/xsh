type Ran = {status: Int, stdout: Str, stderr: Str}

proc timeout_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "timeout")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/timeout.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(
    process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err),
  )?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

proc timeout_signal_run(ctx: TestContext, signal: Str) [fs, process, time, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "timeout-signal")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/timeout.xsh"
  let action = if signal == "INT" { "forwarded-int" } else { "forwarded-term" }
  let command = f"trap 'printf {action}; exit 0' {signal}; while :; do sleep 1; done"
  let argv = [ctx.xsh_bin.display(), script.display(), "60", "sh", "-c", command]
  let child = spawn process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""},
    b"",
    out,
    err,
    new_session: true,
  )?

  time.sleep(100ms)
  process.kill(child.pid, signal)?
  let waited = process.wait_timeout([child], time.millis(3000))?
  if waited == null {
    process.kill(child.pid, "KILL")?
  }

  let completed = if let result = waited {
    result
  } else {
    process.wait_any([child])?
  }

  Ok({
    status: completed.status.shell_code()?,
    stdout: out.read_bytes()?.utf8() ?? "",
    stderr: err.read_text()?,
  })
}

test test_timeout_command_status_and_arguments { |ctx|
  assert timeout_run(ctx, ["5", "true"])?.status == 0
  assert timeout_run(ctx, ["5", "false"])?.status == 1
  assert timeout_run(ctx, ["5", "sh", "-c", "exit 7"])?.status == 7
  assert timeout_run(ctx, ["5", "echo", "-n", "one", "two"])?.stdout == "one two"
}

test test_timeout_zero_disables_the_deadline { |ctx|
  let result = timeout_run(ctx, ["0", "true"])?
  assert result.status == 0
  assert result.stderr == ""

  let parsed = timeout_run(ctx, ["-v", "-s0", "-k0", "0", "sleep", ".1"])?
  assert parsed.status == 0, parsed.stderr
  assert parsed.stderr == ""
}

test test_timeout_options_stop_at_the_duration { |ctx|
  let result = timeout_run(ctx, ["-v", "0", "-s0", "-k0", "sleep", ".1"])?
  assert result.status == 127
  assert result.stderr == "timeout: failed to run command '-s0': No such file or directory\n", result.stderr
}

test test_timeout_accepts_hexadecimal_duration { |ctx|
  let result = timeout_run(ctx, ["0x0.1d", "/usr/bin/sleep", "1"])?
  assert result.status == 124
}

test test_timeout_rejects_directories_as_commands { |ctx|
  let result = timeout_run(ctx, ["1", "/"])?
  assert result.status == 126
  assert "Permission denied" in result.stderr
}

test test_timeout_sends_term_and_returns_124 { |ctx|
  let result = timeout_run(ctx, [".05", "sh", "-c", "trap 'echo term; exit 0' TERM; sleep 5"])?
  assert result.status == 124, result.stderr
  assert "term" in result.stdout, result.stdout
}

test test_timeout_kill_after_and_preserve_status { |ctx|
  let killed = timeout_run(ctx, ["-k", ".05", ".05", "sh", "-c", "trap '' TERM; sleep 3"])?
  assert killed.status == 124, killed.stderr

  let preserved = timeout_run(
    ctx,
    ["--preserve-status", ".05", "sh", "-c", "trap 'exit 42' TERM; sleep 5"],
  )?
  assert preserved.status == 42, preserved.stderr

  let signalled = timeout_run(ctx, ["--preserve-status", ".05", "/usr/bin/sleep", "5"])?
  assert signalled.status == 143, signalled.stderr

  let force_killed = timeout_run(
    ctx,
    ["--preserve-status", "-k", ".05", ".05", "sh", "-c", "trap '' TERM; sleep 5"],
  )?
  assert force_killed.status == 137, force_killed.stderr
}

test test_timeout_verbose_signal_and_help { |ctx|
  let result = timeout_run(ctx, ["-v", ".05", "sleep", "1"])?
  assert result.status == 124
  assert "timeout: sending signal TERM to command 'sleep'\n" == result.stderr, result.stderr

  let help = timeout_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.starts_with("Usage: timeout [OPTION] DURATION COMMAND [ARG]...")
}

test test_timeout_forwards_external_interrupts { |ctx|
  let interrupted = timeout_signal_run(ctx, "INT")?
  assert interrupted.status == 130, interrupted.stderr
  assert interrupted.stdout == "forwarded-int", interrupted.stdout

  let terminated = timeout_signal_run(ctx, "TERM")?
  assert terminated.status == 143, terminated.stderr
  assert terminated.stdout == "forwarded-term", terminated.stdout
}

test test_timeout_rejects_invalid_time_and_signal { |ctx|
  let bad_time = timeout_run(ctx, ["xyz", "true"])?
  assert bad_time.status == 125
  assert bad_time.stderr == "timeout: invalid time interval 'xyz'\nTry 'timeout --help' for more information.\n", bad_time.stderr

  let bad_signal = timeout_run(ctx, ["-s", "invalid", "1", "true"])?
  assert bad_signal.status == 125
  assert bad_signal.stderr == "timeout: 'invalid': invalid signal\nTry 'timeout --help' for more information.\n", bad_signal.stderr
}
