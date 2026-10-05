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
  let _ = test.expect(
    ctx,
    source,
    status: 2,
    stderr: ["err[check.optional-method]: Result propagation leaves an Optional receiver; guard the next hop explicitly"],
  )?
}

type Entry = {kind: Str, size: Int}

pure entry(kind: Str) -> Result[Entry] {
  fail "no entry" when kind == ""
  Ok(Entry(kind:, size: kind.byte_len()))
}

pure optional_entry(kind: Str) -> Entry? {
  if kind == "" { null } else { Entry(kind:, size: kind.byte_len()) }
}

pure entry_of_both(kind: Str) -> Result[Entry?] {
  fail "bad entry" when kind == "bad"
  Ok(optional_entry(kind))
}

pure names(kind: Str) -> Result[List[Str]] {
  fail "no names" when kind == ""
  Ok([kind, "last"])
}

pure optional_names(kind: Str) -> List[Str]? {
  if kind == "" { null } else { [kind, "last"] }
}

pure names_of_both(kind: Str) -> Result[List[Str]?] {
  fail "bad names" when kind == "bad"
  Ok(optional_names(kind))
}

pure kind_after_result(kind: Str) -> Result[Str] {
  # A field after a call's `?` is the propagation and then the field.
  let found = entry(kind)?.kind
  Ok(found)
}

pure first_after_result(kind: Str) -> Result[Str] {
  let found = names(kind)?[0]
  Ok(found)
}

pure kind_after_both(kind: Str) -> Result[Str?] {
  let found = (entry_of_both(kind)?)?.kind
  Ok(found)
}

pure first_after_both(kind: Str) -> Result[Str?] {
  let found = (names_of_both(kind)?)?[0]
  Ok(found)
}

test test_a_field_or_an_index_after_a_guarded_hop_on_a_result_is_not_optional {
  assert kind_after_result("dir")? == "dir"
  assert first_after_result("dir")? == "dir"
  match kind_after_result("") {
    Ok(value) => test.fail(f"propagation yielded {value}")
    Err(problem) => assert problem.message == "no entry"
  }

  match first_after_result("") {
    Ok(value) => test.fail(f"propagation yielded {value}")
    Err(problem) => assert problem.message == "no names"
  }

  # It is the grouped spelling, and the value takes part in an expression
  # as a present one.
  assert (entry("ab")?).kind == entry("ab")?.kind
  assert entry("ab")?.size + 1 == 3
  assert names("ab")?[-1] == "last"
  assert names("ab")?[0..1] == ["ab"]
}

test test_a_field_or_an_index_after_a_guarded_hop_on_an_optional_is_optional {
  let present = optional_entry("file")?.kind
  let absent = optional_entry("")?.kind
  assert present == "file"
  assert absent == null
  let first = optional_names("file")?[0]
  let none = optional_names("")?[0]
  assert first == "file"
  assert none == null
}

test test_a_field_or_an_index_of_a_result_of_an_optional_takes_two_hops { |ctx|
  assert kind_after_both("file")? == "file"
  assert kind_after_both("")? == null
  assert first_after_both("file")? == "file"
  assert first_after_both("")? == null
  assert kind_after_both("bad") is Err(_)
  assert first_after_both("bad") is Err(_)

  # With one hop the optional the `Result` held is unguarded, whatever the
  # access is.
  let held = "Result propagation leaves an Optional receiver; guard the next hop explicitly"
  let declarations = r"""type Entry = {kind: Str}
pure found(kind: Str) -> Result[Entry?] {
  Ok(Entry(kind:))
}
pure listed(kind: Str) -> Result[List[Str]?] {
  Ok([kind])
}
"""
  let field = "pure kind(kind: Str) -> Result[Str?] {\n  Ok(found(kind)?.kind)\n}\n"
  let _ = test.expect(ctx, declarations + field, status: 2, stderr: [f"err[check.null-safe-field]: {held}"])?
  let index = "pure first(kind: Str) -> Result[Str?] {\n  Ok(listed(kind)?[0])\n}\n"
  let _ = test.expect(ctx, declarations + index, status: 2, stderr: [f"err[check.null-safe-index]: {held}"])?
  let slice = "pure some(kind: Str) -> Result[List[Str]?] {\n  Ok(listed(kind)?[0..1])\n}\n"
  let sliced = test.expect(ctx, declarations + slice, status: 2, stderr: [f"err[check.null-safe-index]: {held}"])?
  # The one error is the whole report.
  assert sliced.stderr.split("err[").len() == 2, sliced.stderr
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
  let _ = test.expect(
    ctx,
    "let text = run.text sh -c \"echo hi\"?.trim()\n",
    status: 2,
    stderr: ["err[check.null-safe-field]: `?.` requires an Optional or Result value"],
  )?
  let _ = test.expect(
    ctx,
    "let words = [\"sh\"]\nlet text = run.text @words?.trim()\n",
    status: 2,
    stderr: ["err[parse.expected-terminator]"],
  )?
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
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "lines.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.explicit-run-capture $candidate ?
  # Only the form that is bound keeps its `Result`.
  assert first.stderr.split("warn[lint.explicit-run-capture]").len() == 2, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.explicit-run-capture $candidate ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "  let kept = try run.text sh -c $script\n" in fixed, fixed
  assert "  for line in run.text @words?.lines() {\n" in fixed, fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
}
