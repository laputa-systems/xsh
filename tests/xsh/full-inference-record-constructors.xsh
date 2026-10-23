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
let integer = make(7, 3)
let text = make("kept", 4)
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
