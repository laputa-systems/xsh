proc test_host_localhost(ctx: TestContext) [process, env, error] {
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/host.xsh" -- localhost ?
  test.contains(output, "localhost")?
}

proc test_host_type_and_usage(ctx: TestContext) [fs, process, env, error] {
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let typed = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/host.xsh" -- -t A localhost ?
  test.contains(typed, "localhost")?
  test.contains(typed, "A")?
  let err = test.temp_path(ctx, name: "host.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/host.xsh" -- localhost extra third 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "expected NAME [SERVER]")?
}
