test test_full_inference_keep_preserves_callback_labels_defaults_and_call_order { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure named(payload: Int, scale: Int = 2) -> Int {
  payload * scale
}
pure keep(f, x) {
  let _ = f(x)
  f
}
let returned = keep(f: named, x: 3)
let reversed = keep(x: 4, f: named)
let alias = returned
let explicit_order = returned(payload: 5, scale: 3)
let reversed_order = alias(scale: 4, payload: 6)
let omitted_default = reversed(payload: 7)
assert explicit_order == 15
assert reversed_order == 24
assert omitted_default == 14
print f"{explicit_order} {reversed_order} {omitted_default}"
""",
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == "15 24 14\n"
}

test test_full_inference_distinct_callback_binders_instantiate_independently { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure identity(value) { value }
pure apply_distinct(left, right, left_value, right_value) {
  {left: left(left_value), right: right(right_value)}
}
pure apply_same(callback, first, second) {
  [callback(first), callback(second)]
}
let independent = apply_distinct(identity, identity, 42, "demo")
let integers = apply_same(identity, 42, 7)
let words = apply_same(identity, "one", "two")
assert independent == {left: 42, right: "demo"}
assert integers == [42, 7]
assert words == ["one", "two"]
print f"{independent.left} {independent.right}"
print f"{integers[0]} {integers[1]}"
print f"{words[0]} {words[1]}"
""",
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == "42 demo\n42 7\none two\n"
}

test test_full_inference_callback_effects_follow_invocation_and_defaults { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc read_value(value = env.get_or("XSH_F2_CALLBACK_VALUE", "fallback")?) [env, io, error] -> Result[Str] {
  print "callback invoked"
  Ok(value)
}
proc retain(callback) {
  callback
}
proc invoke(callback, value) {
  callback(value)?
}
proc increment(value: Int) [io, error] -> Result[Int] {
  print f"increment {value}"
  Ok(value + 1)
}
proc return_only() [error] -> Result[Str] {
  let _ = retain(read_value)?
  Ok("returned")
}
proc invoke_default() [env, io, error] -> Result[Str] {
  let callback = retain(read_value)?
  callback()
}
proc invoke_increment() [io, error] -> Result[Int] {
  let value = invoke(increment, 41)?
  Ok(value)
}
let returned = return_only()?
let environment_value = invoke_default()?
let incremented = invoke_increment()?
assert returned == "returned"
assert environment_value == "from environment"
assert incremented == 42
print $returned
print $environment_value
print $incremented
""",
    env: {XSH_F2_CALLBACK_VALUE: "from environment"},
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == "callback invoked\nincrement 41\nreturned\nfrom environment\n42\n"
}

test test_full_inference_omitted_callback_default_effect_cannot_escape_a_bound { |ctx|
  test.expect(
    ctx,
    r"""proc read_value(value = env.get_or("XSH_F2_CALLBACK_VALUE", "fallback")?) [env, error] -> Result[Str] {
  Ok(value)
}
proc retain(callback) {
  callback
}
proc denied() [error] -> Result[Str] {
  let callback = retain(read_value)?
  callback()
}
""",
    status: 2,
    stderr: ["[check.effect-violation]", "env"],
  )?
}

test test_full_inference_proc_context_accepts_pure_but_pure_context_rejects_proc { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc invoke(callback) [] -> Int {
  callback()
}
pure answer() -> Int {
  42
}
let value = invoke(answer)
assert value == 42
print $value
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\n"

  test.expect(
    ctx,
    r"""pure invoke(callback) {
  callback()
}
proc empty_proc() [] -> Int {
  42
}
let invalid = invoke(empty_proc)
""",
    status: 2,
    stderr: ["[check.callable-mismatch]", "requires a pure callable, found a proc callable"],
  )?
}
