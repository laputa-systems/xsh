enum Level { Info, Warn, Fault(Str) }

const release = true
const level: Level = Fault("disk full")
# begin example
let mode = if release { "release" } else { "debug" }
let label = match level {
  Info => "info",
  Warn => "warn",
  Fault(reason) => f"fault: {reason}",
}
# end example
