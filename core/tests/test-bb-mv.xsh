use support.uu

# origin: busybox mv/mv-files-to-dir
test test_bb_mv_mv_files_to_dir_c6eb6c9b { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "file2", "file number two\n")?
  uu.symlink(s, "file2", "link1")?
  uu.mkdir(s, "dir1")?
  uu.touch(s, "dir1/file3")?
  uu.succeeds(uu.invoke(s, "touch", ["-d", "2000-01-30 05:24:08", "dir1/file3"], vars: {TZ: "UTC0"})?)
  uu.mkdir(s, "there")?
  uu.succeeds(uu.invoke(s, "mv", ["file1", "file2", "link1", "dir1", "there"])?)
  assert uu.file_exists(s, "there/file1")?
  assert !uu.exists(s, "file1")?
  assert uu.file_exists(s, "there/file2")?
  assert !uu.exists(s, "file2")?
  assert uu.file_exists(s, "there/dir1/file3")?
  assert !uu.exists(s, "dir1/file3")?
  assert uu.is_symlink(s, "there/link1")?
  assert uu.read_link(s, "there/link1")? == "file2"
  assert !uu.exists(s, "link1")?
}

# origin: busybox mv/mv-files-to-dir-2
test test_bb_mv_mv_files_to_dir_2_bc62d20c { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "file2", "file number two\n")?
  uu.symlink(s, "file2", "link1")?
  uu.mkdir(s, "dir1")?
  uu.touch(s, "dir1/file3")?
  uu.succeeds(uu.invoke(s, "touch", ["-d", "2000-01-30 05:24:08", "dir1/file3"], vars: {TZ: "UTC0"})?)
  uu.mkdir(s, "there")?
  uu.succeeds(uu.invoke(s, "mv", ["-t", "there", "file1", "file2", "link1", "dir1"])?)
  assert uu.file_exists(s, "there/file1")?
  assert !uu.exists(s, "file1")?
  assert uu.file_exists(s, "there/file2")?
  assert !uu.exists(s, "file2")?
  assert uu.file_exists(s, "there/dir1/file3")?
  assert !uu.exists(s, "dir1/file3")?
  assert uu.is_symlink(s, "there/link1")?
  assert uu.read_link(s, "there/link1")? == "file2"
  assert !uu.exists(s, "link1")?
}

# origin: busybox mv/mv-follows-links
test test_bb_mv_mv_follows_links_95d4c49f { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, "foo", "bar")?
  uu.succeeds(uu.invoke(s, "mv", ["bar", "baz"])?)
  assert uu.at(s, "baz").resolve()?.is_file()?
}

# origin: busybox mv/mv-moves-empty-file
test test_bb_mv_mv_moves_empty_file_fd51fd98 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "mv", ["foo", "bar"])?)
  assert !uu.exists(s, "foo")?
  assert uu.file_exists(s, "bar")?
}

# origin: busybox mv/mv-moves-file
test test_bb_mv_mv_moves_file_1af36313 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "mv", ["foo", "bar"])?)
  assert !uu.exists(s, "foo")?
  assert uu.file_exists(s, "bar")?
}

# origin: busybox mv/mv-moves-hardlinks
test test_bb_mv_mv_moves_hardlinks_8436df55 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.hard_link(s, "foo", "bar")?
  uu.succeeds(uu.invoke(s, "mv", ["bar", "baz"])?)
  assert !uu.exists(s, "bar")?
  assert uu.file_exists(s, "baz")?
}

# origin: busybox mv/mv-moves-large-file
test test_bb_mv_mv_moves_large_file_c4d092d4 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.truncate(s, "foo", 5243392)?
  uu.succeeds(uu.invoke(s, "mv", ["foo", "bar"])?)
  assert !uu.exists(s, "foo")?
  assert uu.file_exists(s, "bar")?
}

# origin: busybox mv/mv-moves-small-file
test test_bb_mv_mv_moves_small_file_2e4033e0 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "I WANT\n")?
  uu.succeeds(uu.invoke(s, "mv", ["foo", "bar"])?)
  assert !uu.exists(s, "foo")?
  assert uu.file_exists(s, "bar")?
}

# origin: busybox mv/mv-moves-symlinks
test test_bb_mv_mv_moves_symlinks_74b8a131 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, "foo", "bar")?
  uu.succeeds(uu.invoke(s, "mv", ["bar", "baz"])?)
  assert uu.file_exists(s, "foo")?
  assert !uu.exists(s, "bar")?
  assert uu.is_symlink(s, "baz")?
}

# origin: busybox mv/mv-moves-unreadable-files
test test_bb_mv_mv_moves_unreadable_files_18d15fbc { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.set_mode(s, "foo", 0o222)?
  uu.succeeds(uu.invoke(s, "mv", ["foo", "bar"])?)
  assert !uu.exists(s, "foo")?
  assert uu.file_exists(s, "bar")?
}

# origin: busybox mv/mv-preserves-hard-links
test test_bb_mv_mv_preserves_hard_links_b7a2fb90 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.hard_link(s, "foo", "bar")?
  uu.mkdir(s, "baz")?
  uu.succeeds(uu.invoke(s, "mv", ["foo", "bar", "baz"])?)
  assert fs.stat(uu.at(s, "baz/foo"))?.ino == fs.stat(uu.at(s, "baz/bar"))?.ino
}

# origin: busybox mv/mv-preserves-links
test test_bb_mv_mv_preserves_links_8a719dbb { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, "foo", "bar")?
  uu.succeeds(uu.invoke(s, "mv", ["bar", "baz"])?)
  assert uu.is_symlink(s, "baz")?
  assert uu.read_link(s, "baz")? == "foo"
}

# origin: busybox mv/mv-refuses-mv-dir-to-subdir
test test_bb_mv_mv_refuses_mv_dir_to_subdir_e20c1615 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "file2", "file number two\n")?
  uu.symlink(s, "file2", "link1")?
  uu.mkdir(s, "dir1")?
  uu.touch(s, "dir1/file3")?
  uu.succeeds(uu.invoke(s, "touch", ["-d", "2000-01-30 05:24:08", "dir1/file3"], vars: {TZ: "UTC0"})?)
  uu.mkdir(s, "there")?
  uu.succeeds(uu.invoke(s, "mv", ["file1", "file2", "link1", "dir1", "there"])?)
  assert uu.file_exists(s, "there/file1")?
  assert !uu.exists(s, "file1")?
  assert uu.file_exists(s, "there/file2")?
  assert !uu.exists(s, "file2")?
  assert uu.file_exists(s, "there/dir1/file3")?
  assert !uu.exists(s, "dir1/file3")?
  assert uu.is_symlink(s, "there/link1")?
  assert uu.read_link(s, "there/link1")? == "file2"
  assert !uu.exists(s, "link1")?
  uu.fails(uu.invoke(s, "mv", ["there", "there/dir1"])?)
}

# origin: busybox mv/mv-removes-source-file
test test_bb_mv_mv_removes_source_file_e924ea67 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "mv", ["foo", "bar"])?)
  assert !uu.exists(s, "foo")?
  assert uu.file_exists(s, "bar")?
}

