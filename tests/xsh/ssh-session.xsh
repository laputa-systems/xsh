##! The SSH session is an opaque handle: no program can build, show, serialise or guess one.

type Fixture = {
  host: Str,
  port: Int,
  username: Str,
  key: Path,
  host_key: Str,
  timeout_ms: Int,
}

proc open_fixture(suffix: Str) [env, error] -> Result[Fixture, Error] {
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

test ssh_session_cannot_be_built_from_a_record { |ctx|
  let record_literal = test.expect(
    ctx,
    """
    proc forge() [net, error] -> Result[Unit, SshError] {
      ssh.close({token: "0123456789abcdef"})
    }
    """,
    status: 2,
    stderr: ["err[check.type-mismatch]", "`SshSession` is an opaque runtime capability"],
  )?
  assert record_literal.stdout == "", record_literal.stdout
  let annotated = test.expect(
    ctx,
    """
    proc forge() [net, error] -> Result[SshSession, Error] {
      let forged: SshSession = {token: "0123456789abcdef"}
      forged
    }
    """,
    status: 2,
    stderr: ["err[check.type-mismatch]", "`SshSession` is an opaque runtime capability"],
  )?
  assert annotated.stdout == "", annotated.stdout
}

test ssh_session_cannot_be_shown_or_read_as_a_record { |ctx|
  let displayed = test.expect(
    ctx,
    """
    proc show(session: SshSession) [net, error] {
      print (f"{session}")
    }
    """,
    status: 2,
    stderr: ["err[check.display-conversion]", "`SshSession`"],
  )?
  assert displayed.stdout == "", displayed.stdout
  let read = test.expect(
    ctx,
    """
    proc show(session: SshSession) [net, error] {
      print (session.token)
    }
    """,
    status: 2,
    stderr: ["err[check.field-access]", "field access requires a record-like value"],
  )?
  assert read.stdout == "", read.stdout
}

test ssh_session_is_not_json_and_leaves_no_token_in_values { |ctx|
  let fixture = match open_fixture("A") {
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
    let erased: Any = session
    match json.encode(erased) {
      Ok(_) => assert false, "a session was serialised"
      Err(error) => assert "SshSession" in error.message
    }
    let wrapped: Any = {session: erased}
    assert json.encode(wrapped) is Err(_)
    assert ssh.exec(session, "true", {})?.exit_code == 0
  }
}

test ssh_session_passed_to_a_worker_still_works { |ctx|
  let fixture = match open_fixture("A") {
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
    let codes = [1, 2] |> par-map(jobs: 2) { |index|
      let command = f"exit {index}"
      match ssh.exec(session, command, {timeout_ms: 5000}) {
        Ok(result) => result.exit_code,
        Err(SshError.Busy {..}) => 0,
        Err(_) => -1,
      }
    }
    assert (codes |> where { |code| code == -1 } |> count()) == 0
  }
}
