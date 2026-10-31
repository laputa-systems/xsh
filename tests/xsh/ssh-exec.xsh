##! Exec outcomes through the native SSH module.

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

test ssh_exec_reports_status_and_output {
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
    let result = ssh.exec(session, "printf 'out'; printf 'err' >&2; exit 7", {})?
    assert result.exit_code == 7
    assert result.stdout == b"out"
    assert result.stderr == b"err"
    assert result.missing == false
    assert result.certainty == "submitted"
  }
}

test ssh_exec_reports_signals {
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
    let result = ssh.exec(session, "kill -TERM $$", {})?
    assert result.signal == "TERM"
    assert result.exit_code == -1
    assert result.missing == false
  }
}

test ssh_exec_timeout_carries_certainty {
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
    match ssh.exec(session, "sleep 5", {timeout_ms: 200}) {
      Ok(_) => assert false, "slow command did not time out"
      Err(error) => assert "timed out" in error.message
    }
  }
}

test ssh_exec_rejects_concurrent_use {
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
    let results = [1, 2] |> par-map(jobs: 2) { |index|
      let command = if index == 1 { "sleep 1" } else { "true" }
      match ssh.exec(session, command, {timeout_ms: 5000}) {
        Ok(_) => false,
        Err(_) => true,
      }
    }
    let rejected = results |> where { |value| value } |> count()
    assert rejected >= 1, "concurrent use was not rejected"
  }
}
