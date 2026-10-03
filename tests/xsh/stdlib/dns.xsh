test test_dns_module_with_mocks { |ctx|
  test.mock(
    ctx,
    "dns.lookup",
    {name: "example.test"},
    Ok([{name: "example.test", record: "A", value: "127.0.0.1", ttl: 60}]),
  )?

  test.mock(ctx, "dns.resolve_host", {name: "localhost"}, Ok([{name: "localhost", family: "inet", addr: "127.0.0.1"}]))?
  test.mock(ctx, "dns.reverse", {addr: "127.0.0.1"}, Ok(["localhost"]))?
  test.mock(ctx, "dns.nameservers", {}, Ok(["127.0.0.53"]))?
  assert dns.lookup("example.test", "AAAA", "127.0.0.1:5353", 2s)?[0].value == "127.0.0.1"
  assert dns.resolve_host("localhost", "ipv4")?[0].family == "inet"
  assert dns.reverse("127.0.0.1")?[0] == "localhost"
  assert dns.nameservers()?[0] == "127.0.0.53"
  assert test.calls(ctx, "dns.lookup")[0].args.name == "example.test"
  assert test.calls(ctx, "dns.lookup")[0].args.record == "AAAA"
  assert test.calls(ctx, "dns.lookup")[0].args.server == "127.0.0.1:5353"
  assert test.calls(ctx, "dns.lookup")[0].args.timeout_ms == 2000
  assert test.calls(ctx, "dns.resolve_host")[0].args.family == "ipv4"
  assert test.calls(ctx, "dns.reverse")[0].args.addr == "127.0.0.1"
}

test test_dns_module_rejects_invalid_arguments {
  test.error_kind(dns.lookup("", "A"), "dns-name")?
  test.error_kind(dns.lookup("example.test", "TXT"), "dns-record")?
  test.error_kind(dns.resolve_host("127.0.0.1", "bogus"), "dns-family")?
  test.error_kind(dns.reverse("not-an-ip-address"), "dns-address")?
}

test test_dns_explicit_server_transport {
  let server = env.get_or("XSH_DNS_TEST_SERVER", "")?
  if server == "" {
    test.skip("requires XSH_DNS_TEST_SERVER fixture")
    return
  }

  let a = dns.lookup("fixture.test", "A", server, 1s)?
  let aaaa = dns.lookup("fixture.test", "AAAA", server, 1s)?
  assert a[0].name == "fixture.test"
  assert a[0].record == "A"
  assert a[0].value == "192.0.2.10"
  assert a[0].ttl == 60
  assert aaaa[0].name == "fixture.test"
  assert aaaa[0].record == "AAAA"
  assert aaaa[0].value == "2001:db8::42"
  assert aaaa[0].ttl == 60
}
