let slow = run.capture --text --timeout=200ms sleep 5

match slow {
  Ok(out) => print f"finished: {out.status.ok}"
  Err(ProcessError.Timeout {..}) => print "timed out; process group killed"
  Err(error) => print f"failed: {error.message}"
}
