test test_full_inference_byte_at_requirement_discharges_for_str_and_bytes { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure first_byte(value) {
  value.byte_at(0) ?? -1
}
let text_byte = first_byte("é")
let binary_byte = first_byte(b"\xff")
assert text_byte == 195
assert binary_byte == 255
print $text_byte
print $binary_byte
""",
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == "195\n255\n"
}

test test_full_inference_external_any_requires_a_reifiable_row_target { |ctx|
  let validated = test.run_xsh(
    ctx,
    r"""type Metadata = {name: Str}
type NamedDocument = {metadata: Metadata}
pure document_name(row) { row.metadata.name }
let opaque: Any = {metadata: {name: "draft", extra: 17}, retained: true}
let checked: NamedDocument = opaque.require(NamedDocument)?
let name = document_name(checked)
assert name == "draft"
print $name
""",
  )?
  assert validated.success, validated.stderr
  assert validated.stdout == "draft\n"

  test.expect(
    ctx,
    r"""type Metadata = {name: Str}
type NamedDocument = {metadata: Metadata}
pure document_name(row) { row.metadata.name }
let opaque: Any = {metadata: {name: "draft", extra: 17}, retained: true}
let name = document_name(opaque)
""",
    status: 2,
    stderr: ["[check.field-access]", "is not defined for Any"],
  )?
}

test test_full_inference_recursive_callback_effects_propagate_through_an_scc { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc report(value: Str) [io] -> Result[Str] {
  print f"callback:{value}"
  Ok(value)
}
proc descend(callback, remaining) {
  if remaining == 0 {
    callback("complete")
  } else {
    ascend(callback, remaining - 1)
  }
}
proc ascend(callback, remaining) {
  if remaining == 0 {
    callback("complete")
  } else {
    descend(callback, remaining - 1)
  }
}
let completed = descend(report, 3)?
assert completed == "complete"
print $completed
""",
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == "callback:complete\ncomplete\n"
}

test test_full_inference_recursive_callback_effect_cannot_escape_an_empty_bound { |ctx|
  test.expect(
    ctx,
    r"""proc report(value: Str) [io] -> Result[Str] {
  print f"callback:{value}"
  Ok(value)
}
proc descend(callback, remaining) {
  if remaining == 0 {
    callback("complete")
  } else {
    ascend(callback, remaining - 1)
  }
}
proc ascend(callback, remaining) {
  if remaining == 0 {
    callback("complete")
  } else {
    descend(callback, remaining - 1)
  }
}
proc denied(callback) [] {
  let _ = descend(callback, 3)
}
let _ = denied(report)
""",
    status: 2,
    stderr: ["[check.local-inference]"],
  )?
}
