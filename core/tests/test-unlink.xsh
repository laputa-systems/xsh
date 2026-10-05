test test_unlink_removes_link_and_refuses_directory { |ctx|
  let root = test.temp_dir(ctx)?
  let target = fp"{root}/target"
  target.write("keep")
  let link = fp"{root}/link"
  link.symlink(to: target)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unlink.xsh" -- $link
  assert target.read_text()? == "keep"
  assert ! link.exists()?
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/unlink.xsh" -- $root
  assert status.exited_with(1)
  assert root.is_dir()?
}

test test_unlink_extra_operand_is_not_removed { |ctx|
  let root = test.temp_dir(ctx)?
  let a = fp"{root}/a"
  let b = fp"{root}/b"
  a.write("a")
  b.write("b")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/unlink.xsh" -- $a $b
  assert status.exited_with(1)
  assert a.exists()? and b.exists()?
}
