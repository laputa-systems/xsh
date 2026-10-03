test test_uname_all { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/uname.xsh" -- -a ?
  assert output.fields().len() >= 3
}
