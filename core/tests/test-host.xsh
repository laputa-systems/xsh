test test_host_localhost [process, env, error] { |ctx|
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/host.xsh" -- localhost ?
  "localhost" in output
}

test test_host_type_and_usage [fs, process, env, error] { |ctx|
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let typed = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/host.xsh" -- -t A localhost ?
  "localhost" in typed
  "A" in typed
  let err = test.temp_path(ctx, name: "host.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/host.xsh" -- localhost extra third 2> $err
  ! status.exited_with(0)
  "expected NAME [SERVER]" in err.read_text()?
}
