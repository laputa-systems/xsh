use support.uu as uu

# origin: busybox rmdir/rmdir-removes-parent-directories
test test_bb_rmdir_rmdir_removes_parent_directories_61f3cf10 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "foo/bar")?
  uu.succeeds(uu.invoke(s, "rmdir", ["-p", "foo/bar"])?)
  assert !uu.exists(s, "foo")?
}
