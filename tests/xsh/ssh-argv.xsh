##! Building a remote command string from an argv vector with shlex.join.

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

proc hostile_values() -> List[Str] {
  [
    "",
    " ",
    "two words",
    "it's",
    "\"double\"",
    "a\nb",
    r"$HOME",
    r"${HOME}",
    r"`echo substituted`",
    r"$(echo substituted)",
    "-n",
    "--",
    "-rf",
    "*",
    "?",
    "~",
    "a;b",
    "a&&b",
    "a|b",
    "a>b",
    "\\",
    "héllo",
  ]
}

proc echo_each() -> Str {
  r"""for value in "$@"; do printf '[%s]' "$value"; done"""
}

proc expected(values: List[Str]) -> Str {
  values |> map { |value| f"[{value}]" } |> join("")
}

test ssh_command_strings_round_trip_hostile_arguments_through_a_posix_shell {
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
  let values = hostile_values()
  let command = shlex.join(["sh", "-c", echo_each(), "sh", @values])
  with session = ssh.connect(options)? {
    let result = ssh.exec(session, command, {})?
    assert result.exit_code == 0
    assert result.stdout.utf8()? == expected(values)
  }
}

test ssh_command_strings_with_nul_are_refused_before_submission {
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
    match ssh.exec(session, "printf a\0b", {}) {
      Ok(_) => assert false, "NUL reached the server"
      Err(SshError.Options {certainty, ..}) => assert certainty == .NotSubmitted
      Err(other) => assert false, other.message
    }
    assert ssh.exec(session, "true", {})?.exit_code == 0
  }
}
