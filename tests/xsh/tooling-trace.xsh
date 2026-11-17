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

type ProfileRow = {kind: Str, name: Str, count: Int, total_us: Int, self_us: Int, outer_total_us: Int}
type ProfileSummary = {function_calls: List[ProfileRow]}

test test_trace_function_self_time_and_recursion {
  let traced = run.capture --text "xsht" trace --trace-format jsonl tests/fixtures/runtime/trace-profile.xsh
  assert traced.status.exited_with(0), traced.stderr
  let rows = json.decode(traced.stderr)?.require(ProfileSummary)?.function_calls
  let root = (rows |> where { |row| row.name == "root" } |> collect())[0]
  let recurse = (rows |> where { |row| row.name == "recurse" } |> collect())[0]
  let leaf = (rows |> where { |row| row.name == "leaf" } |> collect())[0]
  assert root.kind == "proc" and recurse.kind == "pure" and leaf.kind == "pure", traced.stderr
  assert recurse.count == 5 and leaf.count == 5, traced.stderr
  assert root.total_us == root.outer_total_us, traced.stderr
  assert recurse.total_us > recurse.outer_total_us, traced.stderr
  assert leaf.self_us == leaf.total_us, traced.stderr
  assert root.self_us + recurse.self_us + leaf.self_us == root.total_us, traced.stderr
  assert recurse.self_us + leaf.self_us == recurse.outer_total_us, traced.stderr
  assert rows[0].self_us >= rows[1].self_us and rows[1].self_us >= rows[2].self_us, traced.stderr
}

test test_trace_flamegraph_includes_function_self_frames {
  let traced = run.capture --text "xsht" trace --trace-format flamegraph tests/fixtures/runtime/trace-profile.xsh
  assert traced.status.exited_with(0), traced.stderr
  assert "script;proc:root " in traced.stderr, traced.stderr
  assert "script;proc:root;pure:recurse " in traced.stderr, traced.stderr
  assert "pure:recurse;pure:recurse" in traced.stderr, traced.stderr
  assert "pure:leaf " in traced.stderr, traced.stderr
  var total = 0
  var script_self = 0
  var root_inclusive = 0
  var root_self = 0
  var root_children = 0
  for line in traced.stderr.lines() {
    let parts = line.split(" ")
    assert parts.len() == 2, line
    let value = parts[1].parse_int()?
    let stack = parts[0]
    assert stack == "script" or stack.starts_with("script;proc:root"), line
    total += value
    if stack == "script" { script_self += value }
    if stack.starts_with("script;proc:root") { root_inclusive += value }
    if stack == "script;proc:root" { root_self += value }
    if stack.starts_with("script;proc:root;") { root_children += value }
  }
  assert script_self > 0 and root_self > 0 and root_children > 0, traced.stderr
  assert root_inclusive == root_self + root_children, traced.stderr
  let script_inclusive = script_self + root_inclusive
  assert total == script_inclusive, traced.stderr
}
