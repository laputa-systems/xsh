proc test_passwd_rejects_extra_operands(ctx: TestContext) [fs, process, error] {
  let err = test.temp_path(ctx, name: "passwd.err")
  let result = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/passwd.xsh" -- user extra 2> $err
  test.ok(! result.exited_with(0))?
  test.contains(err.read_text()?, "extra operand")?
}
