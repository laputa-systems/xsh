proc test_ls(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "ls")?
  fp"${root}/a.txt".write("a")?
  fp"${root}/b.txt".write("bb")?
  fp"${root}/dir".mkdir()?
  fp"${root}/.hidden".write("dot")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- -a -p $root ?
  test.contains(output, "a.txt")?
  test.contains(output, "b.txt")?
  test.contains(output, ".hidden")?
  test.contains(output, "dir/")?
  let long = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- -l $root ?
  test.contains(long, "file")?
  let nested = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- fp"${root}/dir" ?
  test.ok(! (fp"${root}/dir".display() in nested))?
  let file_operand = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/ls.xsh" -- fp"${root}/a.txt" ?
  test.contains(file_operand, fp"${root}/a.txt".display())?
}
