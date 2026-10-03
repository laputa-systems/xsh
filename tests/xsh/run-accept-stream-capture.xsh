test test_accept_late_stream_completion_is_a_checked_process_error { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let rejected: Result[Unit, ProcessError] = try {
  ctx "stream completion" {
    let rows = run.stream --text --accept=[1] sh -c "printf 'row\nfinal'; exit 0" ?
    for row in rows { print $row }
  }
}
match rejected {
  Err(ProcessError.UnexpectedExit {status: child_status}) => {
    guard child_status != null else { abort(99) }
    test.ok(child_status.ok)?
    test.eq(child_status.exit_code()?, 0)?
    print "captured"
  }
  _ => abort(98)
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """row
final
captured
"""
}

test test_accept_stream_decode_failure_is_a_checked_process_error { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let rejected: Result[Unit, ProcessError] = try {
  let rows = run.stream --text --accept=[0] sh -c "printf '\\377'" ?
  for row in rows { print $row }
}
match rejected {
  Err(ProcessError.InvalidUtf8) => print "captured decode"
  _ => abort(98)
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """captured decode
"""
}

test test_accept_malformed_stream_option_stays_outside_checked_capture { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc policy() [] -> List[Int] {
  print "option evaluated"
  [256]
}
let rejected = try {
  let rows = run.stream --text --accept=(policy()) sh -c "printf spawned" ?
  for row in rows { print $row }
}
print "captured"
""",
  )?
  assert ! output.success
  assert output.stdout == """option evaluated
"""
  assert "accept-policy" in output.stderr
}
