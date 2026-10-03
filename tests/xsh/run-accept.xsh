test test_accepted_exit_codes_keep_actual_status {
  run --accept=[0, 1] sh -c "exit 1"
  let status = run.status --accept=[0, 1] sh -c "exit 1"
  (status.exit_code()?) == (1)
  (! status.ok)
  let copied = run.text --accept=[0, 1] sh -c "printf accepted; exit 1" ?
  (copied) == ("accepted")
}

test test_accept_rejections_and_capture_record_status {
  let rejected_1 = run.text --accept=[0, 1] sh -c "exit 2"
  test.error_kind(rejected_1, "unexpected-exit")?
  let rejected_2 = run.bytes --accept=[1] sh -c "exit 0"
  test.error_kind(rejected_2, "unexpected-exit")?
  let captured = run.capture --text --accept=[1] sh -c "printf out; printf err >&2; exit 1" ?
  (captured.stdout) == ("out")
  (captured.stderr) == ("err")
  (captured.status.exited_with(1))
  (! captured.status.ok)
  let rejected_3 = run.capture --bytes --accept=[0] sh -c "exit 1"
  test.error_kind(rejected_3, "unexpected-exit")?
}

test test_accept_never_normalizes_signals_setup_or_decode_failures {
  let rejected_4 = run.text --accept=[0, 143] sh -c "kill -TERM $$"
  test.error_kind(rejected_4, "signal")?
  let rejected_5 = run.text --accept=[0, 127] xsh-accept-definitely-missing-command
  test.error_kind(rejected_5, "not-found")?
  let rejected_6 = run.text --accept=[0, 1] sh -c "printf '\\377'; exit 1"
  test.error_kind(rejected_6, "invalid-utf8")?
  let rejected_7 = run.text --timeout=10ms --accept=[0, 137] sh -c "sleep 5"
  test.error_kind(rejected_7, "timeout")?
}

test test_accept_command_and_owned_wait_preserve_policy {
  let command = process.command {
    accept = [0, 1]
    run sh -c "exit 1"
  }
  (process.run(command)?.exited_with(1))
  let child = spawn command?
  ((wait child?).exited_with(1))
  let argv_command = process.command_argv("sh", ["sh", "-c", "exit 2"], accept: [0, 1])
  test.error_kind(process.run(argv_command), "unexpected-exit")?
  let rejected = spawn argv_command?
  let sibling = spawn run --accept=[0] sh -c "exit 0" ?
  let rejection = wait [rejected, sibling]
  test.error_kind(rejection, "unexpected-exit")?
  let consumed = wait sibling
  test.error_kind(consumed, "unknown")?
  let canceled = spawn run --accept=[0, 143] sh -c "sleep 5" ?
  canceled.cancel(kill_after: 0ms)?
}

test test_accept_status_plain_pipeline_and_external_argv { |ctx|
  for source in [
    "run --accept=[0] sh -c \"exit 1\"\nprint unreachable\n",
    "let status = run.status --accept=[0] sh -c \"exit 1\"\nprint unreachable\n",
    "run --accept=[0,1] sh -c \"exit 1\" | run sh -c \"exit 2\"\nprint unreachable\n",
    "run --accept=[0] sh -c \"exit 2\" | run --accept=[0,2] sh -c \"exit 2\"\nprint unreachable\n",
  ] {
    let output = test.run_script(ctx, source)?
    (! output.success)
    (output.stdout) == ("")
  }
  let output = test.run_script(ctx, "run --accept=[0,1] sh -c \"exit 1\" | run --accept=[0,2] sh -c \"exit 2\"\nprint accepted\n")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("accepted\n")
  let literal = test.run_script(ctx, "let text = run.text --accept=[0] sh -c \"printf %s \\\"$1\\\"\" arg0 --accept=[9] ?\nprint $text\n")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = literal
    assert assertion_condition, assertion_message
  }
  (literal.stdout) == ("--accept=[9]\n")
}

test test_accept_invalid_policy_is_rejected_before_spawn { |ctx|
  for policy in ["[]", "[0,0]", "[-1]", "[256]", "[true]", "0"] {
    let source = f"""
      run --accept=${policy} sh -c \"printf spawned\"

      """
    let output = test.run_script(ctx, source)?
    (! output.success)
    (output.stdout) == ("")
  }
  let duplicate = test.run_script(ctx, "run --accept=[0] --accept=[0] sh -c \"printf spawned\"\n")?
  (! duplicate.success)
  ("parse.run-option" in duplicate.stderr)
  let effects = test.run_script(ctx, "proc main() [process] { let s = run.status --accept=[0] sh -c \"exit 0\" }\n")?
  (! effects.success)
  ("check.effect-violation" in effects.stderr)
  let dynamic = test.run_script(ctx, "var codes = [0]\ncodes = [256]\nrun --accept=(codes) sh -c \"printf spawned\"\n")?
  (! dynamic.success)
  (dynamic.stdout) == ("")
  ("accept-policy" in dynamic.stderr)
}

test test_accept_process_stream_reports_late_failure { |ctx|
  let output = test.run_script(ctx, "let rows = run.stream --text --accept=[0] sh -c \"printf 'row\\n'; exit 1\" ?\nfor row in rows { print $row }\nprint unreachable\n")?
  (! output.success)
  {
    let assertion_condition = output.stdout == "row\n"
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  ("unexpected-exit" in output.stderr)
  let accepted = test.run_script(ctx, "let rows = run.stream --text --accept=[0,1] sh -c \"printf 'row\\nfinal'; exit 1\" ?\nfor row in rows { print $row }\n")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = accepted
    assert assertion_condition, assertion_message
  }
  (accepted.stdout) == ("row\nfinal\n")
  let unterminated = test.run_script(ctx, "let rows = run.stream --text --accept=[0] sh -c \"printf final; exit 1\" ?\nfor row in rows { print $row }\n")?
  (! unterminated.success)
  (unterminated.stdout) == ("final\n")
}

test test_accept_owned_child_keeps_a_validated_snapshot {
  var codes = [0, 1]
  let child = spawn run --accept=(codes) sh -c "exit 1" ?
  codes[1] = 2
  ((wait child?).exited_with(1))
}

test test_accept_wait_any_and_ready_apply_the_owned_policy {
  let selected = spawn run --accept=[0] sh -c "exit 1" ?
  test.error_kind(process.wait_any([selected]), "unexpected-exit")?
  let ready = spawn run --accept=[0] sh -c "exit 1" ?
  test.error_kind(process.wait_ready([ready]), "unexpected-exit")?
}

test test_accept_direct_status_capture_keeps_nominal_error_and_actual_zero {
  let rejected = try {
    let _ = run.status --accept=[1] sh -c "exit 0"
    print "unreachable"
  }
  match rejected {
    Err(ProcessError.UnexpectedExit {status: child_status}) => {
      guard child_status != null else { abort(99) }
      (child_status.ok)
      (child_status.exit_code()?) == (0)
    }
    Err(error) => test.fail(f"unexpected error: ${error.message}")?
    Ok(_) => test.fail("direct status validation succeeded")?
  }
}

test test_accept_validation_effect_is_inferred_and_can_be_captured { |ctx|
  let denied = test.run_script(ctx, """
proc validate_status() {
  let observed = run.status --accept=[0] sh -c "exit 0"
}
proc main() [process] { validate_status() }
""")?
  (! denied.success)
  ("check.effect-violation" in denied.stderr)
  let captured = test.run_script(ctx, """
proc captured_status() [process] -> Result[Unit, ProcessError] {
  try { let observed = run.status --accept=[0] sh -c "exit 1" }
}
match captured_status() {
  Err(ProcessError.UnexpectedExit) => print "rejected"
  Err(error) => test.fail(error.message)?
  Ok(_) => test.fail("completion unexpectedly accepted")?
}
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = captured
    assert assertion_condition, assertion_message
  }
  (captured.stdout) == ("rejected\n")
}

test test_accept_dynamic_configuration_stays_outside_completion_capture { |ctx|
  let output = test.run_script(ctx, """
var codes = [0]
codes = [256]
let rejected = try {
  let observed = run.status --accept=(codes) sh -c "printf spawned"
}
print "unreachable"
""")?
  (! output.success)
  (output.stdout) == ("")
  ("accept-policy" in output.stderr)
}
