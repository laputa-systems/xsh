##! Connect, pin, and scope cleanup for the native SSH module.

type Fixture = {
  host: Str,
  port: Int,
  username: Str,
  key: Path,
  host_key: Str,
  timeout_ms: Int,
}

proc fixture(suffix: Str) [env, error] -> Result[Fixture, Error] {
  let host = env.get(f"XINFRA_FIXTURE_{suffix}_HOST")?
  let port = env.get(f"XINFRA_FIXTURE_{suffix}_PORT")?.parse_int()?
  let username = env.get("XINFRA_FIXTURE_USER")?
  let key_text = env.get("XINFRA_FIXTURE_KEY")?
  let host_key = env.get(f"XINFRA_FIXTURE_{suffix}_HOST_KEY")?
  {
    host: host,
    port: port,
    username: username,
    key: fp"{key_text}",
    host_key: host_key,
    timeout_ms: 10000,
  }
}

test ssh_pinned_connect_succeeds {
  let fixture = match fixture("A") {
    Ok(value) => value,
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }

  let options = {
    host: fixture.host,
    port: fixture.port,
    user: fixture.username,
    key: fixture.key,
    host_key: fixture.host_key,
    timeout_ms: fixture.timeout_ms,
  }
  with session = ssh.connect(options)? {
    assert session.token != ""
  }
}

test ssh_changed_pin_is_rejected {
  let fixture = match fixture("A") {
    Ok(value) => value,
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }
  let changed = fixture.host_key.byte_slice(0, length: fixture.host_key.byte_len() - 1) + "A"
  let options = {
    host: fixture.host,
    port: fixture.port,
    user: fixture.username,
    key: fixture.key,
    host_key: changed,
    timeout_ms: fixture.timeout_ms,
  }
  match ssh.connect(options) {
    Ok(_) => assert false, "changed pin was accepted"
    Err(error) => assert "host key" in error.message
  }
}

test ssh_scope_releases_session {
  let fixture = match fixture("A") {
    Ok(value) => value,
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }
  let options = {
    host: fixture.host,
    port: fixture.port,
    user: fixture.username,
    key: fixture.key,
    host_key: fixture.host_key,
    timeout_ms: fixture.timeout_ms,
  }
  var token = ""
  with session = ssh.connect(options)? {
    token = session.token
  }
  let closed = {token: token}
  match ssh.close(closed) {
    Ok(_) => assert false, "scope did not release the session"
    Err(error) => assert "session" in error.message
  }
}
