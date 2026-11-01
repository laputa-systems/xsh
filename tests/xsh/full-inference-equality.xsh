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
