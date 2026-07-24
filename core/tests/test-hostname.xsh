proc test_hostname_short(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/hostname.xsh" -- -s ?
  test.ok(output.trim() != "")?
}
