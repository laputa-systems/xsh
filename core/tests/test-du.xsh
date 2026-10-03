test test_du { |ctx|
  let target = test.temp_file(ctx, name: "du.txt", contents: b"abcdef")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- $target ?
  "du.txt" in output
  let apparent = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- -b $target ?
  "6" in apparent
  let human = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- -sh $target ?
  "K" in human
}

test test_du_recursive_all_and_total { |ctx|
  let root = test.temp_dir(ctx, name: "du-tree")?
  fp"${root}/a.txt".write("aaa")?
  fs.mkdir(fp"${root}/sub")?
  fp"${root}/sub/b.txt".write("bb")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- -a -c $root ?
  f"${root}/a.txt" in output
  f"${root}/sub/b.txt" in output
  f"${root}/sub" in output
  "total" in output
  let summarized = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/du.xsh" -- --summarize --total $root ?
  f"${root}" in summarized
  "total" in summarized
}
