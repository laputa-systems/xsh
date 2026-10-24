pure guarded_record_continuation(marker, row: DiagnosticRow, other: DiagnosticRow) -> Bool {
  let _ = other
  let _ = marker
  guard let diagnostic = row.diagnostic else { return false }
  let _ = diagnostic
  row.flush
}
pure local_guarded_record_continuation(marker, row: DiagnosticRow) -> Bool {
  let _ = marker
  let selected = row
  guard let diagnostic = selected.diagnostic else { return false }
  let _ = diagnostic
  selected.flush
}
pure nested_guarded_record_continuation(marker, envelope: DiagnosticEnvelope) -> Bool {
  let _ = marker
  guard let diagnostic = envelope.value.diagnostic else { return false }
  let _ = diagnostic
  envelope.value.flush
}
pure handler_record_continuation(marker, row: DiagnosticRow) -> Bool {
  let _ = marker
  guard let diagnostic = row.diagnostic else { return row.flush }
  let _ = diagnostic
  row.flush
}
pure written_record_continuation(marker, row: DiagnosticRow) -> Bool {
  let _ = marker
  var working = row
  guard let diagnostic = working.diagnostic else { return false }
  let _ = diagnostic
  let before = working.flush
  working.diagnostic = row.other
  before == working.flush
}
pure stale_record_continuation(marker, row: DiagnosticRow) -> Bool {
  let _ = marker
  var working = row
  if let before = working.diagnostic { let _ = before; let _ = working.flush }
  let saved = working.diagnostic
  working.diagnostic = row.other
  guard let diagnostic = saved else { return false }
  let _ = diagnostic
  working.flush
}

proc verify_record_guard_continuations() -> Result[Unit] {
  let present: DiagnosticRow = {diagnostic: "kept", other: null, flush: true, counts: {bytes: 1KiB}}
  let absent: DiagnosticRow = {diagnostic: null, other: "other", flush: true, counts: {bytes: 1KiB}}
  assert guarded_record_continuation(7, present, absent)
  assert guarded_record_continuation("independent", present, absent)
  assert !guarded_record_continuation(7, absent, present)
  assert local_guarded_record_continuation(7, present)
  assert !local_guarded_record_continuation(7, absent)
  assert nested_guarded_record_continuation(7, {value: present})
  assert !nested_guarded_record_continuation(7, {value: absent})
  assert handler_record_continuation(7, present)
  assert handler_record_continuation(7, absent)
  assert written_record_continuation(7, present)
  assert !written_record_continuation(7, absent)
  assert stale_record_continuation(7, present)
  assert !stale_record_continuation(7, absent)
}
