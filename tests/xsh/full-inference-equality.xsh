test test_equality_preserves_nullable_field_inputs { |ctx|
  let output = test.run_xsh(ctx, r"""type Input = {x: Str?}
pure same(a) -> Bool { a.x == "hi" }
pure reverse(a) -> Bool { "hi" != a.x }
pure forwarded(a) -> Bool { same(a) }
let empty: Input = {x: null}
let present: Input = {x: "hi"}
print same(empty)
print same(present)
print reverse(empty)
print forwarded(present)
print same({x: "hi"})
""")?
  assert output.success, output.stderr
  assert output.stdout == "false\ntrue\ntrue\ntrue\ntrue\n", output.stdout
}

test test_equality_rejects_incompatible_field { |ctx|
  let output = test.run_xsh(ctx, r"""pure same(a) -> Bool { a.x == "hi" }
print same({x: 7})
""")?
  assert ! output.success, output.stdout
  assert "[check.type-mismatch]" in output.stderr, output.stderr
}

test test_equality_requirements_follow_guards { |ctx|
  let output = test.run_xsh(ctx, r"""pure selected(value) -> Bool {
  if value != null { value.x == "hi" } else { false }
}
type Input = {x: Str?}
let input: Input? = {x: null}
print selected(input)
print selected(null)
""")?
  assert output.success, output.stderr
  assert output.stdout == "false\nfalse\n", output.stdout
}

test test_equality_does_not_widen_container_storage { |ctx|
  let output = test.run_xsh(ctx, r"""pure same(value) -> Bool { value == ["hi"] }
let values: List[Str?] = [null]
print same(values)
""")?
  assert ! output.success, output.stdout
  assert "[check.type-mismatch]" in output.stderr, output.stderr
}

test test_equality_context_fills_only_owned_result_constructor_holes { |ctx|
  let output = test.expect(ctx, r"""pure keep(value) { value }
let expected: Result[Int, Str] = Err("opaque")
assert keep(Err("opaque")) == expected
assert expected == keep(Err("opaque"))
assert keep(Err("other")) != expected
let constructed = keep(Err("opaque"))
let alias = constructed
assert alias == expected
pure compared(error: Str) -> Bool {
  let expected: Result[Int, Str] = Err(error)
  keep(Err(error)) == expected
}
pure reversed(error: Str) -> Bool {
  let expected: Result[Int, Str] = Err(error)
  expected == keep(Err(error))
}
assert compared("body") and reversed("body")
pure aliased(error: Str) -> Bool {
  let expected: Result[Int, Str] = Err(error)
  let constructed = keep(Err(error))
  let alias = constructed
  alias == expected
}
assert aliased("alias")
pure guarded(error: Str?) -> Bool {
  if let text = error {
    let expected: Result[Int, Str] = Err(text)
    keep(Err(text)) == expected
  } else { true }
}
assert guarded(null) and guarded("body")
type Input = {x: Str?}
pure compared_field(value) -> Bool { keep(Err(value.x)) == expected }
let absent: Input = {x: null}
let present: Input = {x: "opaque"}
assert ! compared_field(absent) and compared_field(present)
""", status: 0)?
  assert output.stderr == "", output.stderr
  for input in ["let _ = compared({x: 7})", "let dynamic: Any = \"opaque\"\nlet _ = compared({x: dynamic})"] {
    let wrong = test.run_xsh(ctx, r"""pure keep(value) { value }
let expected: Result[Int, Str] = Err("opaque")
pure compared(value) -> Bool { keep(Err(value.x)) == expected }
""" + input)?
    assert ! wrong.success, wrong.stdout
  }
}

test test_equality_context_preserves_proc_completion_result_data { |ctx|
  let output = test.expect(ctx, r"""type TextRow = {name: Str}
proc retained(row, value) { let _ = row?.name; value }
let success: Result[TextRow, Str] = Ok({name: "demo"})
let nested: Result[Result[TextRow, Str], Str] = Ok(Err("opaque"))
assert retained(success, Err("opaque")) == nested
assert nested == retained(success, Err("opaque"))
""", status: 0)?
  assert output.stderr == "", output.stderr
}
