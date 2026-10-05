enum Level { Info, Warn, Fault(Str) }

const level: Level = Warn
const quiet = true
# begin example
match level {
  Info => print "info"
  else if quiet => print "quiet"  # error: parse.match-else-arm
}

match level {
  else => print "other"
  Info => print "info"  # error: parse.match-else-arm
}
# end example
