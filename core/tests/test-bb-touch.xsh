use support.uu as uu

# origin: busybox touch/touch-creates-file
test test_bb_touch_touch_creates_file_cfefc9ca { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "touch", ["foo"])?)
  assert uu.file_exists(s, "foo")?
}

# origin: busybox touch/touch-does-not-create-file
test test_bb_touch_touch_does_not_create_file_f63b146e { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "touch", ["-c", "foo"])?)
  assert !uu.exists(s, "foo")?
}

# origin: busybox touch/touch-touches-files-after-non-existent-file
test test_bb_touch_touch_touches_files_after_non_existent_file_f36ed69e { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "touch", ["-t", "198001010000", "bar"])?)
  uu.succeeds(uu.invoke(s, "touch", ["-c", "foo", "bar"])?)
  assert time.now() - fs.stat(uu.at(s, "bar"))?.mtime_ns < 86400000000000
}

