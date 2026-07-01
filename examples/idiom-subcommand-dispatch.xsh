error DispatchError = Unknown(message: Str)

proc handle(mode: Str) [error] {
  match mode {
    "quick" => print "quick"
    "verbose" => print "verbose"
    _ => return Err(DispatchError.Unknown(message: f"unknown: ${mode}"))
  }
}

handle("quick")?
handle("verbose")?

match handle("unknown") {
  Err(e) => print f"error: ${e.message}"
  Ok(_) => {}
}
