proc sleeper_bin() [env, error] -> Result[Path] {
  return p"${env.get(\"CARGO_BIN_EXE_xsh-test-sleeper\")?}"
}

proc marker_executable(ctx: TestContext, marker: Str) [env, fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: marker)?
  let executable = fp"${root}/${marker}"
  fs.copy(sleeper_bin().resolve()?, executable)?
  fs.chmod(executable, 0o755)?
  return executable
}

proc wait_for_process_marker(pid: Int, marker: Str) [process, time, error] {
  var visible = false
  var attempts = 0

  while ! visible and attempts < 20 {
    visible = process.list()? |> any .pid == pid and (marker in .command or marker in .argv0)

    if ! visible {
      time.sleep(25ms)?
    }

    attempts += 1
  }

  test.ok(visible, "spawned process marker should be visible in process metadata")?
}

proc test_px_finds_current_test_process() [process, error] {
  let pid = process.current_pid()?
  let pid_arg = f"${pid}"
  let output = run.text "xsh" "showcase/px.xsh" -- $pid_arg ?
  test.contains(output, f"${pid}")?
  test.contains(output, "pid")?
  test.contains(output, "user")?
  test.contains(output, "mem")?
}

proc test_px_default_search_matches_executable_substrings(ctx: TestContext) [env, fs, process, time, error] {
  let marker = "xshpxexec"
  let executable = marker_executable(ctx, marker)?
  let child = process.spawn(process.command_argv(executable, [executable.display()]))?
  defer process.kill(child.pid, signal: "TERM")
  wait_for_process_marker(child.pid, marker)?
  let output = run.text "xsh" "showcase/px.xsh" -- "pxexec" ?
  test.contains(output, marker)?
  test.contains(output, f"${child.pid}")?
}

proc test_px_kill_signals_default_matches(ctx: TestContext) [env, fs, process, time, error] {
  let marker = "xshpxkilld"
  let executable = marker_executable(ctx, marker)?
  let child = spawn process.command_argv(executable, [executable.display()])?
  wait_for_process_marker(child.pid, marker)?
  let pid_arg = f"${child.pid}"
  let output = run.text "xsh" "showcase/px.xsh" -- "--kill=15" $pid_arg ?
  test.contains(output, "signaled 1 process(es) with signal 15")?
  let status = wait child?
  test.ok(status.signaled())?
  test.eq(status.signal_number()?, 15)?
}

proc test_px_kill_accepts_numeric_signal(ctx: TestContext) [env, fs, process, time, error] {
  let marker = "xshpxkills"
  let executable = marker_executable(ctx, marker)?
  let child = spawn process.command_argv(executable, [executable.display()])?
  wait_for_process_marker(child.pid, marker)?
  let pid_arg = f"${child.pid}"
  let output = run.text "xsh" "showcase/px.xsh" -- "--kill" "0" $pid_arg ?
  test.contains(output, "signaled 1 process(es) with signal 0")?
  child.cancel(signal: "TERM", kill_after: 10ms)?
}

proc test_px_kill_requires_a_filter(ctx: TestContext) [fs, process, error] {
  let err = test.temp_file(ctx, name: "px-kill-filter-stderr", contents: b"")?
  let status = run.status "xsh" "showcase/px.xsh" -- "--kill" 2> $err
  test.ok(! status.exited_with(0), "unfiltered kill should fail")?
}

proc test_px_kill_signal_is_parse_bounded(ctx: TestContext) [fs, process, error] {
  let err = test.temp_file(ctx, name: "px-kill-signal-stderr", contents: b"")?
  let status = run.status "xsh" "showcase/px.xsh" -- "--kill=129" "xsh-px-no-such-process-pattern" 2> $err
  test.ok(! status.exited_with(0), "out-of-range kill signal should fail during argument parsing")?
}

proc test_px_returns_one_when_no_process_matches() [process, error] {
  let status = run.status "xsh" "showcase/px.xsh" -- "xsh-px-no-such-process-pattern"
  test.ok(status.exited_with(1), "unmatched process search should exit 1")?
}
