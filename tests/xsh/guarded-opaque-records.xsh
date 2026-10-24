test guarded_opaque_records_preserve_typed_fields { |ctx|
  let source = p"tests/fixtures/runtime/guarded-opaque-records.xsh".read_text()?
  test.expect(ctx, source + "\nverify_guarded_opaque_records()?\n", status: 0)?
  let rejected = test.run_xsh(ctx, source + r"""
pure rejected_opaque(marker, row: GuardedOpaqueRow?) -> Int {
  let _ = marker
  if row == null { 0 } else { row.opaque + 1 }
}
let _ = rejected_opaque(0, {name: "kept", opaque: 7})
""")?
  assert rejected.status == 2
  assert "check.dynamic-boundary" in rejected.stderr
}
