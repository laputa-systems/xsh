proc test_date_format(ctx: TestContext) [process, env, error] {
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/date.xsh" -- -u +%Y ?
  output.trim().count_chars() == 4
  let offset = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/date.xsh" -- -u +%z ?
  offset.trim() == "+0000"
}
