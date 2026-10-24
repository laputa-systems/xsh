type GuardCounts = {bytes: UInt}
type DiagnosticRow = {diagnostic: Str?, other: Str?, flush: Bool, counts: GuardCounts}
type DiagnosticEnvelope = {value: DiagnosticRow}
pure typed_record_guard(marker: Int, row: DiagnosticRow) -> Bool {
  let _ = marker
  if let diagnostic = row.diagnostic {
    let _ = diagnostic
    row.flush
  } else { false }
}
pure record_guard(marker, row: DiagnosticRow, other: DiagnosticRow) -> Bool {
  let _ = other
  let _ = marker
  if let diagnostic = row.diagnostic {
    let _ = diagnostic
    row.flush
  } else { false }
}
pure local_record_guard(marker, row: DiagnosticRow) -> Bool {
  let _ = marker
  let result = row
  if let diagnostic = result.diagnostic {
    let _ = diagnostic
    result.flush
  } else { false }
}
pure nested_record_guard(marker, envelope: DiagnosticEnvelope) -> Bool {
  let _ = marker
  if let diagnostic = envelope.value.diagnostic {
    let _ = diagnostic
    envelope.value.flush
  } else { false }
}
pure record_flush(row: DiagnosticRow) -> Bool { row.flush }
pure record_argument_guard(marker, row: DiagnosticRow) -> Bool {
  let _ = marker
  if let diagnostic = row.diagnostic {
    let _ = diagnostic
    record_flush(row)
  } else { !record_flush(row) }
}
pure written_record_guard(marker, row: DiagnosticRow) -> Bool {
  let _ = marker
  var working = row
  if let diagnostic = working.diagnostic {
    let _ = diagnostic
    let before = working.flush
    working.diagnostic = null
    before == working.flush
  } else { false }
}
pure stale_record_guard(marker, row: DiagnosticRow) -> Bool {
  let _ = marker
  var working = row
  if let before = working.diagnostic { let _ = before; let _ = working.flush }
  let saved = working.diagnostic
  working.diagnostic = row.other
  if let diagnostic = saved { let _ = diagnostic; working.flush } else { false }
}
pure stale_block_record_guard(marker, row: DiagnosticRow) -> Bool {
  let _ = marker
  var working = row
  if let before = working.diagnostic { let _ = before; let _ = working.flush }
  if let diagnostic = { let saved = working.diagnostic; working.diagnostic = row.other; saved } {
    let _ = diagnostic
    working.flush
  } else { false }
}
proc verify_record_field_guards() -> Result[Unit] {
  let present: DiagnosticRow = {diagnostic: "kept", other: null, flush: true, counts: {bytes: 1KiB}}
  let absent: DiagnosticRow = {diagnostic: null, other: "other", flush: true, counts: {bytes: 1KiB}}
  assert typed_record_guard(7, present)
  assert !typed_record_guard(7, absent)
  assert record_guard(7, present, absent)
  assert record_guard("independent", present, absent)
  assert !record_guard(7, absent, present)
  assert local_record_guard("local", present)
  assert !local_record_guard("local", absent)
  assert nested_record_guard(7, {value: present})
  assert !nested_record_guard(7, {value: absent})
  assert record_argument_guard(7, present)
  assert !record_argument_guard(7, absent)
  assert written_record_guard(7, present)
  assert !written_record_guard(7, absent)
  assert stale_record_guard(7, present)
  assert !stale_record_guard(7, absent)
  assert stale_block_record_guard(7, present)
  assert !stale_block_record_guard(7, absent)
}
