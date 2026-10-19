test test_full_inference_callable_list_generalizes_its_outer_relationship { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure preserve(value) { value }
pure echo(value) { value }
let operations = [preserve, echo]
let integer = operations[0](42)
let text = operations[1]("demo")
assert integer == 42
assert text == "demo"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\ndemo\n"
}

test test_full_inference_callable_list_rejects_an_incompatible_shared_contract { |ctx|
  test.expect(
    ctx,
    r"""type IntIdentity = pure(value: Int) -> Int
pure number(value: Int) -> Int { value }
pure text(value: Str) -> Str { value }
let operations: List[IntIdentity] = [number, text]
""",
    status: 2,
    stderr: ["[check.callable-mismatch]"],
  )?
}

test test_full_inference_callable_list_typed_ground_control { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type IntTransform = pure(value: Int) -> Int
pure increment(value: Int) -> Int { value + 1 }
pure decrement(value: Int) -> Int { value - 1 }
let operations: List[IntTransform] = [increment, decrement]
let first = operations[0](41)
let second = operations[1](43)
assert first == 42
assert second == 42
print $first
print $second
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\n42\n"
}
