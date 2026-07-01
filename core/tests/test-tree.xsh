pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_tree_renders_sorted_branches_and_symlinks(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "tree")?
  fp"${root}/dir".mkdir()?
  fp"${root}/dir/file.txt".write("ok")?
  fp"${root}/a.txt".write("a")?
  fp"${root}/z.txt".write("z")?
  fp"${root}/.hidden".write("dot")?
  fs.symlink(fp"${root}/a.txt", fp"${root}/link-a")?
  let output = run.text xsh_bin() tree.xsh -- $root ?
  let lines = output.lines().collect()
  test.eq(lines[0], root.display())?
  test.eq(lines[1], "|-- a.txt")?
  test.eq(lines[2], "|-- dir")?
  test.eq(lines[3], "|   `-- file.txt")?
  test.contains(lines[4], "link-a ->")?
  test.eq(lines[5], "`-- z.txt")?
  test.contains(output, "1 directory, 4 files")?
  test.ok(! output.contains(".hidden"))?
  let all = run.text xsh_bin() tree.xsh -- -a $root ?
  test.contains(all, ".hidden")?
  let dirs = run.text xsh_bin() tree.xsh -- -d $root ?
  test.contains(dirs, "dir")?
  test.ok(! dirs.contains("a.txt"))?
  let shallow = run.text xsh_bin() tree.xsh -- -L 1 $root ?
  test.ok(! shallow.contains("file.txt"))?
}

proc test_tree_supports_multiple_roots_and_rejects_flags(ctx: TestContext) [fs, process, error] {
  let left = test.temp_dir(ctx, name: "tree-left")?
  let right = test.temp_dir(ctx, name: "tree-right")?
  fp"${left}/a".write("a")?
  fp"${right}/b".write("b")?
  let output = run.text xsh_bin() tree.xsh -- $left $right ?

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
  let status = run.status xsh_bin() tree.xsh -- -z $left 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "unsupported option")?
}
