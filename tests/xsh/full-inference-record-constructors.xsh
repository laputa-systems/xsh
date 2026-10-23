test test_generic_record_constructors_keep_evaluation_order_defaults_and_nominal_identity { |ctx|
  let output = test.run_xsh(ctx, r"""type Counts = {left: Int, right: Int = 9}
nominal type Brand = {value: Int}
proc observe(label: Str, value: Int) [io] -> Int {
  print $label
  value
}
proc make(payload, count: Int) [io] {
  let explicit = Counts(right: observe("right", count + 1), left: observe("left", count))
  let defaulted = Counts(left: observe("default", count))
  let brand = Brand(value: count)
  {payload, explicit, defaulted, brand}
}
let integer = make(7, 3)?
let text = make("kept", 4)?
assert integer.payload == 7
assert integer.explicit.left == 3
assert integer.explicit.right == 4
assert integer.defaulted.right == 9
assert integer.brand.value == 3
assert text.payload == "kept"
assert text.explicit.left == 4
assert text.explicit.right == 5
assert text.defaulted.right == 9
assert text.brand.value == 4
""")?
  assert output.success, output.stderr
  assert output.stdout == "right\nleft\ndefault\nright\nleft\ndefault\n"
}

test test_generic_record_constructors_reject_unrelated_nominal_fields { |ctx|
  test.expect(ctx, r"""nominal type Left = {value: Int}
nominal type Right = {value: Int}
type Holder = {item: Left}
pure make(payload, item: Right) {
  let row = Holder(item: item)
  {payload, row}
}
let result = make(7, Right(value: 3))
""", status: 2, stderr: ["[check.type-mismatch]"])?
}

test test_generic_schema_constructors_keep_input_relationships_and_defaults { |ctx|
  let output = test.run_xsh(ctx, r"""type Box[T] = {value: T, extra: Int = 9, items: List[T] = []}
type Envelope[T] = {item: Box[T]}
pure boxed(value) { Box(value:) }
pure wrapped(value) { Envelope(item: boxed(value)) }
proc observed(label: Str, value: Int) [io] -> Int { print $label; return value }
proc ordered(value: Int) [io] -> Box[Int] { return Box(extra: observed("extra", 7), value: { let _ = observed("value", 0); value }) }
let number = wrapped(7)
let word = wrapped("kept")
assert number.item.value == 7
assert word.item.value == "kept"
assert number.item.extra == 9
assert word.item.items.len() == 0
assert ordered(7).value == 7
assert ordered(8).value == 8
""")?
  assert output.success, output.stderr
  assert output.stdout == "extra\nvalue\nextra\nvalue\n"
}

test test_generic_schema_constructors_reject_unanchored_phantom_arguments { |ctx|
  test.expect(ctx, r"""type Phantom[T] = {value: Int}
pure phantom(value: Int) { Phantom(value:) }
let result = phantom(7)
""", status: 2, stderr: ["[check.constructor-inference]"])?
}


test test_ground_return_schema_uses_the_selected_callable { |ctx|
  let output = test.expect(ctx, r"""
nominal type Brand = {value: Int}
type Tagged[T] = Brand
type Branded[T] = {payload: T, tag: Tagged[T]}
pure closed_brand() -> Brand { Brand(value: 7) }
pure inferred_brand() { Brand(value: 19) }
pure wrapped(payload) { Branded(payload:, tag: closed_brand()) }
pure inferred_wrapped(payload) { Branded(payload:, tag: inferred_brand()) }
pure typed(payload: Int) -> Branded[Int] { Branded(payload:, tag: closed_brand()) }
assert wrapped(7).tag.value == 7
assert wrapped("kept").payload == "kept"
assert typed(3).tag.value == 7
assert inferred_wrapped(7).tag.value == 19
assert inferred_wrapped("kept").tag.value == 19
""")?
  assert output.success, output.stderr
  assert output.stdout == ""
}


test test_generic_error_variant_constructors_keep_payload_order_and_defaults { |ctx|
  let output = test.expect(ctx, r"""
error Failure = Rejected(message: Str, code: UInt) | Default
proc observed(label: Str, value: Int) [io] -> Int { print $label; value }
proc make(payload, count: Int) [io] -> Result[Unit, Failure] {
  let _ = payload
  Err(Failure.Rejected(code: observed("code", count), message: { print "message"; "failed" }))
}
proc defaulted(payload) -> Result[Unit, Failure] { let _ = payload; Err(Failure.Default()) }
let result = make(7, 3)
assert match result { Err(Failure.Rejected {message, code}) => message == "failed" && code == 3; _ => false }
assert match defaulted("kept") { Err(Failure.Default {message}) => message == "Failure.Default"; _ => false }
""")?
  assert output.success, output.stderr
  assert output.stdout == "code\nmessage\n"
}
