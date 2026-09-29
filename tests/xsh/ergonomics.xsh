pure ergonomics_result_flag(outcome: Result[Int]) -> Bool {
  match outcome {
    Ok(_) => {
      let accepted = false
      accepted
    }
    Err(_) => true
  }
}

pure ergonomics_guarded_name(name: Str?) -> Str {
  return name.trim() when name != null
  "default"
}

pure ergonomics_conditional_name(name: Str?) -> Str {
  if name != null {
    let label = name.trim()
    label
  } else {
    "default"
  }
}

pure ergonomics_pattern_name(value: Any) -> Str {
  return value when value is Str
  "missing"
}

test test_ergonomics_boolean_branch_tails_and_pattern_values [error] {
  test.eq(ergonomics_result_flag(Ok(1)), false)?
  let outcome: Result[Int] = Ok(1)
  let accepted = outcome is Ok(_)
  test.eq(accepted, true)?
  let filtered = [1, 2, 3] |> where { |value|
    if value == 2 {
      let keep = false
      keep
    } else {
      true
    }
  } |> collect()
  test.eq(filtered, [1, 3])?
}

test test_ergonomics_guarded_return_narrows_nullable_payload [error] {
  test.eq(ergonomics_guarded_name(null), "default")?
  test.eq(ergonomics_guarded_name("  configured  "), "configured")?
  test.eq(ergonomics_conditional_name(null), "default")?
  test.eq(ergonomics_conditional_name("  configured  "), "configured")?
  test.eq(ergonomics_pattern_name("configured"), "configured")?
  test.eq(ergonomics_pattern_name(42), "missing")?
}

test test_ergonomics_retry_branch_false_is_a_value [error] {
  let value = retry [] {
    if true {
      let outcome: Result[Bool] = Ok(false)
      outcome?
    } else {
      true
    }
  }?
  test.eq(value, false)?
}

test test_ergonomics_optional_method_retains_result_layer [error] {
  let absent: Str? = null
  let present: Str? = "42"
  let absent_result = absent?.parse_int()
  let present_result = present?.parse_int()
  test.eq((absent_result ?? Ok(0))?, 0)?
  test.eq((present_result ?? Ok(0))?, 42)?
}

test test_ergonomics_nullable_bytes_slice_and_fallback [error] {
  let absent: Bytes? = null
  let present: Bytes? = b"abcdef"
  test.eq(absent?[1..4] ?? b"fallback", b"fallback")?
  test.eq(present?[1..4] ?? b"fallback", b"bcd")?
  test.eq(present?[..2], b"ab")?
  test.eq(present?[4..], b"ef")?
}

test test_ergonomics_renamed_targets_in_filtered_nested_comprehensions [error] {
  let packages = [
    {name: "first", build: {jobs: [1, 2]}},
    {name: "second", build: {jobs: [3, 4]}},
  ]
  let selected = [
    f"${label}:${job}"
    for {name: label, build: {jobs, ..}, ..} in packages
    if label != "first"
    for job in jobs
    if 2 < job <= 4
  ]
  test.eq(selected, ["second:3", "second:4"])?
  var combined: List[Str] = []
  let previous = combined
  combined += selected
  combined += ["last"]
  test.eq(previous, [])?
  test.eq(combined, ["second:3", "second:4", "last"])?
}

test test_ergonomics_optional_call_skips_punned_argument_evaluation [error] { |ctx|
  let output = test.run_script(
    ctx,
    """var calls = 0
proc separator_value() [] -> Str {
  calls += 1
  return ","
}
let absent: List[Str]? = null
let separator = separator_value()
let skipped = absent?.join(separator:)
let skipped_effect = absent?.join(separator_value())
print $calls
print (skipped ?? "absent")
print (skipped_effect ?? "absent")
""",
  )?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "1\nabsent\nabsent\n")?
}

test test_ergonomics_failed_chain_reports_only_reached_operands [error] { |ctx|
  let output = test.run_script(
    ctx,
    """proc skipped() [] -> Int {
  print "unexpected"
  return 9
}
3 < 2 < skipped()
""",
  )?
  test.ok(! output.success, output.stderr)?
  test.eq(output.stdout, "")?
  test.contains(output.stderr, "3")?
  test.contains(output.stderr, "2")?
}

test test_ergonomics_statement_branch_false_still_asserts [error] { |ctx|
  let output = test.run_script(
    ctx,
    """proc check_branch() [] {
  if true {
    false
  } else {
    true
  }
}
check_branch()?
""",
  )?
  test.ok(! output.success, output.stderr)?
  test.contains(output.stderr, "assert")?
}
