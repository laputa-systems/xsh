proc test_nproc(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/nproc.xsh" ?
  test.ok(output.trim() != "")?
}
