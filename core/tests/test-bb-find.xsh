use support.uu

# origin: busybox find/find ./// -name .
test test_bb_find_find_name_9fe1cb1f { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "find.tempdir")?
  uu.touch(s, "find.tempdir/testfile")?
  let r = uu.invoke(s, "find", [".///", "-name", "."], timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, ".///\n")
}

# origin: busybox find/find ./// -name .///
test test_bb_find_find_name_b975c3ea { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "find.tempdir")?
  uu.touch(s, "find.tempdir/testfile")?
  let r = uu.invoke(s, "find", [".///", "-name", ".///"], timeout: 5s)?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: busybox find/find-supports-minus-xdev
test test_bb_find_find_supports_minus_xdev_7ca1b085 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "find", [".", "-xdev"], stdout: p"/dev/null", stderr: p"/dev/null", timeout: 5s)?
  uu.succeeds(r)
}
