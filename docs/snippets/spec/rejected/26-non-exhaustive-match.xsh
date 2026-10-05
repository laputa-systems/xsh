enum Level { Info, Warn, Fault(Str) }

error FetchError {
    Usage
    Offline : Timeout
    Rejected(url: Str, status: Int)
}

const level: Level = Warn
let failure: FetchError = FetchError.Offline()
# begin example
match level {  # error: check.non-exhaustive-match
  Info => print "info"
  Fault("disk") => print "disk fault"
}

match level {
  Info => print "info"
  else => {}
}

match failure {  # error: check.non-exhaustive-match
  FetchError.Usage {message} => print $message
  is Timeout => print "offline"
}
# end example
