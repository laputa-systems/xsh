test test_pwd { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/pwd.xsh" ?
  output.trim() == fs.cwd()?.display()
}
