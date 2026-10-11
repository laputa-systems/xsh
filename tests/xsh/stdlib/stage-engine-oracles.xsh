test test_stage_engine_frame_mode_matches_list_and_stream_rows { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream values() [] -> Stream[Int] {
  yield 1
  yield 2
  yield 3
}

pure take_count() -> Int { 2 }

pure sum_rows(rows: List[Int]) -> Int {
  var sum = 0
  for n in rows |> map { |n| n * 2 } |> where { |n| n > 2 } {
    sum += n
  }
  sum
}

proc sum_stream() -> Int {
  var sum = 0
  for n in values() |> map { |n| n * 2 } |> where { |n| n > 2 } {
    sum += n
  }
  sum
}

proc sum_each() -> Int {
  var sum = 0
  values() |> each { |n| sum += n }
  sum
}

let list_rows = [1, 2, 3]
  |> map { |n| n * 2 }
  |> where { |n| n > 2 }
  |> collect()
let stream_rows = values()
  |> map { |n| n * 2 }
  |> where { |n| n > 2 }
  |> collect()
let limited_rows = [1, 2, 3]
  |> map { |n| n * 2 }
  |> take(take_count())
assert list_rows == [4, 6]
assert stream_rows == [4, 6]
assert limited_rows == [2, 4]
assert sum_rows([1, 2, 3]) == 10
assert sum_stream() == 10
assert sum_each() == 6
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == "", output.stdout
}

test test_frame_stage_materializers_keep_values_and_order { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let lines = "one\ntwo\n" |> text.lines
let json_rows = "{\"n\":1}\n{\"n\":2}" |> json.lines
let chunks = b"abcdef" |> bytes.chunks(2)
let zipped = [1, 2] |> zip(["a", "b"])
let sorted = [3, 1, 2] |> sort
let descending = [3, 1, 2] |> sort(desc: true)
let shuffled = [1, 2, 3, 4] |> shuffle(7)
let repeated = [1, 2] |> repeat(2)
let ranged = [0] |> range(1, 4)
let batches = [1, 2, 3] |> batch(count: 2)
let total = [1, 2, 3] |> sum
let last = [3, 1, 2] |> last()?
let minimum = [3, 1, 2] |> min?
let maximum = [3, 1, 2] |> max?
assert lines == ["one", "two"]
assert json_rows.len() == 2
assert json.get(json_rows[0], ["n"])?.require(Int)? == 1
assert json.get(json_rows[1], ["n"])?.require(Int)? == 2
assert chunks == [b"ab", b"cd", b"ef"]
assert zipped == [{left: 1, right: "a"}, {left: 2, right: "b"}]
assert sorted == [1, 2, 3]
assert descending == [3, 2, 1]
assert shuffled.len() == 4
assert repeated == [1, 2, 1, 2]
assert ranged == [1, 2, 3]
assert batches == [[1, 2], [3]]
assert total == 6
assert last == 2
assert minimum == 1
assert maximum == 3
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == "", output.stdout
}

test test_frame_table_print_materializes_record_rows { |ctx|
  let output = test.run_script(
    ctx,
    r"""
[{name: "alpha", size: 2}, {name: "beta", size: 1}] |> table.print
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert "alpha" in output.stdout
  assert "beta" in output.stdout
  assert "name" in output.stdout
}

test test_frame_keyed_stages_keep_stable_group_and_unique_order {
  let sorted = [{key: 2, label: "b"}, {key: 1, label: "a"}]
    |> sort-by .key
  let groups = ["b", "a", "b"] |> group-by .
  let counts = [1, 2, 3] |> count { |value| value % 2 }
  let unique = [1, 2, 1, 3] |> unique-by .
  assert [row.label for row in sorted] == ["a", "b"]
  assert groups[0].key == "b"
  assert groups[0].items == ["b", "b"]
  assert groups[1].key == "a"
  assert (counts.get("0") ?? 0) == 1
  assert (counts.get("1") ?? 0) == 2
  assert unique == [1, 2, 3]
}

test test_frame_fold_keeps_callback_on_current_machine { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream values() [] -> Stream[Int] {
  yield 1
  yield 2
  yield 3
}

let list_total = [1, 2, 3] |> fold(0) { |acc, item| acc + item }
let stream_total = values() |> fold(0) { |acc, item| acc + item }
assert list_total == 6
assert stream_total == 6
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
}

test test_frame_reduce_and_parallel_worker_stages_keep_results { |ctx|
  let output = test.run_script(
    ctx,
    r"""
pure configured_reduce(sum_mode: Bool) -> Map[Str, Int] {
  [1, 2, 3]
    |> reduce-by(sum: sum_mode) { |item| {key: "all", value: item} }
}
let configured = configured_reduce(true)
let mapped = [1, 2, 3]
  |> par-map { |item| item * 2 }
let mapped_block = [1, 2, 3]
  |> par-map { |item|
    let doubled = item * 2
    doubled
  }
let fused = [1, 2, 3]
  |> par-map { |item| [item, item] }
  |> flat-map { |items| items }
  |> reduce-by(sum: true) { |item| {key: "all", value: item} }
assert (configured.get("all") ?? 0) == 6
assert mapped == [2, 4, 6]
assert mapped_block == [2, 4, 6]
assert (fused.get("all") ?? 0) == 12
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
}
