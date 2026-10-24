pure guarded_add_output(value) {
  if value != null { value + 1 } else { null }
}

pure guarded_parent_output(value) {
  if value == null { null } else { value.parent() }
}

proc verify_guarded_outputs() [error] -> Result[Unit, Error] {
  let present: Int? = 7
  let missing: Int? = null
  let exact: Int = guarded_add_output(4)
  let absent: Null = guarded_add_output(null)
  let selected: Int? = guarded_add_output(present)
  let skipped: Int? = guarded_add_output(missing)
  assert exact == 5
  assert absent == null
  assert selected == 8
  assert skipped == null
  let present_path: Path? = p"/guarded/leaf"
  let missing_path: Path? = null
  let exact_parent: Path = guarded_parent_output(p"/plain/leaf")
  let absent_parent: Null = guarded_parent_output(null)
  let selected_parent: Path? = guarded_parent_output(present_path)
  let skipped_parent: Path? = guarded_parent_output(missing_path)
  let relative: RelPath = p"relative/leaf"
  let relative_parent: RelPath = guarded_parent_output(relative)
  assert exact_parent == p"/plain"
  assert absent_parent == null
  assert selected_parent == p"/guarded"
  assert skipped_parent == null
  assert relative_parent == p"relative"
  Ok()
}
