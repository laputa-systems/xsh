let worker = spawn run sleep 30 ?
worker.cancel(signal: "TERM", kill_after: 2s)
print "worker stopped and reaped"

let status = run.status sh -c "kill -TERM $$"

if status.signaled() {
  print f"killed by signal {status.signal_number()?}"
}

let strict = run.text sh -c "kill -TERM $$"

match strict {
  Ok(_) => print "finished"
  Err(ProcessError.Signal {..}) => print "run.text: the child died from a signal"
  Err(error) => print f"failed: {error.message}"
}

proc watch() {
  let follower = spawn run sleep 30 ?
  defer {
    print "deferred cleanup runs after the follower is reaped"
  }
  print f"returning while `{follower.argv.join(" ")}` still runs"
}

watch()
