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

test test_default_stream_completion_still_rejects_nonzero_exit { |ctx|
  let output = test.run_script(
    ctx,
    r"""let rows = run.stream --text sh -c "printf 'row\\n'; exit 7" ?
for row in rows { print $row }
""",
  )?
  assert ! output.success
  assert output.stdout == "row\n"
  assert "err: `sh` exited 7" in output.stderr

  let accepted = test.run_script(
    ctx,
    r"""let rows = run.stream --text --accept=[7] sh -c "printf 'row\\n'; exit 7" ?
for row in rows { print $row }
""",
  )?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == "row\n"

  let rejected = test.run_script(
    ctx,
    r"""let rows = run.stream --text --accept=[0] sh -c "printf 'row\\n'; exit 7" ?
for row in rows { print $row }
""",
  )?
  assert ! rejected.success
  assert rejected.stdout == "row\n"
  assert "err: `sh` exited with unaccepted status 7" in rejected.stderr
}

test test_default_stream_decode_failure_follows_yielded_rows { |ctx|
  let output = test.run_script(
    ctx,
    r"""let rows = run.stream --text sh -c "printf 'first\\n'; printf '\\377'" ?
for row in rows { print $row }
""",
  )?
  assert ! output.success
  assert output.stdout == "first\n"
  assert "streamed stdout was not valid UTF-8" in output.stderr
}

test test_process_stream_timeout_counts_while_consumer_is_paused { |ctx|
  let ready = test.temp_path(ctx, name: "stream-timeout-ready")
  let acknowledged = test.temp_path(ctx, name: "stream-timeout-ack")
  let source = r"""let ready = Path(args[0])
let acknowledged = Path(args[1])
let outcome: Result[Unit, Error] = try {
  let rows = run.stream --bytes --timeout=3s sh -c "printf '\\001'; touch \"$1\"; while [ ! -e \"$2\" ]; do sleep 0.01; done; sleep 3.25; printf '\\002'" sh $ready $acknowledged ?
  var first = true
  for chunk in rows {
    io.write_stdout_bytes(chunk)?
    if first {
      first = false
      var attempts = 0
      while ! ready.exists()? and attempts < 5000 { time.sleep(1ms)?; attempts += 1 }
      assert ready.exists()?
      acknowledged.write("go")?
      time.sleep(5s)?
    }
  }
  Ok()
}
match outcome {
  Err(ProcessError.Timeout {..}) => print "timeout"
  _ => exit 98
}
"""
  let output = test.run_script(ctx, source, [ready, acknowledged])?
  assert output.success, output.stderr
  assert output.stdout == "\x01timeout\n"
}

test test_process_stream_text_timeout_counts_while_consumer_is_paused { |ctx|
  let ready = test.temp_path(ctx, name: "stream-text-timeout-ready")
  let acknowledged = test.temp_path(ctx, name: "stream-text-timeout-ack")
  let source = r"""let ready = Path(args[0])
let acknowledged = Path(args[1])
let rows = run.stream --text --timeout=3s sh -c "printf 'ready\\n'; touch \"$1\"; while [ ! -e \"$2\" ]; do sleep 0.01; done; sleep 3.25; printf 'late\\n'" sh $ready $acknowledged ?
var first = true
for row in rows {
  print $row
  if first {
    first = false
    var attempts = 0
    while ! ready.exists()? and attempts < 5000 { time.sleep(1ms)?; attempts += 1 }
    assert ready.exists()?
    acknowledged.write("go")?
    time.sleep(5s)?
  }
}
"""
  let output = test.run_script(ctx, source, [ready, acknowledged])?
  assert ! output.success, output.stderr
  assert output.stdout == "ready\n", output.stderr
  assert "timeout" in output.stderr
}

test test_process_stream_keeps_predeadline_completion_after_consumer_pause { |ctx|
  let ready = test.temp_path(ctx, name: "stream-predeadline-ready")
  let acknowledged = test.temp_path(ctx, name: "stream-predeadline-ack")
  let completed = test.temp_path(ctx, name: "stream-predeadline-completed")
  let source = r"""let ready = Path(args[0])
let acknowledged = Path(args[1])
let completed = Path(args[2])
let rows = run.stream --bytes --timeout=3s sh -c "printf '\\001'; touch \"$1\"; while [ ! -e \"$2\" ]; do sleep 0.01; done; printf '\\002'; touch \"$3\"; exit 0" sh $ready $acknowledged $completed ?
var first = true
for chunk in rows {
  io.write_stdout_bytes(chunk)?
  if first {
    first = false
    var attempts = 0
    while ! ready.exists()? and attempts < 5000 { time.sleep(1ms)?; attempts += 1 }
    assert ready.exists()?
    acknowledged.write("go")?
    attempts = 0
    while ! completed.exists()? and attempts < 5000 { time.sleep(1ms)?; attempts += 1 }
    assert completed.exists()?
    time.sleep(4s)?
  }
}
"""
  let output = test.run_script(ctx, source, [ready, acknowledged, completed])?
  assert output.success, output.stderr
  assert output.stdout == "\x01\x02"
}

test test_process_stream_timeout_waits_for_child_after_stdout_closes { |ctx|
  let source = r"""let rows = run.stream --bytes --timeout=100ms sh -c "exec 1>&-; exec sleep 10" ?
for chunk in rows { io.write_stdout_bytes(chunk)? }
"""
  let output = test.run_script(ctx, source, [])?
  assert ! output.success, output.stderr
  assert "timeout" in output.stderr
  assert output.stdout == ""
}

test test_process_stream_timeout_waits_for_pipe_writers_after_child_exits { |ctx|
  let source = r"""let rows = run.stream --bytes --timeout=100ms sh -c "sleep 10 & exit 0" ?
for chunk in rows { io.write_stdout_bytes(chunk)? }
"""
  let output = test.run_script(ctx, source, [])?
  assert ! output.success, output.stderr
  assert "timeout" in output.stderr
  assert output.stdout == ""
}

test test_timed_process_stream_requires_retained_child_status { |ctx|
  let started = test.temp_path(ctx, name: "stream-no-child-status-started")
  let source = r"""process.set_signal_action("CLD", "ignore")?
let started = Path(args[0])
let outcome: Result[Unit, ProcessError] = try {
  let rows = run.stream --text --timeout=5s sh -c "touch \"$1\"" sh $started ?
  for chunk in rows { var byte_count = chunk.byte_len() }
  Ok()
}
match outcome {
  Err(is HostIo) => {}
  _ => exit 98
}
assert ! started.exists()?
print "not spawned"
"""
  let output = test.run_script(ctx, source, [started])?
  assert output.success, output.stderr
  assert output.stdout == "not spawned\n"
}

test test_process_stream_timeout_preserves_unobservable_child_identity { |ctx|
  let ready = test.temp_path(ctx, name: "stream-timeout-echild-ready")
  let acknowledged = test.temp_path(ctx, name: "stream-timeout-echild-ack")
  let survived = test.temp_path(ctx, name: "stream-timeout-echild-survived")
  let source = r"""let ready = Path(args[0])
let acknowledged = Path(args[1])
let survived = Path(args[2])
let rows = run.stream --bytes --timeout=5s sh -c "printf x && touch \"$1\"; while [ ! -e \"$2\" ]; do sleep 0.01; done; printf survived > \"$3\"" sh $ready $acknowledged $survived ?
var ready_attempts = 0
while ! ready.exists()? and ready_attempts < 5000 { time.sleep(1ms)?; ready_attempts += 1 }
assert ready.exists()?
process.set_signal_action("CLD", "ignore")?
process.set_signal_action("CLD", "default")?
let outcome: Result[Unit, ProcessError] = try {
  for chunk in rows { var byte_count = chunk.len() }
  Ok()
}
match outcome {
  Err(is HostIo) => {}
  _ => exit 98
}
acknowledged.write("go")?
var attempts = 0
while ! survived.exists()? and attempts < 5000 { time.sleep(1ms)?; attempts += 1 }
assert survived.exists()?
print "survived"
"""
  let output = test.run_script(ctx, source, [ready, acknowledged, survived])?
  assert output.success, output.stderr
  assert output.stdout == "survived\n"
}

test test_process_stream_cancel_reports_unobservable_child_identity { |ctx|
  let acknowledged = test.temp_path(ctx, name: "stream-cancel-echild-ack")
  let survived = test.temp_path(ctx, name: "stream-cancel-echild-survived")
  let source = r"""let acknowledged = Path(args[0])
let survived = Path(args[1])
let rows = run.stream --bytes --timeout=5s sh -c "printf x; while [ ! -e \"$1\" ]; do sleep 0.01; done; printf survived > \"$2\"" sh $acknowledged $survived ?
let outcome: Result[Unit, Error] = try {
  for _chunk in rows {
    process.set_signal_action("CLD", "ignore")?
    break
  }
  Ok()
}
match outcome {
  Err(is HostIo) => {}
  _ => exit 98
}
acknowledged.write("go")?
var attempts = 0
while ! survived.exists()? and attempts < 5000 { time.sleep(1ms)?; attempts += 1 }
assert survived.exists()?
print "survived"
"""
  let output = test.run_script(ctx, source, [acknowledged, survived])?
  assert output.success, output.stderr
  assert output.stdout == "survived\n"
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
  let seen = collect {
    for row in rows {
      yield row
      break
    }
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
  let seen = collect {
    for row in rows {
      yield row
      break
    }
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
