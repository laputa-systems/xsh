on USR1 [] {
  print "signal"
  abort(0)
}

let pid = process.current_pid()?
process.kill(pid, signal: "USR1")?
process.stats(pid)?
