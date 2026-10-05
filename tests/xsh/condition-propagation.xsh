# Each script asks `known` in conditions; its trace shows which operands ran.
const known_source = r"""proc known(name: Str) -> Result[Bool] {
  print f"known {name}"
  fail f"cannot stat {name}" when name == "bad"
  Ok(name == "yes")
}

"""

test test_a_result_bool_condition_yields_its_bool_and_propagates_its_error { |ctx|
  let source = known_source + r"""proc probe(name: Str) -> Result[Str] {
  if known(name) {
    return Ok("present")
  } else if ! known(name) and name != "never" {
    return Ok("absent")
  }

  Ok("unreachable")
}

for name in ["yes", "no", "bad"] {
  match probe(name) {
    Ok(text) => print $text
    Err(problem) => print f"error: {problem.message}"
  }
}
"""
  let output = test.expect(ctx, source, status: 0)?
  # The failed condition ended `probe`: its `else if` never ran.
  assert output.stdout == """known yes
present
known no
known no
absent
known bad
error: cannot stat bad
"""
}

test test_every_control_position_of_a_condition_propagates { |ctx|
  let source = known_source + r"""proc every_position(left: Str, right: Str) -> Result[Str] {
  let label = if known(left) or known(right) { "some" } else { "none" }
  var rounds = 0
  while known(left) and rounds < 2 {
    rounds += 1
  }

  guard ! known(right) else {
    return Ok(f"{label} guard {rounds}")
  }

  return Ok(f"{label} when {rounds}") when known(left)
  return Ok(f"{label} unless {rounds}") unless ! (known(left) or known(left))
  Ok(f"{label} {rounds}")
}

print every_position("no", "yes")?
print every_position("yes", "no")?
print every_position("no", "no")?
print every_position("no", "bad")?
"""
  let output = test.expect(ctx, source, status: 3)?
  # `or` skips its right operand after `yes`, and `while` asks three times.
  assert output.stdout == """known no
known yes
known no
known yes
some guard 0
known yes
known yes
known yes
known yes
known no
known yes
some when 2
known no
known no
known no
known no
known no
known no
known no
none 0
known no
known bad
"""
  assert "cannot stat bad" in output.stderr, output.stderr
}

test test_a_skipped_operand_does_not_fail_and_try_captures_one_that_does { |ctx|
  let source = known_source + r"""let skipped = if known("yes") or known("bad") { "kept" } else { "lost" }
let also = if known("no") and known("bad") { "lost" } else { "kept" }
print $skipped $also
let outcome = try {
  if known("bad") { "yes" } else { "no" }
}
match outcome {
  Ok(text) => print $text
  Err(problem) => print f"captured: {problem.message}"
}
"""
  let output = test.expect(ctx, source, status: 0)?
  assert output.stdout == """known yes
known no
kept kept
known bad
captured: cannot stat bad
"""
}

test test_a_failed_condition_unwinds_like_a_statement { |ctx|
  let source = "proc known(name: Str) -> Result[Bool] {\n  fail f\"cannot stat {name}\"\n}\n\nproc report(name: Str) -> Int {\n  defer { print \"cleanup\" }\n  if known(name) {\n    return 1\n  }\n\n  0\n}\n\nprint f\"{report(\"x\")}\"\n"
  let output = test.expect(ctx, source, status: 3)?
  assert output.stdout == "cleanup\n", output.stdout
  assert "cannot stat x" in output.stderr, output.stderr
}

test test_a_result_elsewhere_in_a_condition_is_data { |ctx|
  let prelude = "pure known(name: Str) -> Result[Bool] {\n  Ok(name != \"\")\n}\n\npure show(flag: Bool) -> Bool {\n  flag\n}\n\n"
  for body in [
    "if known(name) == true { return Ok(1) }",
    "if show(known(name)) { return Ok(1) }",
    "let flag = ! known(name)",
    "let both = known(name) and true",
    "assert known(name)",
    "match name {\n    \"x\" if known(name) => return Ok(1)\n    else => return Ok(2)\n  }",
  ] {
    let source = prelude + "pure pick(name: Str) -> Result[Int] {\n  " + body + "\n  Ok(0)\n}\n"
    let output = test.run_script(ctx, source)?
    assert output.status == 2, body
    assert "found Result[Bool, Error]" in output.stderr or "expected Result[Bool, Error]" in output.stderr, output.stderr
  }
}

test test_a_condition_cannot_propagate_where_a_statement_cannot { |ctx|
  let prelude = "pure known(name: Str) -> Result[Bool] {\n  Ok(name != \"\")\n}\n\n"
  let _ = test.expect(
    ctx,
    prelude + "pure label(name: Str) -> Str {\n  if known(name) { return name }\n  \"anonymous\"\n}\n",
    status: 2,
    stderr: [
      "err[check.try-context]: a `Result[Bool]` condition propagates its failure, which requires a Result-returning context",
    ],
  )?
  let _ = test.expect(
    ctx,
    prelude + "proc label(name: Str) [io] -> Str {\n  return name when known(name)\n  \"anonymous\"\n}\n",
    status: 2,
    stderr: ["err[check.effect-violation]: condition failure propagation requires the `error` effect"],
  )?
  let _ = test.expect(ctx, "if \"4\".parse_int() { print \"yes\" }\n", status: 2, stderr: ["err[check.if-condition]"])?
}

test test_redundant_propagation_removes_the_condition_question_mark { |ctx|
  let source = r"""proc known(name: Str) -> Result[Bool] {
  Ok(name == "yes")
}

proc count(text: Str) -> Result[Int] {
  text.parse_int()
}

proc pick(name: Str) -> Result[Str] {
  if known(name)? {
    return Ok("first")
  } else if ! known(name)? and count(name)? > 1 {
    return Ok("second")
  }

  while known(name)? or known(name)? {
    break
  }

  return Ok("guarded") unless known(name)?
  Ok(f"{known(name)?}")
}

let picked = pick("yes")?
print $picked
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "pick.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.redundant-propagation $candidate ?
  assert first.status.exited_with(1), first.stderr
  assert first.stderr.split("`?` on a condition that already propagates its failure").len() == 6, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.redundant-propagation $candidate ?
  let fixed = candidate.read_text()?
  assert "  if known(name) {\n" in fixed, fixed
  # An operand of a comparison keeps its `?`.
  assert "  } else if ! known(name) and count(name)? > 1 {\n" in fixed, fixed
  assert "  while known(name) or known(name) {\n" in fixed, fixed
  assert "  return Ok(\"guarded\") unless known(name)\n" in fixed, fixed
  # A value keeps its `?`.
  assert "  Ok(f\"{known(name)?}\")\n" in fixed, fixed
  assert fixing.status.exited_with(0), fixing.stderr
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
  let formatted = run.capture --text "xsht" fmt --check $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
}
