pure plain(text: Str) -> Result[Str] {
  fail "no text" when text == ""
  Ok(text)
}

pure optional(text: Str) -> Str? {
  if text == "" { null } else { text }
}

pure both(text: Str) -> Result[Str?] {
  fail "bad text" when text == "bad"
  Ok(if text == "" { null } else { text })
}

pure after_result(text: Str) -> Result[Str] {
  # `?.` on a `Result` propagates and then calls: the value is not optional.
  let upper = plain(text)?.upper()
  Ok(upper)
}

test test_guarded_hop_on_a_result_propagates_and_then_accesses {
  assert after_result("ab")? == "AB"
  match after_result("") {
    Ok(value) => test.fail(f"propagation yielded {value}")
    Err(problem) => assert problem.message == "no text"
  }

  # It is the grouped spelling.
  assert (plain("cd")?).upper() == plain("cd")?.upper()
}

test test_guarded_hop_on_an_optional_guards_and_never_fails {
  let present = optional("ab")?.upper()
  let absent = optional("")?.upper()
  assert present == "AB"
  assert absent == null
}

pure after_both(text: Str) -> Result[Str?] {
  # One `?.` is one hop: the `Result` first, then the optional it held.
  let upper = (both(text)?)?.upper()
  Ok(upper)
}

test test_a_result_of_an_optional_takes_one_hop_for_each_layer { |ctx|
  assert after_both("ab")? == "AB"
  assert after_both("")? == null
  match after_both("bad") {
    Ok(_) => test.fail("propagation yielded a value")
    Err(problem) => assert problem.message == "bad text"
  }

  let source = "pure both(text: Str) -> Result[Str?] {\n  Ok(text)\n}\n\npure upper(text: Str) -> Result[Str?] {\n  Ok(both(text)?.upper())\n}\n"
  let output = test.run_script(ctx, source)?
  assert output.status == 2
  assert "err[check.optional-method]: Result propagation leaves an Optional receiver; guard the next hop explicitly" in output.stderr, output.stderr
}

proc trimmed(script: Str) -> Result[Str] {
  let text = (run.text sh -c $script)?.trim()
  Ok(text)
}

proc counted(script: Str) -> Result[Int] {
  let words = ["sh", "-c", script]
  var count = 0
  # In a head, `?.` after the last word belongs to the whole run form.
  for line in run.text @words?.lines() {
    count += line.byte_len()
  }

  Ok(count)
}

test test_guarded_hop_on_a_run_form_propagates_its_failure {
  assert trimmed("echo ' hi '")? == "hi"
  assert trimmed("exit 3") is Err(_)
  assert counted("echo ab; echo c")? == 3
  assert counted("exit 3") is Err(_)
}

# Where a run form is an initializer its words are read first, so a `?.`
# there belongs to the last word and not to the form.
test test_a_guarded_hop_after_an_initializer_run_form_belongs_to_its_last_word { |ctx|
  let word = test.run_script(ctx, "let text = run.text sh -c \"echo hi\"?.trim()\n")?
  assert word.status == 2
  assert "err[check.null-safe-field]: `?.` requires an Optional or Result value" in word.stderr, word.stderr
  let splice = test.run_script(ctx, "let words = [\"sh\"]\nlet text = run.text @words?.trim()\n")?
  assert splice.status == 2
  assert "err[parse.expected-terminator]" in splice.stderr, splice.stderr
}

test test_a_propagated_run_form_is_not_reported_as_captured { |ctx|
  let source = r"""proc lines(script: Str) -> Result[Int] {
  let words = ["sh", "-c", script]
  var count = 0
  for line in run.text @words?.lines() {
    count += line.byte_len()
  }

  let listed = [line for line in run.text sh -c $script?.lines()]
  let first = (run.text sh -c $script)?[0..1]
  let kept = run.text sh -c $script
  Ok(count + listed.len() + first.byte_len() + (kept ?? "").byte_len())
}

let total = lines("echo ab")?
print $total
"""
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "lines.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.explicit-run-capture $candidate ?
  # Only the form that is bound keeps its `Result`.
  assert first.stderr.split("warn[lint.explicit-run-capture]").len() == 2, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.explicit-run-capture $candidate ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "  let kept = try run.text sh -c $script\n" in fixed, fixed
  assert "  for line in run.text @words?.lines() {\n" in fixed, fixed
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
}
