test test_pwd [fs, process, env, error] { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/pwd.xsh" ?
  test.eq(output.trim(), fs.cwd()?.display())?
}
