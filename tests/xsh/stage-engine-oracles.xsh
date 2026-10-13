test test_pipeline_drain_and_for_pull_preserve_callback_order_and_cleanup { |ctx|
  let output = test.expect(
    ctx,
    r"""
pure keep(value: Int) -> Bool { value > 1 }
pure scale(value: Int) -> Int { value * 10 }
pure expand(value: Int) -> List[Int] { [value, value + 1] }

proc close_source(label: Str) [io] { print f"{label}:close" }

stream numbers(label: Str) [io, error] -> Stream[Int] {
  defer close_source(label)
  for value in [1, 2, 3] {
    print f"{label}:pull:{value}"
    yield value
  }
}

proc main() [io, error] {
  let list_values = [1, 2, 3]
    |> where(keep)
    |> map(scale)
    |> flat-map(expand)
    |> collect()
  let drained_values = numbers("drain")
    |> where { |value| keep(value) }
    |> map { |value| scale(value) }
    |> flat-map { |value| expand(value) }
    |> collect()
  print f"list={list_values[0]},{list_values[1]},{list_values[2]},{list_values[3]}"
  print f"drain={drained_values[0]},{drained_values[1]},{drained_values[2]},{drained_values[3]}"

  for value in numbers("pull")
      |> where { |item| keep(item) }
      |> map { |item| scale(item) }
      |> flat-map { |item| expand(item) } {
    print f"row={value}"
    if value == 21 { break }
  }
}
""",
    status: 0,
  )?
  assert output.stdout == """drain:pull:1
drain:pull:2
drain:pull:3
drain:close
list=20,21,30,31
drain=20,21,30,31
pull:pull:1
pull:pull:2
row=20
row=21
pull:close
"""
}
