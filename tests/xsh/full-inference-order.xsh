test test_repeated_optional_guards_keep_their_payload_views { |ctx|
  let output = test.run_xsh(ctx, r"""pure consume(value: Path) -> Str { value.display() }
pure selected(value) -> Str {
  let checked: Path? = if let item = value { item } else { null }
  if let item = value { consume(item) } else { "none" }
}
pure forwarded(value) -> Str { selected(value) }
pure guarded(value) -> Str {
  guard let item = value else { return "none" }
  consume(item)
}
let possible: Path? = p"/"
print selected(p"/")
print selected(null)
print forwarded(possible)
print forwarded(null)
print guarded(possible)
print guarded(null)
""")?
  assert output.success, output.stderr
  assert output.stdout == "/\nnone\n/\nnone\n/\nnone\n", output.stdout
}

test test_optional_destinations_do_not_widen_required_inputs { |ctx|
  let output = test.run_xsh(ctx, r"""pure consume(value: Str) -> Int { value.byte_len() }
pure optional_first(value, choice: Bool) -> Int {
  if choice { let output: Str? = value; 0 } else { consume(value) }
}
pure required_first(value, choice: Bool) -> Int {
  if choice { consume(value) } else { let output: Str? = value; 0 }
}
pure forwarded(value, choice: Bool) -> Int { optional_first(value, choice) }
print optional_first("abc", true)
print optional_first("abc", false)
print required_first("abc", true)
print required_first("abc", false)
print forwarded("hello", false)
""")?
  assert output.success, output.stderr
  assert output.stdout == "0\n3\n3\n0\n5\n", output.stdout
}

test test_repeated_optional_guards_reject_a_wrong_payload { |ctx|
  let output = test.run_xsh(ctx, r"""pure consume(value: Path) -> Str { value.display() }
pure selected(value) -> Str {
  let checked: Path? = if let item = value { item } else { null }
  if let item = value { consume(item) } else { "none" }
}
print selected(7)
""")?
  assert ! output.success, output.stdout
  assert "[check." in output.stderr, output.stderr
}

test test_required_inputs_reject_nullable_callers_after_optional_destinations { |ctx|
  let output = test.run_xsh(ctx, r"""pure consume(value: Str) -> Int { value.byte_len() }
pure selected(value, choice: Bool) -> Int {
  if choice { let output: Str? = value; 0 } else { consume(value) }
}
let possible: Str? = null
print selected(possible, false)
""")?
  assert ! output.success, output.stdout
  assert "[check." in output.stderr, output.stderr
}

test test_guard_local_optional_assignments_keep_their_input_requirement { |ctx|
  let accepted = test.run_xsh(ctx, r"""pure selected(value) -> Str {
  if let item = value { let stored: Str? = item; stored ?? "none" } else { "absent" }
}
pure forwarded(value) -> Str { selected(value) }
let present: Str? = "text"
let missing: Str? = null
print selected(present)
print forwarded(missing)
""")?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == "text\nabsent\n", accepted.stdout
  let rejected = test.run_xsh(ctx, r"""pure selected(value) -> Str {
  if let item = value { let stored: Str? = item; stored ?? "none" } else { "absent" }
}
let present: Int? = 7
print selected(present)
""")?
  assert ! rejected.success, rejected.stdout
  assert "[check." in rejected.stderr, rejected.stderr
}
