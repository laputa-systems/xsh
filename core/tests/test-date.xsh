proc test_date_format(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/date.xsh" -- -u +%Y ?
  test.eq(output.trim().count_chars(), 4)?
  let offset = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/date.xsh" -- -u +%z ?
  test.eq(offset.trim(), "+0000")?
}
