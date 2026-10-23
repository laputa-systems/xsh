error Sample = Missing
type Selection = {label: Str, found: Int}

pure typed_selection(value: Result[Int, Sample], label: Str) -> Selection {
  if let Ok(found) = value { return {label, found} }
  {label, found: 0}
}

pure generic_selection(value: Result[Int, Sample], label) {
  if let Ok(found) = value { return {label, found} }
  {label, found: 0}
}

pure inferred_selection(value, label) {
  if let Ok(found) = value { return {label, found} }
  {label, found: 0}
}

pure shadowed_selection(value: Result[Int, Sample], label) {
  let found = if let Ok(value) = value { value + 1 } else { 0 }
  {label, found}
}

pure optional_selection(value: Int?, label) {
  if let found = value { return {label, found} }
  {label, found: 0}
}

pure expression_selection(value: Result[Int, Sample], label) {
  let found = if let Ok(after) = value { after + 1 } else { 0 }
  {label, found}
}

pure tail_selection(value: Result[Int, Sample], label) {
  if let Ok(found) = value { {label, found} } else { {label, found: 0} }
}

proc procedure_selection(value: Result[Int, Sample], label) [] {
  if let Ok(found) = value { return {label, found} }
  {label, found: 0}
}

test generic_pattern_conditions_preserve_success_and_failure_scopes [error] {
  let success: Result[Int, Sample] = Ok(7)
  let failure: Result[Int, Sample] = Err(Sample.Missing())
  assert typed_selection(success, "kept").found == 7
  assert typed_selection(failure, "kept").found == 0
  assert generic_selection(success, "kept") == {label: "kept", found: 7}
  assert generic_selection(failure, 11) == {label: 11, found: 0}
  assert inferred_selection(success, "kept").found == 7
  assert inferred_selection(failure, 11).found == 0
  assert shadowed_selection(success, "kept").found == 8
  assert shadowed_selection(failure, 11).found == 0
  assert optional_selection(7, "kept").found == 7
  assert optional_selection(null, 11).found == 0
  assert expression_selection(success, "kept").found == 8
  assert expression_selection(failure, 11).found == 0
  assert tail_selection(success, "kept").found == 7
  assert tail_selection(failure, 11).found == 0
  assert procedure_selection(success, "kept") == Ok({label: "kept", found: 7})
  assert procedure_selection(failure, 11) == Ok({label: 11, found: 0})
}
