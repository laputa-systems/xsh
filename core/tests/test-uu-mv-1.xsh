##! Transcribed from the uutils coreutils mv integration tests.

use support.uu as uu

proc absolute_link(s: uu.Scene, target: Str, name: Str) [fs, error] -> Result[Unit, Error] {
  uu.symlink(s, uu.at(s, target).display(), name)?
  Ok()
}

proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let target = uu.at(s, name)
  if target.exists()? { Ok(target.is_file()?) } else { Ok(false) }
}

proc dir_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let target = uu.at(s, name)
  if target.exists()? { Ok(target.is_dir()?) } else { Ok(false) }
}

proc is_symlink(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let target = uu.at(s, name)
  match fs.stat(target) {
    Ok(meta) => Ok(meta.kind == "symlink"),
    Err(failure) => if failure.errno == 2 { Ok(false) } else { Err(failure) },
  }
}

# The scratch filesystem and /dev/shm must differ to exercise the copy path.
proc other_fs(s: uu.Scene) [fs, error] -> Result[Path, Error] {
  let target = fp"/dev/shm/xsh-uu-mv-{s.root.parent().name()}-{s.root.name()}"
  assert fs.stat(s.root)?.dev != fs.stat(p"/dev/shm")?.dev
  target.mkdir()?
  Ok(target)
}

proc cleanup_other(target: Path) [fs, error] {
  target.chmod(0o755)?
  target.remove()?
}

# origin: uutils test_mv::inter_partition_copying::test_mv_dir_with_fifo_across_partitions
test test_uu_mv_inter_partition_copying_mv_dir_with_fifo_across_partitions { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.mkdir(s, "dir")?
  uu.mkfifo(s, "dir/fifo")?
  let r = uu.invoke(s, "mv", ["dir", other.display()])?
  uu.succeeds(r)
  uu.no_output(r)
  assert ! dir_exists(s, "dir")?
  assert fs.stat(fp"{other}/dir/fifo")?.kind == "fifo"
}

# origin: uutils test_mv::inter_partition_copying::test_mv_inter_partition_keeps_setuid_when_ownership_preserved
test test_uu_mv_inter_partition_copying_mv_inter_partition_keeps_setuid_when_ownership_preserved { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.write(s, "src", "src contents")?
  uu.set_mode(s, "src", 0o6755)?
  uu.succeeds(uu.invoke(s, "mv", ["src", fp"{other}/dest".display()])?)
  assert fs.stat(fp"{other}/dest")?.mode.bit_and(0o7777) == 0o6755
}

# origin: uutils test_mv::inter_partition_copying::test_mv_preserves_complex_hardlinks_across_nested_directories
test test_uu_mv_inter_partition_copying_mv_preserves_complex_hardlinks_across_nested_directories { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir1/subdir1")?
  uu.mkdir(s, "dir1/subdir2")?
  uu.mkdir(s, "dir2")?
  uu.mkdir(s, "dir2/subdir1")?
  uu.write(s, "dir1/subdir1/file_a", "content A")?
  uu.hard_link(s, "dir1/subdir1/file_a", "dir1/subdir2/file_a_link1")?
  uu.hard_link(s, "dir1/subdir1/file_a", "dir2/subdir1/file_a_link2")?
  assert fs.stat(uu.at(s, "dir1/subdir1/file_a"))?.ino == fs.stat(uu.at(s, "dir1/subdir2/file_a_link1"))?.ino
  assert fs.stat(uu.at(s, "dir1/subdir1/file_a"))?.ino == fs.stat(uu.at(s, "dir2/subdir1/file_a_link2"))?.ino
  assert fs.stat(uu.at(s, "dir1/subdir1/file_a"))?.nlink == 3
  uu.write(s, "dir1/file_b", "content B")?
  uu.hard_link(s, "dir1/file_b", "dir2/file_b_link")?
  assert fs.stat(uu.at(s, "dir1/file_b"))?.ino == fs.stat(uu.at(s, "dir2/file_b_link"))?.ino
  assert fs.stat(uu.at(s, "dir1/file_b"))?.nlink == 2
  uu.write(s, "dir1/subdir1/nested_file", "nested content")?
  uu.hard_link(s, "dir1/subdir1/nested_file", "dir1/subdir2/nested_file_link")?
  assert fs.stat(uu.at(s, "dir1/subdir1/nested_file"))?.ino == fs.stat(uu.at(s, "dir1/subdir2/nested_file_link"))?.ino
  assert fs.stat(uu.at(s, "dir1/subdir1/nested_file"))?.nlink == 2
  uu.succeeds(uu.invoke(s, "mv", ["dir1", "dir2", other.display()])?)
  assert fs.stat(fp"{other}/dir1/subdir1/file_a")?.ino == fs.stat(fp"{other}/dir1/subdir2/file_a_link1")?.ino
  assert fs.stat(fp"{other}/dir1/subdir1/file_a")?.ino == fs.stat(fp"{other}/dir2/subdir1/file_a_link2")?.ino
  assert fs.stat(fp"{other}/dir1/subdir1/file_a")?.nlink == 3
  assert fp"{other}/dir1/subdir1/file_a".read_text()? == "content A"
  assert fp"{other}/dir1/subdir2/file_a_link1".read_text()? == "content A"
  assert fp"{other}/dir2/subdir1/file_a_link2".read_text()? == "content A"
  assert fs.stat(fp"{other}/dir1/file_b")?.ino == fs.stat(fp"{other}/dir2/file_b_link")?.ino
  assert fs.stat(fp"{other}/dir1/file_b")?.nlink == 2
  assert fp"{other}/dir1/file_b".read_text()? == "content B"
  assert fp"{other}/dir2/file_b_link".read_text()? == "content B"
  assert fs.stat(fp"{other}/dir1/subdir1/nested_file")?.ino == fs.stat(fp"{other}/dir1/subdir2/nested_file_link")?.ino
  assert fs.stat(fp"{other}/dir1/subdir1/nested_file")?.nlink == 2
  assert fp"{other}/dir1/subdir1/nested_file".read_text()? == "nested content"
  assert fp"{other}/dir1/subdir2/nested_file_link".read_text()? == "nested content"
}

# origin: uutils test_mv::inter_partition_copying::test_mv_preserves_hardlinks_across_partitions
test test_uu_mv_inter_partition_copying_mv_preserves_hardlinks_across_partitions { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  
  uu.write(s, "file1", "test content")?
  uu.hard_link(s, "file1", "file2")?
  assert fs.stat(uu.at(s, "file1"))?.ino == fs.stat(uu.at(s, "file2"))?.ino
  assert fs.stat(uu.at(s, "file1"))?.nlink == 2
  uu.succeeds(uu.invoke(s, "mv", ["file1", "file2", other.display()])?)
  assert ! file_exists(s, "file1")?
  assert fp"{other}/file1".exists()?
  assert ! file_exists(s, "file2")?
  assert fp"{other}/file2".exists()?
  assert fs.stat(fp"{other}/file1")?.ino == fs.stat(fp"{other}/file2")?.ino
  assert fs.stat(fp"{other}/file1")?.nlink == 2
  assert fp"{other}/file1".read_text()? == "test content"
  assert fp"{other}/file2".read_text()? == "test content"
}

# origin: uutils test_mv::inter_partition_copying::test_mv_preserves_hardlinks_in_directories_across_partitions
test test_uu_mv_inter_partition_copying_mv_preserves_hardlinks_in_directories_across_partitions { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.write(s, "f", "file content")?
  uu.hard_link(s, "f", "g")?
  assert fs.stat(uu.at(s, "f"))?.ino == fs.stat(uu.at(s, "g"))?.ino
  assert fs.stat(uu.at(s, "f"))?.nlink == 2
  uu.write(s, "a/1", "directory file content")?
  uu.hard_link(s, "a/1", "b/1")?
  assert fs.stat(uu.at(s, "a/1"))?.ino == fs.stat(uu.at(s, "b/1"))?.ino
  assert fs.stat(uu.at(s, "a/1"))?.nlink == 2
  uu.succeeds(uu.invoke(s, "mv", ["f", "g", other.display()])?)
  uu.succeeds(uu.invoke(s, "mv", ["a", "b", other.display()])?)
  assert fs.stat(fp"{other}/f")?.ino == fs.stat(fp"{other}/g")?.ino
  assert fs.stat(fp"{other}/f")?.nlink == 2
  assert fp"{other}/f".read_text()? == "file content"
  assert fp"{other}/g".read_text()? == "file content"
  assert fs.stat(fp"{other}/a/1")?.ino == fs.stat(fp"{other}/b/1")?.ino
  assert fs.stat(fp"{other}/a/1")?.nlink == 2
  assert fp"{other}/a/1".read_text()? == "directory file content"
  assert fp"{other}/b/1".read_text()? == "directory file content"
}

# origin: uutils test_mv::inter_partition_copying::test_mv_preserves_multiple_hardlink_groups_across_partitions
test test_uu_mv_inter_partition_copying_mv_preserves_multiple_hardlink_groups_across_partitions { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  
  uu.write(s, "group1_file1", "content group 1")?
  uu.hard_link(s, "group1_file1", "group1_file2")?
  assert fs.stat(uu.at(s, "group1_file1"))?.ino == fs.stat(uu.at(s, "group1_file2"))?.ino
  assert fs.stat(uu.at(s, "group1_file1"))?.nlink == 2
  uu.write(s, "group2_file1", "content group 2")?
  uu.hard_link(s, "group2_file1", "group2_file2")?
  assert fs.stat(uu.at(s, "group2_file1"))?.ino == fs.stat(uu.at(s, "group2_file2"))?.ino
  assert fs.stat(uu.at(s, "group2_file1"))?.nlink == 2
  uu.write(s, "single_file", "single file content")?
  assert fs.stat(uu.at(s, "single_file"))?.nlink == 1
  assert fs.stat(uu.at(s, "group1_file1"))?.ino != fs.stat(uu.at(s, "group2_file1"))?.ino
  uu.succeeds(uu.invoke(s, "mv", ["group1_file1", "group1_file2", "group2_file1", "group2_file2", "single_file", other.display()])?)
  assert fs.stat(fp"{other}/group1_file1")?.ino == fs.stat(fp"{other}/group1_file2")?.ino
  assert fs.stat(fp"{other}/group1_file1")?.nlink == 2
  assert fp"{other}/group1_file1".read_text()? == "content group 1"
  assert fp"{other}/group1_file2".read_text()? == "content group 1"
  assert fs.stat(fp"{other}/group2_file1")?.ino == fs.stat(fp"{other}/group2_file2")?.ino
  assert fs.stat(fp"{other}/group2_file1")?.nlink == 2
  assert fp"{other}/group2_file1".read_text()? == "content group 2"
  assert fp"{other}/group2_file2".read_text()? == "content group 2"
  assert fs.stat(fp"{other}/single_file")?.nlink == 1
  assert fp"{other}/single_file".read_text()? == "single file content"
  assert fs.stat(fp"{other}/group1_file1")?.ino != fs.stat(fp"{other}/group2_file1")?.ino
}

# origin: uutils test_mv::inter_partition_copying::test_mv_symlink_to_hardlinked_sibling_across_partitions
test test_uu_mv_inter_partition_copying_mv_symlink_to_hardlinked_sibling_across_partitions { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.mkdir(s, "dir")?
  uu.write(s, "dir/realfile", "important data")?
  uu.hard_link(s, "dir/realfile", "dir/realfile2")?
  uu.symlink(s, "realfile", "dir/aaa_link")?
  uu.symlink(s, "realfile", "dir/zzz_link")?
  let r = uu.invoke(s, "mv", ["dir", other.display()])?
  uu.succeeds(r)
  uu.no_output(r)
  for name in ["realfile", "realfile2"] {
    let file = fp"{other}/dir/{name}"
    assert fs.stat(file)?.kind == "file"
    assert file.read_text()? == "important data"
  }
  assert fs.stat(fp"{other}/dir/realfile")?.ino == fs.stat(fp"{other}/dir/realfile2")?.ino
  assert fs.stat(fp"{other}/dir/realfile")?.nlink == 2
  for name in ["aaa_link", "zzz_link"] {
    let link = fp"{other}/dir/{name}"
    assert fs.stat(link)?.kind == "symlink"
    assert link.readlink()? == p"realfile"
  }
}

# origin: uutils test_mv::inter_partition_copying::test_mv_unlinks_dest_symlink
test test_uu_mv_inter_partition_copying_mv_unlinks_dest_symlink { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.write(s, "src", "src contents")?
  let referent = fp"{other}/other_fs_file"
  referent.write("other fs file contents")?
  let link = fp"{other}/symlink_to_file"
  link.symlink(to: referent)
  uu.succeeds(uu.invoke(s, "mv", ["src", link.display()])?)
  assert ! file_exists(s, "src")?
  assert fs.stat(link)?.kind != "symlink"
  assert referent.read_text()? == "other fs file contents"
  assert link.read_text()? == "src contents"
}

# origin: uutils test_mv::inter_partition_copying::test_mv_unlinks_dest_symlink_error_message
test test_uu_mv_inter_partition_copying_mv_unlinks_dest_symlink_error_message { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.write(s, "src", "src contents")?
  let referent = fp"{other}/other_fs_file"
  referent.write("other fs file contents")?
  let link = fp"{other}/symlink_to_file"
  link.symlink(to: referent)
  other.chmod(0o555)?
  let r = uu.invoke(s, "mv", ["src", link.display()])?
  uu.fails(r)
  uu.stderr_contains(r, "inter-device move failed:")
  uu.stderr_contains(r, "Permission denied")
}

# origin: uutils test_mv::test_acl
test test_uu_mv_acl { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.touch(s, "a/file")?
  let setter = match process.which("setfacl") { Ok(tool) => tool, Err(_) => { test.skip("setfacl is not installed"); p"/usr/bin/setfacl" } }
  # The original command resolves a outside the scene. Its failed setup skips.
  let setup = process.run(process.command_argv(setter, [setter.display(), "-m", "group::rwx", "a"], stdout: uu.at(s, "acl-out"), stderr: uu.at(s, "acl-err")))?
  if ! setup.exited_with(0) { test.skip("setfacl failed on its original working-directory operand") }
  uu.succeeds(uu.invoke(s, "mv", [uu.at(s, "a/file").display(), "b"])?)
  # The upstream comparison falls back to an empty list for absent paths.
  let left = match fs.xattr_list(p"a/file") { Ok(names) => names |> sort |> collect(), Err(_) => [] }
  let right = match fs.xattr_list(p"b/file") { Ok(names) => names |> sort |> collect(), Err(_) => [] }
  assert left == right
}

# origin: uutils test_mv::test_move_should_not_fallback_to_copy
test test_uu_mv_move_should_not_fallback_to_copy { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "readonly_dir")?
  uu.touch(s, "readonly_dir/a_file_is_locked")?
  uu.set_mode(s, "readonly_dir", 0o555)?
  let r = uu.invoke(s, "mv", ["readonly_dir/a_file_is_locked", "target_file"])?
  uu.set_mode(s, "readonly_dir", 0o755)?
  uu.fails(r)
  assert file_exists(s, "readonly_dir/a_file_is_locked")?
  assert ! file_exists(s, "target_file")?
}

# origin: uutils test_mv::test_mv_arg_backup_arg_first
test test_uu_mv_mv_arg_backup_arg_first { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_simple_backup_file_a")?
  uu.touch(s, "test_mv_simple_backup_file_b")?
  {
    let r = uu.invoke(s, "mv", ["--backup", "test_mv_simple_backup_file_a", "test_mv_simple_backup_file_b"])?
    uu.succeeds(r)
  }
  assert ! file_exists(s, "test_mv_simple_backup_file_a")?
  assert file_exists(s, "test_mv_simple_backup_file_b")?
  assert file_exists(s, "test_mv_simple_backup_file_b~")?
}

# origin: uutils test_mv::test_mv_arg_interactive_skipped
test test_uu_mv_mv_arg_interactive_skipped { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  {
    let r = uu.invoke(s, "mv", ["-vi", "a", "b"], stdin: b"N\n")?
    uu.fails(r)
    uu.stderr_only(r, "mv: overwrite 'b'? ")
  }
}

# origin: uutils test_mv::test_mv_arg_interactive_skipped_vin
test test_uu_mv_mv_arg_interactive_skipped_vin { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  {
    let r = uu.invoke(s, "mv", ["-vin", "a", "b", "--debug"])?
    uu.succeeds(r)
    uu.stdout_contains(r, "skipped 'b'")
  }
}

# origin: uutils test_mv::test_mv_arg_update_all
test test_uu_mv_mv_arg_update_all { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_arg_update_none_file1", "file1 content\n")?
  uu.write(s, "test_mv_arg_update_none_file2", "file2 content\n")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_arg_update_none_file1", "test_mv_arg_update_none_file2", "--update=all"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  uu.file_is(s, "test_mv_arg_update_none_file2", "file1 content\n")
}

# origin: uutils test_mv::test_mv_arg_update_all_then_none
test test_uu_mv_mv_arg_update_all_then_none { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_arg_update_all_then_none_file1", "old content\n")?
  fs.set_times(uu.at(s, "test_mv_arg_update_all_then_none_file1"), mtime_ns: 0)?
  uu.write(s, "test_mv_arg_update_all_then_none_file2", "new content\n")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_arg_update_all_then_none_file1", "test_mv_arg_update_all_then_none_file2", "--update=all", "--update=none"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  uu.file_is(s, "test_mv_arg_update_all_then_none_file2", "new content\n")
}

# origin: uutils test_mv::test_mv_arg_update_interactive
test test_uu_mv_mv_arg_update_interactive { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_replace_file_a")?
  uu.touch(s, "test_mv_replace_file_b")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_replace_file_a", "test_mv_replace_file_b", "-i", "--update"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_mv::test_mv_arg_update_none
test test_uu_mv_mv_arg_update_none { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_arg_update_none_file1", "file1 content\n")?
  uu.write(s, "test_mv_arg_update_none_file2", "file2 content\n")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_arg_update_none_file1", "test_mv_arg_update_none_file2", "--update=none"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  uu.file_is(s, "test_mv_arg_update_none_file2", "file2 content\n")
}

# origin: uutils test_mv::test_mv_arg_update_none_then_all
test test_uu_mv_mv_arg_update_none_then_all { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_arg_update_none_then_all_file1", "old content\n")?
  fs.set_times(uu.at(s, "test_mv_arg_update_none_then_all_file1"), mtime_ns: 0)?
  uu.write(s, "test_mv_arg_update_none_then_all_file2", "new content\n")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_arg_update_none_then_all_file1", "test_mv_arg_update_none_then_all_file2", "--update=none", "--update=all"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  uu.file_is(s, "test_mv_arg_update_none_then_all_file2", "old content\n")
}

# origin: uutils test_mv::test_mv_arg_update_older_dest_not_older
test test_uu_mv_mv_arg_update_older_dest_not_older { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_arg_update_none_file1", "file1 content\n")?
  fs.set_times(uu.at(s, "test_mv_arg_update_none_file1"), mtime_ns: 0)?
  uu.write(s, "test_mv_arg_update_none_file2", "file2 content\n")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_arg_update_none_file1", "test_mv_arg_update_none_file2", "--update=older"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  uu.file_is(s, "test_mv_arg_update_none_file2", "file2 content\n")
}

# origin: uutils test_mv::test_mv_arg_update_older_dest_older
test test_uu_mv_mv_arg_update_older_dest_older { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_arg_update_none_file1", "file1 content\n")?
  fs.set_times(uu.at(s, "test_mv_arg_update_none_file1"), mtime_ns: 0)?
  uu.write(s, "test_mv_arg_update_none_file2", "file2 content\n")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_arg_update_none_file2", "test_mv_arg_update_none_file1", "--update=all"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  uu.file_is(s, "test_mv_arg_update_none_file1", "file2 content\n")
}

# origin: uutils test_mv::test_mv_arg_update_older_dest_older_interactive
test test_uu_mv_mv_arg_update_older_dest_older_interactive { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "old", "file1 content\n")?
  fs.set_times(uu.at(s, "old"), mtime_ns: 0)?
  uu.write(s, "new", "file2 content\n")?
  {
    let r = uu.invoke(s, "mv", ["new", "old", "--interactive", "--update=older"])?
    uu.fails(r)
    uu.stderr_contains(r, "overwrite 'old'?")
    uu.no_stdout(r)
  }
}

# origin: uutils test_mv::test_mv_arg_update_short_no_overwrite
test test_uu_mv_mv_arg_update_short_no_overwrite { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_arg_update_none_file1", "file1 content\n")?
  fs.set_times(uu.at(s, "test_mv_arg_update_none_file1"), mtime_ns: 0)?
  uu.write(s, "test_mv_arg_update_none_file2", "file2 content\n")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_arg_update_none_file1", "test_mv_arg_update_none_file2", "-u"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  uu.file_is(s, "test_mv_arg_update_none_file2", "file2 content\n")
}

# origin: uutils test_mv::test_mv_arg_update_short_overwrite
test test_uu_mv_mv_arg_update_short_overwrite { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_arg_update_none_file1", "file1 content\n")?
  fs.set_times(uu.at(s, "test_mv_arg_update_none_file1"), mtime_ns: 0)?
  uu.write(s, "test_mv_arg_update_none_file2", "file2 content\n")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_arg_update_none_file2", "test_mv_arg_update_none_file1", "-u"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  uu.file_is(s, "test_mv_arg_update_none_file1", "file2 content\n")
}

# origin: uutils test_mv::test_mv_backup_conflicting_options
test test_uu_mv_mv_backup_conflicting_options { |ctx|
  for option in ["--no-clobber", "--update=none-fail", "--update=none"] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "mv", ["--backup", option, "file1", "file2"])?
    uu.fails(r)
    uu.stderr_only(r, "mv: cannot combine --backup with --exchange, -n, or --update=none-fail\nTry 'mv --help' for more information.\n")
  }
}

# origin: uutils test_mv::test_mv_backup_dir
test test_uu_mv_mv_backup_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "test_mv_backup_dir_dir_a")?
  uu.mkdir(s, "test_mv_backup_dir_dir_b")?
  {
    let r = uu.invoke(s, "mv", ["-vbT", "test_mv_backup_dir_dir_a", "test_mv_backup_dir_dir_b"])?
    uu.succeeds(r)
    uu.stdout_only(r, "renamed 'test_mv_backup_dir_dir_a' -> 'test_mv_backup_dir_dir_b' (backup: 'test_mv_backup_dir_dir_b~')\n")
  }
  assert ! dir_exists(s, "test_mv_backup_dir_dir_a")?
  assert dir_exists(s, "test_mv_backup_dir_dir_b")?
  assert dir_exists(s, "test_mv_backup_dir_dir_b~")?
}

# origin: uutils test_mv::test_mv_backup_existing
test test_uu_mv_mv_backup_existing { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_backup_numbering_file_a")?
  uu.touch(s, "test_mv_backup_numbering_file_b")?
  {
    let r = uu.invoke(s, "mv", ["--backup=existing", "test_mv_backup_numbering_file_a", "test_mv_backup_numbering_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_backup_numbering_file_a")?
  assert file_exists(s, "test_mv_backup_numbering_file_b")?
  assert file_exists(s, "test_mv_backup_numbering_file_b~")?
}

# origin: uutils test_mv::test_mv_backup_existing_mode_protects_source
test test_uu_mv_mv_backup_existing_mode_protects_source { |ctx|
  for mode in ["simple", "existing"] {
    let s = uu.scene(ctx)?
    uu.touch(s, "a")?
    uu.write(s, "a~", "source content")?
    let r = uu.invoke(s, "mv", [f"--backup={mode}", "a~", "a"])?
    uu.fails(r)
    uu.stderr_contains(r, "might destroy source")
    uu.file_is(s, "a~", "source content")
  }
}

# origin: uutils test_mv::test_mv_backup_existing_mode_protects_source_even_with_numbered_present
test test_uu_mv_mv_backup_existing_mode_protects_source_even_with_numbered_present { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.write(s, "a~", "source content")?
  uu.write(s, "a.~1~", "old numbered backup")?
  {
    let r = uu.invoke(s, "mv", ["--backup=existing", "a~", "a"])?
    uu.fails(r)
    uu.stderr_contains(r, "might destroy source")
  }
  uu.file_is(s, "a~", "source content")
}

# origin: uutils test_mv::test_mv_backup_never
test test_uu_mv_mv_backup_never { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_backup_numbering_file_a")?
  uu.touch(s, "test_mv_backup_numbering_file_b")?
  {
    let r = uu.invoke(s, "mv", ["--backup=never", "test_mv_backup_numbering_file_a", "test_mv_backup_numbering_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_backup_numbering_file_a")?
  assert file_exists(s, "test_mv_backup_numbering_file_b")?
  assert file_exists(s, "test_mv_backup_numbering_file_b~")?
}

# origin: uutils test_mv::test_mv_backup_nil
test test_uu_mv_mv_backup_nil { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_backup_numbering_file_a")?
  uu.touch(s, "test_mv_backup_numbering_file_b")?
  {
    let r = uu.invoke(s, "mv", ["--backup=nil", "test_mv_backup_numbering_file_a", "test_mv_backup_numbering_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_backup_numbering_file_a")?
  assert file_exists(s, "test_mv_backup_numbering_file_b")?
  assert file_exists(s, "test_mv_backup_numbering_file_b~")?
}

# origin: uutils test_mv::test_mv_backup_none
test test_uu_mv_mv_backup_none { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_backup_numbering_file_a")?
  uu.touch(s, "test_mv_backup_numbering_file_b")?
  {
    let r = uu.invoke(s, "mv", ["--backup=none", "test_mv_backup_numbering_file_a", "test_mv_backup_numbering_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_backup_numbering_file_a")?
  assert file_exists(s, "test_mv_backup_numbering_file_b")?
  assert ! file_exists(s, "test_mv_backup_numbering_file_b~")?
}

# origin: uutils test_mv::test_mv_backup_numbered
test test_uu_mv_mv_backup_numbered { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_backup_numbering_file_a")?
  uu.touch(s, "test_mv_backup_numbering_file_b")?
  {
    let r = uu.invoke(s, "mv", ["--backup=numbered", "test_mv_backup_numbering_file_a", "test_mv_backup_numbering_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_backup_numbering_file_a")?
  assert file_exists(s, "test_mv_backup_numbering_file_b")?
  assert file_exists(s, "test_mv_backup_numbering_file_b.~1~")?
}

# origin: uutils test_mv::test_mv_backup_numbered_allows_source_named_like_backup
test test_uu_mv_mv_backup_numbered_allows_source_named_like_backup { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.write(s, "a~", "source content")?
  {
    let r = uu.invoke(s, "mv", ["--backup=numbered", "a~", "a"])?
    uu.succeeds(r)
  }
  uu.file_is(s, "a", "source content")
}

# origin: uutils test_mv::test_mv_backup_numbered_with_t
test test_uu_mv_mv_backup_numbered_with_t { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_backup_numbering_file_a")?
  uu.touch(s, "test_mv_backup_numbering_file_b")?
  {
    let r = uu.invoke(s, "mv", ["--backup=t", "test_mv_backup_numbering_file_a", "test_mv_backup_numbering_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_backup_numbering_file_a")?
  assert file_exists(s, "test_mv_backup_numbering_file_b")?
  assert file_exists(s, "test_mv_backup_numbering_file_b.~1~")?
}

# origin: uutils test_mv::test_mv_backup_off
test test_uu_mv_mv_backup_off { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_backup_numbering_file_a")?
  uu.touch(s, "test_mv_backup_numbering_file_b")?
  {
    let r = uu.invoke(s, "mv", ["--backup=off", "test_mv_backup_numbering_file_a", "test_mv_backup_numbering_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_backup_numbering_file_a")?
  assert file_exists(s, "test_mv_backup_numbering_file_b")?
  assert ! file_exists(s, "test_mv_backup_numbering_file_b~")?
}

# origin: uutils test_mv::test_mv_backup_simple
test test_uu_mv_mv_backup_simple { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_backup_numbering_file_a")?
  uu.touch(s, "test_mv_backup_numbering_file_b")?
  {
    let r = uu.invoke(s, "mv", ["--backup=simple", "test_mv_backup_numbering_file_a", "test_mv_backup_numbering_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_backup_numbering_file_a")?
  assert file_exists(s, "test_mv_backup_numbering_file_b")?
  assert file_exists(s, "test_mv_backup_numbering_file_b~")?
}

# origin: uutils test_mv::test_mv_backup_simple_guard_allows_hardlink_source
test test_uu_mv_mv_backup_simple_guard_allows_hardlink_source { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_hl_a", "DSTDATA")?
  uu.write(s, "test_mv_hl_a~", "SRCDATA")?
  uu.hard_link(s, "test_mv_hl_a~", "test_mv_hl_b")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_hl_b", "test_mv_hl_a", "--backup=simple"])?
    uu.succeeds(r)
  }
  uu.file_is(s, "test_mv_hl_a", "SRCDATA")
  uu.file_is(s, "test_mv_hl_a~", "DSTDATA")
}

# origin: uutils test_mv::test_mv_backup_simple_guard_allows_symlink_source
test test_uu_mv_mv_backup_simple_guard_allows_symlink_source { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_sym_a", "DSTDATA")?
  uu.write(s, "test_mv_sym_real", "REALDATA")?
  absolute_link(s, "test_mv_sym_real", "test_mv_sym_a~")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_sym_a~", "test_mv_sym_a", "--backup=simple"])?
    uu.succeeds(r)
  }
}

# origin: uutils test_mv::test_mv_backup_simple_guard_allows_unrelated_source
test test_uu_mv_mv_backup_simple_guard_allows_unrelated_source { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_mv_spell_src", "SRCDATA")?
  uu.write(s, "test_mv_spell_dst", "DSTDATA")?
  {
    let r = uu.invoke(s, "mv", ["test_mv_spell_src", "test_mv_spell_dst", "--backup=simple"])?
    uu.succeeds(r)
  }
  uu.file_is(s, "test_mv_spell_dst", "SRCDATA")
  uu.file_is(s, "test_mv_spell_dst~", "DSTDATA")
}

# origin: uutils test_mv::test_mv_backup_simple_guard_ignores_spelling
test test_uu_mv_mv_backup_simple_guard_ignores_spelling { |ctx|
  for target in ["./test_mv_spell_a", "test_mv_spell_a"] {
    let s = uu.scene(ctx)?
    uu.write(s, "test_mv_spell_a~", "SRCDATA")?
    uu.write(s, "test_mv_spell_a", "DSTDATA")?
    let r = uu.invoke(s, "mv", ["test_mv_spell_a~", target, "--backup=simple"])?
    uu.fails(r)
    uu.stderr_contains(r, "might destroy source")
    uu.file_is(s, "test_mv_spell_a~", "SRCDATA")
  }
}

# origin: uutils test_mv::test_mv_broken_symlink_to_another_fs
test test_uu_mv_mv_broken_symlink_to_another_fs { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.mkdir(s, "foo")?
  absolute_link(s, "missing", "foo/dangling")?
  let r = uu.invoke(s, "mv", ["foo", fp"{other}/foo".display()])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_mv::test_mv_cross_device_broken_symlink_preserved
test test_uu_mv_mv_cross_device_broken_symlink_preserved { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  
  uu.mkdir(s, "src_dir")?
  uu.symlink(s, "/nonexistent/path", "src_dir/broken_link")?
  assert is_symlink(s, "src_dir/broken_link")?
  assert ! file_exists(s, "src_dir/broken_link")?
  let r = uu.invoke(s, "mv", ["src_dir", fp"{other}/dst_dir".display()])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! dir_exists(s, "src_dir")?
  assert fs.stat(fp"{other}/dst_dir/broken_link")?.kind == "symlink"
  assert fp"{other}/dst_dir/broken_link".readlink()? == p"/nonexistent/path"
}

# origin: uutils test_mv::test_mv_cross_device_dir_refuses_symlink_at_recreated_dest
test test_uu_mv_mv_cross_device_dir_refuses_symlink_at_recreated_dest { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  let source = fp"{other}/srcdir"
  source.mkdir()?
  fp"{source}/payload".write("PAYLOAD_FROM_SRC")?
  uu.mkdir(s, "victim")?
  uu.write(s, "victim/guard", "PROTECTED_DATA")?
  absolute_link(s, "victim", "target")?
  let _ = uu.invoke(s, "mv", ["-T", source.display(), "target"])?
  assert ! file_exists(s, "victim/payload")?
  uu.file_is(s, "victim/guard", "PROTECTED_DATA")
}

# origin: uutils test_mv::test_mv_cross_device_dir_xattr_preserved
test test_uu_mv_mv_cross_device_dir_xattr_preserved { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.mkdir(s, "src_dir")?
  uu.write(s, "src_dir/file.txt", "content")?
  fs.xattr_set(uu.at(s, "src_dir"), "user.dirattr", b"dirvalue")?
  let r = uu.invoke(s, "mv", [uu.at(s, "src_dir").display(), fp"{other}/dst_dir".display()])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.xattr_get(fp"{other}/dst_dir", "user.dirattr")? == b"dirvalue"
}

# origin: uutils test_mv::test_mv_cross_device_file_symlink_preserved
test test_uu_mv_mv_cross_device_file_symlink_preserved { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  
  uu.mkdir(s, "src_dir")?
  uu.write(s, "src_dir/target.txt", "target content")?
  absolute_link(s, "src_dir/target.txt", "src_dir/file_link")?
  assert is_symlink(s, "src_dir/file_link")?
  let r = uu.invoke(s, "mv", ["src_dir", fp"{other}/dst_dir".display()])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! dir_exists(s, "src_dir")?
  assert fs.stat(fp"{other}/dst_dir/file_link")?.kind == "symlink"
  assert fp"{other}/dst_dir/target.txt".exists()?
  assert fp"{other}/dst_dir/target.txt".read_text()? == "target content"
}

# origin: uutils test_mv::test_mv_cross_device_permission_denied
test test_uu_mv_mv_cross_device_permission_denied { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.write(s, "k", "source content")?
  let target = fp"{other}/k"
  target.write("target content")?
  other.chmod(0o555)?
  let r = uu.invoke(s, "mv", ["-f", "k", target.display()])?
  uu.fails(r)
  let message = r.stderr.utf8()?
  assert "Permission denied" in message or "permission denied" in message
  other.chmod(0o755)?
}

# origin: uutils test_mv::test_mv_cross_device_preserves_ownership
test test_uu_mv_mv_cross_device_preserves_ownership { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.write(s, "owned_file", "owned content")?
  let original = fs.stat(uu.at(s, "owned_file"))?
  let groups = unix.id()?.supplementary
  let choices = [gid for gid in groups if gid != original.gid]
  if choices.is_empty() { test.skip("no supplementary group available for chgrp") }
  let other_gid = choices[0]
  fs.set_owner(uu.at(s, "owned_file"), gid: other_gid)?
  assert fs.stat(uu.at(s, "owned_file"))?.gid == other_gid
  let r = uu.invoke(s, "mv", ["owned_file", fp"{other}/owned_file".display()])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let moved = fs.stat(fp"{other}/owned_file")?
  assert moved.gid == other_gid
  assert moved.uid == original.uid
}

# origin: uutils test_mv::test_mv_cross_device_preserves_ownership_recursive
test test_uu_mv_mv_cross_device_preserves_ownership_recursive { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.mkdir(s, "owned_dir/sub")?
  uu.write(s, "owned_dir/file1", "content1")?
  uu.write(s, "owned_dir/sub/file2", "content2")?
  let original = fs.stat(uu.at(s, "owned_dir"))?
  let groups = unix.id()?.supplementary
  let choices = [gid for gid in groups if gid != original.gid]
  if choices.is_empty() { test.skip("no supplementary group available for chgrp") }
  let other_gid = choices[0]
  for name in ["owned_dir", "owned_dir/sub", "owned_dir/file1", "owned_dir/sub/file2"] { fs.set_owner(uu.at(s, name), gid: other_gid)? }
  let r = uu.invoke(s, "mv", ["owned_dir", fp"{other}/owned_dir".display()])?
  uu.succeeds(r)
  uu.no_stderr(r)
  for name in ["owned_dir", "owned_dir/file1", "owned_dir/sub/file2"] { assert fs.stat(fp"{other}/{name}")?.gid == other_gid }
}

# origin: uutils test_mv::test_mv_cross_device_refuses_planted_symlink_dest
test test_uu_mv_mv_cross_device_refuses_planted_symlink_dest { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.write(s, "payload", "PAYLOAD_FROM_SRC")?
  let victim = fp"{other}/victim"
  victim.write("PROTECTED_DATA")?
  let target = fp"{other}/target"
  target.symlink(to: victim)
  uu.succeeds(uu.invoke(s, "mv", ["-f", "payload", target.display()])?)
  assert victim.read_text()? == "PROTECTED_DATA"
}

# origin: uutils test_mv::test_mv_cross_device_symlink_onto_existing
test test_uu_mv_mv_cross_device_symlink_onto_existing { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.symlink(s, "/etc/passwd", "src_link")?
  let target = fp"{other}/dst_exists"
  target.write("placeholder")?
  let r = uu.invoke(s, "mv", [uu.at(s, "src_link").display(), target.display()])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(target)?.kind == "symlink"
  assert target.readlink()? == p"/etc/passwd"
  assert ! is_symlink(s, "src_link")?
}

# origin: uutils test_mv::test_mv_cross_device_symlink_onto_existing_dir
test test_uu_mv_mv_cross_device_symlink_onto_existing_dir { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  uu.symlink(s, "/etc/passwd", "src_link")?
  let target = fp"{other}/dst_dir"
  target.mkdir()?
  fp"{target}/guard".write("preserved")?
  let r = uu.invoke(s, "mv", ["-T", uu.at(s, "src_link").display(), target.display()])?
  uu.fails(r)
  assert target.is_dir()?
  assert fp"{target}/guard".is_file()?
  assert is_symlink(s, "src_link")?
}

# origin: uutils test_mv::test_mv_cross_device_symlink_preserved
test test_uu_mv_mv_cross_device_symlink_preserved { |ctx|
  let s = uu.scene(ctx)?
  let other = other_fs(s)?
  defer cleanup_other(other)
  
  uu.mkdir(s, "src_dir")?
  uu.write(s, "src_dir/local.txt", "local content")?
  uu.symlink(s, "/etc", "src_dir/etc_link")?
  assert is_symlink(s, "src_dir/etc_link")?
  let r = uu.invoke(s, "mv", ["src_dir", fp"{other}/dst_dir".display()])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! dir_exists(s, "src_dir")?
  assert fs.stat(fp"{other}/dst_dir/etc_link")?.kind == "symlink"
  assert fp"{other}/dst_dir/etc_link".readlink()? == p"/etc"
  assert fp"{other}/dst_dir/local.txt".exists()?
}

# origin: uutils test_mv::test_mv_custom_backup_suffix
test test_uu_mv_mv_custom_backup_suffix { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_custom_backup_suffix_file_a")?
  uu.touch(s, "test_mv_custom_backup_suffix_file_b")?
  {
    let r = uu.invoke(s, "mv", ["-b", "--suffix=super-suffix-of-the-century", "test_mv_custom_backup_suffix_file_a", "test_mv_custom_backup_suffix_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_custom_backup_suffix_file_a")?
  assert file_exists(s, "test_mv_custom_backup_suffix_file_b")?
  assert file_exists(s, "test_mv_custom_backup_suffix_file_bsuper-suffix-of-the-century")?
}

# origin: uutils test_mv::test_mv_custom_backup_suffix_hyphen_value
test test_uu_mv_mv_custom_backup_suffix_hyphen_value { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_custom_backup_suffix_file_a")?
  uu.touch(s, "test_mv_custom_backup_suffix_file_b")?
  {
    let r = uu.invoke(s, "mv", ["-b", "--suffix=-v", "test_mv_custom_backup_suffix_file_a", "test_mv_custom_backup_suffix_file_b"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_custom_backup_suffix_file_a")?
  assert file_exists(s, "test_mv_custom_backup_suffix_file_b")?
  assert file_exists(s, "test_mv_custom_backup_suffix_file_b-v")?
}

# origin: uutils test_mv::test_mv_custom_backup_suffix_via_env
test test_uu_mv_mv_custom_backup_suffix_via_env { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_mv_custom_backup_suffix_file_a")?
  uu.touch(s, "test_mv_custom_backup_suffix_file_b")?
  {
    let r = uu.invoke(s, "mv", ["-b", "test_mv_custom_backup_suffix_file_a", "test_mv_custom_backup_suffix_file_b"], vars: {SIMPLE_BACKUP_SUFFIX: "super-suffix-of-the-century"})?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert ! file_exists(s, "test_mv_custom_backup_suffix_file_a")?
  assert file_exists(s, "test_mv_custom_backup_suffix_file_b")?
  assert file_exists(s, "test_mv_custom_backup_suffix_file_bsuper-suffix-of-the-century")?
}

# origin: uutils test_mv::test_mv_dangling_symlink_to_folder
test test_uu_mv_mv_dangling_symlink_to_folder { |ctx|
  let s = uu.scene(ctx)?
  absolute_link(s, "404", "abc")?
  uu.mkdir(s, "x")?
  {
    let r = uu.invoke(s, "mv", ["abc", "x"])?
    uu.succeeds(r)
  }
  assert is_symlink(s, "x/abc")?
}

# origin: uutils test_mv::test_mv_dir_into_dir_with_source_name_a_prefix_of_target_name
test test_uu_mv_mv_dir_into_dir_with_source_name_a_prefix_of_target_name { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "test")?
  uu.mkdir(s, "test2")?
  {
    let r = uu.invoke(s, "mv", ["test", "test2"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  assert dir_exists(s, "test2/test")?
}

# origin: uutils test_mv::test_mv_dir_into_file_where_both_are_files
test test_uu_mv_mv_dir_into_file_where_both_are_files { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  {
    let r = uu.invoke(s, "mv", ["a/", "b"])?
    uu.fails(r)
    uu.stderr_contains(r, "mv: cannot stat 'a/': Not a directory")
  }
}

# origin: uutils test_mv::test_mv_dir_into_path_slash
test test_uu_mv_mv_dir_into_path_slash { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  {
    let r = uu.invoke(s, "mv", ["a", "e/"])?
    uu.succeeds(r)
  }
  assert dir_exists(s, "e")?
  uu.mkdir(s, "b")?
  uu.mkdir(s, "f")?
  {
    let r = uu.invoke(s, "mv", ["b", "f/"])?
    uu.succeeds(r)
  }
  assert dir_exists(s, "f/b")?
}

# origin: uutils test_mv::test_mv_dir_with_symlink_cycles_terminates
test test_uu_mv_mv_dir_with_symlink_cycles_terminates { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "dest")?
  uu.write(s, "dir/file", "content")?
  uu.symlink(s, ".", "dir/loop1")?
  uu.symlink(s, ".", "dir/loop2")?
  {
    let r = uu.invoke(s, "mv", ["dir", "dest/"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  assert dir_exists(s, "dest/dir")?
  uu.file_is(s, "dest/dir/file", "content")
  assert is_symlink(s, "dest/dir/loop1")?
  assert is_symlink(s, "dest/dir/loop2")?
  assert ! dir_exists(s, "dir")?
}

# origin: uutils test_mv::test_mv_directory_self::case_01
test test_uu_mv_test_mv_directory_self_case_01 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir")?
  let r = uu.invoke(s, "mv", ["mydir", "mydir"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: cannot move 'mydir' to a subdirectory of itself, 'mydir/mydir'")
}

# origin: uutils test_mv::test_mv_directory_self::case_02
test test_uu_mv_test_mv_directory_self_case_02 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir")?
  let r = uu.invoke(s, "mv", ["mydir/", "mydir/"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: cannot move 'mydir/' to a subdirectory of itself, 'mydir/mydir'")
}

# origin: uutils test_mv::test_mv_directory_self::case_03
test test_uu_mv_test_mv_directory_self_case_03 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir")?
  let r = uu.invoke(s, "mv", ["./mydir", "mydir", "mydir/"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: cannot move './mydir' to a subdirectory of itself, 'mydir/mydir'")
}

# origin: uutils test_mv::test_mv_directory_self::case_04
test test_uu_mv_test_mv_directory_self_case_04 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir")?
  let r = uu.invoke(s, "mv", ["mydir/", "mydir/mydir_2/"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: cannot move 'mydir/' to a subdirectory of itself, 'mydir/mydir_2/'")
}

# origin: uutils test_mv::test_mv_directory_self::case_05
test test_uu_mv_test_mv_directory_self_case_05 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir/mydir_2")?
  let r = uu.invoke(s, "mv", ["mydir", "mydir/mydir_2"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: cannot move 'mydir' to a subdirectory of itself, 'mydir/mydir_2/mydir'\n")
}

# origin: uutils test_mv::test_mv_directory_self::case_06
test test_uu_mv_test_mv_directory_self_case_06 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir/mydir_2")?
  let r = uu.invoke(s, "mv", ["mydir/", "mydir/mydir_2/"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: cannot move 'mydir/' to a subdirectory of itself, 'mydir/mydir_2/mydir'\n")
}

# origin: uutils test_mv::test_mv_directory_self::case_07
test test_uu_mv_test_mv_directory_self_case_07 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir")?
  uu.mkdir_all(s, "mydir_2")?
  let r = uu.invoke(s, "mv", ["mydir/", "mydir_2/", "mydir_2/"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: cannot move 'mydir_2/' to a subdirectory of itself, 'mydir_2/mydir_2'")
}

# origin: uutils test_mv::test_mv_directory_self::case_08
test test_uu_mv_test_mv_directory_self_case_08 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir")?
  let r = uu.invoke(s, "mv", ["mydir/", "mydir"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: cannot move 'mydir/' to a subdirectory of itself, 'mydir/mydir'")
}

# origin: uutils test_mv::test_mv_directory_self::case_09
test test_uu_mv_test_mv_directory_self_case_09 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir")?
  let r = uu.invoke(s, "mv", ["-T", "mydir", "mydir"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: 'mydir' and 'mydir' are the same file")
}

# origin: uutils test_mv::test_mv_directory_self::case_10
test test_uu_mv_test_mv_directory_self_case_10 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "mydir")?
  let r = uu.invoke(s, "mv", ["mydir/", "mydir/../"])?
  uu.fails(r)
  uu.stderr_contains(r, "mv: 'mydir/' and 'mydir/../mydir' are the same file")
}

# origin: uutils test_mv::test_mv_error_msg_with_multiple_sources_that_does_not_exist
test test_uu_mv_mv_error_msg_with_multiple_sources_that_does_not_exist { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  {
    let r = uu.invoke(s, "mv", ["a", "b/", "d"])?
    uu.fails(r)
    uu.stderr_contains(r, "mv: cannot stat 'a': No such file or directory")
    uu.stderr_contains(r, "mv: cannot stat 'b/': No such file or directory")
  }
}

# origin: uutils test_mv::test_mv_error_usage_display_missing_arg
test test_uu_mv_mv_error_usage_display_missing_arg { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "mv", ["--target-directory=."])?
    uu.fails(r)
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, "mv: missing file operand\nTry 'mv --help' for more information.\n")
  }
}

# origin: uutils test_mv::test_mv_error_usage_display_too_few
test test_uu_mv_mv_error_usage_display_too_few { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "mv", ["file1"])?
    uu.fails(r)
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, "mv: missing destination file operand after 'file1'\nTry 'mv --help' for more information.\n")
  }
}

# origin: uutils test_mv::test_mv_errors
test test_uu_mv_mv_errors { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "test_mv_errors_dir")?
  uu.touch(s, "test_mv_errors_file_a")?
  uu.touch(s, "test_mv_errors_file_b")?
  {
    let r = uu.invoke(s, "mv", ["-T", "-t", "test_mv_errors_dir", "test_mv_errors_file_a", "test_mv_errors_file_b"])?
    uu.fails(r)
    uu.stderr_contains(r, "cannot combine --target-directory (-t) and --no-target-directory (-T)")
  }
  {
    let r = uu.invoke(s, "mv", ["-T", "test_mv_errors_file_a", "test_mv_errors_dir"])?
    uu.fails(r)
    uu.stderr_only(r, "mv: cannot overwrite directory 'test_mv_errors_dir' with non-directory 'test_mv_errors_file_a'\n")
  }
  {
    let r = uu.invoke(s, "mv", ["test_mv_errors_dir", "test_mv_errors_file_a"])?
    uu.fails(r)
    uu.stderr_only(r, "mv: cannot overwrite non-directory 'test_mv_errors_file_a' with directory 'test_mv_errors_dir'\n")
  }
}

# origin: uutils test_mv::test_mv_exchange_file_and_dir
test test_uu_mv_mv_exchange_file_and_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "leaf", "payload")?
  uu.mkdir(s, "branch")?
  {
    let r = uu.invoke(s, "mv", ["-T", "--exchange", "leaf", "branch"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
  assert dir_exists(s, "leaf")?
  assert file_exists(s, "branch")?
  uu.file_is(s, "branch", "payload")
}

# origin: uutils test_mv::test_mv_exchange_missing_target
test test_uu_mv_mv_exchange_missing_target { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "present", "data")?
  {
    let r = uu.invoke(s, "mv", ["--exchange", "present", "absent"])?
    uu.fails(r)
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "cannot exchange")
    uu.stderr_contains(r, "present")
    uu.stderr_contains(r, "absent")
  }
}

# origin: uutils test_mv::test_mv_exchange_multiple_operands_into_directory
test test_uu_mv_mv_exchange_multiple_operands_into_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "src-a")?
  uu.write(s, "b", "src-b")?
  uu.write(s, "c", "src-c")?
  uu.mkdir(s, "myfolder")?
  uu.write(s, "myfolder/a", "dst-a")?
  uu.write(s, "myfolder/b", "dst-b")?
  uu.write(s, "myfolder/c", "dst-c")?
  {
    let r = uu.invoke(s, "mv", ["--exchange", "-v", "a", "b", "c", "myfolder/"])?
    uu.succeeds(r)
    uu.stdout_contains(r, "exchanged 'a' <-> 'myfolder/a'")
    uu.stdout_contains(r, "exchanged 'b' <-> 'myfolder/b'")
    uu.stdout_contains(r, "exchanged 'c' <-> 'myfolder/c'")
  }
  uu.file_is(s, "a", "dst-a")
  uu.file_is(s, "b", "dst-b")
  uu.file_is(s, "c", "dst-c")
  uu.file_is(s, "myfolder/a", "src-a")
  uu.file_is(s, "myfolder/b", "src-b")
  uu.file_is(s, "myfolder/c", "src-c")
}

# origin: uutils test_mv::test_mv_exchange_same_file
test test_uu_mv_mv_exchange_same_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "only", "data")?
  {
    let r = uu.invoke(s, "mv", ["--exchange", "only", "only"])?
    uu.fails(r)
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "are the same file")
  }
  uu.file_is(s, "only", "data")
}

# origin: uutils test_mv::test_mv_exchange_verbose
test test_uu_mv_mv_exchange_verbose { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "first", "1")?
  uu.write(s, "second", "2")?
  {
    let r = uu.invoke(s, "mv", ["--exchange", "-v", "first", "second"])?
    uu.succeeds(r)
    uu.stdout_contains(r, "exchanged")
  }
  uu.file_is(s, "first", "2")
  uu.file_is(s, "second", "1")
}

