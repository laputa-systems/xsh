type GuardedNamedRow = {name: Str, opaque: Any}

pure guarded_add(value) -> Int {
  if value == null { 0 } else { value + 1 }
}

pure guarded_parent(value) -> Path {
  if value == null { p"/absent" } else { value.parent() }
}

pure guarded_name(value) -> Str {
  if value == null { "absent" } else { value.name }
}

proc verify_guarded_local_reads() -> Result[Unit] {
  let present_number: Int? = 7
  let missing_number: Int? = null
  assert guarded_add(4) == 5
  assert guarded_add(present_number) == 8
  assert guarded_add(missing_number) == 0
  assert guarded_add(null) == 0
  let present_path: Path? = p"/guarded/child"
  let missing_path: Path? = null
  assert guarded_parent(p"/plain/child") == p"/plain"
  assert guarded_parent(present_path) == p"/guarded"
  assert guarded_parent(missing_path) == p"/absent"
  assert guarded_parent(null) == p"/absent"
  let opaque: Any = {hidden: true}
  let row: GuardedNamedRow = {name: "kept", opaque: opaque}
  let present_row: GuardedNamedRow? = row
  let missing_row: GuardedNamedRow? = null
  assert guarded_name(row) == "kept"
  assert guarded_name(present_row) == "kept"
  assert guarded_name(missing_row) == "absent"
  assert guarded_name(null) == "absent"
  Ok()
}
