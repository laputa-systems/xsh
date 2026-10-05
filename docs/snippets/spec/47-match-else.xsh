enum Level { Info, Warn, Fault(Str) }

const level: Level = Warn

# begin example
match level {
  Fault(reason) => print f"fault: {reason}"
  else => print "fine"
}

let urgent = match level {
  Fault(_) => true,
  else => false,
}
# end example
assert ! urgent
