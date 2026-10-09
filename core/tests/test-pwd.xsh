test test_pwd { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pwd.xsh" ?
  assert output.trim() == fs.cwd()?.display()
}

test test_pwd_physical_option { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pwd.xsh" -- -P ?
  assert output.trim() == fs.cwd()?.resolve()?.display()
}
