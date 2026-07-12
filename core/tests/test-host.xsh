proc xsh_bin() [env] -> Path {
  let bin = (env.get("CARGO_BIN_EXE_xsh") ?? "")
  if bin != "" {
    return fp"${bin}"
  }
  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = (env.get("XSH_CORE_DIR") ?? "")
  if dir != "" {
    return fp"${dir}/${name}"
  }
  return ../name
}

proc test_host_localhost() [env, process, env, error] {
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let output = run.text xsh_bin() core_script("host.xsh") -- localhost ?
  test.contains(output, "localhost")?
}

proc test_host_type_and_usage(ctx: TestContext) [env, fs, process, env, error] {
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let typed = run.text xsh_bin() core_script("host.xsh") -- -t A localhost ?
  test.contains(typed, "localhost")?
  test.contains(typed, "A")?
  let err = test.temp_path(ctx, name: "host.err")
  let status = run.status xsh_bin() core_script("host.xsh") -- localhost extra third 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "expected NAME [SERVER]")?
}
