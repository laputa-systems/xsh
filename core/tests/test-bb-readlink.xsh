use support.uu as uu

# origin: busybox readlink/readlink -f on a file
test test_bb_readlink_readlink_f_on_a_file_47d5bb7a { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "readlink_testdir")?
  uu.touch(s, "readlink_testdir/testfile")?
  uu.symlink(s, "./readlink_testdir/testfile", "testlink")?
  let r = uu.invoke(s, "readlink", ["-f", "./readlink_testdir/testfile"])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root.resolve()?}/readlink_testdir/testfile\n")
}

# origin: busybox readlink/readlink -f on a link
test test_bb_readlink_readlink_f_on_a_link_92b21923 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "readlink_testdir")?
  uu.touch(s, "readlink_testdir/testfile")?
  uu.symlink(s, "./readlink_testdir/testfile", "testlink")?
  let r = uu.invoke(s, "readlink", ["-f", "./testlink"])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root.resolve()?}/readlink_testdir/testfile\n")
}

# origin: busybox readlink/readlink -f on a weird dir
test test_bb_readlink_readlink_f_on_a_weird_dir_82682099 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "readlink_testdir")?
  uu.touch(s, "readlink_testdir/testfile")?
  uu.symlink(s, "./readlink_testdir/testfile", "testlink")?
  let r = uu.invoke(s, "readlink", ["-f", "readlink_testdir/../readlink_testdir/testfile"])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root.resolve()?}/readlink_testdir/testfile\n")
}

# origin: busybox readlink/readlink -f on an invalid link
test test_bb_readlink_readlink_f_on_an_invalid_link_e02d1fc5 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "readlink_testdir")?
  uu.touch(s, "readlink_testdir/testfile")?
  uu.symlink(s, "./readlink_testdir/testfile", "testlink")?
  let r = uu.invoke(s, "readlink", ["-f", "./readlink_testdir/readlink_testdir/testlink"])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox readlink/readlink on a file
test test_bb_readlink_readlink_on_a_file_645c4335 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "readlink_testdir")?
  uu.touch(s, "readlink_testdir/testfile")?
  uu.symlink(s, "./readlink_testdir/testfile", "testlink")?
  let r = uu.invoke(s, "readlink", ["./readlink_testdir/testfile"])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox readlink/readlink on a link
test test_bb_readlink_readlink_on_a_link_42ac5aa6 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "readlink_testdir")?
  uu.touch(s, "readlink_testdir/testfile")?
  uu.symlink(s, "./readlink_testdir/testfile", "testlink")?
  let r = uu.invoke(s, "readlink", ["./testlink"])?
  uu.succeeds(r)
  uu.stdout_only(r, "./readlink_testdir/testfile\n")
}

