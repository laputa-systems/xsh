# begin example
enum Level { Info, Warn, Fault(Str) }

let level = Fault("disk full")

match level {
  Info => print "info"
  Warn => print "warn"
  Fault(reason) => print f"fault: {reason}"
}

# end example
let typed: Level = level
