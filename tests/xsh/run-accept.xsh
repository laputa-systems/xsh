test test_accepted_exit_codes_keep_actual_status { |ctx|
  run --accept=[0, 1] sh -c "exit 1"
  let status = run.status --accept=[0, 1] sh -c "exit 1"
  test.eq(status.exit_code()?, 1)?
  test.ok(! status.ok)?
  let copied = run.text --accept=[0, 1] sh -c "printf accepted; exit 1" ?
  test.eq(copied, "accepted")?
}

test test_accept_rejections_and_capture_record_status { |ctx|
  let rejected_1 = run.text --accept=[0, 1] sh -c "exit 2"
  test.error_kind(rejected_1, "unexpected-exit")?
  let rejected_2 = run.bytes --accept=[1] sh -c "exit 0"
  test.error_kind(rejected_2, "unexpected-exit")?
  let captured = run.capture --text --accept=[1] sh -c "printf out; printf err >&2; exit 1" ?
  test.eq(captured.stdout, "out")?
  test.eq(captured.stderr, "err")?
  test.ok(captured.status.exited_with(1))?
  test.ok(! captured.status.ok)?
  let rejected_3 = run.capture --bytes --accept=[0] sh -c "exit 1"
  test.error_kind(rejected_3, "unexpected-exit")?
}

test test_accept_never_normalizes_signals_setup_or_decode_failures { |ctx|
  let rejected_4 = run.text --accept=[0, 143] sh -c "kill -TERM $$"
  test.error_kind(rejected_4, "signal")?
  let rejected_5 = run.text --accept=[0, 127] xsh-accept-definitely-missing-command
  test.error_kind(rejected_5, "not-found")?
  let rejected_6 = run.text --accept=[0, 1] sh -c "printf '\\377'; exit 1"
  test.error_kind(rejected_6, "invalid-utf8")?
  let rejected_7 = run.text --timeout=10ms --accept=[0, 137] sh -c "sleep 5"
  test.error_kind(rejected_7, "timeout")?
}

test test_accept_command_and_owned_wait_preserve_policy { |ctx|
  let command = process.command {
    accept = [0, 1]
    run sh -c "exit 1"
  }
  test.ok(process.run(command)?.exited_with(1))?
  let child = spawn command?
  test.ok((wait child?).exited_with(1))?
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
    test.ok(! output.success)?
    test.eq(output.stdout, "")?
  }
  let output = test.run_script(ctx, "run --accept=[0,1] sh -c \"exit 1\" | run --accept=[0,2] sh -c \"exit 2\"\nprint accepted\n")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "accepted\n")?
  let literal = test.run_script(ctx, "let text = run.text --accept=[0] sh -c \"printf %s \\\"$1\\\"\" arg0 --accept=[9] ?\nprint $text\n")?
  test.ok(literal.success, literal.stderr)?
  test.eq(literal.stdout, "--accept=[9]\n")?
}

test test_accept_invalid_policy_is_rejected_before_spawn { |ctx|
  for policy in ["[]", "[0,0]", "[-1]", "[256]", "[true]", "0"] {
    let output = test.run_script(ctx, f"run --accept=${policy} sh -c \"printf spawned\"\n")?
    test.ok(! output.success)?
    test.eq(output.stdout, "")?
  }
  let duplicate = test.run_script(ctx, "run --accept=[0] --accept=[0] sh -c \"printf spawned\"\n")?
  test.ok(! duplicate.success)?
  test.ok("parse.run-option" in duplicate.stderr)?
  let effects = test.run_script(ctx, "proc main() [process] { let s = run.status --accept=[0] sh -c \"exit 0\" }\n")?
  test.ok(! effects.success)?
  test.ok("check.effect-violation" in effects.stderr)?
  let dynamic = test.run_script(ctx, "var codes = [0]\ncodes = [256]\nrun --accept=(codes) sh -c \"printf spawned\"\n")?
  test.ok(! dynamic.success)?
  test.eq(dynamic.stdout, "")?
  test.ok("accept-policy" in dynamic.stderr)?
}

test test_accept_process_stream_reports_late_failure { |ctx|
  let output = test.run_script(ctx, "let rows = run.stream --text --accept=[0] sh -c \"printf 'row\\n'; exit 1\" ?\nfor row in rows { print $row }\nprint unreachable\n")?
  test.ok(! output.success)?
  test.ok(output.stdout == "row\n", output.stderr)?
  test.ok("unexpected-exit" in output.stderr)?
  let accepted = test.run_script(ctx, "let rows = run.stream --text --accept=[0,1] sh -c \"printf 'row\\nfinal'; exit 1\" ?\nfor row in rows { print $row }\n")?
  test.ok(accepted.success, accepted.stderr)?
  test.eq(accepted.stdout, "row\nfinal\n")?
  let unterminated = test.run_script(ctx, "let rows = run.stream --text --accept=[0] sh -c \"printf final; exit 1\" ?\nfor row in rows { print $row }\n")?
  test.ok(! unterminated.success)?
  test.eq(unterminated.stdout, "final\n")?
}

test test_accept_owned_child_keeps_a_validated_snapshot { |ctx|
  var codes = [0, 1]
  let child = spawn run --accept=(codes) sh -c "exit 1" ?
  codes[1] = 2
  test.ok((wait child?).exited_with(1))?
}

test test_accept_wait_any_and_ready_apply_the_owned_policy { |ctx|
  let selected = spawn run --accept=[0] sh -c "exit 1" ?
  test.error_kind(process.wait_any([selected]), "unexpected-exit")?
  let ready = spawn run --accept=[0] sh -c "exit 1" ?
  test.error_kind(process.wait_ready([ready]), "unexpected-exit")?
}

test test_accept_direct_status_capture_keeps_nominal_error_and_actual_zero { |ctx|
  let rejected = try {
    let observed = run.status --accept=[1] sh -c "exit 0"
    print "unreachable"
  }
  match rejected {
    Err(ProcessError.UnexpectedExit {status: child_status}) => {
      guard child_status != null else { abort(99) }
      test.ok(child_status.ok)?
      test.eq(child_status.exit_code()?, 0)?
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
  test.ok(! denied.success)?
  test.ok("check.effect-violation" in denied.stderr)?
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
  test.ok(captured.success, captured.stderr)?
  test.eq(captured.stdout, "rejected\n")?
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
  test.ok(! output.success)?
  test.eq(output.stdout, "")?
  test.ok("accept-policy" in output.stderr)?
}
