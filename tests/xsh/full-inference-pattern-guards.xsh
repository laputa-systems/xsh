test test_inferred_optional_bindings_keep_payload_requirements_in_the_present_branch { |ctx|
  let good = test.expect(ctx, r"""pure render(label) -> Str {
  if let note = label { f"{note}" } else { "absent" }
}
type Row = {label: Str?}
pure identity(value) { value }
pure render_call(label) -> Str {
  if let note = identity(label) { f"{note}" } else { "absent" }
}
pure forward(value) { render_call(value) }
pure present_or_null(value) { if let note = value { note } else { null } }
pure exists(value) -> Bool { if let _ = value { true } else { false } }
var reads = 0
proc observed(value) [] { reads += 1; value }
proc counted(value) [error] -> Result[Str] {
  if let note = observed(value)? { f"{note}" } else { "absent" }
}
pure render_row(row) -> Str {
  if let note = row.label { f"{note}" } else { "absent" }
}
let present: Str? = "typed"
let missing: Str? = null
let present_row: Row = {label: present}
let missing_row: Row = {label: missing}
assert render(null) == "absent"
assert render_call(null) == "absent"
assert render_call("once") == "once"
assert forward(null) == "absent"
assert forward("forwarded") == "forwarded"
assert present_or_null(null) == null
assert present_or_null("kept") == "kept"
assert present_or_null(9) == 9
assert !exists(null)
assert exists(7)
assert counted(null)? == "absent"
assert counted("once")? == "once"
assert reads == 2
assert render("text") == "text"
assert render(7) == "7"
assert render(present) == "typed"
assert render(missing) == "absent"
assert render_row(present_row) == "typed"
assert render_row(missing_row) == "absent"
""", status: 0)?
  assert good.stderr == "", good.stderr
  test.expect(ctx, r"""pure render(label) -> Str {
  if let note = label { f"{note}" } else { "absent" }
}
let invalid = render({value: 7})
""", status: 2, stderr: ["[check.callable-mismatch]"])?
}
