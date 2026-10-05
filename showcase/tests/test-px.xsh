pure sleeper_bin(ctx: TestContext) -> Path {
  fp"{ctx.xsh_bin.parent()}/xsh-test-sleeper"
}

proc marker_executable(ctx: TestContext, marker: Str) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: marker)?
  let executable = fp"{root}/{marker}"
  fs.copy(sleeper_bin(ctx).resolve()?, executable)?
  fs.chmod(executable, 0o755)?
  executable
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

  assert visible, "spawned process marker should be visible in process metadata"
}

test test_px_finds_current_test_process {
  let pid = process.current_pid()?
  let pid_arg = f"{pid}"
  let output = run.text "xsh" "showcase/px.xsh" -- $pid_arg ?
  assert f"{pid}" in output
  assert "pid" in output
  assert "user" in output
  assert "mem" in output
}

test test_px_default_search_matches_executable_substrings { |ctx|
  let marker = "xshpxexec"
  let executable = marker_executable(ctx, marker)?
  let child = process.spawn(process.command_argv(executable, [executable]))?
  defer process.kill(child.pid, signal: "TERM")
  wait_for_process_marker(child.pid, marker)?
  let output = run.text "xsh" "showcase/px.xsh" -- "pxexec" ?
  assert marker in output
  assert f"{child.pid}" in output
}

test test_px_kill_signals_default_matches { |ctx|
  let marker = "xshpxkilld"
  let executable = marker_executable(ctx, marker)?
  let child = spawn process.command_argv(executable, [executable])?
  wait_for_process_marker(child.pid, marker)?
  let pid_arg = f"{child.pid}"
  let output = run.text "xsh" "showcase/px.xsh" -- "--kill=15" $pid_arg ?
  assert "signaled 1 process(es) with signal 15" in output
  let status = wait child?
  assert status.signaled()
  assert status.signal_number()? == 15
}

test test_px_kill_accepts_numeric_signal { |ctx|
  let marker = "xshpxkills"
  let executable = marker_executable(ctx, marker)?
  let child = spawn process.command_argv(executable, [executable])?
  wait_for_process_marker(child.pid, marker)?
  let pid_arg = f"{child.pid}"
  let output = run.text "xsh" "showcase/px.xsh" -- "--kill" "0" $pid_arg ?
  assert "signaled 1 process(es) with signal 0" in output
  child.cancel(signal: "TERM", kill_after: 10ms)?
}

test test_px_kill_requires_a_filter { |ctx|
  let err = test.temp_file(ctx, name: "px-kill-filter-stderr", contents: b"")?
  let status = run.status "xsh" "showcase/px.xsh" -- "--kill" 2> $err
  assert ! status.exited_with(0), "unfiltered kill should fail"
}

test test_px_kill_signal_is_parse_bounded { |ctx|
  let err = test.temp_file(ctx, name: "px-kill-signal-stderr", contents: b"")?
  let status = run.status "xsh" "showcase/px.xsh" -- "--kill=129" "xsh-px-no-such-process-pattern" 2> $err
  assert ! status.exited_with(0), "out-of-range kill signal should fail during argument parsing"
}

test test_px_returns_one_when_no_process_matches {
  let status = run.status "xsh" "showcase/px.xsh" -- "xsh-px-no-such-process-pattern"
  assert status.exited_with(1), "unmatched process search should exit 1"
}
