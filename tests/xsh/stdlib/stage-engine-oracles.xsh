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

test test_stage_engine_field_projections_match_list_and_stream_sources { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let rows = [
  {kind: "keep", region: "west", label: "b", rank: 2},
  {kind: "drop", region: "east", label: "a", rank: 1},
  {kind: "keep", region: "west", label: "c", rank: 3},
]

type StageRow = {kind: Str, region: Str, label: Str, rank: Int}

stream source_rows() [] -> Stream[StageRow] {
  yield {kind: "keep", region: "west", label: "b", rank: 2}
  yield {kind: "drop", region: "east", label: "a", rank: 1}
  yield {kind: "keep", region: "west", label: "c", rank: 3}
}

pure list_label_length() -> Int {
  var total = 0
  for label in rows |> map .label {
    total += label.byte_len()
  }
  total
}

proc stream_label_length() -> Int {
  var total = 0
  for label in source_rows() |> map .label {
    total += label.byte_len()
  }
  total
}

pure list_kept_rank() -> Int {
  var total = 0
  for row in rows |> where .kind == "keep" {
    total += row.rank
  }
  total
}

proc stream_kept_rank() -> Int {
  var total = 0
  for row in source_rows() |> where .kind == "keep" {
    total += row.rank
  }
  total
}

let mapped_list = rows |> map .label
let mapped_stream = source_rows() |> map .label |> collect()
let kept_list = rows |> where .kind == "keep" and "west" == .region
let kept_stream = source_rows()
  |> where .kind == "keep" and "west" == .region
  |> collect()
let excluded_list = rows |> where .kind != "drop"
let alternative_list = rows |> where .kind == "keep" or .region == "east"
let sorted_list = rows |> sort-by .rank
let sorted_stream = source_rows() |> sort-by .rank
let grouped_list = rows |> group-by .kind
let grouped_stream = source_rows() |> group-by .kind

assert mapped_list == ["b", "a", "c"]
assert mapped_stream == mapped_list
assert list_label_length() == 3
assert stream_label_length() == 3
assert list_kept_rank() == 5
assert stream_kept_rank() == 5
assert kept_list == [rows[0], rows[2]]
assert kept_stream == kept_list
assert excluded_list == kept_list
assert alternative_list == rows
assert [row.label for row in sorted_list] == ["a", "b", "c"]
assert sorted_stream == sorted_list
assert grouped_list[0].key == "keep"
assert grouped_list[0].items == [rows[0], rows[2]]
assert grouped_list[1].key == "drop"
assert grouped_list[1].items == [rows[1]]
assert grouped_stream == grouped_list
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == "", output.stdout
}

test test_stage_engine_projection_fallbacks_keep_results_and_map_errors { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let rows = [
  {label: "alpha", rank: 2},
  {label: "beta", rank: 1},
]
let mapped = rows |> map .rank + 1
let filtered = rows |> where .rank > 1
assert mapped == [3, 2]
assert filtered == [rows[0]]
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == "", output.stdout

  let failure = test.run_script(
    ctx,
    r"""
let _ = [{rank: 0}] |> map 100 / .rank
""",
  )?
  assert ! failure.success, failure.stdout
  assert "stream stage `map` item 0 failed" in failure.stderr
  assert "division" in failure.stderr
}

test test_stage_engine_map_and_flat_map_block_failures_keep_item_context { |ctx|
  let cases = [
    {
      label: "map list prefix",
      source: r"""
let _ = [1, 0] |> map { |item|
  let quotient = 10 / item
  quotient
} |> collect()
""",
    },
    {
      label: "map stream tail",
      source: r"""
stream numbers() [] -> Stream[Int] {
  yield 1
  yield 2
}
let _ = numbers() |> map { |item|
  let adjusted = item + 0
  10 / (item - 2)
} |> collect()
""",
    },
    {
      label: "flat-map list tail",
      source: r"""
let _ = [1, 2] |> flat-map { |item|
  let adjusted = item + 0
  [10 / (item - 2)]
} |> collect()
""",
    },
    {
      label: "flat-map stream prefix",
      source: r"""
stream numbers() [] -> Stream[Int] {
  yield 1
  yield 0
}
let _ = numbers() |> flat-map { |item|
  let quotient = 10 / item
  [quotient]
} |> collect()
""",
    },
  ]

  for case in cases {
    let failure = test.run_xsht_trace(ctx, case.source, ["--trace", "--raw"])?
    assert failure.status == 3, case.label + ": " + failure.stderr
    assert "stream stage `map` item 1 failed" in failure.stderr, case.label + ": " + failure.stderr
    assert "kind=stream.item.error" in failure.stderr, case.label + ": " + failure.stderr
    assert "item_index=1" in failure.stderr, case.label + ": " + failure.stderr
  }
}

test test_stage_engine_map_field_error_uses_field_span { |ctx|
  let root = test.temp_dir(ctx, name: "stage-map-field-span")?
  fp"{root}/entry.txt".write("payload")
  let output = test.run_script(
    ctx,
    f"""
let root = p"{root}"
let entries = fs.files(root, stat: false)? |> collect()
let _ = entries |> map .size
""",
  )?
  assert ! output.success, output.stdout
  assert "err[runtime.error]: stream stage `map` item 0 failed" in output.stderr
  assert "err: metadata-unavailable: stream stage `map` item 0 failed" in output.stderr
  assert ":4:24-4:29" in output.stderr, output.stderr

  let sort_output = test.run_script(
    ctx,
    f"""
let root = p"{root}"
let entries = fs.files(root, stat: false)? |> collect()
let _ = entries |> sort-by .size
""",
  )?
  assert ! sort_output.success, sort_output.stdout
  assert "err: metadata-unavailable:" in sort_output.stderr
  assert ":4:28-4:33" in sort_output.stderr, sort_output.stderr

  let group_output = test.run_script(
    ctx,
    f"""
let root = p"{root}"
let entries = fs.files(root, stat: false)? |> collect()
let _ = entries |> group-by .size
""",
  )?
  assert ! group_output.success, group_output.stdout
  assert "err: metadata-unavailable:" in group_output.stderr
  assert ":4:29-4:34" in group_output.stderr, group_output.stderr
}
