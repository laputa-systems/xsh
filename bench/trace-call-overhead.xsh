# Keep the same user-call workload for the plain runner and xsht trace modes.
# Usage: xsh bench/trace-call-overhead.xsh -- COUNT
proc increment(value: Int) [] -> Int {
  return value + 1
}

proc main(...argv: List[Str]) [io, error] {
  let count = argv[0].parse_int()?
  var value = 0
  while value < count {
    value = increment(value)
  }
  print f"value=${value}"
}
