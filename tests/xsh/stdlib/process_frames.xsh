test test_run_capture_and_pipeline_keep_operand_order_on_frames { |ctx|
  let output = test.run_xsh(ctx, r"""proc mark(label: Str, value: Str) [io] -> Str {
  print $label
  return value
}
let captured = run.capture --text XSH_PROCESS_FRAME=${mark("capture-env", "ready")} ${mark("capture-target", "printf")} "%s" "captured"
assert captured.stdout == "captured"
let piped = run.capture --text ${mark("pipeline-head", "printf")} "%s" ${mark("pipeline-arg", "pipe")} | run ${mark("pipeline-tail", "tr")} a-z A-Z
assert piped.stdout == "PIPE"
run true
run true ?
print "done"
""")?
  assert output.success, output.stderr
  assert output.stdout == """capture-env
capture-target
pipeline-head
pipeline-arg
pipeline-tail
done
""", output.stdout
}

test test_run_option_conversion_stops_before_later_operands { |ctx|
  let output = test.run_xsh(ctx, r"""proc mark(label: Str, value: Str) [io] -> Str {
  print $label
  return value
}
proc invalid_accept() [io] -> List[Int] {
  print "accept"
  return [300]
}
run.capture --text --accept=invalid_accept() ACCEPT_ORDER=${mark("accept-env", "ready")} ${mark("accept-target", "true")}
""")?
  assert !output.success, output.stderr
  assert "accept-policy" in output.stderr, output.stderr
  assert output.stdout == """accept
""", output.stdout
}

test test_run_cpu_max_validation_stops_before_later_operands { |ctx|
  let output = test.run_xsh(ctx, r"""proc mark(label: Str, value: Str) [io] -> Str {
  print $label
  return value
}
proc invalid_cpu_max() [io] -> Int {
  print "cpu"
  return 0
}
run.capture --text --cpumax=invalid_cpu_max() CPU_ORDER=${mark("cpu-env", "ready")} ${mark("cpu-target", "true")}
""")?
  assert !output.success, output.stderr
  assert "cpu-max" in output.stderr, output.stderr
  assert output.stdout == """cpu
""", output.stdout
}

test test_run_operands_evaluate_in_source_order_before_conversion_stops_later_operands { |ctx|
  let output = test.run_xsh(ctx, r"""proc accept_option() [io] -> List[Int] {
  print "accept"
  return [0]
}
proc timeout_option() [io] -> Duration {
  print "timeout"
  return 30s
}
proc cpu_option() [io] -> Int {
  print "cpu"
  return 100
}
proc env_value() [io] -> Str {
  print "env"
  return "ready"
}
proc later_argument() [io] -> Str {
  print "argument"
  return "late"
}
let empty: List[Str] = []
run.capture --text --accept=accept_option() --timeout=timeout_option() --cpumax=cpu_option() XSH_PROCESS_FRAME=${env_value()} *${empty} ${later_argument()}
""")?
  assert !output.success, output.stderr
  assert "empty-command" in output.stderr, output.stderr
  assert output.stdout == """accept
timeout
cpu
env
""", output.stdout
}

test test_run_target_conversion_fails_before_later_arguments_run { |ctx|
  let output = test.run_xsh(ctx, r"""proc mark() [io] -> Str {
  print "later-argument"
  return "argument"
}
let empty: List[Str] = []
run.capture --text *${empty} ${mark()}
""")?
  assert output.status == 2, output.stderr
  assert output.stdout == "", output.stdout
}

test test_run_tail_arguments_and_redirections_keep_written_order { |ctx|
  let output = test.run_xsh(ctx, r"""proc mark(label: Str, value: Str) [io] -> Str {
  print $label
  return value
}
let captured = run.capture --text ${mark("tail-target", "sh")} -c "printf ignored" ${mark("tail-before-redirection", "before")} > ${mark("tail-redirection", "/dev/null")} ${mark("tail-after-redirection", "after")}
assert captured.stdout == ""
""")?
  assert output.success, output.stderr
  assert output.stdout == """tail-target
tail-before-redirection
tail-redirection
tail-after-redirection
""", output.stdout
}

test test_run_source_order_is_shared_by_spawn_and_process_command_builder { |ctx|
  let output = test.run_xsh(ctx, r"""proc mark(label: Str, value: Str) [io] -> Str {
  print $label
  return value
}
proc timeout(label: Str) [io] -> Duration {
  print $label
  return 1s
}
let plain = run.status --timeout=timeout("plain-timeout") PLAIN_ORDER=${mark("plain-env", "ready")} ${mark("plain-target", "true")} ${mark("plain-argument", "ignored")}
assert plain.exited_with(0)
let child = spawn run --timeout=timeout("spawn-timeout") SPAWN_ORDER=${mark("spawn-env", "ready")} ${mark("spawn-target", "true")} ${mark("spawn-argument", "ignored")} ?
let statuses = wait [child]?
assert statuses[0].exit_code()? == 0
let command = process.command {
  run --timeout=timeout("builder-timeout") BUILDER_ORDER=${mark("builder-env", "ready")} ${mark("builder-target", "true")} ${mark("builder-argument", "ignored")}
}
assert process.run(command)?.exited_with(0)
""")?
  assert output.success, output.stderr
  assert output.stdout == """plain-timeout
plain-env
plain-target
plain-argument
spawn-timeout
spawn-env
spawn-target
spawn-argument
builder-timeout
builder-env
builder-target
builder-argument
""", output.stdout
}

test test_process_frame_forms_keep_trace_events { |ctx|
  let traced = test.run_xsht_trace(
    ctx,
    """let captured = run.capture --text printf captured
let piped = run printf piped | run cat
let child = spawn run true ?
let status = wait child ?
run true ?
""",
    ["--trace", "--raw"],
  )?
  assert traced.success, traced.stderr
  for kind in [
    "kind=run.start",
    "kind=run.end",
    "kind=pipeline.enter",
    "kind=pipeline.segment.end",
    "kind=spawn.start",
    "kind=spawn.ready",
    "kind=wait.start",
    "kind=wait.end",
  ] {
    assert kind in traced.stderr
  }
}

test test_spawn_run_command_and_wait_keep_process_lifecycles_on_frames { |ctx|
  let output = test.run_xsh(ctx, r"""let run_child = spawn run sh -c "exit 3" ?
let command = process.command_argv("sh", ["sh", "-c", "exit 4"])
let command_child = spawn command?
let run_status = wait run_child?
let command_statuses = wait [command_child]?
assert run_status.exit_code()? == 3
assert command_statuses[0].exit_code()? == 4
print "waited"
""")?
  assert output.success, output.stderr
  assert output.stdout == "waited\n", output.stdout
}
