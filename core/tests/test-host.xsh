test test_host_localhost { |ctx|
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/host.xsh" -- localhost ?
  assert "localhost" in output
}

test test_host_type_and_usage { |ctx|
  if env.bool("XSH_SKIP_NET_TESTS")? {
    test.skip("net feature disabled")
  }

  let typed = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/host.xsh" -- -t A localhost ?
  assert "localhost" in typed
  assert "A" in typed
  let err = test.temp_path(ctx, name: "host.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/host.xsh" -- localhost extra third 2> $err
  assert ! status.exited_with(0)
  assert "expected NAME [SERVER]" in err.read_text()?
}
