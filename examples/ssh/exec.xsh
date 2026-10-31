##! Run one command over the pinned fixture host.

let host = env.get("XINFRA_FIXTURE_A_HOST")?
let port = env.get("XINFRA_FIXTURE_A_PORT")?.parse_int()?
let username = env.get("XINFRA_FIXTURE_USER")?
let key = fp"{env.get("XINFRA_FIXTURE_KEY")?}"
let host_key = env.get("XINFRA_FIXTURE_A_HOST_KEY")?

with session = ssh.connect({
  host: host,
  port: port,
  user: username,
  key: key,
  host_key: host_key,
  timeout_ms: 10000,
})? {
  let result = ssh.exec(session, "printf 'hello from ssh'", {})?
  print f"{result.exit_code} {result.stdout.utf8()?}"
}
