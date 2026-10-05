test test_pwd { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pwd.xsh"
  assert output.trim() == fs.cwd()?.display()
}
