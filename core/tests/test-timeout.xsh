type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "timeout")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/timeout.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err, timeout: 5s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

proc interrupt(ctx: TestContext, signal: Str) [fs, process, time, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "timeout-signal")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let ready = fp"{root}/ready"
  let script = fp"{ctx.core_dir}/timeout.xsh"
  let command = [
    ctx.xsh_bin.display(),
    script.display(),
    "10",
    "sh",
    "-c",
    f"trap 'printf forwarded; exit 42' {signal}; printf ready > \"$1\"; while :; do sleep 1; done",
    "sh",
    ready.display(),
  ]
  let plan = process.command_argv(
    ctx.xsh_bin,
    command,
    root,
    {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""},
    b"",
    out,
    err,
  )
  let handle = spawn plan?
  let started = time.now()
  while ! ready.exists()? {
    if time.now() - started > 2000 {
      handle.cancel(signal: "KILL", kill_after: 0ms)?
      return Err(error.failure("timeout child did not install its signal handler"))
    }

    time.sleep(10ms)?
  }
  process.kill(handle.pid, signal)?
  let completed = process.wait_timeout([handle], 2s)?
  let status = if let finished = completed {
    finished.status.shell_code()?
  } else {
    handle.cancel(signal: "KILL", kill_after: 0ms)?
    1
  }
  Ok({status, stdout: out.read_text()?, stderr: err.read_text()?})
}

proc alive(pid: Int) [process] -> Bool {
  if let Err(_) = process.kill(pid, signal: "0") {
    return false
  }
  true
}

test timeout_preserves_normal_exit_and_command_arguments { |ctx|
  assert invoke(ctx, ["2", "sh", "-c", "exit 7"])?.status == 7
  assert invoke(ctx, ["2", "printf", "%s", "--help"])?.stdout == "--help"
}

test timeout_sends_selected_signal_and_preserves_status { |ctx|
  assert invoke(ctx, [".05", "sleep", "10"])?.status == 124
  assert invoke(ctx, ["-p", "-s", "USR1", ".05", "sleep", "10"])?.status == 138
  assert invoke(ctx, ["-s", "KILL", ".05", "sleep", "10"])?.status == 137
}

test timeout_stops_a_cpu_bound_xsh_child_and_runs_its_cleanup { |ctx|
  let root = test.temp_dir(ctx, name: "timeout-xsh-signal")?
  let marker = fp"{root}/cleanup"
  let child = fp"{root}/child.xsh"
  child.write("let marker = Path(args[0])\ndefer marker.write(\"cleaned\")?\nwhile time.now() >= 0 {}\n")?
  let plan = process.command_argv(
    ctx.xsh_bin,
    [ctx.xsh_bin.display(), fp"{ctx.core_dir}/timeout.xsh", ".05", ctx.xsh_bin.display(), child.display(), marker.display()],
    root,
    {},
    b"",
    fp"{root}/stdout",
    fp"{root}/stderr",
    timeout: 2s,
  )
  let status = process.run(plan)?.shell_code()?
  let stderr = fp"{root}/stderr".read_text()?
  assert status == 124, stderr
  assert marker.read_text()? == "cleaned"
}

test timeout_stops_an_xsh_child_blocked_in_stdin_read { |ctx|
  let root = test.temp_dir(ctx, name: "timeout-xsh-stdin")?
  let marker = fp"{root}/cleanup"
  let child = fp"{root}/child.xsh"
  child.write("let marker = Path(args[0])\ndefer marker.write(\"cleaned\")?\nlet _ = io.stdin_bytes()?\n")?
  let plan = process.command_argv(
    p"/bin/sh",
    [
      "sh",
      "-c",
      "(printf x; while [ ! -e \"$4\" ]; do sleep 0.01; done) | \"$1\" \"$2\" .05 \"$1\" \"$3\" \"$4\"",
      "timeout-stdin-probe",
      ctx.xsh_bin.display(),
      fp"{ctx.core_dir}/timeout.xsh".display(),
      child.display(),
      marker.display(),
    ],
    root,
    {},
    b"",
    fp"{root}/stdout",
    fp"{root}/stderr",
    timeout: 2s,
  )
  let status = process.run(plan)?.shell_code()?
  assert status == 124
  assert marker.read_text()? == "cleaned"
}

test timeout_escalates_when_initial_signal_is_ignored { |ctx|
  let result = invoke(ctx, ["-v", "-s", "0", "-k", ".05", ".05", "sleep", "10"])?
  assert result.status == 137
  assert "sending signal 0 to command" in result.stderr
  assert "sending signal KILL to command" in result.stderr
}

test timeout_signal_zero_does_not_resume_a_stopped_command { |ctx|
  let result = invoke(ctx, ["-s", "0", "-k", ".05", ".05", "sh", "-c", "kill -STOP $$; printf resumed; sleep 10"])?
  assert result.status == 137
  assert result.stdout == "", result.stdout
}

test timeout_forwards_interrupt_signals_and_reports_shell_status { |ctx|
  let interrupted = interrupt(ctx, "INT")?
  assert interrupted.status == 130, interrupted.stderr
  assert "forwarded" in interrupted.stdout, interrupted.stdout

  let terminated = interrupt(ctx, "TERM")?
  assert terminated.status == 143, terminated.stderr
  assert "forwarded" in terminated.stdout, terminated.stdout
}

test timeout_zero_disables_deadline { |ctx|
  assert invoke(ctx, ["-v", "0", "sleep", ".05"])?.status == 0
  assert invoke(ctx, ["-v", "0", "true"])?.stderr == ""
}

test timeout_checks_intervals_and_launch_status { |ctx|
  for value in ["bad", "", "1x", "-1"] {
    assert invoke(ctx, ["--", value, "true"])?.status == 125
  }
  assert invoke(ctx, ["-k", "bad", "1", "true"])?.status == 125
  assert invoke(ctx, ["-s", " TERM ", "1", "true"])?.status == 125
  assert invoke(ctx, ["1", "/nonexistent/xsh-command"])?.status == 127
  assert invoke(ctx, ["1", "/"])?.status == 126
}

test timeout_invalid_interval_diagnostics_use_gnu_locale_quotes { |ctx|
  let multibyte = invoke(ctx, ["10€", "sleep", "0"])?
  assert multibyte.status == 125
  assert "invalid time interval '10\\342\\202\\254'" in multibyte.stderr, multibyte.stderr

  let apostrophe = invoke(ctx, ["'1", "sleep", "0"])?
  assert apostrophe.status == 125
  assert "invalid time interval '\\'1'" in apostrophe.stderr, apostrophe.stderr
}

# GNU timeout --foreground signals only the command, not its process group, so a
# descendant the command started outlives the timeout.
test timeout_foreground_signals_only_the_command { |ctx|
  let root = test.temp_dir(ctx, name: "timeout-foreground")?
  let grandchild = fp"{root}/grandchild"
  let result = invoke(ctx, ["--foreground", ".3", "sh", "-c", r"sleep 30 & printf %s $! > $1; wait", "sh", grandchild.display()])?
  assert result.status == 124
  assert result.stdout == ""
  assert result.stderr == ""
  let pid = grandchild.read_text()?.parse_int() ?? -1
  defer process.kill(pid, signal: "KILL")
  process.kill(pid, signal: "0")?
}

test timeout_without_foreground_signals_the_commands_group { |ctx|
  let root = test.temp_dir(ctx, name: "timeout-group")?
  let grandchild = fp"{root}/grandchild"
  let result = invoke(ctx, [".3", "sh", "-c", r"sleep 30 & printf %s $! > $1; wait", "sh", grandchild.display()])?
  assert result.status == 124
  let pid = grandchild.read_text()?.parse_int() ?? -1
  # The group signal is delivered before timeout exits, but the grandchild's
  # exit is observable only once its new parent has reaped it.
  let started = time.now()
  while time.now() - started < 2000 and alive(pid) {
    time.sleep(10ms)?
  }
  test.error_kind(process.kill(pid, signal: "0"), "process-missing")
}

test timeout_foreground_applies_kill_after_to_the_command { |ctx|
  assert invoke(ctx, ["--foreground", "-s0", "-k.1", ".1", "sleep", "10"])?.status == 137
  assert invoke(ctx, ["-f", "-s", "STOP", "-k.1", ".1", "sleep", "10"])?.status == 137
}

test timeout_accepts_hex_intervals_and_saturates_large_intervals { |ctx|
  assert invoke(ctx, ["0x0.1d", "sleep", "10"])?.status == 124
  assert invoke(ctx, ["9223372036854775808d", "true"])?.status == 0
  assert invoke(ctx, ["1e-100", "sleep", "1"])?.status == 124
}

test timeout_options_after_duration_stop_at_command { |ctx|
  let result = invoke(ctx, ["0", "-s0", "-k0", "printf", "%s", "-v"])?
  assert result.status == 0
  assert result.stdout == "-v"
  assert result.stderr == ""
}


test timeout_continues_a_stopped_command_after_signaling { |ctx|
  assert invoke(ctx, ["-s", "STOP", ".05", "sleep", ".1"])?.status == 124
}
