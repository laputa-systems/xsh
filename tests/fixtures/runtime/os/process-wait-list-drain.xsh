let helper = fp"${ARGV[0]}"
let slow_ready = fp"${ARGV[1]}"
let fast_marker = fp"${ARGV[2]}"
let dup_marker = fp"${ARGV[3]}"
let slow_command = process.command_argv(helper, ["os-probe", "ready-sleep", slow_ready.display()], timeout: 50ms)
let fast_command = process.command_argv(helper, ["os-probe", "delayed-marker", fast_marker.display(), "10"])
let slow = spawn slow_command?
let fast = spawn fast_command?

match wait [slow, fast] {
  Err(ProcessError.Timeout {message: message}) => print ${"timed out" in message} fast_marker.exists()?
  Err(e) => print ${e.kind}
  Ok(_) => print "ok"
}

match wait fast {
  Err(ProcessError.Unknown {message: message}) => print ${"no longer live" in message}
  Err(e) => print ${e.kind}
  Ok(_) => print "ok"
}

let dup = spawn process.command_argv(helper, ["os-probe", "delayed-marker", dup_marker.display(), "10"])?

match wait [dup, dup] {
  Err(ProcessError.Unknown {message: message}) => print ${"already requested" in message} dup_marker.exists()?
  Err(e) => print ${e.kind}
  Ok(_) => print "ok"
}

match wait dup {
  Err(ProcessError.Unknown {message: message}) => print ${"no longer live" in message}
  Err(e) => print ${e.kind}
  Ok(_) => print "ok"
}
