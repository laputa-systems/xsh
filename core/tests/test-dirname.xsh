proc test_dirname(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/dirname.xsh" -- /tmp/demo/file.txt ?
  test.eq(output.trim(), "/tmp/demo")?
  let many = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/dirname.xsh" -- /tmp/a/one.txt /tmp/b/two.txt ?
  test.contains(many, "/tmp/a")?
  test.contains(many, "/tmp/b")?
}
