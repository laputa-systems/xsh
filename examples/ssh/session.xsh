##! Shared connection options for the ssh examples, read from the fixture environment.
## Connection options for one fixture server.
export type Target = {
  host: Str,
  port: Int,
  user: Str,
  key: Path,
  host_key: Str,
  timeout_ms: Int,
}

## Reads the connection options of fixture server `suffix` (A or B).
export proc target(suffix: Str) [env, error] -> Result[Target, Error] {
  {
    host: env.get(f"XINFRA_FIXTURE_{suffix}_HOST")?,
    port: env.get(f"XINFRA_FIXTURE_{suffix}_PORT")?.parse_int()?,
    user: env.get("XINFRA_FIXTURE_USER")?,
    key: fp"{env.get("XINFRA_FIXTURE_KEY")?}",
    host_key: env.get(f"XINFRA_FIXTURE_{suffix}_HOST_KEY")?,
    timeout_ms: 10000,
  }
}
