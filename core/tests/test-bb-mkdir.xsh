use support.uu as uu

# origin: busybox mkdir/mkdir-makes-a-directory
test test_bb_mkdir_mkdir_makes_a_directory_8fc344dc { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mkdir", ["foo"])?)
  assert uu.dir_exists(s, "foo")?
}

# origin: busybox mkdir/mkdir-makes-parent-directories
test test_bb_mkdir_mkdir_makes_parent_directories_18f2e7ee { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mkdir", ["-p", "foo/bar"])?)
  assert uu.dir_exists(s, "foo")?
  assert uu.dir_exists(s, "foo/bar")?
}

