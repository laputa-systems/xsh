pure timed_out(result: Result[Int]) -> Bool {
  match result {
    Ok(_) => false
    Err(failure) => failure is Timeout
  }
}

test test_within_returns_the_body_value_when_it_finishes_in_time {
  let value = within 30s {
    time.sleep(1ms)
    7
  }
  assert value == Ok(7)

  # In statement position the scope is a `Result[Unit]`.
  var ran = false
  within 30s {
    ran = true
  }

  assert ran

  # A body that finishes is not failed afterwards, however late it is.
  let late = within 1ms {
    var total = 0
    for step in range(20000) {
      total += step
    }

    total
  }
  assert late is Ok(_) or timed_out(late)
}

test test_within_stops_a_sleep_and_reports_timeout {
  let started = time.now()
  let result = within 50ms {
    time.sleep(30s)
    1
  }
  assert timed_out(result)
  assert time.now() - started < 10000
  match result {
    Ok(_) => assert false
    Err(failure) => assert "did not finish within 50ms" in failure.message
  }
}

test test_within_stops_a_running_child_process {
  let started = time.now()
  let result = within 100ms {
    run sleep 30
    1
  }
  assert timed_out(result)
  assert time.now() - started < 10000

  let captured = within 100ms {
    let text = run.text sh -c "sleep 30; echo late" ?
    text.byte_len()
  }
  assert timed_out(captured)
}

# A child that ignores the request to stop is killed after the grace period.
test test_within_kills_a_child_that_ignores_termination {
  let started = time.now()
  let result = within 100ms {
    run sh -c "trap '' TERM; sleep 30"
    1
  }
  assert timed_out(result)
  assert time.now() - started < 10000
}

# A `retry` inside the block does not start another attempt past the deadline.
test test_within_is_not_retried_from_inside {
  let started = time.now()
  var attempts = 0
  let result = within 100ms {
    retry [0ms, 0ms, 0ms] {
      attempts += 1
      time.sleep(30s)
    }
    attempts
  }
  assert timed_out(result)
  assert attempts == 1
  assert time.now() - started < 10000
}

test test_within_checks_computation_between_statements {
  var spins = 0
  let result = within 50ms {
    while spins >= 0 {
      spins += 1
    }

    spins
  }
  assert timed_out(result)
  assert spins > 0
}

# The timeout unwinds like a failure: every block it leaves runs its `defer`
# and `errdefer` actions, in the callee first, and they are not cut short.
test test_within_runs_cleanup_on_the_way_out { |ctx|
  let output = test.expect(
    ctx,
    """proc nap(pause: Duration) [time, error] -> Result[Int] {
  defer { print "callee defer" }
  time.sleep(pause)
  1
}

for pause in [30s, 1ms] {
  let result = within 50ms {
    defer {
      time.sleep(120ms)
      print "defer"
    }
    errdefer { print "errdefer" }
    let napped = nap(pause)?
    print "body end"
    napped
  }
  match result {
    Ok(value) => print f"ok {value}"
    Err(failure) => print f"timeout {failure is Timeout}"
  }
}
""",
    status: 0,
  )?
  assert output.stdout == """callee defer
errdefer
defer
timeout true
callee defer
body end
defer
ok 1
"""
}

# `try` inside the block does not hold the timeout back.
test test_within_timeout_is_not_captured_by_try {
  var after = false
  let result = within 100ms {
    let inner = try { run sleep 30 }
    after = true
    let again = try {
      time.sleep(30s)
    }
    inner is Ok(_) and again is Ok(_)
  }
  assert result is Err(_)
  assert ! after
}

proc fail_inside() [time, error] -> Result[Int] {
  let value = within 30s {
    error.fail("body failed")
    1
  }?
  value
}

# The scope reports only its deadline. Any other failure of the body goes
# where it would without the scope.
test test_within_lets_body_failures_through {
  let failed = fail_inside()
  match failed {
    Ok(_) => assert false
    Err(failure) => {
      assert "body failed" in failure.message
      assert ! (failure is Timeout)
    }
  }
}

test test_within_nearest_deadline_wins {
  # The inner scope times out; the outer one goes on.
  let outer = within 30s {
    let inner = within 50ms {
      time.sleep(30s)
      1
    }
    timed_out(inner)
  }
  assert outer == Ok(true)

  # The outer deadline is nearer: it ends both, and the inner scope does
  # not report it.
  var inner_reported = false
  let result = within 50ms {
    let inner = within 30s {
      time.sleep(30s)
      1
    }
    inner_reported = true
    inner ?? 0
  }
  assert timed_out(result)
  assert ! inner_reported

  # A scope that has ended leaves no deadline behind.
  let first = within 50ms { 1 }
  time.sleep(80ms)
  let second = within 30s {
    time.sleep(1ms)
    2
  }
  assert first == Ok(1)
  assert second == Ok(2)
}

proc first_fit(limits: List[Duration]) [time, error] -> Result[Int] {
  var rounds = 0
  for limit in limits {
    rounds += 1
    let result = within limit {
      continue when rounds == 1
      return 100 + rounds when rounds == 3
      time.sleep(30s)
      1
    }
    assert timed_out(result)
  }

  rounds
}

# Leaving the block by `continue` or `return` closes its deadline too.
test test_within_closes_its_deadline_however_the_body_leaves {
  assert first_fit([20ms, 20ms, 20ms])? == 103
  time.sleep(60ms)
  let after = within 30s { 5 }
  assert after == Ok(5)
}

# Work the body hands to workers and producers obeys the deadline too.
test test_within_reaches_parallel_stages_and_stream_pulls {
  let parallel = within 100ms {
    [1, 2, 3, 4] |> par-map { |n|
      time.sleep(30s)
      n
    } |> count()
  }
  assert timed_out(parallel)

  let mapped = within 100ms {
    [1, 2] |> map { |n|
      time.sleep(30s)
      n
    } |> count()
  }
  assert timed_out(mapped)
}

test test_within_reaches_a_producer_it_pulls_from { |ctx|
  let output = test.expect(
    ctx,
    """stream slow_numbers() [time, error] -> Stream[Int] {
  defer { print "producer cleanup" }
  yield 1
  time.sleep(30s)
  yield 2
}

let pulled = within 100ms {
  slow_numbers() |> count()
}
match pulled {
  Ok(count) => print f"ok {count}"
  Err(failure) => print f"timeout {failure is Timeout}"
}
""",
    status: 0,
  )?
  assert output.stdout == "producer cleanup\ntimeout true\n"
}

test test_within_stops_waiting_for_a_spawned_child {
  let started = time.now()
  let result = within 100ms {
    let child = spawn run sleep 30 ?
    let status = wait child?
    status.exit_code() ?? -1
  }
  assert timed_out(result)
  assert time.now() - started < 10000
}

test test_within_as_a_statement_fails_the_script_with_timeout { |ctx|
  let output = test.run_script(
    ctx,
    """defer { print "cleanup" }
within 50ms {
  time.sleep(30s)
}
print "never"
""",
  )?
  assert output.status != 0
  assert output.stdout == "cleanup\n"
  assert "did not finish within 50ms" in output.stderr
  assert ":2:" in output.stderr
}

test test_within_is_contextual_and_checked { |ctx|
  let within = 2
  assert within + 1 == 3
  let limit = 30s
  let limits = {short: limit}
  assert within limits.short { 1 } == Ok(1)

  let checks = [
    {
      source: "within 5 { print \"x\" }\n",
      wants: "within",
    },
    {
      source: "let limit = \"5s\"\nwithin limit { print \"x\" }\n",
      wants: "requires a Duration",
    },
    {
      source: "proc quick() [error] {\n  within 5s { print \"x\" }\n}\n",
      wants: "time",
    },
    {
      source: "pure quick() -> Int {\n  let _ = within 5s { 1 }\n  1\n}\n",
      wants: "pure",
    },
    {
      source: "stream numbers() [time] -> Stream[Int] {\n  within 5s {\n    yield 1\n  }\n}\n",
      wants: "`yield` is not allowed inside a `within` block",
    },
  ]
  for check in checks {
    let output = test.run_script(ctx, check.source)?
    assert output.status != 0, check.source
    assert check.wants in output.stderr, f"{check.source}: {output.stderr}"
    assert output.stdout == "", check.source
  }
}

test test_within_formats_stably { |ctx|
  let source = """let limits = {short: 5s}
let slow = within   5s{
  time.sleep(1ms)
  1
}
within limits.short   {
  print "x"
}?
"""
  let candidate = test.temp_file(ctx, name: "within.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == """let limits = {short: 5s}
let slow = within 5s {
  time.sleep(1ms)
  1
}
within limits.short {
  print "x"
}?
"""
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stderr
}

# `process.run` reports a child that was stopped as an `Err` value instead of
# failing. The body that handles that value still cannot go on past the
# deadline: the timeout is delivered at the next statement.
test test_within_stops_a_body_that_handles_an_interrupted_process_run {
  let plan = process.command_argv("sleep", ["sleep", "30"])
  var went_on = false
  let started = time.now()
  let handled = within 200ms {
    let outcome = if let Ok(status) = process.run(plan) {
      status.exit_code() ?? -1
    } else {
      -2
    }
    went_on = true
    outcome
  }
  assert timed_out(handled)
  assert ! went_on
  assert time.now() - started < 10000
}

# A mocked request answers at once, so this shows only that a network call
# and its value pass through the scope. It does not show a wait being
# stopped; the test below and the fixture test in `stdlib/net.xsh` do.
test test_within_passes_a_mocked_network_call_through { |ctx|
  let response = {
    status: 200,
    reason: "OK",
    bytes: 2,
    headers: [{name: "content-type", value: "text/plain"}],
    url: "https://example.test/",
    body: b"ok",
  }
  test.mock(ctx, "net.request", {url: "https://example.test/"}, Ok(response))
  let status = within 30s {
    net.request({method: "GET", url: "https://example.test/"})?.status
  }
  assert status == Ok(200)
}

# A server that accepts the connection and never answers holds a real
# request open. The deadline stops the wait, whether the body propagates the
# interrupted request's error or handles it and tries to go on.
test test_within_stops_a_network_wait_on_a_silent_server {
  let probe = run.capture --text --accept=[0, 1, 2, 127] sh -c "nc --help 2>&1" ?
  if "BusyBox" not in probe.stdout + probe.stderr {
    test.skip("needs BusyBox nc to hold a connection open")
    return
  }

  # One listener per request: the listener leaves with its first connection.
  let port = 20000 + process.current_pid()? % 20000
  let listen = f"sleep 30 | nc -l -p {port} > /dev/null 2>&1"
  let first = spawn run sh -c $listen ?
  defer first.cancel()
  let listen_again = f"sleep 30 | nc -l -p {port + 1} > /dev/null 2>&1"
  let second = spawn run sh -c $listen_again ?
  defer second.cancel()
  time.sleep(300ms)

  let started = time.now()
  let propagated = within 200ms {
    net.request({method: "GET", url: f"http://127.0.0.1:{port}/"})?.status
  }
  assert timed_out(propagated)

  var went_on = false
  let handled = within 200ms {
    let status = if let Ok(response) = net.request({method: "GET", url: f"http://127.0.0.1:{port + 1}/"}) {
      response.status
    } else {
      -1
    }
    went_on = true
    status
  }
  assert timed_out(handled)
  assert ! went_on
  assert time.now() - started < 10000
}
