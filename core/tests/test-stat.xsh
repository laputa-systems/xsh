test test_stat { |ctx|
  let target = test.temp_file(ctx, name: "stat.txt", contents: b"hello")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/stat.xsh" -- $target ?
  "kind file" in output
  "size 5" in output
  let formatted = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/stat.xsh" -- -c "%s %F %n" $target ?
  "5 regular file" in formatted
  "stat.txt" in formatted
  let modes = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/stat.xsh" -- -c "%a %A %U %G" $target ?
  "rw" in modes
}
