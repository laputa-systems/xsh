##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_cp.rs.

use support.uu as uu

proc fixture_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  for name in ["hello_world.txt", "how_are_you.txt", "existing_file.txt"] {
    uu.fixture(s, "cp", name, name)?
  }
  uu.mkdir(s, "hello_dir")?
  uu.mkdir(s, "hello_dir_with_file")?
  uu.fixture(s, "cp", "hello_dir_with_file/hello_world.txt", "hello_dir_with_file/hello_world.txt")?
  Ok(s)
}

# origin: uutils test_cp::test_cp_custom_backup_suffix_via_env
test test_uu_cp_cp_custom_backup_suffix_via_env { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["-b", "hello_world.txt", "how_are_you.txt"], vars: {SIMPLE_BACKUP_SUFFIX: "super-suffix-of-the-century"})?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txtsuper-suffix-of-the-century", "How are you?\n")
}

# origin: uutils test_cp::test_cp_d_overwrites_existing_symlink_dest
test test_uu_cp_cp_d_overwrites_existing_symlink_dest { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.touch(s, "slink-target")?
  uu.symlink(s, "slink-target", "slink")?
  let r = uu.invoke(s, "cp", ["-d", "f", "slink"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "slink")?
  assert uu.read_link(s, "slink")?.ends_with("slink-target")
}

# origin: uutils test_cp::test_cp_dangling_symlink_inside_directory
test test_uu_cp_cp_dangling_symlink_inside_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "good")?
  uu.mkdir(s, "tmp")?
  uu.write(s, "README", "file1")?
  uu.write(s, "good/README", "file2")?
  uu.symlink(s, "foo", "tmp/README")?
  let r = uu.invoke(s, "cp", ["README", "good/README", "tmp"])?
  uu.fails(r)
  uu.stderr_only(r, "cp: not writing through dangling symlink 'tmp/README'\ncp: not writing through dangling symlink 'tmp/README'\n")
}

# origin: uutils test_cp::test_cp_debug_default
test test_uu_cp_cp_debug_default { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["--debug", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: unknown, reflink: Operation not supported, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_default_empty_file_with_hole
test test_uu_cp_cp_debug_default_empty_file_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.truncate(s, "a", 10000)?
  let r = uu.invoke(s, "cp", ["--debug", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: unknown, reflink: Operation not supported, sparse detection: SEEK_HOLE")
  assert fs.stat(uu.at(s, "a"))?.blocks_512 == fs.stat(uu.at(s, "b"))?.blocks_512
}

# origin: uutils test_cp::test_cp_debug_default_less_than_512_bytes
test test_uu_cp_cp_debug_default_less_than_512_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "hello")?
  uu.truncate(s, "a", 400)?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=auto", "--sparse=auto", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: yes, reflink: Operation not supported, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_default_sparse_virtual_file
test test_uu_cp_cp_debug_default_sparse_virtual_file { |ctx|
  let s = uu.scene(ctx)?
  if !p"/sys/kernel/profiling".is_file()? { return }
  let r = uu.invoke(s, "cp", ["--debug", "/sys/kernel/profiling", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: Invalid cross-device link, reflink: Invalid cross-device link, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_default_with_hole
test test_uu_cp_cp_debug_default_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.truncate(s, "a", 4096 * 4)?
  let _ = bytes.write_at(uu.at(s, "a"), 16384, b"hello")?
  let r = uu.invoke(s, "cp", ["--debug", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: yes, reflink: Operation not supported, sparse detection: SEEK_HOLE")
  assert fs.stat(uu.at(s, "a"))?.blocks_512 == fs.stat(uu.at(s, "b"))?.blocks_512
}

# origin: uutils test_cp::test_cp_debug_default_without_hole
test test_uu_cp_cp_debug_default_without_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a", bytes.concat([b"hello", bytes.zero(10000)?]))?
  let r = uu.invoke(s, "cp", ["--debug", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: yes, reflink: Operation not supported, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_default_zero_sized_virtual_file
test test_uu_cp_cp_debug_default_zero_sized_virtual_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["--debug", "/proc/version", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: Invalid cross-device link, reflink: Invalid cross-device link, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_multiple_default
test test_uu_cp_cp_debug_multiple_default { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  uu.mkdir(s, "dir")?
  let r = uu.invoke(s, "cp", ["--debug", "a", "b", "dir"])?
  uu.succeeds(r)
  assert r.stdout.utf8()?.split("copy offload: unknown, reflink: Operation not supported, sparse detection: no").len() == 3
}

# origin: uutils test_cp::test_cp_debug_no_update
test test_uu_cp_cp_debug_no_update { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cp", ["--debug", "--update=none", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "skipped 'b'")
}

# origin: uutils test_cp::test_cp_debug_reflink_auto
test test_uu_cp_cp_debug_reflink_auto { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=auto", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: unknown, reflink: Operation not supported, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_reflink_auto_sparse_always_non_sparse_file_with_long_zero_sequence
test test_uu_cp_cp_debug_reflink_auto_sparse_always_non_sparse_file_with_long_zero_sequence { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a", bytes.concat([bytes.zero(4096 * 4)?, b"hello"]))?
  let r = uu.invoke(s, "cp", ["--debug", "--sparse=always", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: Operation not supported, sparse detection: zeros")
  let dst = fs.stat(uu.at(s, "b"))?
  assert dst.blocks_512 == dst.blksize / 512
}

# origin: uutils test_cp::test_cp_debug_reflink_never_empty_file_with_hole
test test_uu_cp_cp_debug_reflink_never_empty_file_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.truncate(s, "a", 10000)?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: unknown, reflink: no, sparse detection: SEEK_HOLE")
  assert fs.stat(uu.at(s, "a"))?.blocks_512 == fs.stat(uu.at(s, "b"))?.blocks_512
}

# origin: uutils test_cp::test_cp_debug_reflink_never_file_with_hole
test test_uu_cp_cp_debug_reflink_never_file_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.truncate(s, "a", 4096 * 4)?
  let _ = bytes.write_at(uu.at(s, "a"), 16384, b"hello")?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "--sparse=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: SEEK_HOLE")
}

# origin: uutils test_cp::test_cp_debug_reflink_never_less_than_512_bytes
test test_uu_cp_cp_debug_reflink_never_less_than_512_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "hello")?
  uu.truncate(s, "a", 400)?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_reflink_never_sparse_always_empty_file_with_hole
test test_uu_cp_cp_debug_reflink_never_sparse_always_empty_file_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.truncate(s, "a", 10000)?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "--sparse=always", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: unknown, reflink: no, sparse detection: SEEK_HOLE")
}

# origin: uutils test_cp::test_cp_debug_reflink_never_sparse_always_non_sparse_file_with_long_zero_sequence
test test_uu_cp_cp_debug_reflink_never_sparse_always_non_sparse_file_with_long_zero_sequence { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a", bytes.concat([bytes.zero(4096 * 4)?, b"hello"]))?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "--sparse=always", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: zeros")
  let dst = fs.stat(uu.at(s, "b"))?
  assert dst.blocks_512 == dst.blksize / 512
}

# origin: uutils test_cp::test_cp_debug_reflink_never_sparse_always_with_hole
test test_uu_cp_cp_debug_reflink_never_sparse_always_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "hello")?
  uu.truncate(s, "a", 4096 * 4)?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "--sparse=always", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: SEEK_HOLE + zeros")
  assert fs.stat(uu.at(s, "a"))?.blocks_512 == fs.stat(uu.at(s, "b"))?.blocks_512
}

# origin: uutils test_cp::test_cp_debug_reflink_never_sparse_always_without_hole
test test_uu_cp_cp_debug_reflink_never_sparse_always_without_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a", bytes.concat([b"hello", bytes.zero(10000)?]))?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "--sparse=always", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: zeros")
  let dst = fs.stat(uu.at(s, "b"))?
  assert dst.blocks_512 == dst.blksize / 512
}

# origin: uutils test_cp::test_cp_debug_reflink_never_sparse_never_empty_file_with_hole
test test_uu_cp_cp_debug_reflink_never_sparse_never_empty_file_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.truncate(s, "a", 10000)?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "--sparse=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: unknown, reflink: no, sparse detection: SEEK_HOLE")
}

# origin: uutils test_cp::test_cp_debug_reflink_never_with_hole
test test_uu_cp_cp_debug_reflink_never_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "hello")?
  uu.truncate(s, "a", 4096 * 4)?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: SEEK_HOLE")
  assert fs.stat(uu.at(s, "a"))?.blocks_512 == fs.stat(uu.at(s, "b"))?.blocks_512
}

# origin: uutils test_cp::test_cp_debug_reflink_never_without_hole
test test_uu_cp_cp_debug_reflink_never_without_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a", bytes.concat([b"hello", bytes.zero(1000)?]))?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_sparse_always
test test_uu_cp_cp_debug_sparse_always { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["--debug", "--sparse=always", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: Operation not supported, sparse detection: zeros")
}

# origin: uutils test_cp::test_cp_debug_sparse_always_reflink_auto
test test_uu_cp_cp_debug_sparse_always_reflink_auto { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["--debug", "--sparse=always", "--reflink=auto", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: Operation not supported, sparse detection: zeros")
}

# origin: uutils test_cp::test_cp_debug_sparse_always_sparse_virtual_file
test test_uu_cp_cp_debug_sparse_always_sparse_virtual_file { |ctx|
  let s = uu.scene(ctx)?
  if !p"/sys/kernel/profiling".is_file()? { return }
  let r = uu.invoke(s, "cp", ["--debug", "--sparse=always", "/sys/kernel/profiling", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: Invalid cross-device link, sparse detection: zeros")
}

# origin: uutils test_cp::test_cp_debug_sparse_auto
test test_uu_cp_cp_debug_sparse_auto { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["--debug", "--sparse=auto", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: unknown, reflink: Operation not supported, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_sparse_never
test test_uu_cp_cp_debug_sparse_never { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["--debug", "--sparse=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_sparse_never_empty_file_with_hole
test test_uu_cp_cp_debug_sparse_never_empty_file_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.truncate(s, "a", 10000)?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=auto", "--sparse=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: unknown, reflink: no, sparse detection: SEEK_HOLE")
}

# origin: uutils test_cp::test_cp_debug_sparse_never_empty_sparse_file
test test_uu_cp_cp_debug_sparse_never_empty_sparse_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["--debug", "--sparse=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_sparse_never_file_with_hole
test test_uu_cp_cp_debug_sparse_never_file_with_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.truncate(s, "a", 4096 * 4)?
  let _ = bytes.write_at(uu.at(s, "a"), 16384, b"hello")?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=auto", "--sparse=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: SEEK_HOLE")
}

# origin: uutils test_cp::test_cp_debug_sparse_never_less_than_512_bytes
test test_uu_cp_cp_debug_sparse_never_less_than_512_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "hello")?
  uu.truncate(s, "a", 400)?
  let r = uu.invoke(s, "cp", ["--debug", "--reflink=auto", "--sparse=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_sparse_never_without_hole
test test_uu_cp_cp_debug_sparse_never_without_hole { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a", bytes.concat([b"hello", bytes.zero(10000)?]))?
  let r = uu.invoke(s, "cp", ["--reflink=auto", "--sparse=never", "--debug", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_sparse_never_zero_sized_virtual_file
test test_uu_cp_cp_debug_sparse_never_zero_sized_virtual_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["--debug", "--sparse=never", "/proc/version", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: no")
}

# origin: uutils test_cp::test_cp_debug_sparse_reflink
test test_uu_cp_cp_debug_sparse_reflink { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["--debug", "--sparse=always", "--reflink=never", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "copy offload: avoided, reflink: no, sparse detection: zeros")
}

# origin: uutils test_cp::test_cp_default_virtual_file
test test_uu_cp_cp_default_virtual_file { |ctx|
  let s = uu.scene(ctx)?
  if !p"/sys/kernel/profiling".is_file()? { return }
  let r = uu.invoke(s, "cp", ["/sys/kernel/profiling", "b"])?
  uu.succeeds(r)
  assert uu.size(s, "b")? > 0
}

# origin: uutils test_cp::test_cp_deref
test test_uu_cp_cp_deref { |ctx|
  let s = fixture_scene(ctx)?
  uu.symlink(s, "hello_world.txt", "hello_world.txt.link")?
  let r = uu.invoke(s, "cp", ["-L", "hello_world.txt", "hello_world.txt.link", "hello_dir/"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "hello_dir/hello_world.txt.link")?
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
  uu.file_is(s, "hello_dir/hello_world.txt.link", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_deref_conflicting_options
test test_uu_cp_cp_deref_conflicting_options { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["-LP", "hello_dir/", "hello_world.txt"])?
  uu.fails(r)
}

# origin: uutils test_cp::test_cp_deref_folder_to_folder
test test_uu_cp_cp_deref_folder_to_folder { |ctx|
  let s = fixture_scene(ctx)?
  uu.symlink(s, uu.at(s, "hello_dir_with_file/hello_world.txt").display(), "hello_dir_with_file/hello_world.txt.link")?
  let r = uu.invoke(s, "cp", ["-L", "-R", "-v", "hello_dir_with_file/", "hello_dir_new"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "hello_dir_new/hello_world.txt.link")?
  uu.file_is(s, "hello_dir_new/hello_world.txt", "Hello, World!\n")
  uu.file_is(s, "hello_dir_new/hello_world.txt.link", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_dest_no_permissions
test test_uu_cp_cp_dest_no_permissions { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "valid.txt")?
  uu.touch(s, "invalid_perms.txt")?
  uu.set_mode(s, "invalid_perms.txt", 0o444)?
  let r = uu.invoke(s, "cp", ["valid.txt", "invalid_perms.txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "invalid_perms.txt")
  uu.stderr_contains(r, "denied")
}

# origin: uutils test_cp::test_cp_directory_not_recursive
test test_uu_cp_cp_directory_not_recursive { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_dir/", "copy_of_hello_world.txt"])?
  uu.fails(r)
  uu.stderr_is(r, "cp: -r not specified; omitting directory 'hello_dir/'\n")
}

# origin: uutils test_cp::test_cp_duplicate_directories_merge
test test_uu_cp_cp_duplicate_directories_merge { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "src_dir/subdir")?
  uu.write(s, "src_dir/subdir/file1.txt", "content1")?
  uu.write(s, "src_dir/subdir/file2.txt", "content2")?
  uu.mkdir_all(s, "src_dir2/subdir")?
  uu.write(s, "src_dir2/subdir/file1.txt", "content3")?
  uu.mkdir(s, "dest")?
  let r = uu.invoke(s, "cp", ["-r", "src_dir/subdir", "src_dir2/subdir", "dest"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "dest/subdir")?
  assert uu.file_exists(s, "dest/subdir/file1.txt")?
  uu.file_is(s, "dest/subdir/file1.txt", "content3")
  assert uu.file_exists(s, "dest/subdir/file2.txt")?
  uu.file_is(s, "dest/subdir/file2.txt", "content2")
}

# origin: uutils test_cp::test_cp_duplicate_files
test test_uu_cp_cp_duplicate_files { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "hello_world.txt", "hello_dir/"])?
  uu.succeeds(r)
  uu.stderr_contains(r, "source file 'hello_world.txt' specified more than once")
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_duplicate_files_normalized_path
test test_uu_cp_cp_duplicate_files_normalized_path { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "./hello_world.txt", "hello_dir/"])?
  uu.succeeds(r)
  uu.stderr_contains(r, "source file './hello_world.txt' specified more than once")
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_duplicate_files_with_numbered_backup
test test_uu_cp_cp_duplicate_files_with_numbered_backup { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "hello_world.txt", "hello_dir/", "--backup=numbered"])?
  uu.succeeds(r)
}

# origin: uutils test_cp::test_cp_duplicate_files_with_plain_backup
test test_uu_cp_cp_duplicate_files_with_plain_backup { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "hello_world.txt", "hello_dir/", "--backup"])?
  uu.fails(r)
  uu.stderr_contains(r, "will not overwrite just-created 'hello_dir/hello_world.txt' with 'hello_world.txt")
}

# origin: uutils test_cp::test_cp_duplicate_folder
test test_uu_cp_cp_duplicate_folder { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["-r", "hello_dir_with_file/", "hello_dir_with_file/", "hello_dir/"])?
  uu.succeeds(r)
  uu.stderr_contains(r, "source directory 'hello_dir_with_file/' specified more than once")
  assert uu.dir_exists(s, "hello_dir/hello_dir_with_file/")?
}

# origin: uutils test_cp::test_cp_empty_backup_suffix_uses_default
test test_uu_cp_cp_empty_backup_suffix_uses_default { |ctx|
  for flag in ["--backup=nil", "--backup"] {
    let s = fixture_scene(ctx)?
    let r = uu.invoke(s, "cp", [flag, "--suffix=", "hello_world.txt", "how_are_you.txt"])?
    uu.succeeds(r)
    uu.no_stderr(r)
    uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
    uu.file_is(s, "how_are_you.txt~", "How are you?\n")
  }
}

# origin: uutils test_cp::test_cp_existing_perm_dir
test test_uu_cp_cp_existing_perm_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mkdir", ["-p", "-m", "ug-s,u=rwx,g=rwx,o=rx", "src/dir"], umask: 0o022)?)
  uu.succeeds(uu.invoke(s, "mkdir", ["-p", "-m", "ug-s,u=rwx,g=,o=", "dst/dir"], umask: 0o022)?)
  let r = uu.invoke(s, "cp", ["-r", "src/.", "dst/"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "dst/dir"))?.mode == 0o40700
}

# origin: uutils test_cp::test_cp_existing_target
test test_uu_cp_cp_existing_target { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "existing_file.txt"])?
  uu.succeeds(r)
  uu.file_is(s, "existing_file.txt", "Hello, World!\n")
  assert !uu.exists(s, "existing_file.txt~")? or !uu.file_exists(s, "existing_file.txt~")?
}

# origin: uutils test_cp::test_cp_f_i_verbose_non_writeable_destination_empty
test test_uu_cp_cp_f_i_verbose_non_writeable_destination_empty { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  uu.set_mode(s, "b", 0o0000)?
  let r = uu.invoke(s, "cp", ["-f", "-i", "--verbose", "a", "b"], stdin: b"")?
  uu.fails(r)
  uu.stderr_only(r, "cp: replace 'b', overriding mode 0000 (---------)? ")
}

# origin: uutils test_cp::test_cp_f_i_verbose_non_writeable_destination_y
test test_uu_cp_cp_f_i_verbose_non_writeable_destination_y { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  uu.set_mode(s, "b", 0o0000)?
  let r = uu.invoke(s, "cp", ["-f", "-i", "--verbose", "a", "b"], stdin: b"y")?
  uu.succeeds(r)
  uu.stderr_is(r, "cp: replace 'b', overriding mode 0000 (---------)? ")
  uu.stdout_is(r, "'a' -> 'b'\nremoved 'b'\n")
}

# origin: uutils test_cp::test_cp_fifo
test test_uu_cp_cp_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  uu.set_mode(s, "fifo", 0o731)?
  let r = uu.invoke(s, "cp", ["--preserve=mode", "-r", "fifo", "fifo2"], timeout: 5s)?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "fifo2"))?.kind == "fifo"
  assert uu.mode(s, "fifo2")? == 0o731
}

# origin: uutils test_cp::test_cp_fifo_preserve_timestamps
test test_uu_cp_cp_fifo_preserve_timestamps { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "pipe")?
  uu.set_mode(s, "pipe", 0o624)?
  let r = uu.invoke(s, "cp", ["--preserve=timestamps", "-r", "pipe", "pipe_dup"], timeout: 5s)?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "pipe_dup"))?.kind == "fifo"
}

# origin: uutils test_cp::test_cp_force_remove_destination_attributes_only_with_symlink
test test_uu_cp_cp_force_remove_destination_attributes_only_with_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "1")?
  uu.write(s, "file2", "2")?
  uu.symlink(s, "file1", "sym1")?
  let r = uu.invoke(s, "cp", ["-a", "--remove-destination", "--attributes-only", "sym1", "file2"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "file2")?
  assert uu.read(s, "file1")? == uu.read(s, "file2")?
}

# origin: uutils test_cp::test_cp_from_stdin
test test_uu_cp_cp_from_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["/dev/fd/0", "target"], stdin: b"Hello, World!\n")?
  uu.succeeds(r)
  assert uu.file_exists(s, "target")?
  uu.file_is(s, "target", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_from_stream
test test_uu_cp_cp_from_stream { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "target")?
  for input in ["longer: Hello, World!\n", "shorter"] {
    let r = uu.invoke(s, "cp", ["/dev/fd/0", "target"], stdin: bytes.from_text(input))?
    uu.succeeds(r)
    uu.file_is(s, "target", input)
  }
}

# origin: uutils test_cp::test_cp_from_stream_permission
test test_uu_cp_cp_from_stream_permission { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "target")?
  uu.symlink(s, "target", "link")?
  uu.set_mode(s, "target", 0o777)?
  let r = uu.invoke(s, "cp", ["/dev/fd/0", "link"], stdin: b"Hello, World!\n")?
  uu.succeeds(r)
  uu.file_is(s, "target", "Hello, World!\n")
  assert fs.stat(uu.at(s, "target"))?.mode == 0o100777
}

# origin: uutils test_cp::test_cp_gnu_preserve_mode
test test_uu_cp_cp_gnu_preserve_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d1")?
  uu.mkdir(s, "d2")?
  uu.set_mode(s, "d2", 0o705)?
  let r = uu.invoke(s, "cp", ["--no-preserve=mode", "-r", "d2", "d3"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "d1"))?.mode == fs.stat(uu.at(s, "d3"))?.mode
}

# origin: uutils test_cp::test_cp_hlp_flag_ordering
test test_uu_cp_cp_hlp_flag_ordering { |ctx|
  for pair in [["-HP", "dest_hp"], ["-PH", "dest_ph"]] {
    let s = uu.scene(ctx)?
    uu.touch(s, "file.txt")?
    uu.symlink(s, "file.txt", "symlink")?
    let r = uu.invoke(s, "cp", [pair[0], "symlink", pair[1]])?
    uu.succeeds(r)
    if pair[0] == "-HP" { assert uu.is_symlink(s, pair[1])? } else {
      assert !uu.is_symlink(s, pair[1])?
      assert uu.file_exists(s, pair[1])?
    }
  }
}

# origin: uutils test_cp::test_cp_issue_1665
test test_uu_cp_cp_issue_1665 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["/dev/null", "foo"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "foo")?
  uu.file_is(s, "foo", "")
}

# origin: uutils test_cp::test_cp_link_backup
test test_uu_cp_cp_link_backup { |ctx|
  let s = fixture_scene(ctx)?
  uu.touch(s, "file2")?
  let r = uu.invoke(s, "cp", ["-l", "-b", "hello_world.txt", "file2"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "file2~")?
  uu.file_is(s, "file2", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_mode_hardlink
test test_uu_cp_cp_mode_hardlink { |ctx|
  for source in ["file", "slink", "slink2"] {
    let s = uu.scene(ctx)?
    uu.write(s, "file", "f")?
    uu.symlink(s, "file", "slink")?
    uu.symlink(s, "slink", "slink2")?
    let r = uu.invoke(s, "cp", ["--link", "-L", source, "z"])?
    uu.succeeds(r)
    assert uu.file_exists(s, "z")? and !uu.is_symlink(s, "z")?
    uu.file_is(s, "z", "f")
    uu.append(s, "z", "g")?
    uu.file_is(s, "file", "fg")
  }
}

# origin: uutils test_cp::test_cp_mode_hardlink_no_dereference
test test_uu_cp_cp_mode_hardlink_no_dereference { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "f")?
  uu.symlink(s, "file", "slink")?
  uu.symlink(s, "slink", "slink2")?
  let r = uu.invoke(s, "cp", ["--link", "-P", "slink2", "z"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "z")?
  assert uu.read_link(s, "z")? == "slink"
}

# origin: uutils test_cp::test_cp_mode_symlink
test test_uu_cp_cp_mode_symlink { |ctx|
  for source in ["file", "slink", "slink2"] {
    let s = uu.scene(ctx)?
    uu.write(s, "file", "f")?
    uu.symlink(s, "file", "slink")?
    uu.symlink(s, "slink", "slink2")?
    let r = uu.invoke(s, "cp", ["-s", "-L", source, "z"])?
    uu.succeeds(r)
    assert uu.is_symlink(s, "z")?
    assert uu.read_link(s, "z")? == source
  }
}

# origin: uutils test_cp::test_cp_multiple_files
test test_uu_cp_cp_multiple_files { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "how_are_you.txt", "hello_dir/"])?
  uu.succeeds(r)
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
  uu.file_is(s, "hello_dir/how_are_you.txt", "How are you?\n")
}

# origin: uutils test_cp::test_cp_multiple_files_target_is_file
test test_uu_cp_cp_multiple_files_target_is_file { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "hello_world.txt", "existing_file.txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "Not a directory")
}

# origin: uutils test_cp::test_cp_multiple_files_with_empty_file_name
test test_uu_cp_cp_multiple_files_with_empty_file_name { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "", "how_are_you.txt", "hello_dir/"])?
  uu.fails(r)
  uu.stderr_contains(r, "'': No such file or directory")
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
  uu.file_is(s, "hello_dir/how_are_you.txt", "How are you?\n")
}

# origin: uutils test_cp::test_cp_multiple_files_with_nonexistent_file
test test_uu_cp_cp_multiple_files_with_nonexistent_file { |ctx|
  let s = fixture_scene(ctx)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "nonexistent_file.txt", "how_are_you.txt", "hello_dir/"])?
  uu.fails(r)
  uu.stderr_contains(r, "'nonexistent_file.txt': No such file or directory")
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
  uu.file_is(s, "hello_dir/how_are_you.txt", "How are you?\n")
}

# origin: uutils test_cp::test_cp_no_deref
test test_uu_cp_cp_no_deref { |ctx|
  let s = fixture_scene(ctx)?
  uu.symlink(s, "hello_world.txt", "hello_world.txt.link")?
  let r = uu.invoke(s, "cp", ["-P", "hello_world.txt", "hello_world.txt.link", "hello_dir/"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "hello_dir/hello_world.txt.link")?
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
  uu.file_is(s, "hello_dir/hello_world.txt.link", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_no_deref_folder_to_folder
test test_uu_cp_cp_no_deref_folder_to_folder { |ctx|
  let s = fixture_scene(ctx)?
  uu.symlink(s, uu.at(s, "hello_dir_with_file/hello_world.txt").display(), "hello_dir_with_file/hello_world.txt.link")?
  let r = uu.invoke(s, "cp", ["-P", "-R", "-v", "hello_dir_with_file/", "hello_dir_new"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "hello_dir_new/hello_world.txt.link")?
  uu.file_is(s, "hello_dir_new/hello_world.txt", "Hello, World!\n")
  uu.file_is(s, "hello_dir_new/hello_world.txt.link", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_no_deref_link_onto_link
test test_uu_cp_cp_no_deref_link_onto_link { |ctx|
  let s = fixture_scene(ctx)?
  uu.at(s, "hello_world.txt").copy(to: uu.at(s, "copy_of_hello_world.txt"))?
  uu.symlink(s, "hello_world.txt", "hello_world.txt.link")?
  uu.symlink(s, "copy_of_hello_world.txt", "copy_of_hello_world.txt.link")?
  let r = uu.invoke(s, "cp", ["-P", "hello_world.txt.link", "copy_of_hello_world.txt.link"])?
  uu.succeeds(r)
  assert !uu.is_symlink(s, "copy_of_hello_world.txt")?
  assert uu.is_symlink(s, "copy_of_hello_world.txt.link")?
  uu.file_is(s, "copy_of_hello_world.txt.link", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_no_deref_preserve_with_deref_keeps_hardlinks
test test_uu_cp_cp_no_deref_preserve_with_deref_keeps_hardlinks { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file1")?
  uu.hard_link(s, "file1", "file2")?
  uu.mkdir(s, "target_dir")?
  let r = uu.invoke(s, "cp", ["-dL", "file1", "file2", "target_dir"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "target_dir/file1"))?.nlink == 2
}

# origin: uutils test_cp::test_cp_no_dereference_attributes_only_with_symlink
test test_uu_cp_cp_no_dereference_attributes_only_with_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "1")?
  uu.write(s, "file2", "2")?
  uu.write(s, "file2.exp", "2")?
  uu.symlink(s, "file1", "sym1")?
  let r = uu.invoke(s, "cp", ["--no-dereference", "--attributes-only", "sym1", "file2"])?
  uu.fails_with_code(r, 1)
  assert uu.read(s, "file2")? == uu.read(s, "file2.exp")?
}

# origin: uutils test_cp::test_cp_no_dereference_copies_symlink_as_symlink
test test_uu_cp_cp_no_dereference_copies_symlink_as_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "target", "secret target contents")?
  uu.symlink(s, "target", "src_link")?
  let r = uu.invoke(s, "cp", ["-P", "src_link", "dst"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "dst")?
  assert uu.read_link(s, "dst")?.ends_with("target")
}

# origin: uutils test_cp::test_cp_no_dereference_symlink_with_parents
test test_uu_cp_cp_no_dereference_symlink_with_parents { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "directory")?
  uu.symlink(s, "directory", "symlink-to-directory")?
  let r = uu.invoke(s, "cp", ["--parents", "--no-dereference", "symlink-to-directory", "x"])?
  uu.fails(r)
  uu.stderr_contains(r, "with --parents, the destination must be a directory")
  uu.mkdir(s, "x")?
  let copied = uu.invoke(s, "cp", ["--parents", "--no-dereference", "symlink-to-directory", "x"])?
  uu.succeeds(copied)
  assert uu.read_link(s, "x/symlink-to-directory")? == "directory"
}

# origin: uutils test_cp::test_cp_no_preserve_target_directory
test test_uu_cp_cp_no_preserve_target_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "a/b/c/d")?
  uu.touch(s, "a/b/c/d/f1")?
  let r = uu.invoke(s, "cp", ["-rT", "a", "e"])?
  uu.succeeds(r)
  uu.touch(s, "e/f2")?
  uu.succeeds(uu.invoke(s, "cp", ["-rT", "a/", "e/"])?)
  uu.touch(s, "e/f3")?
  uu.succeeds(uu.invoke(s, "cp", ["-rvT", "a/b/c", "e/"])?)
  uu.succeeds(uu.invoke(s, "cp", ["-rvT", "a/b/", "e/b/c/d/"])?)
  uu.succeeds(uu.invoke(s, "cp", ["-rT", "a/b/c", "."])?)
  for name in ["e/a", "e/c", "e/c/d/b"] { assert !uu.exists(s, name)? or !uu.dir_exists(s, name)? }
  for name in ["e/b/c/d/f1", "e/b/c/d/c/d/f1", "e/d/f1", "./d/f1", "e/f2", "e/f3"] { assert uu.file_exists(s, name)? }
}

# origin: uutils test_cp::test_cp_final_mode_unchanged_after_restrictive_create
test test_uu_cp_cp_final_mode_unchanged_after_restrictive_create { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  uu.set_mode(s, "src", 0o644)?
  uu.succeeds(uu.invoke(s, "cp", ["src", "dst"], umask: 0o022)?)
  assert uu.mode(s, "dst")? == 0o644
}

# origin: uutils test_cp::test_cp_no_preserve_mode
test test_uu_cp_cp_no_preserve_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.set_mode(s, "a", 0o731)?
  uu.succeeds(uu.invoke(s, "cp", ["-a", "--no-preserve=mode", "a", "b"], umask: 0o077)?)
  assert uu.file_exists(s, "b")?
  assert uu.mode(s, "b")? == 0o600
}
