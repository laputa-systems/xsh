proc test_ls(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "ls")?
  fp"${root}/a.txt".write("a")?
  fp"${root}/b.txt".write("bb")?
  fp"${root}/dir".mkdir()?
  fp"${root}/.hidden".write("dot")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- -a -p $root ?
  "a.txt" in output
  "b.txt" in output
  ".hidden" in output
  "dir/" in output
  let long = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- -l $root ?
  "file" in long
  let nested = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- fp"${root}/dir" ?
  ! (fp"${root}/dir".display() in nested)
  let file_operand = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- fp"${root}/a.txt" ?
  fp"${root}/a.txt".display() in file_operand
}
