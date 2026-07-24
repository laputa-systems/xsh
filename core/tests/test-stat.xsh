proc test_stat(ctx: TestContext) [fs, process, env, error] {
  let target = test.temp_file(ctx, name: "stat.txt", contents: b"hello")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/stat.xsh" -- $target ?
  test.contains(output, "kind file")?
  test.contains(output, "size 5")?
  let formatted = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/stat.xsh" -- -c "%s %F %n" $target ?
  test.contains(formatted, "5 regular file")?
  test.contains(formatted, "stat.txt")?
  let modes = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/stat.xsh" -- -c "%a %A %U %G" $target ?
  test.contains(modes, "rw")?
}
