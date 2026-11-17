test test_nested_accumulator_list_store_original { |ctx|
  let original = test.expect(
    ctx,
    r"""var xs = [0 for _ in range(3)]
for round in range(2) {
  var carry = round
  for i in range(xs.len()) {
    carry += xs[i] * 4294967296
    xs[i] = carry % 656356768
    carry /= 656356768
  }
}
print xs[0]
""",
    status: 0,
  )?
  assert original.stdout == "1\n"
}

test test_nested_accumulator_list_store_minimal { |ctx|
  let minimal = test.expect(
    ctx,
    r"""var xs = [0]
for r in range(1) {
  var c = r
  for i in range(xs.len()) {
    xs[i] = c + 3
    c += 3
  }
}
print xs[0]
""",
    status: 0,
  )?
  assert minimal.stdout == "3\n"
}
