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
  assert output.stdout == """capture-target
capture-env
pipeline-head
pipeline-arg
pipeline-tail
done
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
