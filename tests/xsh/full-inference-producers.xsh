test test_full_inference_unannotated_generic_stream_infers_items_and_closes_early { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""stream values(value, count) {
  defer { print "closed" }
  repeat count times {
    yield value
  }
}
let producer = values("row", 2)
print "created"
for item in producer {
  print $item
  break
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "created\nrow\nclosed\n"
}

test test_full_inference_live_producer_keeps_its_item_type_monomorphic { |ctx|
  test.expect(
    ctx,
    r"""stream values(value: Int, count: Int) -> Stream[Int] {
  repeat count times {
    yield value
  }
}
let producer = values(42, 2)
let words: List[Str] = producer |> collect()
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_stream_rejects_incompatible_yield_types { |ctx|
  test.expect(
    ctx,
    r"""stream inconsistent(flag: Bool) -> Stream[Int] {
  if flag {
    yield 42
  } else {
    yield "demo"
  }
}
let producer = inconsistent(true)
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_recursive_returns_are_inferred_monotype_per_component { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure sum_to(value) {
  if value <= 0 { 0 } else { value + sum_to(value - 1) }
}
let result = sum_to(4)
assert result == 10
print $result
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "10\n"
}

test test_full_inference_mutual_recursive_returns_are_inferred_per_component { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure even(value) {
  if value <= 0 { true } else { odd(value - 1) }
}
pure odd(value) {
  if value <= 0 { false } else { even(value - 1) }
}
let result = even(6)
assert result
print $result
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "true\n"
}
