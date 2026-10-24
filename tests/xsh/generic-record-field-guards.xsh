test generic_record_field_guards { |ctx|
  let source = p"tests/fixtures/runtime/generic-record-field-guards.xsh".read_text()?
  test.expect(ctx, source + "\nverify_record_field_guards()?\n", status: 0)?
}

test generic_record_guard_continuations { |ctx|
  let source = p"tests/fixtures/runtime/generic-record-field-guards.xsh".read_text()? + "\n" + p"tests/fixtures/runtime/generic-record-guard-continuations.xsh".read_text()?
  test.expect(ctx, source + "\nverify_record_guard_continuations()?\n", status: 0)?
}
