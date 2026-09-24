proc test_wait_for_usage() [process, error] {
  let output = run.text "xsh" "showcase/wait-for.xsh" -- --help ?
  test.contains(output, "usage:")?
}

proc test_wait_for_timeout_exits_unsuccessfully(ctx: TestContext) [fs, process, error] {
  let output = test.temp_path(ctx, name: "wait-for-output")
  let status = run.status "xsh" "showcase/wait-for.xsh" -- "unsupported://no-endpoint" --timeout 1 --interval 1 > $output
  test.ok(status.exited_with(1), "a timed-out endpoint must fail the command")?
  test.contains(output.read_text()?, "timed out after 1s")?
}

proc test_wait_for_total_timeout_caps_long_poll_interval(ctx: TestContext) [fs, process, error] {
  let output = test.temp_path(ctx, name: "wait-for-long-interval")
  let command = process.command {
    timeout = 3s
    stdout = output
    run "xsh" "showcase/wait-for.xsh" -- "unsupported://no-endpoint" --timeout 1 --interval 10
  }
  let status = process.run(command)?
  test.ok(status.exited_with(1), "the requested timeout must bound the poll interval")?
  test.contains(output.read_text()?, "timed out after 1s")?
}
