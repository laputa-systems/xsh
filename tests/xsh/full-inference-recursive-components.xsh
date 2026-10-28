test test_full_inference_self_recursive_scc_generalizes_after_checking { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure descend(value, count) {
  if count == 0 { value } else { descend(value, count - 1) }
}
let integer = descend(42, 2)
let text = descend("demo", 3)
assert integer == 42
assert text == "demo"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n"
}

test test_full_inference_mutual_recursive_scc_is_declaration_order_independent { |ctx|
  let forward = test.run_xsh(
    ctx,
    r"""pure left(value, count) {
  if count == 0 { value } else { right(value, count - 1) }
}
pure right(value, count) {
  if count == 0 { value } else { left(value, count - 1) }
}
let integer = left(17, 4)
let text = right("mutual", 3)
assert integer == 17
assert text == "mutual"
print $integer
print $text
""",
  )?
  let reverse = test.run_xsh(
    ctx,
    r"""pure right(value, count) {
  if count == 0 { value } else { left(value, count - 1) }
}
pure left(value, count) {
  if count == 0 { value } else { right(value, count - 1) }
}
let integer = left(17, 4)
let text = right("mutual", 3)
assert integer == 17
assert text == "mutual"
print $integer
print $text
""",
  )?
  assert forward.success, forward.stderr
  assert reverse.success, reverse.stderr
  assert forward.stdout == "17\nmutual\n"
  assert reverse.stdout == forward.stdout
}

test test_full_inference_recursive_peer_callback_requirements_remain_independent { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure left(action, other, value, count) {
  if count == 0 { action(value) } else { right(action, other, value, count - 1) }
}
pure right(action, other, value, count) {
  if count == 0 { other(payload: value) } else { left(action, other, value, count - 1) }
}
pure integer(value: Int) -> Int { value + 1 }
pure integer_other(payload: Int) -> Int { payload + 2 }
pure text(value: Str) -> Str { value + "!" }
pure text_other(payload: Str) -> Str { payload + "?" }
assert left(integer, integer_other, 41, 2) == 42
assert right(text, text_other, "demo", 2) == "demo?"
""",
  )?
  assert output.success, output.stderr
  test.expect(
    ctx,
    r"""pure left(action, other, value, count) {
  if count == 0 { action(value) } else { right(action, other, value, count - 1) }
}
pure right(action, other, value, count) {
  if count == 0 { other(payload: value) } else { left(action, other, value, count - 1) }
}
pure integer(value: Int) -> Int { value + 1 }
pure text_other(payload: Str) -> Str { payload + "?" }
let invalid = left(integer, text_other, 41, 2)
""",
    status: 2,
    stderr: ["[check.callable-mismatch]"],
  )?
}

test test_full_inference_recursive_proc_preserves_independent_result_instances { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc descend(value, count) {
  if count == 0 { return value }
  descend(value, count - 1)
}
assert descend(42, 2)? == 42
assert descend("demo", 3)? == "demo"
pure inner_ok() -> Result[Int, Str] { Ok(7) }
pure inner_error() -> Result[Int, Str] { Err("stored") }
assert descend(inner_ok(), 0)? == inner_ok()
assert descend(inner_ok(), 2)? == inner_ok()
assert descend(inner_error(), 0)? == inner_error()
assert descend(inner_error(), 2)? == inner_error()
""",
  )?
  assert output.success, output.stderr
}

test test_full_inference_recursive_peer_retains_callback_effects { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc left(action, value, count) {
  if count == 0 { return value }
  right(action, value, count - 1)
}
proc right(action, value, count) {
  if count == 0 { return action(value) }
  left(action, value, count - 1)
}
proc report(value: Str) [io] -> Str { print $value; return value }
let result = left(report, "peer", 1)?
assert result == "peer"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "peer\n"
  test.expect(
    ctx,
    r"""proc left(action, value, count) [] {
  if count == 0 { return value }
  right(action, value, count - 1)
}
proc right(action, value, count) {
  if count == 0 { return action(value) }
  left(action, value, count - 1)
}
proc report(value: Str) [io] -> Str { print $value; return value }
let result = left(report, "peer", 1)?
""",
    status: 2,
    stderr: ["[check.effect-violation]"],
  )?
}

test test_full_inference_recursive_capture_keeps_nested_mutable_state_monomorphic { |ctx|
  let accepted = test.run_xsh(
    ctx,
    r"""var shared = {items: [1]}
proc store(value, count) {
  if count == 0 {
    shared.items = [value]
    return value
  }
  store(value, count - 1)
}
assert store(42, 2)? == 42
assert shared.items == [42]
""",
  )?
  assert accepted.success, accepted.stderr
  test.expect(
    ctx,
    r"""var shared = {items: [1]}
proc store(value, count) {
  if count == 0 {
    shared.items = [value]
    return value
  }
  store(value, count - 1)
}
let integer = store(42, 2)?
let text = store("wrong", 2)?
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_recursive_pure_indexes_a_generic_receiver { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure total(items, count) {
  if count == 0 { 0 } else { items[0] + total(items, count - 1) }
}
pure weigh(items, count) {
  if count == 0 { 0.0 } else { items[0] * 0.5 + weigh(items, count - 1) }
}
let integers = total([1, 2, 3], 2)
let floats = weigh([1.0, 2.0], 2)
assert integers == 2
assert floats == 1.0
print $integers
print $floats
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "2\n1\n"
}
