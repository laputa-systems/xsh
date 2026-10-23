test test_full_inference_returned_generic_callable_uses_immutable_capture { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""let marker = "capture:"
pure attach(value) { {marker: marker, value: value} }
let operation = attach
pure factory() { operation }
let make = factory()
let integer = make(42)
let text = make("demo")
assert integer.marker == "capture:"
assert integer.value == 42
assert text.marker == "capture:"
assert text.value == "demo"
print f"{integer.marker}{integer.value}"
print f"{text.marker}{text.value}"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "capture:42\ncapture:demo\n"
}

test test_full_inference_returned_callable_keeps_shared_capture_monomorphic { |ctx|
  test.expect(
    ctx,
    r"""var shared = {items: [0]}
proc remember(value) {
  shared.items = shared.items.push(value)
  value
}
proc factory() { return remember }
let captured = factory()?
let integer = captured(42)
let text = captured("demo")
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_proc_return_materializes_ground_callable { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure increment(value: Int) -> Int { value + 1 }
proc factory() { return increment }
let operation = factory()?
let answer = operation(41)
assert answer == 42
print $answer
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\n"
}

test test_full_inference_proc_return_preserves_generic_callable_branches { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure identity(value) { value }
proc factory(flag) {
  if flag { return identity }
  return identity
}
let operation = factory(true)?
let integer: Int = operation(42)
let text: Str = operation("demo")
assert integer == 42
assert text == "demo"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n"
}

test test_full_inference_proc_return_keeps_explicit_and_missing_context_rules { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type IntIdentity = pure(value: Int) -> Int
pure identity(value: Int) -> Int { value }
proc factory() -> Result[IntIdentity] { return identity }
let operation = factory()?
assert operation(42) == 42
print $operation(42)
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\n"
  test.expect(
    ctx,
    r"""proc unknown_return(value) { return null }
let integer: Null = unknown_return(42)?
let text: Null = unknown_return("demo")?
assert integer == null
assert text == null
""",
    status: 0,
  )?
}

test test_full_inference_returned_mutable_capture_ground_control { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""var shared = {items: [0]}
pure item_count() -> Int { shared.items.len() }
type IntRemember = proc(value: Int) -> Result[Int]
proc remember(value: Int) -> Result[Int] {
  shared.items = shared.items.push(value)
  Ok(value)
}
proc factory() -> Result[IntRemember] { Ok(remember) }
let before = item_count()
let captured = factory()?
let integer = captured(42)?
let after = item_count()
assert integer == 42
assert before == 1
assert after == 2
assert shared.items == [0, 42]
print $integer
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\n"
}
