test sum_and_reduce_by_report_overflow { |ctx|
  for values in ["9223372036854775807, 1", "-9223372036854775807 - 1, -1"] {
    for expression in [
      "values |> sum()",
      "values |> reduce-by(sum: true) { |item| {key: \"all\", value: item} }",
      "values |> reduce-by(sum: true) { |item| {key: \"all\", value: {total: item}} }",
    ] {
      let source = "let values = [" + values + "]\nlet _ = " + expression + "\n"
      test.expect(ctx, source, status: 3, stderr: ["integer-overflow: integer overflow"])?
    }
  }
}

test finite_extrema_compare_values { |ctx|
  test.expect(ctx, r"""
pure minimum(values) { values |> min() }
pure maximum(values) { values |> max() }
assert minimum([9.0, 1.0])? == 1.0
assert maximum([1.0, 9.0])? == 9.0
assert minimum([9s, 1s])? == 1s
assert maximum([1s, 9s])? == 9s
""", status: 0)?
}

test aggregate_controls_and_numeric_routes { |ctx|
  test.expect(ctx, r"""
pure minimum(values) { values |> min() }
pure maximum(values) { values |> max() }
stream items(first, second) { yield first; yield second }
assert [9.0, 1.0] |> min()? == 1.0
assert [1.0, 9.0] |> min()? == 1.0
assert [9.0, 1.0] |> max()? == 9.0
assert [1.0, 9.0] |> max()? == 9.0
assert minimum(items(9.0, 1.0))? == 1.0
assert maximum(items(1.0, 9.0))? == 9.0
assert [9s, 1s] |> min()? == 1s
assert [1s, 9s] |> min()? == 1s
assert [9s, 1s] |> max()? == 9s
assert [1s, 9s] |> max()? == 9s
assert minimum(items(9s, 1s))? == 1s
assert maximum(items(1s, 9s))? == 9s
let lo = [9.0, 1.0] |> reduce-by(min: true) { |item| {key: "all", value: item} }
let hi = [1.0, 9.0] |> reduce-by(max: true) { |item| {key: "all", value: item} }
assert lo["all"] == 1.0
assert hi["all"] == 9.0
let small_total = [1, 2, 3] |> sum()
assert small_total == 6
let summed = [1.5, 2.5] |> reduce-by(sum: true) { |item| {key: "all", value: item} }
assert summed["all"] == 4.0
""", status: 0)?
}

test live_and_fused_overflow { |ctx|
  for values in ["9223372036854775807, 1", "-9223372036854775807 - 1, -1"] {
    for expression in [
      "items(first, second) |> sum()",
      "items(first, second) |> reduce-by(sum: true) { |item| {key: \"all\", value: item} }",
      "items(first, second) |> reduce-by(sum: true) { |item| {key: \"all\", value: {total: item}} }",
      "[{amount: first}, {amount: second}] |> reduce-by(sum: true) { |row| {key: \"all\", value: {total: row.amount}} }",
      "[first, second] |> par-map(jobs: 2) { |item| [{bucket: \"all\", amount: item}] } |> flat-map { |rows| rows } |> reduce-by(sum: true) { |row| {key: row.bucket, value: {total: row.amount}} }",
    ] {
      let source = "stream items(a: Int, b: Int) { yield a; yield b }\nlet values = [" + values + "]\nlet first = values[0]\nlet second = values[1]\nlet _ = " + expression + "\n"
      test.expect(ctx, source, status: 3, stderr: ["integer-overflow: integer overflow"])?
    }
  }
}

test overflow_closes_a_live_source { |ctx|
  let source = "stream items() -> Stream[Int] {\n  defer { print \"closed\" }\n  yield 9223372036854775807\n  yield 1\n  print \"pulled\"\n  yield 2\n}\nlet _ = items() |> sum()\n"
  let outcome = test.expect(ctx, source, status: 3, stdout: ["closed"], stderr: ["integer-overflow: integer overflow"])?
  assert "pulled" not in outcome.stdout, outcome.stdout
}
