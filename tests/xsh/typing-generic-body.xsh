test test_typing_generic_body_composes_host_and_monomorphic_calls { |ctx|
  test.expect(
    ctx,
    r"""use hash
pure append(items: List[Int], row) -> List[Int] {
  let value = row.value
  let mapped = item(value)
  let output = items.push(mapped)
  return output
}
pure item(value: Int) -> Int { value }
assert append([1], {value: 2, extra: false}) == [1, 2]
pure data(value: Bytes) -> Bytes { value }
pure digest(row) -> Digest { hash.sha256(data(row.value)) }
assert digest({value: b"a", extra: false}).hex() == "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"
""",
    status: 0,
  )?
}

test test_typing_generic_body_retains_ordinary_ground_helper_syntax { |ctx|
  test.expect(
    ctx,
    r"""type Action = pure(value: Int) -> Int
pure target(value: Int) -> Int { value }
pure offset(value: Int = 5) -> Int { value }
pure rest(base: Int, ...items: List[Int]) -> Int { base + items.len() }
pure helper(flag: Bool) -> Int {
  let source = if flag { [1] } else { [2] }
  let values = source.push(2)
  let actions: List[Action] = [target]
  let selected: List[Action] = [action for action in actions]
  with value = Ok(selected[0](values[0])) {
    return value + rest(offset(), @[3])
  } else { |_| return 0 }
}
pure read(row) -> Int { let ignored = row.anchor; helper(true).bit_or(0) }
assert read({anchor: false}) == 7
""",
    status: 0,
  )?
}

test test_typing_generic_body_sequences_immutable_values_and_captures { |ctx|
  test.expect(
    ctx,
    r"""pure copy(value) {
  let first = value
  let second = first
  second
}
let marker = 5
pure plus(value: Int, marker: Int) -> Int { value + marker }
pure read(row) -> Int {
  let value = row.value
  plus(marker: marker, value: value)
}
assert copy(7) == 7
assert copy("text") == "text"
assert copy(false) == false
assert read({value: 2, extra: false}) == 7
""",
    status: 0,
  )?
}

test test_typing_generic_body_preserves_ground_return_contracts { |ctx|
  test.expect(
    ctx,
    r"""pure truth(value: Int) -> Bool { value == 0 }
pure unit(value: Int) -> Unit { let copy = value }
pure item(value: Int) -> Int { value }
pure boolean(row) -> Bool { truth(row.value) }
pure ignored(row) -> Unit { unit(row.value) }
pure wrapped(row) -> Result[Int] { item(row.value) }
pure parsed(row) -> Result[Int] { row.text.parse_int() }
assert boolean({value: 1}) == false
ignored({value: 1})
assert wrapped({value: 1}) == Ok(1)
assert parsed({text: "2"}) == Ok(2)
""",
    status: 0,
  )?
  test.expect(
    ctx,
    r"""pure read(values: Map[UInt, Str], row) -> Result[Str] { values.get(row.key) }
let values: Map[UInt, Str] = {[0]: "zero"}
assert read(values, {key: 0}) == Ok("zero")
""",
    status: 0,
  )?
  test.expect(
    ctx,
    r"""pure read(values: Map[UInt, Str], row) -> Result[Str] { values.get(row.key) }
let values: Map[UInt, Str] = {[0]: "zero"}
let output = read(values, {key: -1})?
""",
    status: 3,
    stderr: ["schema", "UInt"],
  )?

  for source in [
    r"""pure minus(value: Int) -> Int { value - 1 }
pure probe(row) -> Int { let key: UInt = minus(row.key); key }
let value = probe({key: 0})
""",
    r"""pure minus(value: Int) -> Int { value - 1 }
pure read(row) -> Result[UInt] { minus(row.key) }
let value = read({key: 0})?
""",
    r"""type SourceOutput = {key: Int}
type Output = {key: UInt}
pure make(value: Int) -> SourceOutput { {key: value - 1} }
pure read(row) -> Output { make(row.key) }
let value = read({key: 0})
""",
  ] {
    test.expect(ctx, source, status: 3, stderr: ["UInt"])?
  }

  test.expect(ctx, r"""pure make(value: Int) -> List[Int] { [value - 1] }
pure read(row) -> List[UInt] { make(row.key) }
let value = read({key: 0})
""", status: 2, stderr: ["[check.type-mismatch]"])?

}

test test_typing_generic_body_keeps_named_argument_failure_order { |ctx|
  test.expect(
    ctx,
    r"""pure left(value: Int) -> Int { 1 / value }
pure right(value: Int) -> Int { 9223372036854775807 + (value + 1) }
pure pair(left: Int, right: Int) -> Int { left + right }
pure fail(row) -> Int {
  pair(right: right(row.value), left: left(row.value))
}
let output = fail({value: 0})
""",
    status: 3,
    stderr: ["integer-overflow"],
  )?
  test.expect(
    ctx,
    r"""pure key(value: Int) -> Int { value }
pure explode(value: Int) -> Int { 1 / value }
pure write(values: Map[UInt, Int], row) -> Map[UInt, Int] {
  values.set(key(row.key), explode(row.value))
}
let values: Map[UInt, Int] = {}
let output = write(values, {key: -1, value: 0})
""",
    status: 3,
    stderr: ["schema", "UInt"],
  )?

}

test test_typing_generic_body_rejects_mutable_state_and_local_mutation { |ctx|
  test.expect(
    ctx,
    r"""var marker = 5
pure plus(value: Int, marker: Int) -> Int { value + marker }
pure read(row) -> Int { plus(row.value, marker) }
""",
    status: 2,
    stderr: ["[check.local-inference]"],
  )?
  test.expect(
    ctx,
    r"""pure item(value: Int) -> Int { value }
pure read(row) -> Int {
  var value = row.value
  item(value)
}
""",
    status: 2,
    stderr: ["[check.local-inference]"],
  )?
}

test test_typing_generic_body_keeps_host_ambiguity_and_effect_rejections { |ctx|
  test.expect(
    ctx,
    r"""pure read(row) { row.value.byte_at(0) }
let caller = read({value: b"x"})
""",
    status: 2,
    stderr: ["[check.ambiguous-overload]"],
  )?
  test.expect(
    ctx,
    r"""pure item(value: Path) -> Path { value }
pure read(row) -> Result[Str] { item(row.path).read_text() }
""",
    status: 2,
    stderr: ["[check.pure-effect]"],
  )?
}
