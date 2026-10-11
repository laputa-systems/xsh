use support.uu as uu

# origin: busybox cp/cp -R
test test_bb_cp_cp_R_9402f971 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "cp.testdir/dir")?
  uu.touch(s, "cp.testdir/dir/file")?
  uu.symlink(s, "file", "cp.testdir/dir/file_symlink")?
  uu.touch(s, "cp.testdir/file")?
  uu.symlink(s, "file", "cp.testdir/file_symlink")?
  uu.symlink(s, "dir", "cp.testdir/dir_symlink")?
  uu.mkdir(s, "cp.testdir2")?
  let source = {ctx: ctx, root: uu.at(s, "cp.testdir")}
  uu.succeeds(uu.invoke(source, "cp", ["-R", "dir", "dir_symlink", "file", "file_symlink", "../cp.testdir2"])?)
  assert uu.at(s, "cp.testdir2/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir").resolve()?.is_dir()?
  assert uu.at(s, "cp.testdir2/dir_symlink").resolve()?.is_dir()?
  assert !uu.is_symlink(s, "cp.testdir2/file")?
  assert !uu.is_symlink(s, "cp.testdir2/dir")?
  assert !uu.is_symlink(s, "cp.testdir2/dir/file")?
  assert uu.is_symlink(s, "cp.testdir2/file_symlink")?
  assert uu.is_symlink(s, "cp.testdir2/dir_symlink")?
  assert uu.is_symlink(s, "cp.testdir2/dir/file_symlink")?
}

# origin: busybox cp/cp -RH
test test_bb_cp_cp_RH_4e5d93b0 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "cp.testdir/dir")?
  uu.touch(s, "cp.testdir/dir/file")?
  uu.symlink(s, "file", "cp.testdir/dir/file_symlink")?
  uu.touch(s, "cp.testdir/file")?
  uu.symlink(s, "file", "cp.testdir/file_symlink")?
  uu.symlink(s, "dir", "cp.testdir/dir_symlink")?
  uu.mkdir(s, "cp.testdir2")?
  let source = {ctx: ctx, root: uu.at(s, "cp.testdir")}
  uu.succeeds(uu.invoke(source, "cp", ["-RH", "dir", "dir_symlink", "file", "file_symlink", "../cp.testdir2"])?)
  assert uu.at(s, "cp.testdir2/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir").resolve()?.is_dir()?
  assert uu.at(s, "cp.testdir2/dir_symlink").resolve()?.is_dir()?
  assert !uu.is_symlink(s, "cp.testdir2/file")?
  assert !uu.is_symlink(s, "cp.testdir2/dir")?
  assert !uu.is_symlink(s, "cp.testdir2/dir/file")?
  assert !uu.is_symlink(s, "cp.testdir2/file_symlink")?
  assert !uu.is_symlink(s, "cp.testdir2/dir_symlink")?
  assert uu.is_symlink(s, "cp.testdir2/dir/file_symlink")?
}

# origin: busybox cp/cp -RHL
test test_bb_cp_cp_RHL_ea37f208 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "cp.testdir/dir")?
  uu.touch(s, "cp.testdir/dir/file")?
  uu.symlink(s, "file", "cp.testdir/dir/file_symlink")?
  uu.touch(s, "cp.testdir/file")?
  uu.symlink(s, "file", "cp.testdir/file_symlink")?
  uu.symlink(s, "dir", "cp.testdir/dir_symlink")?
  uu.mkdir(s, "cp.testdir2")?
  let source = {ctx: ctx, root: uu.at(s, "cp.testdir")}
  uu.succeeds(uu.invoke(source, "cp", ["-RHL", "dir", "dir_symlink", "file", "file_symlink", "../cp.testdir2"])?)
  assert uu.at(s, "cp.testdir2/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir").resolve()?.is_dir()?
  assert uu.at(s, "cp.testdir2/dir_symlink").resolve()?.is_dir()?
  assert !uu.is_symlink(s, "cp.testdir2/file")?
  assert !uu.is_symlink(s, "cp.testdir2/dir")?
  assert !uu.is_symlink(s, "cp.testdir2/dir/file")?
  assert !uu.is_symlink(s, "cp.testdir2/file_symlink")?
  assert !uu.is_symlink(s, "cp.testdir2/dir_symlink")?
  assert !uu.is_symlink(s, "cp.testdir2/dir/file_symlink")?
}

# origin: busybox cp/cp -RL
test test_bb_cp_cp_RL_998aff81 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "cp.testdir/dir")?
  uu.touch(s, "cp.testdir/dir/file")?
  uu.symlink(s, "file", "cp.testdir/dir/file_symlink")?
  uu.touch(s, "cp.testdir/file")?
  uu.symlink(s, "file", "cp.testdir/file_symlink")?
  uu.symlink(s, "dir", "cp.testdir/dir_symlink")?
  uu.mkdir(s, "cp.testdir2")?
  let source = {ctx: ctx, root: uu.at(s, "cp.testdir")}
  uu.succeeds(uu.invoke(source, "cp", ["-RL", "dir", "dir_symlink", "file", "file_symlink", "../cp.testdir2"])?)
  assert uu.at(s, "cp.testdir2/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir").resolve()?.is_dir()?
  assert uu.at(s, "cp.testdir2/dir_symlink").resolve()?.is_dir()?
  assert !uu.is_symlink(s, "cp.testdir2/file")?
  assert !uu.is_symlink(s, "cp.testdir2/dir")?
  assert !uu.is_symlink(s, "cp.testdir2/dir/file")?
  assert !uu.is_symlink(s, "cp.testdir2/file_symlink")?
  assert !uu.is_symlink(s, "cp.testdir2/dir_symlink")?
  assert !uu.is_symlink(s, "cp.testdir2/dir/file_symlink")?
}

# origin: busybox cp/cp -RP
test test_bb_cp_cp_RP_68d80992 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "cp.testdir/dir")?
  uu.touch(s, "cp.testdir/dir/file")?
  uu.symlink(s, "file", "cp.testdir/dir/file_symlink")?
  uu.touch(s, "cp.testdir/file")?
  uu.symlink(s, "file", "cp.testdir/file_symlink")?
  uu.symlink(s, "dir", "cp.testdir/dir_symlink")?
  uu.mkdir(s, "cp.testdir2")?
  let source = {ctx: ctx, root: uu.at(s, "cp.testdir")}
  uu.succeeds(uu.invoke(source, "cp", ["-RP", "dir", "dir_symlink", "file", "file_symlink", "../cp.testdir2"])?)
  assert uu.at(s, "cp.testdir2/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir").resolve()?.is_dir()?
  assert uu.at(s, "cp.testdir2/dir_symlink").resolve()?.is_dir()?
  assert !uu.is_symlink(s, "cp.testdir2/file")?
  assert !uu.is_symlink(s, "cp.testdir2/dir")?
  assert !uu.is_symlink(s, "cp.testdir2/dir/file")?
  assert uu.is_symlink(s, "cp.testdir2/file_symlink")?
  assert uu.is_symlink(s, "cp.testdir2/dir_symlink")?
  assert uu.is_symlink(s, "cp.testdir2/dir/file_symlink")?
}

# origin: busybox cp/cp -Rd
test test_bb_cp_cp_Rd_139d4e2b { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "cp.testdir/dir")?
  uu.touch(s, "cp.testdir/dir/file")?
  uu.symlink(s, "file", "cp.testdir/dir/file_symlink")?
  uu.touch(s, "cp.testdir/file")?
  uu.symlink(s, "file", "cp.testdir/file_symlink")?
  uu.symlink(s, "dir", "cp.testdir/dir_symlink")?
  uu.mkdir(s, "cp.testdir2")?
  let source = {ctx: ctx, root: uu.at(s, "cp.testdir")}
  uu.succeeds(uu.invoke(source, "cp", ["-Rd", "dir", "dir_symlink", "file", "file_symlink", "../cp.testdir2"])?)
  assert uu.at(s, "cp.testdir2/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir/file_symlink").resolve()?.is_file()?
  assert uu.at(s, "cp.testdir2/dir").resolve()?.is_dir()?
  assert uu.at(s, "cp.testdir2/dir_symlink").resolve()?.is_dir()?
  assert !uu.is_symlink(s, "cp.testdir2/file")?
  assert !uu.is_symlink(s, "cp.testdir2/dir")?
  assert !uu.is_symlink(s, "cp.testdir2/dir/file")?
  assert uu.is_symlink(s, "cp.testdir2/file_symlink")?
  assert uu.is_symlink(s, "cp.testdir2/dir_symlink")?
  assert uu.is_symlink(s, "cp.testdir2/dir/file_symlink")?
}

# origin: busybox cp/cp-RHL-does_not_preserve-links
test test_bb_cp_cp_RHL_does_not_preserve_links_396db8b8 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.touch(s, "a/file")?
  uu.symlink(s, "file", "a/link")?
  uu.succeeds(uu.invoke(s, "cp", ["-RHL", "a", "b"])?)
  assert !uu.is_symlink(s, "b/link")?
}

# origin: busybox cp/cp-a-files-to-dir
test test_bb_cp_cp_a_files_to_dir_de355ad7 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "file2", "file number two\n")?
  uu.symlink(s, "file2", "link1")?
  uu.mkdir(s, "there")?
  uu.mkdir(s, "dir1")?
  uu.succeeds(uu.invoke(s, "cp", ["-a", "file1", "file2", "link1", "dir1", "there"])?)
  assert uu.file_exists(s, "there/file1")?
  assert uu.file_exists(s, "there/file2")?
  assert !uu.exists(s, "there/dir1/file3")?
  assert !uu.exists(s, "dir1/file3")?
  assert uu.is_symlink(s, "there/link1")?
  assert uu.read_link(s, "there/link1")? == "file2"
}

# origin: busybox cp/cp-a-preserves-links
test test_bb_cp_cp_a_preserves_links_09e11d53 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, "foo", "bar")?
  uu.succeeds(uu.invoke(s, "cp", ["-a", "bar", "baz"])?)
  assert uu.is_symlink(s, "baz")?
  assert uu.read_link(s, "baz")? == "foo"
}

# origin: busybox cp/cp-copies-empty-file
test test_bb_cp_cp_copies_empty_file_05eff620 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "cp", ["foo", "bar"])?)
  assert uu.read(s, "foo")? == uu.read(s, "bar")?
}

# origin: busybox cp/cp-copies-large-file
test test_bb_cp_cp_copies_large_file_cd742440 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.truncate(s, "foo", 5243392)?
  uu.succeeds(uu.invoke(s, "cp", ["foo", "bar"])?)
  assert uu.read(s, "foo")? == uu.read(s, "bar")?
}

# origin: busybox cp/cp-copies-small-file
test test_bb_cp_cp_copies_small_file_394ce5e0 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "I WANT\n")?
  uu.succeeds(uu.invoke(s, "cp", ["foo", "bar"])?)
  assert uu.read(s, "foo")? == uu.read(s, "bar")?
}

# origin: busybox cp/cp-d-files-to-dir
test test_bb_cp_cp_d_files_to_dir_0d478733 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "file2", "file number two\n")?
  uu.symlink(s, "file2", "link1")?
  uu.mkdir(s, "there")?
  uu.touch(s, "file3")?
  uu.succeeds(uu.invoke(s, "cp", ["-d", "file1", "file2", "file3", "link1", "there"])?)
  assert uu.file_exists(s, "there/file1")?
  assert uu.file_exists(s, "there/file2")?
  assert uu.size(s, "there/file3")? == 0
  assert uu.is_symlink(s, "there/link1")?
  assert uu.read_link(s, "there/link1")? == "file2"
}

# origin: busybox cp/cp-dev-file
test test_bb_cp_cp_dev_file_65c96361 { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "cp", ["/dev/null", "foo"])?)
  assert uu.file_exists(s, "foo")?
}

# origin: busybox cp/cp-dir-create-dir
test test_bb_cp_cp_dir_create_dir_e14fba14 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "bar")?
  uu.touch(s, "bar/baz")?
  uu.succeeds(uu.invoke(s, "cp", ["-R", "bar", "foo"])?)
  assert uu.file_exists(s, "foo/baz")?
}

# origin: busybox cp/cp-dir-existing-dir
test test_bb_cp_cp_dir_existing_dir_af0b2b6d { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "bar")?
  uu.touch(s, "bar/baz")?
  uu.mkdir(s, "foo")?
  uu.succeeds(uu.invoke(s, "cp", ["-R", "bar", "foo"])?)
  assert uu.file_exists(s, "foo/bar/baz")?
}

# origin: busybox cp/cp-does-not-copy-unreadable-file
test test_bb_cp_cp_does_not_copy_unreadable_file_b036d15e { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.set_mode(s, "foo", 0o222)?
  let r = uu.invoke(s, "cp", ["foo", "bar"])?
  uu.fails(r)
  assert !uu.exists(s, "bar")?
}

# origin: busybox cp/cp-files-to-dir
test test_bb_cp_cp_files_to_dir_68cf19e5 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "file2", "file number two\n")?
  uu.symlink(s, "file2", "link1")?
  uu.mkdir(s, "there")?
  uu.touch(s, "file3")?
  uu.succeeds(uu.invoke(s, "cp", ["file1", "file2", "file3", "link1", "there"])?)
  assert uu.file_exists(s, "there/file1")?
  assert uu.file_exists(s, "there/file2")?
  assert uu.size(s, "there/file3")? == 0
  assert uu.file_exists(s, "there/link1")?
  assert uu.read(s, "there/file2")? == uu.read(s, "there/link1")?
}

# origin: busybox cp/cp-follows-links
test test_bb_cp_cp_follows_links_8e0025cc { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, "foo", "bar")?
  uu.succeeds(uu.invoke(s, "cp", ["bar", "baz"])?)
  assert uu.file_exists(s, "baz")?
}

# origin: busybox cp/cp-parents
test test_bb_cp_cp_parents_d482d63d { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "foo/bar/baz")?
  uu.touch(s, "foo/bar/baz/file")?
  uu.mkdir(s, "dir")?
  uu.succeeds(uu.invoke(s, "cp", ["--parents", "foo/bar/baz/file", "dir"])?)
  assert uu.file_exists(s, "dir/foo/bar/baz/file")?
}

# origin: busybox cp/cp-preserves-hard-links
test test_bb_cp_cp_preserves_hard_links_dd1d04b6 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.hard_link(s, "foo", "bar")?
  uu.mkdir(s, "baz")?
  uu.succeeds(uu.invoke(s, "cp", ["-d", "foo", "bar", "baz"])?)
  assert fs.stat(uu.at(s, "baz/foo"))?.ino == fs.stat(uu.at(s, "baz/bar"))?.ino
}

# origin: busybox cp/cp-preserves-links
test test_bb_cp_cp_preserves_links_9bf08b66 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, "foo", "bar")?
  uu.succeeds(uu.invoke(s, "cp", ["-d", "bar", "baz"])?)
  assert uu.is_symlink(s, "baz")?
  assert uu.read_link(s, "baz")? == "foo"
}

# origin: busybox cp/cp-preserves-source-file
test test_bb_cp_cp_preserves_source_file_445afd58 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "cp", ["foo", "bar"])?)
  assert uu.file_exists(s, "foo")?
}

