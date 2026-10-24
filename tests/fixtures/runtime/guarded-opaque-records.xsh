type GuardedOpaqueRow = {name: Str, opaque: Any}

pure guarded_name(marker, row: GuardedOpaqueRow?) -> Str {
  let _ = marker
  if row == null { "absent" } else { row.name }
}

proc verify_guarded_opaque_records() -> Result[Unit] {
  let opaque: Any = {hidden: true}
  let row: GuardedOpaqueRow = {name: "kept", opaque: opaque}
  let present: GuardedOpaqueRow? = row
  let absent: GuardedOpaqueRow? = null
  assert guarded_name(0, row) == "kept"
  assert guarded_name("marker", present) == "kept"
  assert guarded_name(false, absent) == "absent"
  Ok()
}
