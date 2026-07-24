proc test_du(ctx: TestContext) [fs, process, env, error] {
  let target = test.temp_file(ctx, name: "du.txt", contents: b"abcdef")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- $target ?
  test.contains(output, "du.txt")?
  let apparent = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- -b $target ?
  test.contains(apparent, "6")?
  let human = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- -sh $target ?
  test.contains(human, "K")?
}

proc test_du_recursive_all_and_total(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "du-tree")?
  fp"${root}/a.txt".write("aaa")?
  fs.mkdir(fp"${root}/sub")?
  fp"${root}/sub/b.txt".write("bb")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- -a -c $root ?
  test.contains(output, f"${root}/a.txt")?
  test.contains(output, f"${root}/sub/b.txt")?
  test.contains(output, f"${root}/sub")?
  test.contains(output, "total")?
  let summarized = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- --summarize --total $root ?
  test.contains(summarized, f"${root}")?
  test.contains(summarized, "total")?
}
