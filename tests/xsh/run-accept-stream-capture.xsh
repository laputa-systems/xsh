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
    guard child_status != null else { exit 99 }
    test.ok(child_status.ok)?
    test.eq(child_status.exit_code()?, 0)?
    print "captured"
  }
  _ => exit 98
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
  _ => exit 98
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
  let rows = run.stream --text --accept=policy() sh -c "printf spawned" ?
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

test test_accept_byte_stream_delivers_exact_bytes_before_a_late_policy_failure { |ctx|
  let source = r"""let rows = run.stream --bytes --accept=[0] sh -c "printf '\\000\\377'; exit 1"
for row in rows { run cat < (row) }
"""
  let script = test.temp_file(ctx, name: "accept-stream-bytes.xsh", contents: bytes.from_text(source))?
  let output = run.capture --bytes ${ctx.xsh_bin} $script
  assert ! output.status.ok
  assert output.stdout == b"\0\xff"
  assert "err: `sh` exited with unaccepted status 1" in output.stderr as Str
}

test test_accept_does_not_lift_the_capture_limit {
  match try run.bytes --accept=[0] head -c 16777217 /dev/zero {
    Err(ProcessError.CaptureLimit {..}) => {}
    Err(error) => test.fail(error.message)
    Ok(_) => test.fail("capture limit was accepted")
  }
}

test test_accept_policy_expression_runs_once_before_the_child_starts { |ctx|
  let output = test.expect(
    ctx,
    r"""proc accepted_codes() [process, error] -> List[Int] {
  run printf "option\n"
  return [0, 1]
}
run --accept=accepted_codes() sh -c "printf child; exit 1"
""",
    status: 0,
  )?
  assert output.stdout == "option\nchild"
}

test test_accept_stream_consumer_that_stops_early_ends_the_child { |ctx|
  let root = test.temp_dir(ctx, name: "accept-stream-cancel")?
  let marker = fp"{root}/marker"
  let rows = run.stream --text --accept=[0] sh -c "printf 'ready\n'; sleep 2; touch $1" sh $marker
  var seen = []
  for row in rows {
    seen += [row]
    break
  }

  assert seen == ["ready"]
  # The child would create the marker two seconds after its first row.
  time.sleep(2200ms)
  assert ! marker.exists()?, "early consumer left its child alive"
}

test test_accept_stream_consumer_that_stops_early_ends_descendants_of_an_exited_child { |ctx|
  let root = test.temp_dir(ctx, name: "accept-stream-exited-child")?
  let marker = fp"{root}/marker"
  let rows = run.stream --text --accept=[0] sh -c "(sleep 0.2; printf 'ready\n'; sleep 2; touch $1) & exit 0" sh \
    $marker
  var seen = []
  for row in rows {
    seen += [row]
    break
  }

  assert seen == ["ready"]
  time.sleep(2300ms)
  assert ! marker.exists()?, "stream cancellation left its exited child's descendant alive"
}

test test_accept_byte_stream_feeds_stdin_while_draining_large_output {
  let payload = bytes.concat([b"a\0\xff\n", bytes.zero(2097152)?])
  let rows = run.stream --bytes --timeout=3s --accept=[0] cat < $payload
  let chunks = [row for row in rows]
  assert payload.len() == 2097156
  assert bytes.concat(chunks) == payload
}
