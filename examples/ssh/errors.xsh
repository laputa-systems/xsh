##! Branch on a remote outcome, a refused host key and a timeout.
use session

let target = session.target("A")?

with connection = ssh.connect(target)? {
  let result = ssh.exec(connection, "exit 7", {})?
  print f"remote exit status {result.exit_code} is a result, not an error"
}

let wrong = {...target, host_key: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}
match ssh.connect(wrong) {
  Ok(_) => print "unexpectedly connected"
  Err(SshError.HostKeyMismatch {..}) => print "a different host key is refused before authentication"
  Err(other) => print f"unexpected failure: {other.message}"
}

with connection = ssh.connect(target)? {
  match ssh.exec(connection, "sleep 5", {timeout_ms: 300}) {
    Ok(_) => print "unexpectedly finished"
    Err(SshError.Timeout {certainty, ..}) => print f"timed out; the command may have run (certainty {certainty})"
    Err(other) => print f"unexpected failure: {other.message}"
  }
}
