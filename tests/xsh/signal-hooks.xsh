# Signal hooks (`on NAME [effects] { ... }`) in scripts that signal themselves:
# a child the script started sends the signal to its parent, so each script
# decides when the signal arrives.

test test_signal_hook_exit_sets_the_script_status { |ctx|
  let output = test.expect(
    ctx,
    r"""on USR1 [] {
  print "hook"
  exit 0
}

run sh -c r"kill -USR1 $PPID; sleep 1"
print "after"
""",
    status: 0,
  )?
  assert output.stdout == "hook\n"
}

test test_signal_hook_without_exit_ends_with_the_signal_status { |ctx|
  let output = test.expect(
    ctx,
    r"""on USR1 [] {
  print "hook"
}

run sh -c r"kill -USR1 $PPID; sleep 1"
print "after"
""",
    status: 128 + process.signal("USR1")?.number,
  )?
  assert output.stdout == "hook\n"
}

test test_startup_signal_skips_main_before_its_captured_global_is_initialized { |ctx|
  let root = test.temp_dir(ctx, name: "signal-before-main")?
  let ready = fp"{root}/ready"
  let child_script = fp"{root}/child.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  child_script.write(
    """on TERM [] {
  exit 143
}

let _ = match process.run(process.command_argv(
  "sh",
  ["sh", "-c", "printf ready > \\u{22}$1\\u{22}; while :; do sleep 1; done", "sh", Path(args[0])],
)) {
  Err(is Canceled) => 0
  Err(problem) => {
    test.fail(f"unexpected startup process error: {problem.message}")?
    0
  }
  Ok(_) => 0
}
let late_captured_global = "initialized"

proc main(...argv: List[Str]) [fs, process, env, error, io, time] {
  print $late_captured_global
}
""",
  )

  let command = process.command_argv(
    ctx.xsh_bin,
    [ctx.xsh_bin, child_script, ready],
    root,
    {},
    b"",
    stdout,
    stderr,
  )
  let handle = spawn command?
  defer {
    if let Ok(_) = process.kill(handle.pid, signal: "0") {
      handle.cancel(signal: "KILL", kill_after: 0ms)
    }
  }

  let started = time.now()
  while ! ready.exists() {
    if time.now() - started > 2000 {
      let errors = stderr.read_text()?
      test.fail(f"startup barrier was not reached: {errors}")
    }
    time.sleep(10ms)
  }

  process.kill(handle.pid, signal: "TERM")
  let finished = process.wait_timeout([handle], 2s)?
  let output = stdout.read_text()?
  let errors = stderr.read_text()?
  if let child = finished {
    let status = child.status.shell_code()?
    assert status == 143, f"expected TERM status 143, got {status}: stdout={output}, stderr={errors}"
  } else {
    test.fail("startup-signal child did not stop")
  }

  assert output == ""
  assert ! ("unknown captured name" in errors), errors
}

test test_signal_hook_trace_records_the_shutdown_path { |ctx|
  let traced = test.run_xsht_trace(
    ctx,
    r"""on USR1 [] {
  exit 0
}

run sh -c r"kill -USR1 $PPID; sleep 1"
""",
    ["--trace", "--raw"],
  )?
  assert traced.status == 0, traced.stderr
  for kind in ["kind=signal.received", "kind=signal.hook.enter", "kind=signal.hook.exit", "kind=signal.forward"] {
    assert kind in traced.stderr, traced.stderr
  }
}

test test_signal_repeated_during_its_hook_escalates_once { |ctx|
  let traced = test.run_xsht_trace(
    ctx,
    r"""on USR1 [process, time, error] {
  let _hook_sender = process.spawn(process.command_argv("sh", ["sh", "-c", r"sleep 0.05; kill -USR1 $PPID"]))?
  time.sleep(1s)?
  exit 0
}

let _outer_sender = process.spawn(process.command_argv("sh", ["sh", "-c", r"sleep 0.05; kill -USR1 $PPID"]))?
time.sleep(5s)?
""",
    ["--trace", "--raw"],
  )?
  assert traced.status == 128 + process.signal("USR1")?.number, traced.stderr
  assert "kind=signal.received" in traced.stderr, traced.stderr
  assert "kind=signal.escalate" in traced.stderr, traced.stderr
  assert traced.stderr.split("kind=signal.hook.enter").len() == 2, traced.stderr
}

test test_signal_hook_interrupts_time_sleep_promptly { |ctx|
  let started = time.now()
  let output = test.expect(
    ctx,
    r"""on USR1 [] {
  print "hook"
  exit 0
}

let _sender = process.spawn(process.command_argv("sh", ["sh", "-c", r"sleep 0.05; kill -USR1 $PPID"]))?
time.sleep(5s)?
print "after"
""",
    status: 0,
  )?
  assert time.now() - started < 2000, "sleep did not observe the signal promptly"
  assert output.stdout == "hook\n"
}

test test_signal_hook_runs_from_a_parallel_stream_parent_checkpoint { |ctx|
  let output = test.expect(
    ctx,
    r"""on USR1 [] {
  print "hook"
  exit 0
}

let _sender = process.spawn(process.command_argv("sh", ["sh", "-c", r"sleep 0.05; kill -USR1 $PPID"]))?
let values = [1, 2, 3] |> par-map(jobs: 2) { |value|
  time.sleep(1s)?
  value
}
print "after"
""",
    status: 0,
  )?
  assert output.stdout == "hook\n"
}

test test_signal_hook_local_defers_run_at_hook_exit { |ctx|
  let root = test.temp_dir(ctx, name: "hook-defer")?
  let marker = fp"{root}/marker"
  test.expect(
    ctx,
    r"""let marker = Path(args[0])

on USR1 [fs, error] {
  defer marker.write("defer")?
  exit 0
}

run sh -c r"kill -USR1 $PPID; sleep 1"
""",
    status: 0,
    args: [marker],
  )?
  assert marker.read_text()? == "defer"
}

test test_signal_hook_runs_during_an_outer_defer_and_cleanup_resumes { |ctx|
  let root = test.temp_dir(ctx, name: "hook-outer-defer")?
  let hook_marker = fp"{root}/hook"
  let cleanup_marker = fp"{root}/cleanup"
  test.expect(
    ctx,
    r"""let hook_marker = Path(args[0])
let cleanup_marker = Path(args[1])

on USR1 [fs, error] {
  hook_marker.write("hook")?
  exit 0
}

defer cleanup_marker.write("cleanup")?
defer time.sleep(300ms)?
let _sender = process.spawn(process.command_argv("sh", ["sh", "-c", r"sleep 0.05; kill -USR1 $PPID"]))?
""",
    status: 0,
    args: [hook_marker, cleanup_marker],
  )?
  assert hook_marker.read_text()? == "hook"
  assert cleanup_marker.read_text()? == "cleanup"
}

test test_nonforced_signal_exit_runs_outer_cleanup_without_abort_error { |ctx|
  let root = test.temp_dir(ctx, name: "hook-nonforced-exit-cleanup")?
  let marker = fp"{root}/cleanup"
  let output = test.expect(
    ctx,
    r"""let marker = Path(args[0])

on USR1 [] {
  exit 130
}

defer {
  if true {
    marker.write("cleanup")?
  }
}
run sh -c r"kill -USR1 $PPID; sleep 1"
""",
    status: 130,
    args: [marker],
  )?
  assert marker.read_text()? == "cleanup"
  assert output.stderr.is_empty(), output.stderr
}

test test_signal_hook_process_work_ignores_the_primary_signal { |ctx|
  let output = test.expect(
    ctx,
    r"""on USR1 [process, error] {
  run sh -c "printf hook"
  exit 0
}

run sh -c r"kill -USR1 $PPID; sleep 1"
""",
    status: 0,
  )?
  assert output.stdout == "hook"
}

test test_signal_hook_pre_cancel_forwards_to_the_active_child_before_the_hook_finishes { |ctx|
  let root = test.temp_dir(ctx, name: "hook-pre-cancel")?
  let marker = fp"{root}/forwarded"
  test.expect(
    ctx,
    r"""let marker = Path(args[0])

on USR1 --pre-cancel=0ms [time, error] {
  time.sleep(300ms)?
  exit 0
}

let command = process.command_argv("sh", ["sh", "-c", r"trap 'printf forwarded > $1; exit 0' USR1; kill -USR1 $PPID; while :; do sleep 1; done", "sh", marker.display()])
let _ = process.run(command)?
""",
    status: 0,
    args: [marker],
  )?
  assert marker.read_text()? == "forwarded"
}

test test_signal_hook_exit_status_survives_time_measure_child_cancellation { |ctx|
  let output = test.expect(
    ctx,
    r"""on USR1 [] {
  exit 0
}

let command = process.command_argv("sh", ["sh", "-c", r"kill -USR1 $PPID; sleep 1"])
let _ = time.measure(command)?
print "after"
""",
    status: 0,
  )?
  assert output.stdout == ""
}

test test_signal_hook_failure_does_not_orphan_active_child_processes { |ctx|
  let root = test.temp_dir(ctx, name: "hook-failure")?
  let leaked = fp"{root}/leaked"
  test.expect(
    ctx,
    r"""let leaked = Path(args[0])

error HookFailed = failed(message: Str)

on USR1 [error] {
  Err(HookFailed.failed(message: "boom"))?
}

run sh -c r"trap '' USR1; (sleep 2; printf leaked > $1) & kill -USR1 $PPID; wait" sh (leaked.display())
""",
    status: 3,
    args: [leaked],
  )?
  # The grandchild would write the marker two seconds after the signal.
  time.sleep(2300ms)
  assert ! leaked.exists()?
}
