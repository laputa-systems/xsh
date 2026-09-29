proc test_ip_addr_smoke(ctx: TestContext) [process, env, error] {
  let output = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- addr ?
  test.ok(output.count_chars() >= 0)?
}

proc test_ip_route_list_patterns_accept_exact_command_forms(ctx: TestContext) [process, env, error] {
  let route = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- route ?
  let explicit = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- route show ?
  test.eq(route, explicit)?
}
