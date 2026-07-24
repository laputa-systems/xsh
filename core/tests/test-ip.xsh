proc test_ip_addr_smoke(ctx: TestContext) [process, env, error] {
  let output = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- addr ?
  test.ok(output.count_chars() >= 0)?
}
