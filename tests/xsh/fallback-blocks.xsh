type FallbackRecord = {message: Str}

error FallbackError = invalid(message: Str)

pure fallback_outcome(success: Bool) -> Result[Str] {
  return Ok("loaded") when success
  Err(FallbackError.invalid(message: "invalid config"))
}

pure fallback_nominal_message(failure: FallbackError) -> Str {
  failure.message
}

test test_error_fallback_is_lazy_and_binds_exact_error {
  var calls = 0
  let loaded = fallback_outcome(true) ?? { |failure|
    calls += 1
    failure.message
  }
  assert loaded == "loaded"
  assert calls == 0
  let recovered = fallback_outcome(false) ?? { |failure|
    calls += 1
    failure.message
  }
  assert recovered == "invalid config"
  assert calls == 1
  let exact: Result[Str, FallbackError] = Err(FallbackError.invalid(message: "nominal"))
  let message = exact ?? { |failure|
    fallback_nominal_message(failure)
  }
  assert message == "nominal"
}

test test_error_fallback_evaluates_result_once_before_handler { |ctx|
  let output = test.run_script(
    ctx,
    r"""error LoadError = Missing(message: Str)
proc load(found: Bool) [io] -> Result[Str, LoadError] {
  print f"load {found}"
  return Ok("loaded") when found
  return Err(LoadError.Missing(message: "missing"))
}
let loaded = load(true) ?? { |_| print "unexpected"; "fallback" }
let recovered = load(false) ?? { |failure| print f"handler {failure.message}"; "fallback" }
print $loaded $recovered
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """load true
load false
handler missing
loaded fallback
"""
}

test test_error_fallback_boolean_tail_is_a_value {
  let outcome: Result[Bool, FallbackError] = Err(FallbackError.invalid(message: "invalid"))
  let recovered = outcome ?? { |_|
    let answer = false
    answer
  }
  assert ! recovered
}

test test_error_fallback_keeps_record_expression {
  let outcome: Result[FallbackRecord, FallbackError] = Err(FallbackError.invalid(message: "invalid"))
  let recovered = outcome ?? {message: "record"}
  assert recovered == {message: "record"}
}

test test_error_fallback_literal_error_and_right_associativity {
  let direct = Err(FallbackError.invalid(message: "direct")) ?? { |failure|
    failure.message
  }
  assert direct.trim() == "direct"
  let first = Ok("first")
  let second: Result[Str] = Err(FallbackError.invalid(message: "second"))
  var reached = 0
  let skipped = first ?? second ?? { |failure|
    reached += 1
    failure.message
  }
  assert skipped == "first"
  assert reached == 0
  let handled = fallback_outcome(false) ?? second ?? { |failure|
    reached += 1
    failure.message
  }
  assert handled == "second"
  assert reached == 1
}

test test_error_fallback_cleanup_and_lexical_return { |ctx|
  let output = test.run_script(
    ctx,
    r"""error FallbackError = invalid(message: Str)
proc mark(message: Str) [] { print $message }
proc value() [error] -> Int { mark("value"); 7 }
proc recover() [error] -> Int {
  let failed: Result[Int] = Err(FallbackError.invalid(message: "failed"))
  let selected = failed ?? { |failure|
    defer mark("cleanup")
    mark(failure.message)
    value()
  }
  mark("after")
  selected
}
proc escape() [error] -> Int {
  let failed: Result[Int] = Err(FallbackError.invalid(message: "failed"))
  let selected = failed ?? { |_|
    defer mark("return cleanup")
    return 9
  }
  selected
}
print ${recover()} ${escape()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """failed
value
cleanup
after
return cleanup
7 9
"""
}

test test_error_fallback_keeps_enclosing_loop_targets {
  var visited = 0
  for number in [1, 2, 3] {
    let outcome: Result[Int] = Err(FallbackError.invalid(message: "failed"))
    let selected = outcome ?? { |_|
      continue when number == 2
      number
    }
    visited += selected
  }

  assert visited == 4
  visited = 0
  for number in [1, 2, 3] {
    let outcome: Result[Int] = Err(FallbackError.invalid(message: "failed"))
    let selected = outcome ?? { |_|
      break when number == 2
      number
    }
    visited += selected
  }

  assert visited == 1
}

test test_error_fallback_failure_propagates_to_retry_attempt { |ctx|
  let output = test.run_script(
    ctx,
    r"""error FallbackError = invalid(message: Str)
proc main() [time, error] {
var attempts = 0
let recovered = retry [0ms] {
  attempts += 1
  let failed: Result[Str] = Err(FallbackError.invalid(message: "primary"))
  failed ?? { |_|
    if attempts == 1 { Err(FallbackError.invalid(message: "handler"))? }
    "recovered"
  }
}?
print $recovered $attempts
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """recovered 2
"""
}

test test_error_fallback_rejects_invalid_contexts { |ctx|
  for {source, code} in [
    {
      source: """let absent: Str? = null
let value = absent ?? { |_| "no" }
""",
      code: "check.fallback-block-result",
    },
    {
      source: """let value = Ok(1) ?? { |a, b| 2 }
""",
      code: "check.fallback-block-params",
    },
    {
      source: """let value = Ok(1) ?? { || 2 }
""",
      code: "parse.fallback-block-params",
    },
    {
      source: """let value = Ok(1) ?? { |_| "wrong" }
""",
      code: "check.type-mismatch",
    },
    {
      source: """let value = Ok() ?? { |_| false }
""",
      code: "check.type-mismatch",
    },
    {
      source: """let value = Ok(1) ?? { |failure| failure = "wrong"; 2 }
""",
      code: "check.assign-let",
    },
    {
      source: """let value = Ok(1) ?? { |failure| 2 }
let escaped = failure
""",
      code: "check.unresolved-name",
    },
    {
      source: """let value = { |failure| 2 }
""",
      code: "check.fallback-block-context",
    },
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success
    assert code in output.stderr
  }
}

test test_error_fallback_handler_failure_retains_its_error { |ctx|
  let output = test.run_script(
    ctx,
    r"""error FallbackError = source(message: Str) | handler(message: Str)
let failed: Result[Str] = Err(FallbackError.source(message: "primary"))
let value = failed ?? { |_|
  Err(FallbackError.handler(message: "handler failed"))?
  "unreachable"
}
print $value
""",
  )?
  assert output.status == 3
  assert "FallbackError.handler" in output.stderr
  assert "handler failed" in output.stderr
  assert output.stdout == ""
}
