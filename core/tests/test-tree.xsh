proc test_tree_renders_sorted_branches_and_symlinks(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "tree")?
  fp"${root}/dir".mkdir()?
  fp"${root}/dir/file.txt".write("ok")?
  fp"${root}/a.txt".write("a")?
  fp"${root}/z.txt".write("z")?
  fp"${root}/.hidden".write("dot")?
  fs.symlink(fp"${root}/a.txt", fp"${root}/link-a")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- $root ?
  let lines = output.lines().collect()
  test.eq(lines[0], root.display())?
  test.eq(lines[1], "|-- a.txt")?
  test.eq(lines[2], "|-- dir")?
  test.eq(lines[3], "|   `-- file.txt")?
  test.contains(lines[4], "link-a ->")?
  test.eq(lines[5], "`-- z.txt")?
  test.contains(output, "1 directory, 4 files")?
  test.ok(! (".hidden" in output))?
  let all = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- -a $root ?
  test.contains(all, ".hidden")?
  let dirs = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- -d $root ?
  test.contains(dirs, "dir")?
  test.ok(! ("a.txt" in dirs))?
  let shallow = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- -L 1 $root ?
  test.ok(! ("file.txt" in shallow))?
}

proc test_tree_supports_multiple_roots_and_rejects_flags(ctx: TestContext) [fs, process, env, error] {
  let left = test.temp_dir(ctx, name: "tree-left")?
  let right = test.temp_dir(ctx, name: "tree-right")?
  fp"${left}/a".write("a")?
  fp"${right}/b".write("b")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- $left $right ?

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
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- -z $left 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "unsupported option")?
}
