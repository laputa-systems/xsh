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
    r"""var shared = {items: []}
proc remember(value) {
  shared.items = shared.items.push(value)
  value
}
proc factory() { remember }
let captured = factory()?
let integer = captured(42)
let text = captured("demo")
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_returned_mutable_capture_ground_control { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""var shared = {items: [0]}
type IntRemember = proc(value: Int) -> Result[Int]
proc remember(value: Int) -> Result[Int] {
  shared.items = shared.items.push(value)
  Ok(value)
}
proc factory() -> Result[IntRemember] { Ok(remember) }
let captured = factory()?
let integer = captured(42)?
assert integer == 42
assert shared.items == [0, 42]
print $integer
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\n"
}
