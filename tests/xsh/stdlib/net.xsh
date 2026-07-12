proc test_net_module_with_mocks(ctx: TestContext) [fs, net, error] {
  let response = {
    status: 200,
    reason: "OK",
    bytes: 2,
    headers: [
      {
        name: "content-type",
        value: "text/plain",
      },
    ],
    url: "https://example.test/",
    body: b"ok",
  }

  test.mock(ctx, "net.request", {url: "https://example.test/"}, Ok(response))?

  test.mock(
    ctx,
    "net.download",
    {url: "https://example.test/file"},
    Ok({
      status: 200,
      reason: "OK",
      bytes: 2,
      headers: [{name: "content-type", value: "text/plain"}],
      url: "https://example.test/file",
    }),
  )?

  test.mock(
    ctx,
    "net.upload",
    {url: "https://example.test/upload"},
    Ok({
      status: 200,
      reason: "OK",
      bytes: 2,
      headers: [{name: "content-type", value: "text/plain"}],
      url: "https://example.test/upload",
    }),
  )?

  test.eq(net.request({method: "GET", url: "https://example.test/"})?.body, b"ok")?
  test.eq(net.download({method: "GET", url: "https://example.test/file", dest: p"out"})?.status, 200)?
  test.eq(net.upload({method: "PUT", url: "https://example.test/upload", source: p"in"})?.bytes, 2)?
  let pool = net.pool(name: "stdlib-test", max_idle_per_host: 1, idle_timeout: 1s)?
  test.eq(pool.name, "stdlib-test")?
  test.eq(pool.max_idle_per_host, 1)?
  net.close_pool("stdlib-test")?
  net.close_all_pools()?
  test.eq(test.calls(ctx, "net.request")[0].args.method, "GET")?
  test.eq(test.calls(ctx, "net.download")[0].args.dest.display(), "out")?
  test.eq(test.calls(ctx, "net.upload")[0].args.source.display(), "in")?
}
