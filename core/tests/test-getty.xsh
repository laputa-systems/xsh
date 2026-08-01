proc test_getty_requires_baud_and_tty(ctx: TestContext) [fs, process, error] {
  let err = test.temp_path(ctx, name: "getty.err")
  let result = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/getty.xsh" -- -n -i 2> $err
  test.ok(! result.exited_with(0))?
  test.contains(err.read_text()?, "missing operand")?
}
