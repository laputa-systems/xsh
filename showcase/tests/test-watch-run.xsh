proc test_watch_run_once(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "watch")?
  fp"${root}/input.txt".write("hello")?
  let output = run.text "xsh" "showcase/watch-run.xsh" -- --root $root --once true ?
  test.contains(output, "watching ")?
  test.contains(output, "[run 1]")?
}

proc test_watch_run_once_reports_child_failure(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "watch-failure")?
  let output = test.temp_path(ctx, name: "watch-failure-output")
  let status = run.status "xsh" "showcase/watch-run.xsh" -- --root $root --once false > $output
  test.ok(status.exited_with(1), "one-shot watch must report child failure")?
  test.contains(output.read_text()?, "exit 1")?
}

proc test_watch_run_cancellation_reaps_child_descendants(ctx: TestContext) [fs, process, time, error] {
  let root = test.temp_dir(ctx, name: "watch-cancel")?
  let ready = test.temp_path(ctx, name: "watch-child-ready")
  let leaked = test.temp_path(ctx, name: "watch-child-leaked")
  let executable = fp"${fs.cwd()?}/target/debug/xsh"
  let wrapper = spawn process.command_argv(
    executable,
    [
      executable.display(),
      "showcase/watch-run.xsh",
      "--",
      "--root",
      root,
      "--once",
      "--",
      "sh",
      "-c",
      "touch \"$1\"; sleep 1; touch \"$2\"",
      "sh",
      ready,
      leaked,
    ],
  )?

  for _ in range(0, 500) {
    break when ready.exists()?

    time.sleep(10ms)?
  }

  test.ok(ready.exists()?, "child must start before cancellation")?
  process.kill(wrapper.pid, signal: "TERM")?
  let status = wait wrapper?
  test.ok(status.exited_with(3), f"canceled watch wrapper must exit with status 3, got ${status.exit_code() ?? -1}")?
  time.sleep(1500ms)?
  test.ok(! leaked.exists()?, "canceled child group must not leave a descendant running")?
}
