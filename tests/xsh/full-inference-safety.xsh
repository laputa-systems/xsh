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

test test_full_inference_record_element_type_constraint_ground_control { |ctx|
  test.expect(
    ctx,
    r"""var shared = {items: [1]}
pure remember(value) {
  shared.items.push(value)
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

test test_full_inference_concrete_producer_item_type_ground_control { |ctx|
  test.expect(
    ctx,
    r"""stream values(value: Int) -> Stream[Int] {
  yield value
}
let producer = values(42)
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

test test_full_inference_returned_callable_effect_ground_control { |ctx|
  test.expect(
    ctx,
    r"""type Logged = proc(value: Str) [io] -> Result[Str]
proc log(value: Str) [io] -> Result[Str] {
  print $value
  Ok(value)
}
let logged = log
proc factory() [error] -> Result[Logged] {
  return Ok(logged)
}
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
let integer: Int = current(42)
current = identity
let text: Str = current("demo")
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_mutable_callable_binding_accepts_compatible_reassignment { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure increment(value: Int) -> Int { value + 1 }
pure decrement(value: Int) -> Int { value - 1 }
var operation = increment
let incremented = operation(41)
operation = decrement
let decremented = operation(43)
proc read_environment(value: Int) [env] -> Int {
  let _ = env.get("XSH_MUTABLE_CALLABLE")
  value
}
proc read_environment_again(value: Int) [env] -> Int {
  let _ = env.get("XSH_MUTABLE_CALLABLE")
  value
}
proc exercise_environment(value: Int) [env, error] -> Int {
  var operation = read_environment
  let first = operation(value)
  operation = read_environment_again
  let second = operation(value)
  first + second
}
let proc_total = exercise_environment(21)
assert incremented == 42
assert decremented == 42
assert proc_total == 42
print $incremented
print $decremented
""",
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == "42\n42\n"
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

test test_full_inference_iteration_input_relationship_preserves_collection_domains { |ctx|
  test.expect(
    ctx,
    r"""pure collect(values) { [value for value in values] }
assert collect([1, 2]) == [1, 2]
assert collect(["a"]) == ["a"]
assert collect("ab") == ["a", "b"]
assert collect(b"ab") == [97, 98]
assert collect({1, 2}) == [1, 2]
assert collect({["key"]: 7}) == [{key: "key", value: 7}]
""",
    status: 0,
  )?
}

test test_full_inference_iteration_input_relationship_rejects_noniterable_instance { |ctx|
  test.expect(
    ctx,
    r"""pure collect(values) { [value for value in values] }
let rejected = collect(7)
""",
    status: 2,
  )?
}

test test_full_inference_iteration_result_domains_preserve_grounded_behavior { |ctx|
  test.expect(
    ctx,
    r"""proc collect(values) [error] {
  var output = []
  for value in values { output = output.push(value) }
  output
}
stream items() [] -> Stream[Int] { yield 4; yield 8 }
let list: Result[List[Int]] = Ok([1, 2])
let text: Result[Str] = Ok("ab")
let byte_source: Result[Bytes] = Ok(b"ab")
let map_source: Result[Map[Str, Int]] = Ok({["key"]: 7})
let producer: Result[Stream[Int]] = Ok(items())
assert collect(list)? == [1, 2]
assert collect(text)? == ["a", "b"]
assert collect(byte_source)? == [97, 98]
assert collect(map_source)? == [{key: "key", value: 7}]
assert collect(producer)? == [4, 8]
assert collect(items())? == [4, 8]
""",
    status: 0,
  )?
}

test test_full_inference_iteration_stream_failure_preserves_pull_order_and_cleanup { |ctx|
  test.expect(
    ctx,
    r"""error IterationFailure = Stopped
stream values() [io, error] -> Stream[Int] {
  defer { print "stream-closed" }
  print "pulled"
  yield 1
  let stopped: Result[Int, IterationFailure] = Err(IterationFailure.Stopped())
  yield stopped?
}
proc collect(values) [io, error] {
  defer { print "collect-closed" }
  var output = []
  for value in values { output = output.push(value) }
  output
}
let source: Result[Stream[Int]] = Ok(values())
print "created"
let result = try { collect(source)? }
assert match result { Ok(_) => false, Err(_) => true }
print "caught"
let stopped: Result[List[Int], IterationFailure] = Err(IterationFailure.Stopped())
let rejected = try { collect(stopped)? }
assert match rejected { Ok(_) => false, Err(_) => true }
print "input-caught"
""",
    status: 0,
    stdout: ["created\npulled\nstream-closed\ncollect-closed\ncaught\ncollect-closed\ninput-caught\n"],
  )?
}

test test_full_inference_iteration_rejects_nested_result_sources { |ctx|
  test.expect(
    ctx,
    r"""proc collect(values) [error] { [value for value in values] }
let nested: Result[Result[List[Int]]] = Ok(Ok([1]))
let rejected = collect(nested)
""",
    status: 2,
  )?
}

test test_full_inference_iteration_relationship_follows_callback_results_and_caught_errors { |ctx|
  test.expect(
    ctx,
    r"""error IterationFailure = Stopped
pure from_factory(factory) { [value for value in factory()] }
pure numbers() { [1, 2] }
pure words() { ["a", "b"] }
pure captured(values) { try { [value for value in values] } }
assert from_factory(numbers) == [1, 2]
assert from_factory(words) == ["a", "b"]
assert captured([1, 2]) == Ok([1, 2])
let stopped: Result[List[Int], IterationFailure] = Err(IterationFailure.Stopped())
assert match captured(stopped) { Ok(_) => false, Err(_) => true }
""",
    status: 0,
  )?
}

test test_full_inference_iteration_respects_an_explicit_empty_effect_bound { |ctx|
  test.expect(
    ctx,
    r"""proc collect(values) [] { [value for value in values] }
assert collect([1, 2])? == [1, 2]
assert collect("ab")? == ["a", "b"]
""",
    status: 0,
  )?
  test.expect(
    ctx,
    r"""proc collect(values) [] { [value for value in values] }
stream values() [error] -> Stream[Int] { yield 1 }
let rejected = collect(values())
""",
    status: 2,
  )?
}
