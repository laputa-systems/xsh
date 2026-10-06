type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], variables: Record, input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "getent-capture")?
  let output = fp"{root}/stdout"
  let errors = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/getent.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    root, variables, input, output, errors))?
  Ok({status: status.exit_code()?, stdout: output.read_text()?, stderr: errors.read_text()?})
}

test test_getent_passwd_numeric_name_and_missing_keys { |ctx|
  let source = test.temp_file(ctx, name: "passwd", contents: b"alice:x:1001:100:Alice:/home/alice:/bin/sh\nbob:x:1002:100:Bob:/home/bob:/bin/sh\n")?
  let vars = {XSH_PASSWD_FILE: source.display(), LC_ALL: "C"}
  let all = invoke(ctx, ["passwd"], vars)?
  assert all.status == 0
  assert all.stdout == source.read_text()?
  let one = invoke(ctx, ["passwd", "1002"], vars)?
  assert one.status == 0
  assert one.stdout.starts_with("bob:x:1002:")
  let partial = invoke(ctx, ["passwd", "missing", "alice"], vars)?
  assert partial.status == 2
  assert partial.stdout.starts_with("alice:x:1001:")
}

test test_getent_groups_and_members { |ctx|
  let source = test.temp_file(ctx, name: "group", contents: b"staff:x:100:alice,bob\nempty:x:200:\n")?
  let vars = {XSH_GROUP_FILE: source.display()}
  assert invoke(ctx, ["group", "100"], vars)?.stdout == "staff:x:100:alice,bob\n"
  assert invoke(ctx, ["group", "empty"], vars)?.stdout == "empty:x:200:\n"
}

test test_getent_hosts_services_protocol_aliases { |ctx|
  let hosts = test.temp_file(ctx, name: "hosts", contents: b"127.0.0.1 localhost loopback # local\n::1 localhost6\n")?
  let services = test.temp_file(ctx, name: "services", contents: b"http 80/tcp www\ndomain 53/udp dns\n")?
  let protocols = test.temp_file(ctx, name: "protocols", contents: b"tcp 6 TCP\nudp 17 UDP\n")?
  let vars = {XSH_HOSTS_FILE: hosts.display(), XSH_SERVICES_FILE: services.display(), XSH_PROTOCOLS_FILE: protocols.display()}
  let host = invoke(ctx, ["hosts", "loopback"], vars)?
  assert host.status == 0
  assert host.stdout.words().collect() == ["127.0.0.1", "localhost", "loopback"]
  assert invoke(ctx, ["services", "www/tcp"], vars)?.stdout.words().collect() == ["http", "80/tcp", "www"]
  assert invoke(ctx, ["protocols", "17"], vars)?.stdout.words().collect() == ["udp", "17", "UDP"]
}

test test_getent_rejects_unknown_database_and_backend { |ctx|
  assert invoke(ctx, ["unknown"], {})?.status == 1
  assert invoke(ctx, ["-s", "unsupported-nss", "passwd"], {})?.status == 1
  assert invoke(ctx, ["-s", "dns", "hosts"], {})?.status == 3
}

# A numeric host exercises the typed resolver without querying a DNS server.
test test_getent_dns_numeric_host { |ctx|
  let result = invoke(ctx, ["-s", "dns", "hosts", "127.0.0.1"], {})?
  assert result.status == 0, result.stderr
  assert result.stdout.words().collect() == ["127.0.0.1", "localhost"]
}
