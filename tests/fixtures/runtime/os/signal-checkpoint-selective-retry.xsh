let helper = fp"${ARGV[0]}"
error FetchError = Busy(message: Str)

on USR1 [] {
  print "hook"
  abort(0)
}

proc attempt() -> Result[Str, FetchError] {
  print "attempt"
  Err(FetchError.Busy(message: "busy"))
}

let _sender = process.spawn(process.command_argv(helper, ["os-probe", "signal-parent-after", "USR1", "50"]))?
let result = retry [5s] on (FetchError.Busy) {
  defer { print "cleanup" }
  attempt()?
}
print "after"
