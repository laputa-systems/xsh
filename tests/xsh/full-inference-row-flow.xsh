test test_full_inference_optional_join_instantiates_int_and_str_independently { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure maybe(flag, value) {
  if flag { value } else { null }
}
let present_integer = maybe(true, 42) ?? 0
let absent_integer = maybe(false, 42) ?? -1
let present_text = maybe(true, "ready") ?? "missing"
let absent_text = maybe(false, "ready") ?? "missing"
assert present_integer == 42
assert absent_integer == -1
assert present_text == "ready"
assert absent_text == "missing"
print f"{present_integer} {absent_integer} {present_text} {absent_text}"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42 -1 ready missing\n"
}

test test_full_inference_mutable_row_keeps_disjoint_refinement_after_write { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type SelectionRow = {selected: Str?, label: Str, unchanged: Int}
pure update_label(source) {
  var current = source
  if current.selected != null {
    current.label = "changed"
    let selected: Str = current.selected
    {selected: selected, label: current.label, unchanged: current.unchanged}
  } else {
    {selected: "missing", label: current.label, unchanged: current.unchanged}
  }
}
let present: SelectionRow = {selected: "ready", label: "before", unchanged: 7}
let absent: SelectionRow = {selected: null, label: "empty", unchanged: 9}
let updated_present = update_label(present)
let updated_absent = update_label(absent)
assert updated_present == {selected: "ready", label: "changed", unchanged: 7}
assert updated_absent == {selected: "missing", label: "empty", unchanged: 9}
print f"{updated_present.selected} {updated_present.label} {updated_present.unchanged}"
print f"{updated_absent.selected} {updated_absent.label} {updated_absent.unchanged}"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "ready changed 7\nmissing empty 9\n"
}

test test_full_inference_overlapping_row_write_invalidates_null_refinement { |ctx|
  test.expect(
    ctx,
    r"""type SelectionRow = {selected: Str?, label: Str}
pure read_after_write(source) {
  var current = source
  if current.selected != null {
    current.selected = null
    let invalid: Str = current.selected
    invalid
  } else {
    "missing"
  }
}
let row: SelectionRow = {selected: "ready", label: "before"}
let invalid = read_after_write(row)
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_whole_row_write_invalidates_null_refinement { |ctx|
  test.expect(
    ctx,
    r"""type SelectionRow = {selected: Str?, label: Str}
pure read_after_replace(source) {
  var current = source
  if current.selected != null {
    current = {selected: null, label: "replacement"}
    let invalid: Str = current.selected
    invalid
  } else {
    "missing"
  }
}
let row: SelectionRow = {selected: "ready", label: "before"}
let invalid = read_after_replace(row)
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}
