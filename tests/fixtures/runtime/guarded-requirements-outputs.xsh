pure guarded_add_output(value) {
  if value != null { value + 1 } else { null }
}

pure guarded_parent_output(value) {
  if value == null { null } else { value.parent() }
}

pure guarded_bound_output(value) {
  let result = if value != null { value + 1 } else { null }
  result
}

type GuardedOutputRow = {name: Str, opaque: Any}

pure guarded_name_output(value) {
  if value != null { value.name } else { null }
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
  let bound: Int = guarded_bound_output(4)
  let absent_bound: Null = guarded_bound_output(null)
  let optional_bound: Int? = guarded_bound_output(present)
  assert bound == 5
  assert absent_bound == null
  assert optional_bound == 8
  let row: GuardedOutputRow = {name: "present", opaque: {hidden: [1, 2]}}
  let optional_row: GuardedOutputRow? = row
  let missing_row: GuardedOutputRow? = null
  let name: Str = guarded_name_output(row)
  let absent_name: Null = guarded_name_output(null)
  let selected_name: Str? = guarded_name_output(optional_row)
  let missing_name: Str? = guarded_name_output(missing_row)
  assert name == "present"
  assert absent_name == null
  assert selected_name == "present"
  assert missing_name == null
  Ok()
}
