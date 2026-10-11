use support.uu

# origin: gnu mkdir/p-1.log
test test_gnu_mkdir_p_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mkdir", ["--parents", uu.at(s, "t").display()])?)
  assert uu.dir_exists(s, "t")?
}

# origin: gnu mkdir/p-2.log
test test_gnu_mkdir_p_2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mkdir", ["--parents", uu.at(s, "t/u").display()])?)
  assert uu.dir_exists(s, "t/u")?
}

# origin: gnu mkdir/p-slashdot.log
test test_gnu_mkdir_p_slashdot_log { |ctx|
  let s = uu.scene(ctx)?
  for item in [{operand: "d1/.", directory: "d1"}, {operand: "d2/..", directory: "d2"}] {
    uu.succeeds(uu.invoke(s, "mkdir", ["-p", item.operand])?)
    assert uu.dir_exists(s, item.directory)?
  }
}

# origin: gnu mkdir/p-thru-slink.log
test test_gnu_mkdir_p_thru_slink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, ".", "slink")?
  uu.succeeds(uu.invoke(s, "mkdir", ["-p", "slink/x"])?)
  assert uu.dir_exists(s, "x")?
}

# origin: gnu mkdir/p-v.log
test test_gnu_mkdir_p_v_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mkdir", ["-pv", "foo/a/b/c/d"])?
  uu.succeeds(r)
  uu.stdout_is(r, "mkdir: created directory 'foo'\nmkdir: created directory 'foo/a'\nmkdir: created directory 'foo/a/b'\nmkdir: created directory 'foo/a/b/c'\nmkdir: created directory 'foo/a/b/c/d'\n")
}

# origin: gnu mkdir/parents.log
test test_gnu_mkdir_parents_log { |ctx|
  let s = uu.scene(ctx)?
  if fs.stat(s.root)?.mode.bit_and(0o2000) != 0 { test.skip("requires a parent without setgid inheritance"); return }
  uu.succeeds(uu.invoke(s, "mkdir", ["-m", "700", "e-dir"])?)
  uu.succeeds(uu.invoke(s, "mkdir", ["-p", "e-dir"])?)
  uu.fails_with_code(uu.invoke(s, "mkdir", ["e-dir"])?, 1)
  uu.succeeds(uu.invoke(s, "mkdir", ["-m", "753", "a"], umask: 0o077)?)
  let _ = uu.invoke(s, "mkdir", ["-p", "-m", "723", "a/b/c/d"], umask: 0o077)?
  assert uu.mode(s, "a")? == 0o753
  assert uu.mode(s, "a/b")? == 0o700
  assert uu.mode(s, "a/b/c")? == 0o700
  assert uu.mode(s, "a/b/c/d")? == 0o723
}

# origin: gnu mkdir/special-1.log
test test_gnu_mkdir_special_1_log { |ctx|
  let s = uu.scene(ctx)?
  let mode = "-mu=rwx,g=rx,o=w,-s,+t"
  uu.succeeds(uu.invoke(s, "mkdir", [mode, "t"])?)
  assert uu.dir_exists(s, "t")?
  assert uu.mode(s, "t")? == 0o1752
  uu.at(s, "t").remove_dir()?
  uu.fails_with_code(uu.invoke(s, "mkdir", [mode, "t/sub"])?, 1)
  uu.succeeds(uu.invoke(s, "mkdir", ["--parents", mode, "t/sub"])?)
  assert uu.dir_exists(s, "t/sub")?
  assert uu.mode(s, "t/sub")? == 0o1752
}

# origin: gnu mkdir/t-slash.log
test test_gnu_mkdir_t_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mkdir", ["-p", "dir/"])?)
  assert uu.dir_exists(s, "dir")?
  uu.succeeds(uu.invoke(s, "mkdir", ["d2/"])?)
  assert uu.dir_exists(s, "d2")?
}
