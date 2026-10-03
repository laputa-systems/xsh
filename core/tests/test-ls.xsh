test test_ls { |ctx|
  let root = test.temp_dir(ctx, name: "ls")?
  fp"${root}/a.txt".write("a")?
  fp"${root}/b.txt".write("bb")?
  fp"${root}/dir".mkdir()?
  fp"${root}/.hidden".write("dot")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- -a -p $root ?
  assert "a.txt" in output
  assert "b.txt" in output
  assert ".hidden" in output
  assert "dir/" in output
  let long = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- -l $root ?
  assert "file" in long
  let long_alias = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- -g $root ?
  assert "file" in long_alias
  let nested = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- fp"${root}/dir" ?
  assert ! (fp"${root}/dir".display() in nested)
  let file_operand = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- fp"${root}/a.txt" ?
  assert fp"${root}/a.txt".display() in file_operand
}
