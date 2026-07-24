proc test_uname_all(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/uname.xsh" -- -a ?
  test.ok(output.fields().len() >= 3)?
}
