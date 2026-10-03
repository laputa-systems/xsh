test test_wait_for_usage {
  let output = run.text "xsh" "showcase/wait-for.xsh" -- --help ?
  "usage:" in output
}

test test_wait_for_timeout_exits_unsuccessfully { |ctx|
  let output = test.temp_path(ctx, name: "wait-for-output")
  let status = run.status "xsh" "showcase/wait-for.xsh" -- "unsupported://no-endpoint" --timeout 1 --interval 1 > $output
  assert status.exited_with(1), "a timed-out endpoint must fail the command"
  "timed out after 1s" in output.read_text()?
}

test test_wait_for_total_timeout_caps_long_poll_interval { |ctx|
  let output = test.temp_path(ctx, name: "wait-for-long-interval")
  let command = process.command {
    timeout = 3s
    stdout = output
    run "xsh" "showcase/wait-for.xsh" -- "unsupported://no-endpoint" --timeout 1 --interval 10
  }
  let status = process.run(command)?
  assert status.exited_with(1), "the requested timeout must bound the poll interval"
  "timed out after 1s" in output.read_text()?
}
