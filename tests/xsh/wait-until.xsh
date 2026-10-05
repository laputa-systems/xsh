test test_wait_until_ends_as_soon_as_the_condition_holds {
  # A condition that already holds is tested once, and nothing sleeps.
  var tests = 0
  let started = time.now()
  wait until {
    tests += 1
    true
  } within 30s every 10s
  assert tests == 1
  assert time.now() - started < 5000

  # Otherwise it is tested again after each interval.
  var polls = 0
  wait until {
    polls += 1
    polls >= 4
  } within 30s every 1ms
  assert polls == 4

  # Without `every`, the interval is 100ms.
  var slow = 0
  let began = time.now()
  wait until {
    slow += 1
    slow >= 3
  } within 30s
  assert slow == 3
  assert time.now() - began >= 200
}

test test_wait_until_fails_with_timeout_when_the_limit_passes {
  var tests = 0
  let started = time.now()
  let waited = try {
    wait until {
      tests += 1
      false
    } within 60ms every 10ms
  }
  assert waited is Err(is Timeout)
  assert tests >= 2
  assert time.now() - started < 10000
  match waited {
    Ok(_) => assert false
    Err(failure) => assert "did not finish within 60ms" in failure.message
  }
}

# The deadline ends the sleep between two tests: the statement does not run
# to the end of its interval, and the condition is not tested again.
test test_wait_until_deadline_interrupts_the_sleep {
  var tests = 0
  let started = time.now()
  let waited = try {
    wait until {
      tests += 1
      false
    } within 50ms every 30s
  }
  assert waited is Err(is Timeout)
  assert tests == 1
  assert time.now() - started < 10000
}

test test_wait_until_backoff_doubles_to_its_cap {
  var stamps = []
  let waited = try {
    wait until {
      stamps += [time.now()]
      stamps.len() >= 5
    } within 30s backoff 20ms..80ms
  }
  assert waited is Ok(_)
  # The sleeps are 20ms, 40ms, 80ms, and 80ms.
  assert stamps[1] - stamps[0] >= 20
  assert stamps[2] - stamps[1] >= 40
  assert stamps[3] - stamps[2] >= 80
  assert stamps[4] - stamps[3] >= 80
  assert stamps[4] - stamps[0] < 10000

  # A first interval above the cap is slept once, as written.
  var tests = 0
  let started = time.now()
  wait until {
    tests += 1
    tests >= 3
  } within 30s backoff 40ms..10ms
  assert time.now() - started >= 50

  let late = try {
    wait until false within 50ms backoff 10ms..20ms
  }
  assert late is Err(is Timeout)
}

proc settled(marker: Path, broken: Bool) [fs] -> Result[Bool] {
  return Err(error.failure("the probe broke")) when broken

  marker.exists()
}

# The condition is a control position: a `Result[Bool]` in it propagates.
test test_wait_until_condition_propagates_a_result { |ctx|
  let marker = test.temp_file(ctx, name: "marker", contents: bytes.from_text(""))?
  wait until settled(marker, false) within 30s every 1ms

  let broken = try {
    wait until settled(marker, true) within 30s every 1ms
  }
  match broken {
    Ok(_) => assert false
    Err(failure) => {
      assert failure.message == "the probe broke"
      assert ! (failure is Timeout)
    }
  }
}

test test_wait_until_reads_names_for_its_durations {
  let limits = {patience: 30s, step: 1ms, first: 1ms, cap: 4ms}
  let patience = 30s
  var polls = 0
  wait until {
    polls += 1
    polls >= 3
  } within limits.patience every limits.step
  wait until {
    polls += 1
    polls >= 6
  } within patience backoff limits.first..limits.cap
  assert polls == 6

  # The words stay names everywhere else.
  let until = 1
  let every = 2
  let backoff = 3
  let within = 4
  assert until + every + backoff + within == 10
}

test test_wait_until_sits_in_loops_and_blocks_that_defer {
  var left = 0
  var rounds = 0
  for round in range(3) {
    defer { left += 1 }
    rounds += 1
    var tests = 0
    wait until {
      tests += 1
      tests >= 2
    } within 30s every 1ms
    break when round == 1
  }

  assert rounds == 2
  assert left == 2
}

test test_wait_until_as_a_statement_fails_the_script_with_timeout { |ctx|
  let output = test.run_script(
    ctx,
    """defer { print "cleanup" }
wait until false within 50ms every 10ms
print "never"
""",
  )?
  assert output.status != 0
  assert output.stdout == "cleanup\n"
  assert "did not finish within 50ms" in output.stderr
  assert ":2:" in output.stderr
}

test test_wait_until_is_checked { |ctx|
  let checks = [
    {
      source: "let waited = wait until true within 5s\n",
      wants: "`wait until` is a statement",
    },
    {
      source: "wait until true\n",
      wants: "expected `within LIMIT`",
    },
    {
      source: "wait until true within 2 * 5s\n",
      wants: "expected a duration literal or a name after `within`",
    },
    {
      source: "wait until true within 5s every\n",
      wants: "expected a duration literal or a name after `every`",
    },
    {
      source: "wait until true within 5s backoff 1s\n",
      wants: "expected `..`",
    },
    {
      source: "wait until true within 5s backoff 1s. .2s\n",
      wants: "expected `..`",
    },
    {
      source: "wait until true within 5s when true\n",
      wants: "terminator",
    },
    {
      source: "wait until 1 within 5s\n",
      wants: "check.if-condition",
    },
    {
      source: "let limit = \"5s\"\nwait until true within limit\n",
      wants: "requires a Duration",
    },
    {
      source: "let step = 3\nwait until true within 5s every step\n",
      wants: "Duration",
    },
    {
      source: "proc settle() [error] {\n  wait until true within 5s\n}\n",
      wants: "time",
    },
    {
      source: "pure settle() -> Int {\n  wait until true within 5s\n  1\n}\n",
      wants: "pure",
    },
    {
      source: "stream numbers() [time] -> Stream[Int] {\n  wait until {\n    yield 1\n    true\n  } within 5s\n}\n",
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

test test_wait_until_formats_and_desugars { |ctx|
  let source = """let step = 5ms
wait   until  step  >  1ms   within   5s
wait until true within 5s   every   step
wait until true within 5s backoff 1ms  ..  step
"""
  let candidate = test.temp_file(ctx, name: "wait.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == """let step = 5ms
wait until step > 1ms within 5s
wait until true within 5s every step
wait until true within 5s backoff 1ms..step
"""
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr

  let desugared = run.capture --text "xsht" desugar $candidate
  assert desugared.status.exited_with(0), desugared.stderr
  assert "within 5s {" in desugared.stdout
  assert "if step > 1ms { break }" in desugared.stdout
  assert "let delay_1: Duration = 100ms" in desugared.stdout
  assert "var delay_1: Duration = 1ms" in desugared.stdout
  assert "wait until" not in desugared.stdout
}

test test_retry_backoff_doubles_its_delays_to_the_cap {
  var stamps = []
  let value = retry backoff 20ms..40ms within 30s {
    stamps += [time.now()]
    assert stamps.len() >= 4
    stamps.len()
  }
  assert value == Ok(4)
  # The delays are 20ms, 40ms, and 40ms.
  assert stamps[1] - stamps[0] >= 20
  assert stamps[2] - stamps[1] >= 40
  assert stamps[3] - stamps[2] >= 40
  assert stamps[3] - stamps[0] < 10000
}

# The limit bounds when an attempt may start: the last failure is the result,
# and it is not a timeout.
test test_retry_backoff_stops_at_its_limit_with_the_last_error {
  var attempts = 0
  let started = time.now()
  let result: Result[Int] = retry backoff 10ms..10ms within 45ms {
    attempts += 1
    Err(error.failure(f"attempt {attempts}"))?
  }
  # Attempts start at 0, 10, 20, 30, and 40 ms, and later when the machine
  # is busy; the delay after the last would end past the limit.
  assert attempts >= 2 and attempts <= 5
  assert time.now() - started < 10000
  match result {
    Ok(_) => assert false
    Err(failure) => {
      assert failure.message == f"attempt {attempts}"
      assert ! (failure is Timeout)
    }
  }

  # A limit below the first interval allows one attempt.
  var single = 0
  let once: Result[Int] = retry backoff 1s..5s within 10ms {
    single += 1
    Err(error.failure("no"))?
  }
  assert once is Err(_)
  assert single == 1
}

test test_retry_backoff_selects_errors_and_reads_names {
  let pace = {first: 1ms, cap: 2ms}
  let limit = 30s
  var attempts = 0
  let result: Result[Int] = retry backoff pace.first..pace.cap within limit on (is Timeout) {
    attempts += 1
    Err(error.failure("not a timeout"))?
  }
  assert result is Err(_)
  assert attempts == 1

  var later = 0
  let value = retry backoff pace.first..pace.cap within limit {
    later += 1
    assert later >= 3
    later
  }
  assert value == Ok(3)
}

test test_retry_backoff_is_checked_and_formats { |ctx|
  let checks = [
    {
      source: "let r = retry backoff 1s..5s { 1 }\n",
      wants: "expected `within LIMIT`",
    },
    {
      source: "let r = retry backoff 1s within 5s { 1 }\n",
      wants: "expected `..`",
    },
    {
      source: "let r = retry backoff 1..5s within 5s { 1 }\n",
      wants: "expected a duration literal or a name after `backoff`",
    },
    {
      source: "let first = 1\nlet r = retry backoff first..5s within 5s { 1 }\n",
      wants: "Duration",
    },
    {
      source: "proc again() [error] -> Result[Int] {\n  retry backoff 1s..5s within 5s { 1 }\n}\n",
      wants: "time",
    },
    {
      source: "let r = retry { 1 }\n",
      wants: "expected `[` or `backoff` after `retry`",
    },
  ]
  for check in checks {
    let output = test.run_script(ctx, check.source)?
    assert output.status != 0, check.source
    assert check.wants in output.stderr, f"{check.source}: {output.stderr}"
    assert output.stdout == "", check.source
  }

  let source = """let cap = 5ms
let value = retry   backoff  1ms .. cap   within 5s   on (is Timeout){
  1
}
"""
  let candidate = test.temp_file(ctx, name: "retry.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == """let cap = 5ms
let value = retry backoff 1ms..cap within 5s on (is Timeout) {
  1
}
"""
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
}

test test_retry_backoff_traces_the_attempts_its_durations_allow { |ctx|
  let traced = test.run_xsht_trace(
    ctx,
    """let value = retry backoff 10ms..20ms within 100ms {
  1
}
""",
    ["--raw", "--trace-format", "jsonl"],
  )?
  assert traced.success, traced.stderr
  # Delays of 10, 20, 20, 20, and 20 ms end within 100ms: six attempts.
  assert "\"kind\":\"retry.attempt\"" in traced.stderr
  assert "\"max_attempts\":6" in traced.stderr
}
