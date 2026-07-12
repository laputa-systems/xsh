proc xsh_bin() [env] -> Path {
  let bin = env.get("CARGO_BIN_EXE_xsh") ?? ""

  if bin != "" {
    return fp"${bin}"
  }

  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = env.get("XSH_CORE_DIR") ?? ""

  if dir != "" {
    return fp"${dir}/${name}"
  }

  return ../name
}

proc test_du(ctx: TestContext) [fs, process, env, error] {
  let target = test.temp_file(ctx, name: "du.txt", contents: b"abcdef")?
  let output = run.text xsh_bin() core_script("du.xsh") -- $target ?
  test.contains(output, "du.txt")?
  let apparent = run.text xsh_bin() core_script("du.xsh") -- -b $target ?
  test.contains(apparent, "6")?
  let human = run.text xsh_bin() core_script("du.xsh") -- -sh $target ?
  test.contains(human, "K")?
}

proc test_du_recursive_all_and_total(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "du-tree")?
  fp"${root}/a.txt".write("aaa")?
  fs.mkdir(fp"${root}/sub")?
  fp"${root}/sub/b.txt".write("bb")?
  let output = run.text xsh_bin() core_script("du.xsh") -- -a -c $root ?
  test.contains(output, f"${root}/a.txt")?
  test.contains(output, f"${root}/sub/b.txt")?
  test.contains(output, f"${root}/sub")?
  test.contains(output, "total")?
  let summarized = run.text xsh_bin() core_script("du.xsh") -- --summarize --total $root ?
  test.contains(summarized, f"${root}")?
  test.contains(summarized, "total")?
}
