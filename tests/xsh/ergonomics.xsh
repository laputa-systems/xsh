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

test test_ergonomics_boolean_branch_tails_and_pattern_values {
  assert !ergonomics_result_flag(Ok(1))
  let outcome = Ok(1)
  let accepted = outcome is Ok(_)
  assert accepted, "Ok pattern recognizes the successful outcome"
  let filtered = [1, 2, 3] |> where { |value|
    if value == 2 {
      let keep = false
      keep
    } else {
      true
    }
  } |> collect()
  assert filtered == [1, 3]
}

test test_ergonomics_guarded_return_narrows_nullable_payload {
  assert ergonomics_guarded_name(null) == "default"
  assert ergonomics_guarded_name("  configured  ") == "configured"
  assert ergonomics_conditional_name(null) == "default"
  assert ergonomics_conditional_name("  configured  ") == "configured"
  assert ergonomics_pattern_name("configured") == "configured"
  assert ergonomics_pattern_name(42) == "missing"
}

test test_ergonomics_retry_branch_false_is_a_value {
  let value = retry [] {
    if true {
      let outcome = Ok(false)
      outcome?
    } else {
      true
    }
  }?
  assert !value
}

test test_ergonomics_optional_method_retains_result_layer {
  let absent: Str? = null
  let present: Str? = "42"
  let absent_result = absent?.parse_int()
  let present_result = present?.parse_int()
  assert (absent_result ?? Ok(0))? == 0
  assert (present_result ?? Ok(0))? == 42
}

test test_ergonomics_nullable_bytes_slice_and_fallback {
  let absent: Bytes? = null
  let present: Bytes? = b"abcdef"
  assert (absent?[1..4] ?? b"fallback") == b"fallback"
  assert (present?[1..4] ?? b"fallback") == b"bcd"
  assert present?[..2] == b"ab"
  assert present?[4..] == b"ef"
}

test test_ergonomics_renamed_targets_in_filtered_nested_comprehensions {
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
  assert selected == ["second:3", "second:4"]
  var combined = []
  let previous = combined
  combined += selected
  combined += ["last"]
  assert previous.len() == 0
  assert combined == ["second:3", "second:4", "last"]
}

test test_ergonomics_optional_call_skips_punned_argument_evaluation { |ctx|
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
  assert output.success, output.stderr
  assert output.stdout == "1\nabsent\nabsent\n"
}

test test_ergonomics_failed_chain_reports_only_reached_operands { |ctx|
  let output = test.run_script(
    ctx,
    """proc skipped() [] -> Int {
  print "unexpected"
  return 9
}
3 < 2 < skipped()
""",
  )?
  assert ! output.success, output.stderr
  assert output.stdout == ""
  assert "3" in output.stderr
  assert "2" in output.stderr
}

test test_ergonomics_statement_branch_false_still_asserts { |ctx|
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
  assert ! output.success, output.stderr
  assert "assert" in output.stderr
}

test test_ergonomics_list_concatenation_does_not_merge_maps { |ctx|
  for source in [
    "let table: Map[Int] = {x: 1}\nlet merged = table + table\n",
    "var table: Map[Int] = {x: 1}\ntable += {y: 2}\n",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert "check.type-mismatch" in output.stderr
  }
}
