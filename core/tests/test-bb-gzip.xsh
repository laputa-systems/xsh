use support.uu

# origin: busybox gzip/gzip-accepts-multiple-files
test test_bb_gzip_gzip_accepts_multiple_files_eb79442c { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.touch(s, "bar")?
  uu.succeeds(uu.invoke(s, "gzip", ["foo", "bar"])?)
  assert uu.file_exists(s, "foo.gz")?
  assert uu.file_exists(s, "bar.gz")?
}

# origin: busybox gzip/gzip-accepts-single-minus
test test_bb_gzip_gzip_accepts_single_minus_22f7f00f { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "gzip", ["-"], stdin: b"foo\n")?)
}

# origin: busybox gzip/gzip-compression-levels
test test_bb_gzip_gzip_compression_levels_81785e0c { |ctx|
  let s = uu.scene(ctx)?
  let fast = uu.invoke(s, "gzip", ["-c", "-1", "/bin/busybox"])?
  let dense = uu.invoke(s, "gzip", ["-c", "-9", "/bin/busybox"])?
  uu.succeeds(fast)
  uu.succeeds(dense)
  assert fast.stdout.len() > dense.stdout.len()
}

# origin: busybox gzip/gzip-removes-original-file
test test_bb_gzip_gzip_removes_original_file_d8c1d446 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "gzip", ["foo"])?)
  assert ! uu.exists(s, "foo")?
}
