stream stage_parity_values() [] -> Stream[Int] {
  yield 3
  yield 1
  yield 2
  yield 2
}

stream stage_parity_zero() [] -> Stream[Int] {
  yield 0
}

stream stage_parity_paths() [] -> Stream[Path] {
  yield p"aaaa"
  yield p"bbbb"
  yield p"cccc"
}

pure stage_parity_duplicate(item: Int) -> List[Int] {
  [item, item + 10]
}

test test_scalar_text_json_and_bytes_adapters_are_not_list_stream_sources {
  # These adapters consume scalar Str or Bytes values, not List/Stream sources.
  let lines = "alpha\nbeta\n" |> text.lines
  let parsed_rows = "{\"n\":1}\n{\"n\":2}\n" |> json.lines
  let chunks = b"abcdef" |> bytes.chunks(2)

  assert lines == ["alpha", "beta"]
  assert json.get(parsed_rows[0], ["n"])?.require(Int)? == 1
  assert json.get(parsed_rows[1], ["n"])?.require(Int)? == 2
  assert chunks == [b"ab", b"cd", b"ef"]
}

test test_stage_transform_outputs_match_list_and_live_stream_sources {
  let values = [3, 1, 2, 2]

  let mapped_list = values |> map . + 1 |> collect()
  let mapped_stream = stage_parity_values() |> map . + 1 |> collect()
  assert mapped_stream == mapped_list

  let mapped_block_list = values |> map { |item| item * 2 } |> collect()
  let mapped_block_stream = stage_parity_values() |> map { |item| item * 2 } |> collect()
  assert mapped_block_stream == mapped_block_list

  let where_list = values |> where . > 1 |> collect()
  let where_stream = stage_parity_values() |> where . > 1 |> collect()
  assert where_stream == where_list

  let where_block_list = values |> where { |item|
    let kept = item % 2 == 0
    kept
  } |> collect()
  let where_block_stream = stage_parity_values() |> where { |item|
    let kept = item % 2 == 0
    kept
  } |> collect()
  assert where_block_stream == where_block_list

  let flat_list = values |> flat-map(stage_parity_duplicate) |> collect()
  let flat_stream = stage_parity_values() |> flat-map(stage_parity_duplicate) |> collect()
  assert flat_stream == flat_list

  let flat_block_list = values |> flat-map { |item| [item, item + 10] } |> collect()
  let flat_block_stream = stage_parity_values() |> flat-map { |item| [item, item + 10] } |> collect()
  assert flat_block_stream == flat_block_list

  let enumerated_list = values |> enumerate |> collect()
  let enumerated_stream = stage_parity_values() |> enumerate |> collect()
  assert enumerated_stream == enumerated_list

  let zipped_list = values |> zip([10, 20, 30])
  let zipped_stream = stage_parity_values() |> zip([10, 20, 30])
  assert zipped_stream == zipped_list

  let sorted_list = values |> sort
  let sorted_stream = stage_parity_values() |> sort
  assert sorted_stream == sorted_list

  let sorted_by_list = values |> sort-by .
  let sorted_by_stream = stage_parity_values() |> sort-by .
  assert sorted_by_stream == sorted_by_list

  let shuffled_list = values |> shuffle(7)
  let shuffled_stream = stage_parity_values() |> shuffle(7)
  assert shuffled_stream == shuffled_list

  let taken_list = values |> take(2) |> collect()
  let taken_stream = stage_parity_values() |> take(2) |> collect()
  assert taken_stream == taken_list

  let dropped_list = values |> drop(2) |> collect()
  let dropped_stream = stage_parity_values() |> drop(2) |> collect()
  assert dropped_stream == dropped_list

  let repeated_list = values |> repeat(2) |> collect()
  let repeated_stream = stage_parity_values() |> repeat(2) |> collect()
  assert repeated_stream == repeated_list

  let ranged_list = [0] |> range(1, 4)
  let ranged_stream = stage_parity_zero() |> range(1, 4)
  assert ranged_stream == ranged_list
}

test test_stage_terminals_and_keyed_aggregates_match_list_and_live_stream_sources {
  let values = [3, 1, 2, 2]

  let counted_list = values |> count()
  let counted_stream = stage_parity_values() |> count()
  assert counted_stream == counted_list

  let summed_list = values |> sum
  let summed_stream = stage_parity_values() |> sum
  assert summed_stream == summed_list

  let collected_list = values |> collect()
  let collected_stream = stage_parity_values() |> collect()
  assert collected_stream == collected_list

  let first_list = values |> first()?
  let first_stream = stage_parity_values() |> first()?
  assert first_stream == first_list

  let last_list = values |> last()?
  let last_stream = stage_parity_values() |> last()?
  assert last_stream == last_list

  let minimum_list = values |> min?
  let minimum_stream = stage_parity_values() |> min?
  assert minimum_stream == minimum_list

  let maximum_list = values |> max?
  let maximum_stream = stage_parity_values() |> max?
  assert maximum_stream == maximum_list

  let folded_list = values |> fold(0) { |total, item| total + item }
  let folded_stream = stage_parity_values() |> fold(0) { |total, item| total + item }
  assert folded_stream == folded_list

  let grouped_list = values |> group-by .
  let grouped_stream = stage_parity_values() |> group-by .
  assert grouped_stream == grouped_list

  let counts_list = values |> count { |item| item % 2 }
  let counts_stream = stage_parity_values() |> count { |item| item % 2 }
  assert counts_stream == counts_list

  let unique_list = values |> unique-by .
  let unique_stream = stage_parity_values() |> unique-by .
  assert unique_stream == unique_list

  let reduced_list = values |> reduce-by(sum: true) { |item|
    {key: if item % 2 == 0 { "even" } else { "odd" }, value: item}
  }
  let reduced_stream = stage_parity_values() |> reduce-by(sum: true) { |item|
    {key: if item % 2 == 0 { "even" } else { "odd" }, value: item}
  }
  assert reduced_stream == reduced_list

  let sum_mode = true
  let configured_list = values |> reduce-by(sum: sum_mode) { |item|
    {key: "all", value: item}
  }
  let configured_stream = stage_parity_values() |> reduce-by(sum: sum_mode) { |item|
    {key: "all", value: item}
  }
  assert configured_stream == configured_list

  let any_list = values |> any . > 2
  let any_stream = stage_parity_values() |> any . > 2
  assert any_stream == any_list

  let all_list = values |> all . > 0
  let all_stream = stage_parity_values() |> all . > 0
  assert all_stream == all_list

  let any_block_list = values |> any { |item|
    let matched = item == 2
    matched
  }
  let any_block_stream = stage_parity_values() |> any { |item|
    let matched = item == 2
    matched
  }
  assert any_block_stream == any_block_list

  let all_block_list = values |> all { |item|
    let positive = item > 0
    positive
  }
  let all_block_stream = stage_parity_values() |> all { |item|
    let positive = item > 0
    positive
  }
  assert all_block_stream == all_block_list
}

test test_stage_batch_and_parallel_forms_match_list_and_live_stream_sources {
  let paths = [p"aaaa", p"bbbb", p"cccc"]

  let by_count_list = paths |> batch(count: 2)
  let by_count_stream = stage_parity_paths() |> batch(count: 2)
  assert by_count_stream == by_count_list

  let by_argv_list = paths |> batch(max_argv: true)
  let by_argv_stream = stage_parity_paths() |> batch(max_argv: true)
  assert by_argv_stream == by_argv_list

  let by_bytes_list = paths |> batch(max_bytes: 10)
  let by_bytes_stream = stage_parity_paths() |> batch(max_bytes: 10)
  assert by_bytes_stream == by_bytes_list

  let options = {count: 2, max_bytes: 10, max_argv: false}
  let by_limits_list = paths |> batch(...options)
  let by_limits_stream = stage_parity_paths() |> batch(...options)
  assert by_limits_stream == by_limits_list

  let mapped_list = [3, 1, 2, 2] |> par-map(jobs: 2) . + 1 |> collect()
  let mapped_stream = stage_parity_values() |> par-map(jobs: 2) . + 1 |> collect()
  assert mapped_stream == mapped_list

  let mapped_block_list = [3, 1, 2, 2] |> par-map(jobs: 2) { |item| item + 1 } |> collect()
  let mapped_block_stream = stage_parity_values() |> par-map(jobs: 2) { |item| item + 1 } |> collect()
  assert mapped_block_stream == mapped_block_list

  let reduced_list = [3, 1, 2, 2]
    |> par-map(jobs: 1) { |item| [item] }
    |> flat-map { |items| items }
    |> reduce-by(sum: true) { |item| {key: "all", value: item} }
  let reduced_stream = stage_parity_values()
    |> par-map(jobs: 1) { |item| [item] }
    |> flat-map { |items| items }
    |> reduce-by(sum: true) { |item| {key: "all", value: item} }
  assert reduced_stream == reduced_list

  let unflattened_list = [3, 1, 2, 2]
    |> par-map(jobs: 1) { |item| item }
    |> reduce-by(sum: true) { |item| {key: "all", value: item} }
  let unflattened_stream = stage_parity_values()
    |> par-map(jobs: 1) { |item| item }
    |> reduce-by(sum: true) { |item| {key: "all", value: item} }
  assert unflattened_stream == unflattened_list
}

test test_stage_side_effects_and_table_output_match_list_and_live_stream_sources { |ctx|
  let list_script = r"""
proc main() [io] {
  let values = [3, 1, 2]
  let _ = values |> tee { |item| print f"tee:{item}" } |> collect()
  values |> each { |item| print f"each:{item}" }
  let rows = [{name: "beta", size: 2}, {name: "alpha", size: 1}]
  rows |> table.print
  rows |> table.print(columns: ["name", "size"])
}
"""
  let stream_script = r"""
type Row = {name: Str, size: Int}
stream values() [] -> Stream[Int] { yield 3; yield 1; yield 2 }
stream rows() [] -> Stream[Row] {
  yield {name: "beta", size: 2}
  yield {name: "alpha", size: 1}
}
proc main() [io] {
  let _ = values() |> tee { |item| print f"tee:{item}" } |> collect()
  values() |> each { |item| print f"each:{item}" }
  rows() |> table.print
  rows() |> table.print(columns: ["name", "size"])
}
"""

  let list_output = test.run_script(ctx, list_script, [], {COLUMNS: "80"})?
  let stream_output = test.run_script(ctx, stream_script, [], {COLUMNS: "80"})?
  assert list_output.success, list_output.stderr
  assert stream_output.success, stream_output.stderr
  assert stream_output.stdout == list_output.stdout
}
