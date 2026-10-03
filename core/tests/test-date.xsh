test test_date_format { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/date.xsh" -- -u +%Y ?
  assert output.trim().count_chars() == 4
  let offset = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/date.xsh" -- -u +%z ?
  assert offset.trim() == "+0000"
}
