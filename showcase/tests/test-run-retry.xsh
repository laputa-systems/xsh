test test_run_retry [process, error] {
  let ok = run.text "xsh" "showcase/run-retry.xsh" -- true ?
  "ok (try 1)" in ok
}

test test_run_retry_exhaustion_exits_unsuccessfully [fs, process, error] { |ctx|
  let output = test.temp_path(ctx, name: "run-retry-output")
  let status = run.status "xsh" "showcase/run-retry.xsh" -- false > $output
  assert status.exited_with(1), "exhausted retries must fail the command"
  "failed after 3" in output.read_text()?
}

test test_run_retry_cancellation_reaps_child_descendants [fs, process, time, error] { |ctx|
  let ready = test.temp_path(ctx, name: "retry-child-ready")
  let leaked = test.temp_path(ctx, name: "retry-child-leaked")
  let executable = fp"${fs.cwd()?}/target/debug/xsh"
  let wrapper = spawn process.command_argv(
    executable,
    [
      executable.display(),
      "showcase/run-retry.xsh",
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

  assert ready.exists()?, "child must start before cancellation"
  process.kill(wrapper.pid, signal: "TERM")?
  let status = wait wrapper?
  let canceled = status.exited_with(3)
  let cancellation_message = f"canceled retry wrapper must exit with status 3, got ${status.exit_code() ?? -1}"
  assert canceled, cancellation_message
  time.sleep(1500ms)?
  assert ! leaked.exists()?, "canceled child group must not leave a descendant running"
}
