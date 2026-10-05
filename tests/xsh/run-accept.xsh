test test_accepted_exit_codes_keep_actual_status {
  run --accept=[0, 1] sh -c "exit 1"
  let status = run.status --accept=[0, 1] sh -c "exit 1"
  assert status.exit_code()? == 1
  assert ! status.ok
  let copied = run.text --accept=[0, 1] sh -c "printf accepted; exit 1" ?
  assert copied == "accepted"
}

test test_accept_rejections_and_capture_record_status {
  let rejected_1 = try run.text --accept=[0, 1] sh -c "exit 2"
  test.error_kind(rejected_1, "unexpected-exit")
  let rejected_2 = try run.bytes --accept=[1] sh -c "exit 0"
  test.error_kind(rejected_2, "unexpected-exit")
  let captured = run.capture --text --accept=[1] sh -c "printf out; printf err >&2; exit 1" ?
  assert captured.stdout == "out"
  assert captured.stderr == "err"
  assert captured.status.exited_with(1)
  assert ! captured.status.ok
  let rejected_3 = try run.capture --bytes --accept=[0] sh -c "exit 1"
  test.error_kind(rejected_3, "unexpected-exit")
}

test test_accept_never_normalizes_signals_setup_or_decode_failures {
  let rejected_4 = try run.text --accept=[0, 143] sh -c "kill -TERM $$"
  test.error_kind(rejected_4, "signal")
  let rejected_5 = try run.text --accept=[0, 127] xsh-accept-definitely-missing-command
  test.error_kind(rejected_5, "not-found")
  let rejected_6 = try run.text --accept=[0, 1] sh -c "printf '\\377'; exit 1"
  test.error_kind(rejected_6, "invalid-utf8")
  let rejected_7 = try run.text --timeout=10ms --accept=[0, 137] sh -c "sleep 5"
  test.error_kind(rejected_7, "timeout")
}

test test_accept_command_and_owned_wait_preserve_policy {
  let command = process.command {
    accept = [0, 1]
    run sh -c "exit 1"
  }
  assert process.run(command)?.exited_with(1)
  let child = spawn command?
  assert (wait child?).exited_with(1)
  let argv_command = process.command_argv("sh", ["sh", "-c", "exit 2"], accept: [0, 1])
  test.error_kind(process.run(argv_command), "unexpected-exit")
  let rejected = spawn argv_command?
  let sibling = spawn run --accept=[0] sh -c "exit 0" ?
  let rejection = wait [rejected, sibling]
  test.error_kind(rejection, "unexpected-exit")
  let consumed = wait sibling
  test.error_kind(consumed, "unknown")
  let canceled = spawn run --accept=[0, 143] sh -c "sleep 5" ?
  canceled.cancel(kill_after: 0ms)
}

test test_accept_status_plain_pipeline_and_external_argv { |ctx|
  for source in [
    """run --accept=[0] sh -c "exit 1"
print unreachable
""",
    """let status = run.status --accept=[0] sh -c "exit 1"
print unreachable
""",
    """run --accept=[0,1] sh -c "exit 1" | run sh -c "exit 2"
print unreachable
""",
    """run --accept=[0] sh -c "exit 2" | run --accept=[0,2] sh -c "exit 2"
print unreachable
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success
    assert output.stdout == ""
  }

  let output = test.run_script(
    ctx,
    """run --accept=[0,1] sh -c "exit 1" | run --accept=[0,2] sh -c "exit 2"
print accepted
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """accepted
"""
  let literal = test.run_script(
    ctx,
    """let text = run.text --accept=[0] sh -c "printf %s \\"$1\\"" arg0 --accept=[9] ?
print $text
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = literal
    assert assertion_condition, assertion_message
  }
  assert literal.stdout == """--accept=[9]
"""
}

test test_accept_invalid_policy_is_rejected_before_spawn { |ctx|
  for policy in ["[]", "[0,0]", "[-1]", "[256]", "[true]", "0"] {
    let source = f"""
      run --accept={policy} sh -c \"printf spawned\"

      """
    let output = test.run_script(ctx, source)?
    assert ! output.success
    assert output.stdout == ""
  }

  let duplicate = test.run_script(
    ctx,
    """run --accept=[0] --accept=[0] sh -c "printf spawned"
""",
  )?
  assert ! duplicate.success
  assert "parse.run-option" in duplicate.stderr
  let effects = test.run_script(
    ctx,
    """proc main() [process] { let s = run.status --accept=[0] sh -c "exit 0" }
""",
  )?
  assert ! effects.success
  assert "check.effect-violation" in effects.stderr
  let dynamic = test.run_script(
    ctx,
    """var codes = [0]
codes = [256]
run --accept=codes sh -c "printf spawned"
""",
  )?
  assert ! dynamic.success
  assert dynamic.stdout == ""
  assert "accept-policy" in dynamic.stderr
}

test test_accept_process_stream_reports_late_failure { |ctx|
  let output = test.run_script(
    ctx,
    """let rows = run.stream --text --accept=[0] sh -c "printf 'row\\n'; exit 1" ?
for row in rows { print $row }
print unreachable
""",
  )?
  assert ! output.success
  {
    let assertion_condition = output.stdout == """row
"""
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  assert "err: `sh` exited with unaccepted status 1" in output.stderr
  let accepted = test.run_script(
    ctx,
    """let rows = run.stream --text --accept=[0,1] sh -c "printf 'row\\nfinal'; exit 1" ?
for row in rows { print $row }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = accepted
    assert assertion_condition, assertion_message
  }
  assert accepted.stdout == """row
final
"""
  let unterminated = test.run_script(
    ctx,
    """let rows = run.stream --text --accept=[0] sh -c "printf final; exit 1" ?
for row in rows { print $row }
""",
  )?
  assert ! unterminated.success
  assert unterminated.stdout == """final
"""
}

test test_accept_owned_child_keeps_a_validated_snapshot {
  var codes = [0, 1]
  let child = spawn run --accept=codes sh -c "exit 1" ?
  codes[1] = 2
  assert (wait child?).exited_with(1)
}

test test_accept_wait_any_and_ready_apply_the_owned_policy {
  let selected = spawn run --accept=[0] sh -c "exit 1" ?
  test.error_kind(process.wait_any([selected]), "unexpected-exit")
  let ready = spawn run --accept=[0] sh -c "exit 1" ?
  test.error_kind(process.wait_ready([ready]), "unexpected-exit")
}

test test_accept_direct_status_capture_keeps_nominal_error_and_actual_zero {
  let rejected = try {
    let _ = run.status --accept=[1] sh -c "exit 0"
    print "unreachable"
  }
  match rejected {
    Err(ProcessError.UnexpectedExit {status: child_status}) => {
      guard child_status != null else {
        exit 99
      }
      assert child_status.ok
      assert child_status.exit_code()? == 0
    }
    Err(error) => test.fail(f"unexpected error: {error.message}")
    Ok(_) => test.fail("direct status validation succeeded")
  }
}

test test_accept_validation_effect_is_inferred_and_can_be_captured { |ctx|
  let denied = test.run_script(
    ctx,
    """
proc validate_status() {
  let observed = run.status --accept=[0] sh -c "exit 0"
}
proc main() [process] { validate_status() }
""",
  )?
  assert ! denied.success
  assert "check.effect-violation" in denied.stderr
  let captured = test.run_script(
    ctx,
    """
proc captured_status() [process] -> Result[Unit, ProcessError] {
  try { let observed = run.status --accept=[0] sh -c "exit 1" }
}
match captured_status() {
  Err(ProcessError.UnexpectedExit) => print "rejected"
  Err(error) => test.fail(error.message)?
  Ok(_) => test.fail("completion unexpectedly accepted")?
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = captured
    assert assertion_condition, assertion_message
  }
  assert captured.stdout == """rejected
"""
}

test test_accept_dynamic_configuration_stays_outside_completion_capture { |ctx|
  let output = test.run_script(
    ctx,
    """
var codes = [0]
codes = [256]
let rejected = try {
  let observed = run.status --accept=codes sh -c "printf spawned"
}
print "unreachable"
""",
  )?
  assert ! output.success
  assert output.stdout == ""
  assert "accept-policy" in output.stderr
}
