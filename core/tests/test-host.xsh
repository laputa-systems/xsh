pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_host_localhost() [process, env, error] {
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let output = run.text xsh_bin() host.xsh -- localhost ?
  test.contains(output, "localhost")?
}

proc test_host_type_and_usage(ctx: TestContext) [fs, process, env, error] {
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let typed = run.text xsh_bin() host.xsh -- -t A localhost ?
  test.contains(typed, "localhost")?
  test.contains(typed, "A")?
  let err = test.temp_path(ctx, name: "host.err")
  let status = run.status xsh_bin() host.xsh -- localhost extra third 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "expected NAME [SERVER]")?
}
