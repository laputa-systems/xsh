enum Level { Info, Warn, Fault(Str) }

const level: Level = Warn

# begin example
if let Fault(reason) = level {
  print f"fault: {reason}"
} else {
  print "fine"
}

let urgent = level is Fault(_)
# end example
assert ! urgent
