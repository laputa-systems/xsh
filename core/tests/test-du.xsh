pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_du(ctx: TestContext) [fs, process, error] {
  let target = test.temp_file(ctx, name: "du.txt", contents: b"abcdef")?
  let output = run.text xsh_bin() du.xsh -- $target ?
  test.contains(output, "du.txt")?
  let apparent = run.text xsh_bin() du.xsh -- -b $target ?
  test.contains(apparent, "6")?
  let human = run.text xsh_bin() du.xsh -- -sh $target ?
  test.contains(human, "K")?
}

proc test_du_recursive_all_and_total(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "du-tree")?
  fp"${root}/a.txt".write("aaa")?
  fs.mkdir(fp"${root}/sub")?
  fp"${root}/sub/b.txt".write("bb")?
  let output = run.text xsh_bin() du.xsh -- -a -c $root ?
  test.contains(output, f"${root}/a.txt")?
  test.contains(output, f"${root}/sub/b.txt")?
  test.contains(output, f"${root}/sub")?
  test.contains(output, "total")?
  let summarized = run.text xsh_bin() du.xsh -- --summarize --total $root ?
  test.contains(summarized, f"${root}")?
  test.contains(summarized, "total")?
}
