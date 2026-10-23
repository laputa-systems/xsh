pure parsed_value(value: Str?) -> Int? {
  return null when value == null
  1
}
pure typed_guard_original(value: Str?, minimum: Int) -> Int? {
  let parsed = parsed_value(value)
  guard let number = parsed else { return null }
  return null when number < minimum
  parsed
}
pure inferred_guard_original(value, minimum) {
  let parsed = parsed_value(value)
  guard let number = parsed else { return null }
  return null when number < minimum
  parsed
}
pure header_guard_original(marker, value: Int?) -> Int? {
  let _ = marker
  guard let number = value else { return null }
  let _ = number
  value
}
proc verify_guard_original_slot() -> Result[Unit] {
  assert inferred_guard_original("kept", 0) == typed_guard_original("kept", 0)
  assert inferred_guard_original("kept", 0) == 1
  assert inferred_guard_original("kept", 2) == null
  assert inferred_guard_original(null, 0) == null
  assert header_guard_original("marker", 7) == 7
  assert header_guard_original("marker", null) == null
}
