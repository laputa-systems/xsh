test test_full_inference_activation_local_mutable_accumulator_is_generic { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure copy_all(values) {
  var output = []
  for value in values {
    output = output.push(value)
  }
  output
}
let numbers = copy_all([1, 2])
let words = copy_all(["one", "two"])
assert numbers == [1, 2]
assert words == ["one", "two"]
print $numbers.len()
print $words.len()
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "2\n2\n"
}

test test_full_inference_factory_returns_a_stateless_immutable_capture { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""let marker = "captured"
pure retain(value) {
  let captured = marker
  let _ = captured
  value
}
let retained = retain
pure factory() { retained }
let integer_factory = factory()
let text_factory = factory()
let integer = integer_factory(42)
let text = text_factory("demo")
assert integer == 42
assert text == "demo"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n"
}

test test_full_inference_shared_mutable_capture_through_record_stays_monomorphic { |ctx|
  test.expect(
    ctx,
    r"""var shared = {items: []}
proc remember(value) {
  shared.items = shared.items.push(value)
  value
}
let integer = remember(42)
let text = remember("demo")
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_generic_alias_does_not_duplicate_a_resource_handle { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value) { value }
proc use_released_alias() [fs, error] -> Result[Bool] {
  let root = fs.tempdir()?
  let alias = identity(root)
  root.close()?
  alias.exists(p".")
}
let _ = use_released_alias()?
""",
    status: 3,
    stderr: ["fs-root: root handle is not active"],
  )?
}

test test_full_inference_live_producer_keeps_one_incompatible_item_type { |ctx|
  test.expect(
    ctx,
    r"""stream values(value) {
  yield value
}
let producer = values(42)
let integers: Stream[Int] = producer
let words: Stream[Str] = producer
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_returned_callable_keeps_its_effect_obligation { |ctx|
  test.expect(
    ctx,
    r"""proc log(value: Str) [io] -> Result[Str] {
  print $value
  Ok(value)
}
let logged = log
proc factory() { logged }
proc denied() [error] -> Result[Str] {
  let callback = factory()?
  callback("demo")
}
""",
    status: 2,
    stderr: ["[check.effect-violation]"],
  )?
}

test test_full_inference_mutable_callable_binding_keeps_one_instance { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value) { value }
var current = identity
let integer = current(42)
current = identity
let text = current("demo")
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_recursive_infinite_type_is_rejected { |ctx|
  test.expect(
    ctx,
    r"""pure nest(value) {
  nest([value])
}
let nested = nest(42)
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_polymorphic_recursion_is_rejected { |ctx|
  test.expect(
    ctx,
    r"""pure descend(value, count: Int) {
  if count == 0 { 0 } else { descend([value], count - 1) }
}
let result = descend(42, 2)
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}
