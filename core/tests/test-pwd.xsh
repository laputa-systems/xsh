proc test_pwd(ctx: TestContext) [fs, process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/pwd.xsh" ?
  test.eq(output.trim(), fs.cwd()?.display())?
}
