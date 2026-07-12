proc xsh_bin() [env] -> Path {
  let bin = (env.get("CARGO_BIN_EXE_xsh") ?? "")
  if bin != "" {
    return fp"${bin}"
  }
  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = (env.get("XSH_CORE_DIR") ?? "")
  if dir != "" {
    return fp"${dir}/${name}"
  }
  return ../name
}

proc test_tree_renders_sorted_branches_and_symlinks(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "tree")?
  fp"${root}/dir".mkdir()?
  fp"${root}/dir/file.txt".write("ok")?
  fp"${root}/a.txt".write("a")?
  fp"${root}/z.txt".write("z")?
  fp"${root}/.hidden".write("dot")?
  fs.symlink(fp"${root}/a.txt", fp"${root}/link-a")?
  let output = run.text xsh_bin() core_script("tree.xsh") -- $root ?
  let lines = output.lines().collect()
  test.eq(lines[0], root.display())?
  test.eq(lines[1], "|-- a.txt")?
  test.eq(lines[2], "|-- dir")?
  test.eq(lines[3], "|   `-- file.txt")?
  test.contains(lines[4], "link-a ->")?
  test.eq(lines[5], "`-- z.txt")?
  test.contains(output, "1 directory, 4 files")?
  test.ok(! (".hidden" in output))?
  let all = run.text xsh_bin() core_script("tree.xsh") -- -a $root ?
  test.contains(all, ".hidden")?
  let dirs = run.text xsh_bin() core_script("tree.xsh") -- -d $root ?
  test.contains(dirs, "dir")?
  test.ok(! ("a.txt" in dirs))?
  let shallow = run.text xsh_bin() core_script("tree.xsh") -- -L 1 $root ?
  test.ok(! ("file.txt" in shallow))?
}

proc test_tree_supports_multiple_roots_and_rejects_flags(ctx: TestContext) [env, fs, process, error] {
  let left = test.temp_dir(ctx, name: "tree-left")?
  let right = test.temp_dir(ctx, name: "tree-right")?
  fp"${left}/a".write("a")?
  fp"${right}/b".write("b")?
  let output = run.text xsh_bin() core_script("tree.xsh") -- $left $right ?

  test.contains(
    output,
    f"""${left.display()}
`-- a
""",
  )?

  test.contains(
    output,
    f"""
${right.display()}
`-- b
""",
  )?

  let err = test.temp_path(ctx, name: "tree.err")
  let status = run.status xsh_bin() core_script("tree.xsh") -- -z $left 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "unsupported option")?
}
