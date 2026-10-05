# The `xsht trace` command line: where the script's output and the trace go,
# and which options the subcommand takes.

test test_trace_runs_the_script_and_summarizes_on_stderr {
  let traced = run.capture --text "xsht" trace tests/fixtures/runtime/cli-simple.xsh
  assert traced.status.exited_with(0), traced.stderr
  assert traced.stdout == "hello\n"
  assert "trace summary" in traced.stderr, traced.stderr
}

test test_trace_passes_script_arguments_without_a_separator {
  let traced = run.capture --text "xsht" trace tests/fixtures/runtime/cli-args.xsh one two
  assert traced.status.exited_with(0), traced.stderr
  assert traced.stdout == "one\ntwo\n"
  assert "trace summary" in traced.stderr, traced.stderr
}

test test_xsht_rejects_unknown_commands_and_trace_options_without_the_subcommand {
  for command in ["stale", "--syscalls"] {
    let output = run.capture --text "xsht" $command tests/fixtures/runtime/cli-simple.xsh
    assert output.status.exited_with(2), output.stderr
    assert f"unknown command '{command}'" in output.stderr, output.stderr
  }
}

test test_trace_rejects_a_zero_top_syscalls_count {
  let output = run.capture --text "xsht" trace --syscalls --trace-top-syscalls 0 \
    tests/fixtures/runtime/cli-simple.xsh
  assert output.status.exited_with(2), output.stderr
  assert "`--trace-top-syscalls` must be a positive integer" in output.stderr, output.stderr
}

test test_trace_rejects_syscalls_outside_linux {
  guard system.uname()?.sysname != "Linux" else {
    test.skip("syscall tracing is supported on Linux")
    return
  }
  let output = run.capture --text "xsht" trace --syscalls tests/fixtures/runtime/cli-simple.xsh
  assert output.status.exited_with(2), output.stderr
  assert "`--syscalls` is only supported on Linux" in output.stderr, output.stderr
}

test test_raw_trace_preserves_argv_boundaries {
  let traced = run.capture --text "xsht" trace --raw tests/fixtures/runtime/run-trace-argv.xsh
  assert traced.status.exited_with(0), traced.stderr
  for fragment in ["kind=run.start", "b\"hello world\"", "b\"line\\nfeed\"", "b\"-dash\""] {
    assert fragment in traced.stderr, traced.stderr
  }
}

test test_trace_jsonl_is_on_stderr {
  let traced = run.capture --text "xsht" trace --trace-format jsonl tests/fixtures/runtime/cli-simple.xsh
  assert traced.status.exited_with(0), traced.stderr
  assert traced.stdout == "hello\n"
  let summaries = traced.stderr.lines() |> where { |line| "\"trace.summary\"" in line } |> count()
  assert summaries > 0, traced.stderr
  for field in ["\"function_calls\":", "\"hot_commands\":", "\"script_duration_us\":"] {
    assert field in traced.stderr, traced.stderr
  }
}

test test_trace_file_keeps_runtime_stderr_separate { |ctx|
  let root = test.temp_dir(ctx, name: "trace-file")?
  let file = fp"{root}/trace.txt"
  let traced = run.capture --text "xsht" trace --trace-file $file tests/fixtures/runtime/cli-simple.xsh
  assert traced.status.exited_with(0), traced.stderr
  assert traced.stdout == "hello\n"
  assert traced.stderr == ""
  let trace = file.read_text()?
  for fragment in ["trace summary", "script duration", "hot commands (top 10 by total ms)", "┌"] {
    assert fragment in trace, trace
  }

  assert "kind=script.enter" not in trace, trace
}
