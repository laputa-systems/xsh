type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/sleep.xsh by its real path (so the invoked name is sleep and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "sleep")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/sleep.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_sleep_waits_for_the_sum_of_its_intervals { |ctx|
  let script = fp"{ctx.core_dir}/sleep.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, script, "0.1s", "0.05"])
  let timing = time.measure(plan)?

  assert timing.status.exited_with(0)
  assert timing.wall_ns >= 150000000
}

test test_sleep_accepts_suffixes_hexadecimal_and_leading_blanks { |ctx|
  for interval in [
    "0",
    "0s",
    "0m",
    "0h",
    "0d",
    "0x0",
    "0x0s",
    "0x0.1",
    "0x1.0p-4s",
    "1e-3",
    " 0.01s",
    "+0",
    "0x0h",
    "0.0001d",
  ] {
    let result = applet_run(ctx, [interval])?
    assert result.status == 0, interval
    assert result.stderr == "", interval
  }
}

test test_sleep_reports_every_invalid_interval_then_the_usage_hint { |ctx|
  let result = applet_run(ctx, ["abc", "100000.0", "1years", " ", "0.1s "])?
  assert result.status == 1
  assert result.stderr == "sleep: invalid time interval 'abc'\nsleep: invalid time interval '1years'\nsleep: invalid time interval ' '\nsleep: invalid time interval '0.1s '\nTry 'sleep --help' for more information.\n", result.stderr
}

test test_sleep_rejects_negative_nan_and_malformed_numbers { |ctx|
  for interval in ["nan", "infD", "iNfD", "'1", "1e", "0x", "0x1p", "1_0", "1m1"] {
    let result = applet_run(ctx, [interval])?
    assert result.status == 1, interval
    assert result.stderr.starts_with("sleep: invalid time interval "), interval
  }
}

test test_sleep_missing_operand_and_option_errors { |ctx|
  let missing = applet_run(ctx, [])?
  assert missing.status == 1
  assert missing.stderr == "sleep: missing operand\nTry 'sleep --help' for more information.\n", missing.stderr

  let negative = applet_run(ctx, ["-1"])?
  assert negative.status == 1
  assert negative.stderr == "sleep: invalid option -- '1'\nTry 'sleep --help' for more information.\n", negative.stderr
}

test test_sleep_help_and_version { |ctx|
  let help = applet_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.starts_with("Usage: sleep NUMBER[SUFFIX]...\n")
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("sleep ")
}

proc sleep_signal_exit_status(ctx: TestContext, signal: Str) [fs, process, time, error] -> Result[Int] {
  let root = test.temp_dir(ctx, name: f"sleep-signal-{signal}")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/sleep.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "100"]
  let child = spawn process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {LC_ALL: "C"},
    b"",
    stdout,
    stderr,
    new_session: true,
  )?

  time.sleep(100ms)?
  process.kill(child.pid, signal: signal)?

  if let completed = process.wait_timeout([child], 2000ms)? {
    return completed.status.shell_code()?
  }

  child.cancel(signal: "KILL", kill_after: 0ms)?
  -1
}

test test_sleep_stops_on_default_signal_actions { |ctx|
  assert sleep_signal_exit_status(ctx, "TERM")? == 128 + 15
  assert sleep_signal_exit_status(ctx, "BUS")? == 128 + 7
  assert sleep_signal_exit_status(ctx, "SEGV")? == 128 + 11
}

test test_sleep_preserves_an_inherited_ignored_signal { |ctx|
  let env_script = fp"{ctx.core_dir}/env.xsh"
  let sleep_script = fp"{ctx.core_dir}/sleep.xsh"
  for signal in ["INT", "TERM"] {
    let root = test.temp_dir(ctx, name: f"sleep-ignored-{signal}")?
    let argv = [
      ctx.xsh_bin.display(),
      env_script.display(),
      f"--ignore-signal={signal}",
      ctx.xsh_bin.display(),
      sleep_script.display(),
      "30",
    ]
    let child = spawn process.command_argv(ctx.xsh_bin, argv, root)?

    time.sleep(100ms)?
    process.kill(child.pid, signal: signal)?
    let still_running = process.wait_timeout([child], 100ms)? == null

    if still_running {
      child.cancel(signal: "KILL", kill_after: 0ms)?
    }

    assert still_running, signal
  }
}
