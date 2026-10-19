test test_full_inference_defaults_and_rest_parameters_keep_body_relationships { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure fallback(value = "fallback") {
  value
}
pure append_first(...items) {
  let first = items[0]
  items.push(first)
}
let omitted = fallback()
let supplied = fallback(value: "provided")
let integers = append_first(2, 4)
let words = append_first("one", "two")
assert omitted == "fallback"
assert supplied == "provided"
assert integers == [2, 4, 2]
assert words == ["one", "two", "one"]
print $omitted
print $supplied
print $integers
print $words
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "fallback\nprovided\n[2, 4, 2]\n[one, two, one]\n"
}

test test_full_inference_default_parameter_rejects_a_conflicting_argument { |ctx|
  test.expect(
    ctx,
    r"""pure fallback(value = 3) {
  value
}
let invalid = fallback("three")
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_rest_parameter_rejects_incompatible_items { |ctx|
  test.expect(
    ctx,
    r"""pure append_first(...items) {
  let first = items[0]
  items.push(first)
}
let invalid: List[Int] = append_first(1, "two")
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_nested_row_projection_preserves_open_fields { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure file_name(row) {
  row.file.name
}
let name = file_name({file: {name: "document", size: 12}, root: "workspace"})
assert name == "document"
print $name
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "document\n"
}

test test_full_inference_nested_row_projection_rejects_a_missing_field { |ctx|
  test.expect(
    ctx,
    r"""pure file_name(row) {
  row.file.name
}
let invalid = file_name({file: {size: 12}})
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_stream_default_runs_at_first_pull_and_closes_on_stop { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc default_value() [io] -> Str {
  let flushed = io.flush_stdout()
  print "default evaluated"
  return "row"
}
stream rows(value = default_value()) [io] -> Stream[Str] {
  defer { print "stream closed" }
  yield value
  yield value
}
let producer = rows()
print "producer created"
for row in producer {
  print $row
  break
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "producer created\ndefault evaluated\nrow\nstream closed\n"
}

test test_full_inference_stream_default_effect_is_charged_at_creation { |ctx|
  test.expect(
    ctx,
    r"""proc default_value() [io] -> Str {
  let flushed = io.flush_stdout()
  return "row"
}
stream rows(value = default_value()) [io] -> Stream[Str] {
  yield value
}
proc denied() [error] -> Result[Stream[Str]] {
  Ok(rows())
}
""",
    status: 2,
    stderr: ["[check.effect-violation]", "io"],
  )?
}
