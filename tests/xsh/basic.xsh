proc test_pass() [error] {
  test.eq(1, 1)?
}

proc test_skip() {
  test.skip("later")
}

proc test_temp(ctx: TestContext) [fs, error] {
  let one: Path = test.temp_path(ctx)
  let two: Path = test.temp_path(ctx)
  test.ne(one, two)?
  let file: Path = test.temp_file(ctx, name: "data", contents: b"ok")?
  let data: Str = fs.read_text(file)?
  test.eq(data, "ok")?
}

proc test_process_command_builder() [process, error] {
  let command = process.command {
    run true
  }

  let status = process.run(command)?
  test.ok(status.exited_with(0), "builder command should run")?
}

proc test_dns_mock(ctx: TestContext) [net, error] {
  test.mock(
    ctx,
    "dns.lookup",
    {name: "example.test"},
    Ok([{name: "example.test", record: "A", value: "127.0.0.1", ttl: 60}]),
  )?

  let records = dns.lookup("example.test")?
  test.eq(records[0].value, "127.0.0.1")?
  let calls = test.calls(ctx, "dns.lookup")
  test.eq(calls.len(), 1)?
}

proc test_net_mock(ctx: TestContext) [net, error] {
  test.mock(
    ctx,
    "net.request",
    {url: "https://example.test/"},
    Ok(
      {
        status: 200,
        reason: "OK",
        bytes: 2,
        headers: [{name: "content-type", value: "text/plain"}],
        url: "https://example.test/",
        body: b"ok",
      },
    ),
  )?

  let response = net.request({method: "GET", url: "https://example.test/"})?
  test.eq(response.body, b"ok")?
  let calls = test.calls(ctx, "net.request")
  test.eq(calls[0].args.method, "GET")?
}
