test test_conversion_before_operand_anchor { |ctx|
  let output = test.expect(ctx, r"""pure length(value: Bytes) -> Int { value.len() }
pure decode(value) -> Result[Str] {
  let text = value[0..1] as Str
  let _ = length(value)
  text
}
pure anchored(value) -> Result[Str] {
  let _ = length(value)
  value[0..1] as Str
}
proc main() [error] {
  assert decode(b"hello")? == "h"
  assert anchored(b"hello")? == "h"
}
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_conversion_unique_source_and_later_field_anchor { |ctx|
  let output = test.expect(ctx, r"""pure length(value: Bytes) -> Int { value.len() }
pure unique(value) -> Result[Str] { value[0..1] as Str }
pure field(record) -> Result[Str] {
  let text = record.data[0..1] as Str
  let _ = length(record.data)
  text
}
proc main() [error] {
  assert unique(b"hello")? == "h"
  assert field({data: b"word"})? == "w"
}
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_conversion_ambiguous_target_uses_later_anchor { |ctx|
  let output = test.expect(ctx, r"""pure text_length(value: Str) -> Int { value.byte_len() }
pure byte_length(value: Bytes) -> Int { value.len() }
pure from_text(value) -> Result[Path] {
  let resolved = value as Path
  let _ = text_length(value)
  resolved
}
pure from_bytes(value) -> Result[Path] {
  let resolved = value as Path
  let _ = byte_length(value)
  resolved
}
proc main() [error] {
  assert from_text("name")? == p"name"
  assert from_bytes(b"name")? == p"name"
}
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_conversion_rejects_unresolved_invalid_and_dynamic_sources { |ctx|
  for source in [
    "pure bad(value: Str) -> Result[Str] { value as Str }",
    "pure bad(value: Any) -> Result[Str] { value as Str }",
  ] {
    let output = test.run_xsh(ctx, source)?
    assert ! output.success, source
    assert "err[check.conversion]" in output.stderr, output.stderr
  }
  let wrong_receiver = test.run_xsh(ctx, r"""pure unique(value) -> Result[Str] { value[0..1] as Str }
let _ = unique("hello")
""")?
  assert ! wrong_receiver.success, wrong_receiver.stderr
  assert "err[check." in wrong_receiver.stderr, wrong_receiver.stderr
}

test test_conversion_keeps_its_error_boundary { |ctx|
  let output = test.expect(ctx, r"""pure length(value: Bytes) -> Int { value.len() }
pure captured(value) -> Str {
  let text = try { value[0..1] as Str }
  let _ = length(value)
  text ?? "invalid"
}
proc decoded(value) {
  let text = value[0..1] as Str
  let _ = length(value)
  text
}
proc main() [error] {
  assert captured(b"hello") == "h"
  assert captured(b"\xff") == "invalid"
  match decoded(b"\xff") { Ok(_) => assert false; Err(_) => assert true }
}
""", status: 0)?
  assert output.stderr == "", output.stderr
}

test test_conversion_keeps_its_declared_effect_boundary { |ctx|
  let output = test.run_xsh(ctx, r"""pure length(value: Bytes) -> Int { value.len() }
proc denied(value) [] { let _ = value as Str; let _ = length(value) }
""")?
  assert ! output.success, output.stderr
  assert "requires the `error` effect" in output.stderr, output.stderr
  assert "`as` fails the enclosing function" in output.stderr, output.stderr
}

test test_conversion_keeps_guarded_operand_scope { |ctx|
  let output = test.expect(ctx, r"""type Sample = {data: Bytes?}
pure length(value: Bytes) -> Int { value.len() }
pure field(row) -> Result[Str] {
  if row.data != null {
    let text = row.data[0..1] as Str
    let _ = length(row.data)
    return text
  }
  "none"
}
proc main() [error] {
  let present: Sample = {data: b"abc"}
  let absent: Sample = {data: null}
  assert field(present)? == "a"
  assert field(absent)? == "none"
}
""", status: 0)?
  assert output.stderr == "", output.stderr
}
