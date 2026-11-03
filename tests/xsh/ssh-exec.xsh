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
      Err(SshError.Timeout {certainty, ..}) => assert certainty == "submitted"
      Err(other) => assert false, other.message
    }
    assert ssh.exec(session, "true", {}) is Err(SshError.Unusable {..})
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
        Err(SshError.Busy {..}) => true,
        Err(_) => false,
      }
    }
    let rejected = results |> where { |value| value } |> count()
    assert rejected >= 1, "concurrent use was not rejected"
  }
}

test ssh_exec_moves_stdin_larger_than_the_window {
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
  let payload = bytes.zero(3 * 1024 * 1024)?
  with session = ssh.connect(options)? {
    let result = ssh.exec(session, "cat", {stdin: payload, timeout_ms: 60000})?
    assert result.exit_code == 0
    assert result.stdout == payload
  }
}

test ssh_session_serves_successive_commands {
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
    for round in [0, 1, 2, 3] {
      let result = ssh.exec(session, f"printf round-{round}; exit {round}", {})?
      assert result.exit_code == round
      assert result.stdout.utf8()? == f"round-{round}"
    }
  }
}

test ssh_exec_output_past_the_limit_is_an_outcome_error {
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
    match ssh.exec(session, "head -c 4096 /dev/zero", {max_output: 1024}) {
      Ok(_) => assert false, "output limit was not enforced"
      Err(SshError.OutputLimit {certainty, ..}) => assert certainty == "submitted"
      Err(other) => assert false, other.message
    }
  }
}

test ssh_exec_file_options_leave_the_capture_limit_on_every_other_stream { |ctx|
  let root = test.temp_dir(ctx, name: "limit")?
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
    let flood = "head -c 3000000 /dev/zero"
    let input = {max_output: 1024, stdin_path: p"/dev/null"}
    assert ssh.exec(session, flood, input) is Err(SshError.OutputLimit {..})
  }
  with session = ssh.connect(options)? {
    let to_file = {max_output: 1024, stdout_path: fp"{root}/out"}
    let result = ssh.exec(session, "head -c 100000 /dev/zero", to_file)?
    assert result.stdout == b""
    assert fp"{root}/out".read_bytes()?.len() == 100000
  }
  with session = ssh.connect(options)? {
    let stderr_flood = "head -c 3000000 /dev/zero >&2"
    let to_file = {max_output: 1024, stdout_path: fp"{root}/sink"}
    assert ssh.exec(session, stderr_flood, to_file) is Err(SshError.OutputLimit {..})
  }
}

test ssh_exec_sends_inline_bytes_before_a_streamed_file { |ctx|
  let fixture = match fixture("A") {
    Ok(value) => value,
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }
  let root = test.temp_dir(ctx, name: "chained-stdin")?
  let body = fp"{root}/body.bin"
  body.write("tail-from-file")?
  let options = {
    host: fixture.host,
    port: fixture.port,
    user: fixture.username,
    key: fixture.key,
    host_key: fixture.host_key,
    timeout_ms: fixture.timeout_ms,
  }
  with session = ssh.connect(options)? {
    let result = ssh.exec(session, "cat", {stdin: bytes.from_text("head-inline|"), stdin_path: body})?
    assert result.stdout.utf8()? == "head-inline|tail-from-file"
  }
}

test ssh_exec_environment_is_sent_when_the_server_accepts_it { |ctx|
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
    let accepted = ssh.exec(session, "printenv LC_XSH", {env: {LC_XSH: "set by the client"}})?
    assert accepted.stdout.utf8()? == "set by the client\n"
    assert ssh.exec(session, "printenv", {env: {["1BAD"]: "x"}}) is Err(SshError.Options {..})
    assert ssh.exec(session, "true", {})?.exit_code == 0
    match ssh.exec(session, "touch /tmp/ran-despite-refused-env", {env: {NOT_ACCEPTED: "x"}}) {
      Ok(_) => assert false, "a refused variable still ran the command"
      Err(SshError.Rejected {certainty, ..}) => assert certainty == "not-submitted"
      Err(other) => assert false, other.message
    }
  }
  with session = ssh.connect(options)? {
    assert ssh.exec(session, "test -e /tmp/ran-despite-refused-env", {})?.exit_code != 0
  }
}

test ssh_session_reports_itself_dead_after_a_stdin_timeout { |ctx|
  let fixture = match fixture("A") {
    Ok(value) => value,
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }
  let root = test.temp_dir(ctx, name: "stdin-timeout")?
  let payload = fp"{root}/payload.bin"
  let _ = bytes.zero_at(payload, 0, 64 * 1024 * 1024, create: true)?
  let options = {
    host: fixture.host,
    port: fixture.port,
    user: fixture.username,
    key: fixture.key,
    host_key: fixture.host_key,
    timeout_ms: fixture.timeout_ms,
  }
  with session = ssh.connect(options)? {
    assert ssh.info(session)?.usable
    match ssh.exec(session, "sleep 30", {stdin_path: payload, timeout_ms: 300}) {
      Ok(_) => assert false, "the command finished"
      Err(SshError.Timeout {certainty, ..}) => assert certainty == "submitted"
      Err(other) => assert false, other.message
    }
    assert ! ssh.info(session)?.usable
    assert ssh.exec(session, "true", {}) is Err(SshError.Unusable {..})
  }
  with session = ssh.connect(options)? {
    assert ssh.exec(session, "true", {})?.exit_code == 0
  }
}

test ssh_lost_connection_after_submission_is_a_submitted_transport_error {
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
    match ssh.exec(session, "kill -9 $PPID; sleep 5", {}) {
      Ok(result) => assert result.missing or result.signal != "", "the connection survived"
      Err(SshError.Transport {certainty, ..}) => assert certainty == "submitted"
      Err(other) => assert false, other.message
    }
  }
}
