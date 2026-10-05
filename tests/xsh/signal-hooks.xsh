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
