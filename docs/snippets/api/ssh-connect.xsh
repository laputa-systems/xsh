##! Native SSH session example.

let options = {
  host: "example.invalid",
  port: 22,
  user: "root",
  key: p"/etc/xsh/id_ed25519",
  host_key: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
  timeout_ms: 10000,
}

with session = ssh.connect(options)? {
  let result = ssh.exec(session, "printf 'hello'", {})?
  print f"{result.exit_code} {result.stdout.utf8()?}"
}
