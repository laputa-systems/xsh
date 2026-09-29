pure observed(value: Int) -> Int {
  return value + 1
}

pure repeated_call(value: Int) -> Bool {
  return 0 < observed(value) and observed(value) < 10
}

pure mutable_read(value: Int) -> Bool {
  var middle = value
  return 0 < middle and middle < 10
}

pure preserve_hash_literal(value: Str) -> Bool {
  let middle = value
  return "#" < middle and middle < "z"
}
