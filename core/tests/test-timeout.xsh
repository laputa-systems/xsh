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
  let input = fp"{root}/input"
  fs.mkfifo(input, 0o600)?
  let script = fp"{ctx.core_dir}/timeout.xsh"
  let command = [
    ctx.xsh_bin.display(),
    script.display(),
    "10",
    "sh",
    "-c",
    f"exec 3<> \"$2\"; trap 'printf forwarded; exit 42' {signal}; printf ready > \"$1\"; while :; do read -r ignored <&3; done",
    "sh",
    ready.display(),
    input.display(),
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
  while ! ready.exists() {
    if time.now() - started > 2000 {
      handle.cancel(signal: "KILL", kill_after: 0ms)
      return Err(error.failure("timeout child did not install its signal handler"))
    }

    time.sleep(10ms)
  }
  process.kill(handle.pid, signal)
  let completed = process.wait_timeout([handle], 2s)?
  let status = if let finished = completed {
    finished.status.shell_code()?
  } else {
    handle.cancel(signal: "KILL", kill_after: 0ms)
    1
  }
  Ok({status, stdout: out.read_text()?, stderr: err.read_text()?})
}

proc wait_for_marker(marker: Path, handle: ProcessHandle, description: Str, started: Int) [fs, process, time, error] -> Result[Unit] {
  while ! marker.exists()? {
    if time.now() - started >= 2000 {
      handle.cancel(signal: "KILL", kill_after: 0ms)
      return Err(error.failure(f"{description} marker was not written"))
    }

    time.sleep(1ms)
  }
  Ok()
}

proc wait_for_process(handle: ProcessHandle, description: Str, started: Int) [process, time, error] -> Result[Status] {
  while time.now() - started < 2000 {
    if let finished = process.wait_timeout([handle], 0ms)? {
      return Ok(finished.status)
    }

    time.sleep(1ms)
  }

  handle.cancel(signal: "KILL", kill_after: 0ms)
  Err(error.failure(f"{description} did not exit within the test safety bound"))
}

proc terminate_timeout(handle: ProcessHandle, cleanup: Path, started: Int) [fs, process, time, error] -> Result[Int] {
  process.kill(handle.pid, signal: "TERM")
  wait_for_marker(cleanup, handle, "deferred cleanup", started)?
  Ok((wait_for_process(handle, "timeout wrapper", started)?).shell_code()?)
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
  let ready = fp"{root}/ready"
  let child = fp"{root}/child.xsh"
  child.write("let marker = Path(args[0])\nlet ready = Path(args[1])\ndefer marker.write(\"cleaned\")?\nready.write(\"ready\")?\nwhile time.now() >= 0 {}\n")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let plan = process.command_argv(
    ctx.xsh_bin,
    [ctx.xsh_bin, fp"{ctx.core_dir}/timeout.xsh", "10", ctx.xsh_bin, child, marker, ready],
    root,
    {},
    b"",
    stdout,
    stderr,
    timeout: 2s,
  )
  let handle = spawn plan?
  defer handle.cancel(signal: "KILL", kill_after: 0ms)
  let started = time.now()
  wait_for_marker(ready, handle, "CPU-bound child readiness", started)?
  let status = terminate_timeout(handle, marker, started)?
  assert status == 143
  assert marker.read_text()? == "cleaned"
  assert stdout.read_bytes()? == b""
  assert stderr.read_bytes()? == b""
}

test timeout_stops_an_xsh_child_blocked_in_stdin_read { |ctx|
  let root = test.temp_dir(ctx, name: "timeout-xsh-stdin")?
  let marker = fp"{root}/cleanup"
  let ready = fp"{root}/ready"
  let blocked = fp"{root}/blocked"
  let fifo = fp"{root}/stdin"
  fs.mkfifo(fifo, 0o600)?
  let child = fp"{root}/child.xsh"
  child.write("let marker = Path(args[0])\nlet ready = Path(args[1])\nlet blocked = Path(args[2])\ndefer marker.write(\"cleaned\")?\nready.write(\"ready\")?\nlet first = io.stdin_read(1)?\nassert first == b\"x\"\nblocked.write(\"blocked\")?\nlet _ = io.stdin_bytes()?\n")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let writer_stdout = fp"{root}/writer-stdout"
  let writer_stderr = fp"{root}/writer-stderr"
  let writer_plan = process.command_argv(
    p"/bin/sh",
    [
      "sh",
      "-c",
      "exec 3> \"$1\"; i=0; while [ ! -e \"$2\" ] && [ \"$i\" -lt 600 ]; do sleep 0.01; i=$((i+1)); done; if [ ! -e \"$2\" ]; then exit 91; fi; printf x >&3; i=0; while [ ! -e \"$3\" ] && [ \"$i\" -lt 600 ]; do sleep 0.01; i=$((i+1)); done; if [ ! -e \"$3\" ]; then exit 92; fi",
      "timeout-stdin-writer",
      fifo,
      ready,
      marker,
    ],
    root,
    {},
    b"",
    writer_stdout,
    writer_stderr,
  )
  let writer = spawn writer_plan?
  defer writer.cancel(signal: "KILL", kill_after: 0ms)
  let target = "exec \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" < \"$6\""
  let plan = process.command_argv(
    ctx.xsh_bin,
    [
      ctx.xsh_bin,
      fp"{ctx.core_dir}/timeout.xsh",
      "10",
      p"/bin/sh",
      "-c",
      target,
      "timeout-stdin-target",
      ctx.xsh_bin,
      child,
      marker,
      ready,
      blocked,
      fifo,
    ],
    root,
    {},
    b"",
    stdout,
    stderr,
    timeout: 2s,
  )
  let handle = spawn plan?
  defer handle.cancel(signal: "KILL", kill_after: 0ms)
  let started = time.now()
  wait_for_marker(ready, handle, "stdin child readiness", started)?
  wait_for_marker(blocked, handle, "stdin blocking read", started)?
  assert ! marker.exists()?
  assert process.wait_timeout([writer], 0ms)? == null
  let status = terminate_timeout(handle, marker, started)?
  assert status == 143
  let writer_status = wait_for_process(writer, "stdin producer", started)?
  assert writer_status.shell_code()? == 0
  assert ready.read_text()? == "ready"
  assert blocked.read_text()? == "blocked"
  assert marker.read_text()? == "cleaned"
  assert stdout.read_bytes()? == b""
  assert stderr.read_bytes()? == b""
  assert writer_stdout.read_bytes()? == b""
  assert writer_stderr.read_bytes()? == b""
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
  process.kill(pid, signal: "0")
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
    time.sleep(10ms)
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

test timeout_duration_stops_option_parsing { |ctx|
  for args in [["-v", "0", "-s0", "-k0", "sleep", ".1"], ["0", "--help"], ["--", "0", "-s0"]] {
    let result = invoke(ctx, args)?
    assert result.status == 127, result.stderr
    assert result.stdout == "", result.stdout
    let command = if args[0] == "0" { "--help" } else { "-s0" }
    assert result.stderr == f"timeout: failed to run command '{command}': No such file or directory\n", result.stderr
  }
}


test timeout_continues_a_stopped_command_after_signaling { |ctx|
  assert invoke(ctx, ["-s", "STOP", ".05", "sleep", ".1"])?.status == 124
}

# A negative value that underflows keeps its sign and is rejected, while a
# positive underflow is a tiny positive deadline and a bare zero disables it.
test timeout_underflowing_intervals_keep_their_sign { |ctx|
  assert invoke(ctx, [" -1e-10000", "true"])?.status == 125
  assert invoke(ctx, ["1.5e-10000", "sleep", "10"])?.status == 124
  assert invoke(ctx, [" -0", "true"])?.status == 0
}
