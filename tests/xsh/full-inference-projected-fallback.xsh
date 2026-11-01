test test_guarded_record_field_passes_to_inferred_fallback { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Settings = {write_out: Str?, enabled: Bool}
pure finish(settings) { if settings.enabled { settings.write_out ?? "" } else { "disabled" } }
pure present_length(settings) { settings.write_out.byte_len() }
pure caller(settings: Settings) -> Str {
  if settings.write_out != null {
    if present_length(settings) == settings.write_out.byte_len() { finish(settings) } else { "bad" }
  } else { "empty" }
}
let present: Settings = {write_out: "yes", enabled: true}
let missing: Settings = {write_out: null, enabled: false}
assert caller(present) == "yes"
assert caller(missing) == "empty"
print caller(present)
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "yes\n"
}

test test_guarded_fallback_keeps_invariant_record_fields { |ctx|
  test.expect(
    ctx,
    r"""type Settings = {write_out: Str?, items: List[Str?]}
pure consume(items: List[Str]) { items.len() }
pure finish(settings) { let _ = consume(settings.items); settings.write_out ?? "" }
pure caller(settings: Settings) -> Str {
  if settings.write_out != null { finish(settings) } else { "empty" }
}
let settings: Settings = {write_out: "yes", items: [null]}
let _ = caller(settings)
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_guarded_fallback_rejects_dynamic_record_field { |ctx|
  test.expect(
    ctx,
    r"""type Settings = {write_out: Any, enabled: Bool}
pure finish(settings) { settings.write_out ?? "" }
pure caller(settings: Settings) -> Str {
  if settings.write_out != null { finish(settings) } else { "empty" }
}
let settings: Settings = {write_out: "yes", enabled: true}
let _ = caller(settings)
""",
    status: 2,
    stderr: ["[check.result-fallback]"],
  )?
}

test test_fallback_field_cannot_also_require_a_present_value { |ctx|
  test.expect(
    ctx,
    r"""type Settings = {write_out: Str?, enabled: Bool}
pure finish(settings) { let _ = settings.write_out.trim(); settings.write_out ?? "" }
pure caller(settings: Settings) -> Str {
  if settings.write_out != null { finish(settings) } else { "empty" }
}
let settings: Settings = {write_out: "yes", enabled: true}
let _ = caller(settings)
""",
    status: 2,
    stderr: ["[check.callable-mismatch]", "trim has no canonical result for Str?"],
  )?
}
