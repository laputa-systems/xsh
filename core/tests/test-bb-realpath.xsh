use support.uu as uu

# origin: busybox realpath/realpath on link to non-existent file 1
test test_bb_realpath_realpath_on_link_to_non_existent_file_1_2a3b13fa { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["link1"])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root.resolve()?}/realpath_testdir/not_file\n")
}

# origin: busybox realpath/realpath on link to non-existent file 2
test test_bb_realpath_realpath_on_link_to_non_existent_file_2_7ec10cc5 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["link2"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "realpath: link2: No such file or directory\n")
}

# origin: busybox realpath/realpath on link to non-existent file 3
test test_bb_realpath_realpath_on_link_to_non_existent_file_3_c585f47f { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["./link1"])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root.resolve()?}/realpath_testdir/not_file\n")
}

# origin: busybox realpath/realpath on link to non-existent file 4
test test_bb_realpath_realpath_on_link_to_non_existent_file_4_04f3b7ab { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["./link2"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "realpath: ./link2: No such file or directory\n")
}

# origin: busybox realpath/realpath on non-existent absolute path 1
test test_bb_realpath_realpath_on_non_existent_absolute_path_1_f35b4628 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["/not_file"])?
  uu.succeeds(r)
  uu.stdout_only(r, "/not_file\n")
}

# origin: busybox realpath/realpath on non-existent absolute path 2
test test_bb_realpath_realpath_on_non_existent_absolute_path_2_6e5d8bb1 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["/not_file/"])?
  uu.succeeds(r)
  uu.stdout_only(r, "/not_file\n")
}

# origin: busybox realpath/realpath on non-existent absolute path 3
test test_bb_realpath_realpath_on_non_existent_absolute_path_3_63c4d299 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["//not_file"])?
  uu.succeeds(r)
  uu.stdout_only(r, "/not_file\n")
}

# origin: busybox realpath/realpath on non-existent absolute path 4
test test_bb_realpath_realpath_on_non_existent_absolute_path_4_eca769f9 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["/not_dir/not_file"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "realpath: /not_dir/not_file: No such file or directory\n")
}

# origin: busybox realpath/realpath on non-existent local file 1
test test_bb_realpath_realpath_on_non_existent_local_file_1_180b285b { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["realpath_testdir/not_file"])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root.resolve()?}/realpath_testdir/not_file\n")
}

# origin: busybox realpath/realpath on non-existent local file 2
test test_bb_realpath_realpath_on_non_existent_local_file_2_d3a70692 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "realpath_testdir")?
  uu.symlink(s, "./realpath_testdir/not_file", "link1")?
  uu.symlink(s, "./realpath_testdir/not_file/not_dir", "link2")?
  let r = uu.invoke(s, "realpath", ["realpath_testdir/not_dir/not_file"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "realpath: realpath_testdir/not_dir/not_file: No such file or directory\n")
}

