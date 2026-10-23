pure optional_choice(absent, value, marker) {
  if absent { return null }
  value
}

pure optional_other(absent, value, marker) {
  if absent { return null }
  value
}

test generic_optional_returns_preserve_payloads_and_absence {
  let number: Int? = optional_choice(false, 7, "marker")
  let word: Str? = optional_choice(false, "kept", 9)
  let absent: Int? = optional_choice(true, 7, "marker")
  assert number == 7
  assert word == "kept"
  assert absent == null
  let error_payload: Result[Int, Str] = Err("kept error")
  let present_error: Result[Int, Str]? = optional_choice(false, error_payload, true)
  let absent_error: Result[Int, Str]? = optional_choice(true, error_payload, false)
  let present_default: Result[Int, Str] = Ok(0)
  let absent_default: Result[Int, Str] = Ok(5)
  let preserved: Result[Int, Str] = present_error ?? present_default
  let fallback: Result[Int, Str] = absent_error ?? absent_default
  assert match preserved { Err(problem) => problem == "kept error"; Ok(_) => false }
  assert fallback == absent_default
  let other: Str? = optional_other(false, "other", 3)
  assert other == "other"
}
