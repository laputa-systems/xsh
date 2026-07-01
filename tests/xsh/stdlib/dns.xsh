proc test_dns_module_with_mocks(ctx: TestContext) [net, error] {
  test.mock(
    ctx,
    "dns.lookup",
    {name: "example.test"},
    Ok([{name: "example.test", record: "A", value: "127.0.0.1", ttl: 60}]),
  )?

  test.mock(ctx, "dns.resolve_host", {name: "localhost"}, Ok([{name: "localhost", family: "inet", addr: "127.0.0.1"}]))?
  test.mock(ctx, "dns.reverse", {addr: "127.0.0.1"}, Ok(["localhost"]))?
  test.mock(ctx, "dns.nameservers", {}, Ok(["127.0.0.53"]))?
  test.eq(dns.lookup("example.test")?[0].value, "127.0.0.1")?
  test.eq(dns.resolve_host("localhost")?[0].family, "inet")?
  test.eq(dns.reverse("127.0.0.1")?[0], "localhost")?
  test.eq(dns.nameservers()?[0], "127.0.0.53")?
  test.eq(test.calls(ctx, "dns.lookup")[0].args.name, "example.test")?
}
