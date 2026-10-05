enum Level { Info, Warn, Fault(Str) }

const level: Level = Warn
# begin example
match level {  # error: check.non-exhaustive-match
  Info => print "info"
  Fault("disk") => print "disk fault"
}

match level {
  Info => print "info"
  else => {}
}
# end example
