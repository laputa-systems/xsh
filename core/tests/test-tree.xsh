test test_tree_renders_sorted_branches_and_symlinks [fs, process, env, error] { |ctx|
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
  "link-a ->" in lines[4].require(Str)?
  test.eq(lines[5], "`-- z.txt")?
  "1 directory, 4 files" in output
  ! (".hidden" in output)
  let all = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- -a $root ?
  ".hidden" in all
  let dirs = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- -d $root ?
  "dir" in dirs
  ! ("a.txt" in dirs)
  let shallow = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- -L 1 $root ?
  ! ("file.txt" in shallow)
}

test test_tree_supports_multiple_roots_and_rejects_flags [fs, process, env, error] { |ctx|
  let left = test.temp_dir(ctx, name: "tree-left")?
  let right = test.temp_dir(ctx, name: "tree-right")?
  fp"${left}/a".write("a")?
  fp"${right}/b".write("b")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- $left $right ?

  f"""${left.display()}
`-- a
""" in output

  f"""
${right.display()}
`-- b
""" in output

  let err = test.temp_path(ctx, name: "tree.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/tree.xsh" -- -z $left 2> $err
  ! status.exited_with(0)
  "unknown argument" in (err.read_text()?)
}
