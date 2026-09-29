proc test_dirname(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/dirname.xsh" -- /tmp/demo/file.txt ?
  output.trim() == "/tmp/demo"
  let many = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/dirname.xsh" -- /tmp/a/one.txt /tmp/b/two.txt ?
  "/tmp/a" in many
  "/tmp/b" in many
}
