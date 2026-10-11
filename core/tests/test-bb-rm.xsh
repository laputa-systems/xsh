use support.uu as uu

# origin: busybox rm/rm-removes-file
test test_bb_rm_rm_removes_file_a4932268 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "rm", ["foo"])?)
  assert !uu.exists(s, "foo")?
}
