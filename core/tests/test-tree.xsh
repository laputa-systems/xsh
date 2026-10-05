test test_tree_renders_sorted_branches_and_symlinks { |ctx|
  let root = test.temp_dir(ctx, name: "tree")?
  fp"{root}/dir".mkdir()
  fp"{root}/dir/file.txt".write("ok")
  fp"{root}/a.txt".write("a")
  fp"{root}/z.txt".write("z")
  fp"{root}/.hidden".write("dot")
  fp"{root}/link-a".symlink(to: fp"{root}/a.txt")
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tree.xsh" -- $root
  let lines = output.lines().collect()
  assert lines[0] == root.display()
  assert lines[1] == "|-- a.txt"
  assert lines[2] == "|-- dir"
  assert lines[3] == "|   `-- file.txt"
  assert "link-a ->" in lines[4]
  assert lines[5] == "`-- z.txt"
  assert "1 directory, 4 files" in output
  assert ! (".hidden" in output)
  let all = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tree.xsh" -- -a $root
  assert ".hidden" in all
  let dirs = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tree.xsh" -- -d $root
  assert "dir" in dirs
  assert ! ("a.txt" in dirs)
  let shallow = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tree.xsh" -- -L 1 $root
  assert ! ("file.txt" in shallow)
}

test test_tree_supports_multiple_roots_and_rejects_flags { |ctx|
  let left = test.temp_dir(ctx, name: "tree-left")?
  let right = test.temp_dir(ctx, name: "tree-right")?
  fp"{left}/a".write("a")
  fp"{right}/b".write("b")
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tree.xsh" -- $left $right

  assert f"""{left}
`-- a
""" in output

  assert f"""
{right}
`-- b
""" in output

  let err = test.temp_path(ctx, name: "tree.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/tree.xsh" -- -z $left 2> $err
  assert ! status.exited_with(0)
  assert "unknown argument" in err.read_text()?
}
