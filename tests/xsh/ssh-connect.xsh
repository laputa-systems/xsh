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
    assert ssh.info(session)?.usable
  }
}

proc open_session(options: Record) -> Result[SshSession, Error] {
  ssh.connect(options)
}

test ssh_session_survives_a_proc_return {
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
  let session = open_session(options)?
  assert ssh.exec(session, "true", {})?.exit_code == 0
  let _ = ssh.close(session)
}

test ssh_changed_pin_is_rejected {
  let fixture = match fixture("A") {
    Ok(value) => value,
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }
  let last = fixture.host_key.byte_slice(fixture.host_key.byte_len() - 1, length: 1)
  let swapped = if last == "A" { "E" } else { "A" }
  let changed = fixture.host_key.byte_slice(0, length: fixture.host_key.byte_len() - 1) + swapped
  let options = {
    host: fixture.host,
    port: fixture.port,
    user: fixture.username,
    key: fixture.key,
    host_key: changed,
    timeout_ms: fixture.timeout_ms,
  }
  assert ssh.connect(options) is Err(SshError.HostKeyMismatch {..})
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
  let released = leave_early(options)?
  assert ssh.close(released) is Err(SshError.Session {..})
}

type Base = {
  host: Str,
  port: Int,
  user: Str,
  key: Path,
  timeout_ms: Int,
}

proc connect_options(suffix: Str) [env, error] -> Result[Base, Error] {
  let fixture = fixture(suffix)?
  {
    host: fixture.host,
    port: fixture.port,
    user: fixture.username,
    key: fixture.key,
    timeout_ms: fixture.timeout_ms,
  }
}

proc known_hosts_file(root: Path, name: Str, lines: List[Str]) [fs, error] -> Result[Path, Error] {
  let file = fp"{root}/{name}"
  file.write(lines.join("\n") + "\n")?
  file
}

proc known_hosts_line(suffix: Str, marker: Str, key_suffix: Str) [env, error] -> Result[Str, Error] {
  let host = env.get(f"XINFRA_FIXTURE_{suffix}_HOST")?
  let port = env.get(f"XINFRA_FIXTURE_{suffix}_PORT")?
  let public = env.get(f"XINFRA_FIXTURE_{key_suffix}_HOST_KEY_PUB")?.fields()
  f"{marker}[{host}]:{port} {public[0]} {public[1]}"
}

test ssh_known_hosts_trusts_a_listed_endpoint { |ctx|
  let root = test.temp_dir(ctx, name: "known-hosts")?
  let base = match connect_options("A") {
    Ok(value) => value
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }
  let file = known_hosts_file(root, "known-hosts", [known_hosts_line("A", "", "A")?])?
  with session = ssh.connect({...base, known_hosts: file})? {
    assert ssh.exec(session, "true", {})?.exit_code == 0
  }
}

test ssh_known_hosts_refuses_unknown_changed_and_revoked_keys { |ctx|
  let root = test.temp_dir(ctx, name: "known-hosts")?
  let base = match connect_options("A") {
    Ok(value) => value
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }
  let unknown = known_hosts_file(root, "unknown", [known_hosts_line("B", "", "B")?])?
  assert ssh.connect({...base, known_hosts: unknown}) is Err(SshError.HostKeyUnknown {..})
  let changed = known_hosts_file(root, "changed", [known_hosts_line("A", "", "B")?])?
  assert ssh.connect({...base, known_hosts: changed}) is Err(SshError.HostKeyMismatch {..})
  let revoked = known_hosts_file(root, "revoked", [known_hosts_line("A", "", "A")?, known_hosts_line("A", "@revoked ", "A")?])?
  assert ssh.connect({...base, known_hosts: revoked}) is Err(SshError.HostKeyRevoked {..})
  let hashed = known_hosts_file(root, "hashed", ["|1|c2FsdA==|aGFzaA== ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA=="])?
  assert ssh.connect({...base, known_hosts: hashed}) is Err(SshError.TrustMaterial {..})
}

test ssh_connect_requires_exactly_one_trust_and_identity {
  let fixture = match fixture("A") {
    Ok(value) => value
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }
  let missing_trust = {host: fixture.host, port: fixture.port, user: fixture.username, key: fixture.key}
  assert ssh.connect(missing_trust) is Err(SshError.Options {..})
  let missing_identity = {host: fixture.host, port: fixture.port, user: fixture.username, host_key: fixture.host_key}
  assert ssh.connect(missing_identity) is Err(SshError.Options {..})
  let both_trusts = {host: fixture.host, port: fixture.port, user: fixture.username, key: fixture.key, host_key: fixture.host_key, known_hosts: fixture.key}
  assert ssh.connect(both_trusts) is Err(SshError.Options {..})
  let both_identities = {host: fixture.host, port: fixture.port, user: fixture.username, key: fixture.key, agent: fixture.key, host_key: fixture.host_key}
  assert ssh.connect(both_identities) is Err(SshError.Options {..})
}

test ssh_info_reports_the_authenticated_host_and_algorithms {
  let fixture = match fixture("A") {
    Ok(value) => value
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
    let info = ssh.info(session)?
    assert info.host_key == fixture.host_key
    assert info.host == fixture.host
    assert info.port == fixture.port
    assert info.user == fixture.username
    assert info.kex == "mlkem768x25519-sha256"
    assert info.host_key_algorithm == "ssh-ed25519"
  }
}

test ssh_return_from_a_with_body_releases_the_session {
  let fixture = match fixture("A") {
    Ok(value) => value
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
  let released = leave_early(options)?
  assert ssh.close(released) is Err(SshError.Session {..})
}

proc leave_early(options: Record) [fs, net, error] -> Result[SshSession, Error] {
  with session = ssh.connect(options)? {
    return session
  }
  fail "the body returned before the block ended"
}

proc open_both(a: Base, b: Base, wrong_pin: Str, fingerprint_a: Str, fingerprint_b: Str) [net, error] -> Result[List[Str], Error] {
  let first = {...a, host_key: fingerprint_a}
  let second = {...b, host_key: fingerprint_b}
  let third = {...b, host_key: wrong_pin}
  let results = ssh.connect_all({targets: [first, second, third], jobs: 3})?
  assert results.len() == 3
  assert results[2] is Err(SshError.HostKeyMismatch {..})
  let sessions = [results[0]?, results[1]?]
  let names = sessions |> par-map(jobs: 2) { |session| ssh.exec(session, "hostname", {})?.stdout.utf8()?.trim() }
  names
}

test ssh_connect_all_opens_hosts_in_parallel_and_hands_them_to_the_caller {
  let a = match connect_options("A") {
    Ok(value) => value
    Err(_) => {
      test.skip("fixture environment is absent")?
      return
    }
  }
  let b = connect_options("B")?
  let fingerprint_a = env.get("XINFRA_FIXTURE_A_HOST_KEY")?
  let fingerprint_b = env.get("XINFRA_FIXTURE_B_HOST_KEY")?
  let names = open_both(a, b, fingerprint_a, fingerprint_a, fingerprint_b)?
  assert names.len() == 2
  assert names[0] != names[1]
}

pure certainty_of(error: SshError) -> ssh.Certainty {
  ssh.certainty(error)
}

pure retry_is_safe(certainty: ssh.Certainty) -> Bool {
  match certainty {
    ssh.Certainty.NotSubmitted => true
    ssh.Certainty.Submitted => false
  }
}

test ssh_certainty_is_callable_from_a_pure_function {
  match ssh.connect({}) {
    Ok(_) => assert false, "connect without options succeeded"
    Err(error) => assert certainty_of(error) == .NotSubmitted
  }
}

test ssh_certainty_reads_the_certainty_of_any_ssh_error {
  match ssh.connect({}) {
    Ok(_) => assert false, "connect without options succeeded"
    Err(error) => assert ssh.certainty(error) == .NotSubmitted
  }
  match ssh.connect({host: "example.invalid"}) {
    Ok(_) => assert false, "connect without trust succeeded"
    Err(error) => {
      let certainty: ssh.Certainty = ssh.certainty(error)
      assert certainty == .NotSubmitted
      assert retry_is_safe(certainty)
    }
  }
}

test ssh_certainty_field_and_accessor_agree {
  match ssh.connect({}) {
    Ok(_) => assert false, "connect without options succeeded"
    Err(SshError.Options {certainty, ..} as error) => assert certainty == ssh.certainty(error)
    Err(other) => assert false, other.message
  }
}

test ssh_certainty_displays_as_its_wire_string {
  match ssh.connect({}) {
    Ok(_) => assert false, "connect without options succeeded"
    Err(error) => {
      let certainty = ssh.certainty(error)
      assert f"{certainty}" == "not-submitted"
      assert f"certainty {certainty}" == "certainty not-submitted"
    }
  }
}
