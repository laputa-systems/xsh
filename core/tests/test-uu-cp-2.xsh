##! Transcribed from the MIT-licensed uutils cp integration tests.

use support.uu as uu

# The upstream scene includes these text fixtures and the existing hello_dir.
proc scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  uu.write(s, "hello_world.txt", "Hello, World!\n")?
  uu.write(s, "how_are_you.txt", "How are you?\n")?
  uu.write(s, "existing_file.txt", "Cogito ergo sum.\n")?
  uu.mkdir(s, "hello_dir")?
  uu.touch(s, "hello_dir/hello.txt")?
  Ok(s)
}

# Upstream file_exists treats missing paths as false.
proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let file = uu.at(s, name)
  if !file.exists()? { return Ok(false) }
  Ok(file.is_file()?)
}

# origin: uutils test_cp::test_cp_archive_deref_flag_ordering
test test_uu_cp_cp_archive_deref_flag_ordering { |ctx|
  for case in [ {flags: "-Ha", symlink: true}, {flags: "-aH", symlink: false}, {flags: "-Hd", symlink: true}, {flags: "-dH", symlink: false}, {flags: "-La", symlink: true}, {flags: "-aL", symlink: false}, {flags: "-Ld", symlink: true}, {flags: "-dL", symlink: false} ] {
    let s = scene(ctx)?
    uu.touch(s, "file.txt")?
    uu.symlink(s, uu.at(s, "file.txt").display(), "symlink")?
    let dest = f"dest{case.flags}"
    uu.succeeds(uu.invoke(s, "cp", [case.flags, "symlink", dest])?)
    assert uu.is_symlink(s, dest)? == case.symlink, case.flags
  }
}

# origin: uutils test_cp::test_cp_archive_deref_preserves_mode
test test_uu_cp_cp_archive_deref_preserves_mode { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "srcdir")?
  uu.touch(s, "srcdir/file.txt")?
  uu.set_mode(s, "srcdir/file.txt", 0o705)?
  uu.succeeds(uu.invoke(s, "cp", ["-aL", "srcdir", "dest"])?)
  assert uu.mode(s, "dest/file.txt")? % 512 == 0o705
}

# origin: uutils test_cp::test_cp_archive_deref_preserves_recursive
test test_uu_cp_cp_archive_deref_preserves_recursive { |ctx|
  for flags in ["-afL", "-aLf", "-aHL", "-adL"] {
    let s = scene(ctx)?
    uu.mkdir(s, "srcdir")?
    uu.touch(s, "srcdir/file.txt")?
    let dest = f"dest_{flags.replace("-", with: "")}"
    uu.succeeds(uu.invoke(s, "cp", [flags, "srcdir", dest])?)
    assert file_exists(s, f"{dest}/file.txt")?, flags
  }
}

# origin: uutils test_cp::test_cp_archive_deref_repeated_flag_last_wins
test test_uu_cp_cp_archive_deref_repeated_flag_last_wins { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "srcdir")?
  uu.touch(s, "srcdir/real.txt")?
  uu.symlink(s, "real.txt", "srcdir/link.txt")?
  let r1 = uu.invoke(s, "cp", ["-aL", "-a", "srcdir", "dest"])?
  uu.succeeds(r1)
  assert uu.is_symlink(s, "dest/link.txt")?
  let r2 = uu.invoke(s, "cp", ["-La", "-L", "srcdir", "dest2"])?
  uu.succeeds(r2)
  assert !uu.is_symlink(s, "dest2/link.txt")?
}

# origin: uutils test_cp::test_cp_archive_deref_symlinks_inside_dir
test test_uu_cp_cp_archive_deref_symlinks_inside_dir { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "srcdir")?
  uu.touch(s, "srcdir/real.txt")?
  uu.symlink(s, "real.txt", "srcdir/link.txt")?
  let r1 = uu.invoke(s, "cp", ["-a", "srcdir", "dest_a"])?
  uu.succeeds(r1)
  assert uu.is_symlink(s, "dest_a/link.txt")?
  let r2 = uu.invoke(s, "cp", ["-aL", "srcdir", "dest_aL"])?
  uu.succeeds(r2)
  assert !uu.is_symlink(s, "dest_aL/link.txt")?
  let r3 = uu.invoke(s, "cp", ["-La", "srcdir", "dest_La"])?
  uu.succeeds(r3)
  assert uu.is_symlink(s, "dest_La/link.txt")?
}

# origin: uutils test_cp::test_cp_archive_on_directory_ending_dot
test test_uu_cp_cp_archive_on_directory_ending_dot { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir1/file")?
  let r1 = uu.invoke(s, "cp", ["-a", "dir1/.", "dir2"])?
  uu.succeeds(r1)
  assert file_exists(s, "dir2/file")?
}

# origin: uutils test_cp::test_cp_archive_on_nonexistent_file
test test_uu_cp_cp_archive_on_nonexistent_file { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["-a", "nonexistent_file.txt", "existing_file.txt"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot stat 'nonexistent_file.txt'")
  uu.stderr_contains(r1, "No such file or directory")
}

# origin: uutils test_cp::test_cp_archive_preserves_directory_permissions
test test_uu_cp_cp_archive_preserves_directory_permissions { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "test-images")?
  let subdirs = ["fail", "gif-test-suite", "randomly-modified", "reftests"]
  var index = 1
  for subdir in subdirs {
    let directory = f"test-images/{subdir}"
    uu.mkdir(s, directory)?
    uu.set_mode(s, directory, 0o755)?
    uu.write(s, f"{directory}/test{index}.txt", "test content")?
    index += 1
  }
  uu.succeeds(uu.invoke(s, "cp", ["-a", "test-images", "test-images-copy"])?)
  for subdir in subdirs { assert uu.mode(s, f"test-images-copy/{subdir}")? % 512 == 0o755 }
}

# origin: uutils test_cp::test_cp_archive_recursive
test test_uu_cp_cp_archive_recursive { |ctx|
  let s = scene(ctx)?
  for name in ["1", "2"] {
    uu.touch(s, f"hello_dir/{name}")?
    uu.symlink(s, uu.at(s, name).display(), f"hello_dir/{name}.link")?
  }
  uu.succeeds(uu.invoke(s, "cp", ["--archive", "hello_dir/", "hello_dir_new"])?)
  for name in ["1", "2"] {
    assert file_exists(s, f"hello_dir_new/{name}")?
    assert uu.is_symlink(s, f"hello_dir_new/{name}.link")?
  }
}

# origin: uutils test_cp::test_cp_arg_backup
test test_uu_cp_cp_arg_backup { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "how_are_you.txt", "-b"])?
  uu.succeeds(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_backup_arg_first
test test_uu_cp_cp_arg_backup_arg_first { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_backup_with_dest_a_symlink
test test_uu_cp_cp_arg_backup_with_dest_a_symlink { |ctx|
  let s = scene(ctx)?
  uu.write(s, "source", "content")?
  uu.write(s, "original", "original")?
  uu.symlink(s, uu.at(s, "original").display(), "symlink")?
  let r1 = uu.invoke(s, "cp", ["-b", "source", "symlink"])?
  uu.succeeds(r1)
  assert !uu.is_symlink(s, "symlink")?
  uu.file_is(s, "symlink", "content")
  assert uu.is_symlink(s, "symlink~")?
  assert uu.read_link(s, "symlink~")? == uu.at(s, "original").display()
}

# origin: uutils test_cp::test_cp_arg_backup_with_dest_a_symlink_to_source
test test_uu_cp_cp_arg_backup_with_dest_a_symlink_to_source { |ctx|
  let s = scene(ctx)?
  uu.write(s, "source", "content")?
  uu.symlink(s, uu.at(s, "source").display(), "symlink")?
  let r1 = uu.invoke(s, "cp", ["-b", "source", "symlink"])?
  uu.succeeds(r1)
  assert !uu.is_symlink(s, "symlink")?
  uu.file_is(s, "symlink", "content")
  assert uu.is_symlink(s, "symlink~")?
  assert uu.read_link(s, "symlink~")? == uu.at(s, "source").display()
}

# origin: uutils test_cp::test_cp_arg_backup_with_other_args
test test_uu_cp_cp_arg_backup_with_other_args { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "how_are_you.txt", "-vbL"])?
  uu.succeeds(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_force
test test_uu_cp_cp_arg_force { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "copy_of_hello_world.txt")?
  uu.set_mode(s, "copy_of_hello_world.txt", 0o444)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "--force", "copy_of_hello_world.txt"])?
  uu.succeeds(r1)
  uu.file_is(s, "copy_of_hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_arg_interactive
test test_uu_cp_cp_arg_interactive { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r1 = uu.invoke(s, "cp", ["-i", "a", "b"] , stdin: b"N\n")?
  uu.fails(r1)
  uu.stderr_only(r1, "cp: overwrite 'b'? ")
}

# origin: uutils test_cp::test_cp_arg_interactive_update_overwrite_newer
test test_uu_cp_cp_arg_interactive_update_overwrite_newer { |ctx|
  for closed in [false, true] {
    let s = scene(ctx)?
    uu.touch(s, "a")?
    uu.touch(s, "b")?
    let r1 = if closed { uu.invoke(s, "cp", ["-i", "-u", "a", "b"])? } else { uu.invoke(s, "cp", ["-i", "-u", "a", "b"], stdin: b"")? }
    uu.succeeds(r1)
    uu.no_stdout(r1)
  }
}

# origin: uutils test_cp::test_cp_arg_interactive_update_overwrite_older
test test_uu_cp_cp_arg_interactive_update_overwrite_older { |ctx|
  for response in ["N\n", "Y\n"] {
    let s = scene(ctx)?
    uu.touch(s, "b")?
    time.sleep(100ms)?
    uu.touch(s, "a")?
    let r1 = uu.invoke(s, "cp", ["-i", "-u", "a", "b"], stdin: bytes.from_text(response))?
    if response == "N\n" {
      uu.fails_with_code(r1, 1)
      uu.stderr_is(r1, "cp: overwrite 'b'? ")
    } else { uu.succeeds(r1) }
    uu.no_stdout(r1)
  }
}

# origin: uutils test_cp::test_cp_arg_interactive_verbose
test test_uu_cp_cp_arg_interactive_verbose { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r1 = uu.invoke(s, "cp", ["-vi", "a", "b"] , stdin: b"N\n")?
  uu.fails(r1)
  uu.stderr_only(r1, "cp: overwrite 'b'? ")
}

# origin: uutils test_cp::test_cp_arg_interactive_verbose_clobber
test test_uu_cp_cp_arg_interactive_verbose_clobber { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r1 = uu.invoke(s, "cp", ["-vin", "--debug", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "skipped 'b'")
}

# origin: uutils test_cp::test_cp_arg_link
test test_uu_cp_cp_arg_link { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "--link", "copy_of_hello_world.txt"])?
  uu.succeeds(r1)
  assert fs.stat(uu.at(s, "hello_world.txt"))?.nlink == 2
}

# origin: uutils test_cp::test_cp_arg_link_with_dest_hardlink_to_source
test test_uu_cp_cp_arg_link_with_dest_hardlink_to_source { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "file")?
  uu.hard_link(s, "file", "hardlink")?
  let r1 = uu.invoke(s, "cp", ["--link", "file", "hardlink"])?
  uu.succeeds(r1)
  assert fs.stat(uu.at(s, "file"))?.nlink == 2
  assert file_exists(s, "file")?
  assert file_exists(s, "hardlink")?
}

# origin: uutils test_cp::test_cp_arg_link_with_same_file
test test_uu_cp_cp_arg_link_with_same_file { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "file")?
  let r1 = uu.invoke(s, "cp", ["--link", "file", "file"])?
  uu.succeeds(r1)
  assert fs.stat(uu.at(s, "file"))?.nlink == 1
  assert file_exists(s, "file")?
}

# origin: uutils test_cp::test_cp_arg_no_clobber
test test_uu_cp_cp_arg_no_clobber { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "how_are_you.txt", "--no-clobber", "--debug"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "skipped 'how_are_you.txt'")
  uu.file_is(s, "how_are_you.txt", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_no_clobber_inferred_arg
test test_uu_cp_cp_arg_no_clobber_inferred_arg { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "how_are_you.txt", "--no-clob", "--debug"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "skipped 'how_are_you.txt'")
  uu.file_is(s, "how_are_you.txt", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_no_clobber_twice
test test_uu_cp_cp_arg_no_clobber_twice { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "hello_world.txt")?
  let r1 = uu.invoke(s, "cp", ["--no-clobber", "hello_world.txt", "copy_of_hello_world.txt", "--debug"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "hello_world.txt", "")
  uu.append(s, "hello_world.txt", "some-content")?
  let r2 = uu.invoke(s, "cp", ["--no-clobber", "hello_world.txt", "copy_of_hello_world.txt", "--debug"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "skipped 'copy_of_hello_world.txt'")
  uu.file_is(s, "hello_world.txt", "some-content")
  uu.file_is(s, "copy_of_hello_world.txt", "")
}

# origin: uutils test_cp::test_cp_arg_no_target_directory
test test_uu_cp_cp_arg_no_target_directory { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "-v", "-T", "hello_dir/"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot overwrite directory")
}

# origin: uutils test_cp::test_cp_arg_no_target_directory_with_recursive
test test_uu_cp_cp_arg_no_target_directory_with_recursive { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir/a")?
  uu.touch(s, "dir/b")?
  let r1 = uu.invoke(s, "cp", ["-rT", "dir", "dir2"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert uu.exists(s, "dir2/a")?
  assert uu.exists(s, "dir2/b")?
  assert !uu.exists(s, "dir2/dir")?
}

# origin: uutils test_cp::test_cp_arg_no_target_directory_with_recursive_target_does_not_exists
test test_uu_cp_cp_arg_no_target_directory_with_recursive_target_does_not_exists { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/a")?
  uu.touch(s, "dir/b")?
  assert !uu.exists(s, "create_me")?
  let r1 = uu.invoke(s, "cp", ["-rT", "dir", "create_me"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert uu.exists(s, "create_me/a")?
  assert uu.exists(s, "create_me/b")?
  assert !uu.exists(s, "create_me/dir")?
}

# origin: uutils test_cp::test_cp_arg_remove_destination
test test_uu_cp_cp_arg_remove_destination { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "copy_of_hello_world.txt")?
  uu.set_mode(s, "copy_of_hello_world.txt", 0o444)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "--remove-destination", "copy_of_hello_world.txt"])?
  uu.succeeds(r1)
  uu.file_is(s, "copy_of_hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_arg_suffix
test test_uu_cp_cp_arg_suffix { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "-b", "--suffix", ".bak", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt.bak", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_suffix_hyphen_value
test test_uu_cp_cp_arg_suffix_hyphen_value { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "-b", "--suffix", "-v", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt-v", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_suffix_without_backup_option
test test_uu_cp_cp_arg_suffix_without_backup_option { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "--suffix", ".bak", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt.bak", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_symlink
test test_uu_cp_cp_arg_symlink { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "--symbolic-link", "copy_of_hello_world.txt"])?
  uu.succeeds(r1)
  assert uu.is_symlink(s, "copy_of_hello_world.txt")?
  assert uu.read_link(s, "copy_of_hello_world.txt")? == "hello_world.txt"
  uu.file_is(s, "copy_of_hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_arg_target_directory
test test_uu_cp_cp_arg_target_directory { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "-t", "hello_dir/"])?
  uu.succeeds(r1)
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_arg_update_all
test test_uu_cp_cp_arg_update_all { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "how_are_you.txt", "--update=all"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert uu.read(s, "how_are_you.txt")? == uu.read(s, "hello_world.txt")?
}

# origin: uutils test_cp::test_cp_arg_update_all_then_none
test test_uu_cp_cp_arg_update_all_then_none { |ctx|
  let s = scene(ctx)?
  let old = "test_cp_arg_update_all_then_none_file1"
  let new = "test_cp_arg_update_all_then_none_file2"
  uu.write(s, old, "old content\n")?
  fs.set_times(uu.at(s, old), mtime_ns: 0)?
  uu.write(s, new, "new content\n")?
  let r1 = uu.invoke(s, "cp", ["test_cp_arg_update_all_then_none_file1", "test_cp_arg_update_all_then_none_file2", "--update=all", "--update=none"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "test_cp_arg_update_all_then_none_file2", "new content\n")
}

# origin: uutils test_cp::test_cp_arg_update_interactive_error
test test_uu_cp_cp_arg_update_interactive_error { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "how_are_you.txt", "-i"])?
  uu.fails(r1)
  uu.no_stdout(r1)
}

# origin: uutils test_cp::test_cp_arg_update_none
test test_uu_cp_cp_arg_update_none { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "how_are_you.txt", "--update=none"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "how_are_you.txt", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_update_none_fail
test test_uu_cp_cp_arg_update_none_fail { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "how_are_you.txt", "--update=none-fail"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "not replacing 'how_are_you.txt'")
  uu.file_is(s, "how_are_you.txt", "How are you?\n")
}

# origin: uutils test_cp::test_cp_arg_update_none_then_all
test test_uu_cp_cp_arg_update_none_then_all { |ctx|
  let s = scene(ctx)?
  let old = "test_cp_arg_update_none_then_all_file1"
  let new = "test_cp_arg_update_none_then_all_file2"
  uu.write(s, old, "old content\n")?
  fs.set_times(uu.at(s, old), mtime_ns: 0)?
  uu.write(s, new, "new content\n")?
  let r1 = uu.invoke(s, "cp", ["test_cp_arg_update_none_then_all_file1", "test_cp_arg_update_none_then_all_file2", "--update=none", "--update=all"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "test_cp_arg_update_none_then_all_file2", "old content\n")
}

# origin: uutils test_cp::test_cp_arg_update_older_dest_not_older_than_src
test test_uu_cp_cp_arg_update_older_dest_not_older_than_src { |ctx|
  let s = scene(ctx)?
  let old = "test_cp_arg_update_dest_not_older_file1"
  let new = "test_cp_arg_update_dest_not_older_file2"
  uu.write(s, old, "old content\n")?
  uu.write(s, new, "new content\n")?
  let r1 = uu.invoke(s, "cp", ["test_cp_arg_update_dest_not_older_file1", "test_cp_arg_update_dest_not_older_file2", "--update=older"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "test_cp_arg_update_dest_not_older_file2", "new content\n")
}

# origin: uutils test_cp::test_cp_arg_update_older_dest_not_older_than_src_no_verbose_output
test test_uu_cp_cp_arg_update_older_dest_not_older_than_src_no_verbose_output { |ctx|
  let s = scene(ctx)?
  let old = "test_cp_arg_update_dest_not_older_file1"
  let new = "test_cp_arg_update_dest_not_older_file2"
  uu.write(s, old, "old content\n")?
  uu.write(s, new, "new content\n")?
  let r1 = uu.invoke(s, "cp", ["test_cp_arg_update_dest_not_older_file1", "test_cp_arg_update_dest_not_older_file2", "--verbose", "--update=older"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "test_cp_arg_update_dest_not_older_file2", "new content\n")
}

# origin: uutils test_cp::test_cp_arg_update_older_dest_older_than_src
test test_uu_cp_cp_arg_update_older_dest_older_than_src { |ctx|
  let s = scene(ctx)?
  let old = "test_cp_arg_update_dest_older_file1"
  let new = "test_cp_arg_update_dest_older_file2"
  uu.write(s, old, "old content\n")?
  fs.set_times(uu.at(s, old), mtime_ns: 0)?
  uu.write(s, new, "new content\n")?
  let r1 = uu.invoke(s, "cp", ["test_cp_arg_update_dest_older_file2", "test_cp_arg_update_dest_older_file1", "--update=older"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "test_cp_arg_update_dest_older_file1", "new content\n")
}

# origin: uutils test_cp::test_cp_arg_update_older_dest_older_than_src_with_verbose_output
test test_uu_cp_cp_arg_update_older_dest_older_than_src_with_verbose_output { |ctx|
  let s = scene(ctx)?
  let old = "test_cp_arg_update_dest_older_file1"
  let new = "test_cp_arg_update_dest_older_file2"
  uu.write(s, old, "old content\n")?
  fs.set_times(uu.at(s, old), mtime_ns: 0)?
  uu.write(s, new, "new content\n")?
  let r1 = uu.invoke(s, "cp", ["test_cp_arg_update_dest_older_file2", "test_cp_arg_update_dest_older_file1", "--verbose", "--update=older"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "'test_cp_arg_update_dest_older_file2' -> 'test_cp_arg_update_dest_older_file1'\n")
  uu.file_is(s, "test_cp_arg_update_dest_older_file1", "new content\n")
}

# origin: uutils test_cp::test_cp_arg_update_short_no_overwrite
test test_uu_cp_cp_arg_update_short_no_overwrite { |ctx|
  let s = scene(ctx)?
  let old = "test_cp_arg_update_short_no_overwrite_file1"
  let new = "test_cp_arg_update_short_no_overwrite_file2"
  uu.write(s, old, "old content\n")?
  fs.set_times(uu.at(s, old), mtime_ns: 0)?
  uu.write(s, new, "new content\n")?
  let r1 = uu.invoke(s, "cp", ["test_cp_arg_update_short_no_overwrite_file1", "test_cp_arg_update_short_no_overwrite_file2", "-u"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "test_cp_arg_update_short_no_overwrite_file2", "new content\n")
}

# origin: uutils test_cp::test_cp_arg_update_short_overwrite
test test_uu_cp_cp_arg_update_short_overwrite { |ctx|
  let s = scene(ctx)?
  let old = "test_cp_arg_update_short_overwrite_file1"
  let new = "test_cp_arg_update_short_overwrite_file2"
  uu.write(s, old, "old content\n")?
  fs.set_times(uu.at(s, old), mtime_ns: 0)?
  uu.write(s, new, "new content\n")?
  let r1 = uu.invoke(s, "cp", ["test_cp_arg_update_short_overwrite_file2", "test_cp_arg_update_short_overwrite_file1", "-u"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "test_cp_arg_update_short_overwrite_file1", "new content\n")
}

# origin: uutils test_cp::test_cp_attributes_only
test test_uu_cp_cp_attributes_only { |ctx|
  let s = scene(ctx)?
  uu.write(s, "file_a", "a")?
  uu.write(s, "file_b", "b")?
  uu.set_mode(s, "file_a", 0o500)?
  uu.set_mode(s, "file_b", 0o777)?
  let mode_a = fs.stat(uu.at(s, "file_a"))?.mode
  let mode_b = fs.stat(uu.at(s, "file_b"))?.mode
  let r1 = uu.invoke(s, "cp", ["--attributes-only", "file_a", "file_b"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  uu.file_is(s, "file_a", "a")
  uu.file_is(s, "file_b", "b")
  assert fs.stat(uu.at(s, "file_a"))?.mode == mode_a
  assert fs.stat(uu.at(s, "file_b"))?.mode == mode_b
}

# origin: uutils test_cp::test_cp_attributes_only_dest_open_error
test test_uu_cp_cp_attributes_only_dest_open_error { |ctx|
  let s = scene(ctx)?
  uu.write(s, "s.txt", "hi")?
  # GNU reports destination stat failure before opening a file under /dev/null.
  let r1 = uu.invoke(s, "cp", ["--attributes-only", "s.txt", "/dev/null/n.txt"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "cp: cannot stat '/dev/null/n.txt'")
}

# origin: uutils test_cp::test_cp_attributes_only_fifo_keeps_type_and_returns
test test_uu_cp_cp_attributes_only_fifo_keeps_type_and_returns { |ctx|
  for recursive in ["-a", "-R"] {
    let s = scene(ctx)?
    uu.mkfifo(s, "fifo")?
    let r1 = uu.invoke(s, "cp", [recursive, "--attributes-only", "fifo", "copy"], timeout: 10s)?
    uu.succeeds(r1)
    uu.no_stderr(r1)
    assert fs.stat(uu.at(s, "copy"))?.kind == "fifo"
  }
}

# origin: uutils test_cp::test_cp_attributes_only_same_file
test test_uu_cp_cp_attributes_only_same_file { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "a")?
  let r1 = uu.invoke(s, "cp", ["--attributes-only", "a", "a"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "'a' and 'a' are the same file")
}

# origin: uutils test_cp::test_cp_attributes_only_same_file_dot_path
test test_uu_cp_cp_attributes_only_same_file_dot_path { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "a")?
  let r1 = uu.invoke(s, "cp", ["--attributes-only", "a", "./a"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "'a' and './a' are the same file")
}

# origin: uutils test_cp::test_cp_backup_existing
test test_uu_cp_cp_backup_existing { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup=existing", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_backup_existing_target_is_fifo
test test_uu_cp_cp_backup_existing_target_is_fifo { |ctx|
  let s = scene(ctx)?
  uu.mkfifo(s, "how_are_you.txt~")?
  let r1 = uu.invoke(s, "cp", ["--backup=simple", "hello_world.txt", "how_are_you.txt"], timeout: 10s)?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  assert fs.stat(uu.at(s, "how_are_you.txt~"))?.kind != "fifo"
  uu.file_is(s, "how_are_you.txt~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_backup_never
test test_uu_cp_cp_backup_never { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup=never", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_backup_nil
test test_uu_cp_cp_backup_nil { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup=nil", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_backup_no_clobber_conflicting_options
test test_uu_cp_cp_backup_no_clobber_conflicting_options { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup", "--no-clobber", "hello_world.txt", "how_are_you.txt"])?
  uu.fails(r1)
  uu.stderr_only(r1, "cp: --backup is mutually exclusive with -n or --update=none-fail\nTry 'cp --help' for more information.\n")
}

# origin: uutils test_cp::test_cp_backup_none
test test_uu_cp_cp_backup_none { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup=none", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  assert !file_exists(s, "how_are_you.txt~")?
}

# origin: uutils test_cp::test_cp_backup_numbered
test test_uu_cp_cp_backup_numbered { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup=numbered", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt.~1~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_backup_numbered_allows_source_named_like_backup
test test_uu_cp_cp_backup_numbered_allows_source_named_like_backup { |ctx|
  let s = scene(ctx)?
  uu.write(s, "hello_world.txt~", "source content")?
  let r1 = uu.invoke(s, "cp", ["--backup=numbered", "hello_world.txt~", "hello_world.txt"])?
  uu.succeeds(r1)
  uu.file_is(s, "hello_world.txt~", "source content")
  uu.file_is(s, "hello_world.txt", "source content")
}

# origin: uutils test_cp::test_cp_backup_numbered_with_t
test test_uu_cp_cp_backup_numbered_with_t { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup=t", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt.~1~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_backup_off
test test_uu_cp_cp_backup_off { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup=off", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  assert !file_exists(s, "how_are_you.txt~")?
}

# origin: uutils test_cp::test_cp_backup_simple
test test_uu_cp_cp_backup_simple { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["--backup=simple", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  uu.file_is(s, "how_are_you.txt~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_backup_simple_allows_hardlink_under_another_name
test test_uu_cp_cp_backup_simple_allows_hardlink_under_another_name { |ctx|
  let s = scene(ctx)?
  uu.write(s, "hello_world.txt~", "backup content")?
  uu.hard_link(s, "hello_world.txt~", "other")?
  let r1 = uu.invoke(s, "cp", ["--backup=simple", "other", "hello_world.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.file_is(s, "hello_world.txt", "backup content")
}

# origin: uutils test_cp::test_cp_backup_simple_protect_source
test test_uu_cp_cp_backup_simple_protect_source { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "hello_world.txt~")?
  let r1 = uu.invoke(s, "cp", ["--backup=simple", "hello_world.txt~", "hello_world.txt"])?
  uu.fails(r1)
  uu.stderr_only(r1, "cp: backing up 'hello_world.txt' might destroy source;  'hello_world.txt~' not copied\n")
  uu.file_is(s, "hello_world.txt", "Hello, World!\n")
  uu.file_is(s, "hello_world.txt~", "")
}

# origin: uutils test_cp::test_cp_backup_simple_protect_source_regardless_of_spelling
test test_uu_cp_cp_backup_simple_protect_source_regardless_of_spelling { |ctx|
  let s = scene(ctx)?
  uu.write(s, "hello_world.txt~", "source content")?
  let r1 = uu.invoke(s, "cp", ["--backup=simple", "./hello_world.txt~", "hello_world.txt"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "might destroy source")
  uu.file_is(s, "hello_world.txt~", "source content")
}

# origin: uutils test_cp::test_cp_block_device_no_permission
test test_uu_cp_cp_block_device_no_permission { |ctx|
  let s = scene(ctx)?
  match fs.mknod(uu.at(s, "sda"), "block", 0o600, major: 8, minor: 0) {
    Err(_) => return,
    Ok(_) => {},
  }
  let r1 = uu.invoke(s, "cp", ["-R", "sda", "sda2"])?
  uu.fails(r1)
  uu.stderr_is(r1, "cp: cannot create special file 'sda2': Operation not permitted\n")
}

# origin: uutils test_cp::test_cp_cannot_create_regular_file
test test_uu_cp_cp_cannot_create_regular_file { |ctx|
  let s = scene(ctx)?
  uu.write(s, "source.txt", "hello")?
  let r1 = uu.invoke(s, "cp", ["source.txt", "/dev/null/n.txt"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "cp: cannot stat '/dev/null/n.txt'")
}

# origin: uutils test_cp::test_cp_cannot_create_regular_file_attributes_only
test test_uu_cp_cp_cannot_create_regular_file_attributes_only { |ctx|
  let s = scene(ctx)?
  uu.write(s, "source.txt", "hello")?
  let r1 = uu.invoke(s, "cp", ["--attributes-only", "source.txt", "/dev/null/n.txt"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "cp: cannot stat '/dev/null/n.txt': Not a directory\n")
}

# origin: uutils test_cp::test_cp_char_device
test test_uu_cp_cp_char_device { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["/dev/null", "null2"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, "null2")?
  uu.file_is(s, "null2", "")
}

# origin: uutils test_cp::test_cp_conflicting_update
test test_uu_cp_cp_conflicting_update { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["-b", "--update=none", "a", "b"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "--backup is mutually exclusive with -n or --update=none-fail")
}

# origin: uutils test_cp::test_cp_copy_symlink_contents_recursive
test test_uu_cp_cp_copy_symlink_contents_recursive { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "src-dir")?
  uu.mkdir(s, "dest-dir")?
  uu.write(s, "f", "f")?
  uu.symlink(s, "f", "slink")?
  uu.symlink(s, "no-file", "src-dir/slink")?
  uu.succeeds(uu.invoke(s, "cp", ["-H", "-R", "slink", "src-dir", "dest-dir"])?)
  assert uu.dir_exists(s, "src-dir")?
  assert uu.dir_exists(s, "dest-dir")?
  assert uu.dir_exists(s, "dest-dir/src-dir")?
  assert !uu.is_symlink(s, "dest-dir/slink")?
  assert file_exists(s, "dest-dir/slink")?
  uu.file_is(s, "dest-dir/slink", "f")
}

# origin: uutils test_cp::test_cp_cp
test test_uu_cp_cp_cp { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "cp", ["hello_world.txt", "copy_of_hello_world.txt"])?
  uu.succeeds(r1)
  uu.file_is(s, "copy_of_hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_current_directory_preserve_attributes
test test_uu_cp_cp_current_directory_preserve_attributes { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "source_dir/file1.txt")?
  uu.touch(s, "source_dir/file2.txt")?
  uu.set_mode(s, "source_dir/file1.txt", 0o644)?
  uu.set_mode(s, "source_dir/file2.txt", 0o755)?
  let previous = (time.now() - 3600000) * 1000000
  for name in ["file1.txt", "file2.txt"] {
    fs.set_times(uu.at(s, f"source_dir/{name}"), atime_ns: previous, mtime_ns: previous)?
  }
  uu.mkdir(s, "dest_dir")?
  let cwd = {ctx: ctx, root: uu.at(s, "source_dir")}
  uu.succeeds(uu.invoke(cwd, "cp", ["-rp", ".", "../dest_dir"])?)
  for name in ["file1.txt", "file2.txt"] {
    assert file_exists(s, f"dest_dir/{name}")?
    let src = fs.stat(uu.at(s, f"source_dir/{name}"))?
    let dst = fs.stat(uu.at(s, f"dest_dir/{name}"))?
    assert src.mode % 4096 == dst.mode % 4096
    assert src.mtime_ns == dst.mtime_ns
  }
}

# origin: uutils test_cp::test_cp_current_directory_to_existing_directory
test test_uu_cp_cp_current_directory_to_existing_directory { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "source_dir/file1.txt")?
  uu.touch(s, "source_dir/file2.txt")?
  uu.mkdir(s, "source_dir/subdir")?
  uu.touch(s, "source_dir/subdir/file3.txt")?
  uu.mkdir(s, "dest_dir")?
  let cwd = {ctx: ctx, root: uu.at(s, "source_dir")}
  let r1 = uu.invoke(cwd, "cp", ["-r", ".", "../dest_dir"])?
  uu.succeeds(r1)
  assert file_exists(s, "dest_dir/file1.txt")?
  assert file_exists(s, "dest_dir/file2.txt")?
  assert uu.dir_exists(s, "dest_dir/subdir")?
  assert file_exists(s, "dest_dir/subdir/file3.txt")?
  assert !file_exists(s, "dest_dir/source_dir/file1.txt")?
}

# origin: uutils test_cp::test_cp_current_directory_to_itself_disallowed
test test_uu_cp_cp_current_directory_to_itself_disallowed { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "test_dir")?
  uu.touch(s, "test_dir/file1.txt")?
  let cwd = {ctx: ctx, root: uu.at(s, "test_dir")}
  let r1 = uu.invoke(cwd, "cp", ["-r", ".", "."])?
  uu.fails(r1)
  uu.stderr_contains(r1, "'.' and './.' are the same file")
}

# origin: uutils test_cp::test_cp_current_directory_to_new_directory
test test_uu_cp_cp_current_directory_to_new_directory { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "source_dir/file1.txt")?
  uu.touch(s, "source_dir/file2.txt")?
  uu.mkdir(s, "source_dir/subdir")?
  uu.touch(s, "source_dir/subdir/file3.txt")?
  let cwd = {ctx: ctx, root: uu.at(s, "source_dir")}
  let r1 = uu.invoke(cwd, "cp", ["-r", ".", "../new_dest_dir"])?
  uu.succeeds(r1)
  assert uu.dir_exists(s, "new_dest_dir")?
  assert file_exists(s, "new_dest_dir/file1.txt")?
  assert file_exists(s, "new_dest_dir/file2.txt")?
  assert uu.dir_exists(s, "new_dest_dir/subdir")?
  assert file_exists(s, "new_dest_dir/subdir/file3.txt")?
}

# origin: uutils test_cp::test_cp_current_directory_verbose
test test_uu_cp_cp_current_directory_verbose { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "source_dir/file1.txt")?
  uu.touch(s, "source_dir/file2.txt")?
  uu.mkdir(s, "dest_dir")?
  let cwd = {ctx: ctx, root: uu.at(s, "source_dir")}
  let r1 = uu.invoke(cwd, "cp", ["-rv", ".", "../dest_dir"])?
  uu.succeeds(r1)
  assert file_exists(s, "dest_dir/file1.txt")?
  assert file_exists(s, "dest_dir/file2.txt")?
  uu.stdout_contains(r1, "file1.txt")
  uu.stdout_contains(r1, "file2.txt")
  uu.stdout_contains(r1, "dest_dir")
}

# origin: uutils test_cp::test_cp_current_directory_with_entry_matching_parent_basename
test test_uu_cp_cp_current_directory_with_entry_matching_parent_basename { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "demo")?
  uu.touch(s, "demo/file")?
  uu.touch(s, "demo/demo")?
  let cwd = {ctx: ctx, root: uu.at(s, "demo")}
  uu.succeeds(uu.invoke(cwd, "cp", ["-R", ".", "../out"])?)
  assert file_exists(s, "out/file")?
  assert file_exists(s, "out/demo")?
  uu.mkdir(s, "existing_out")?
  uu.succeeds(uu.invoke(cwd, "cp", ["-R", ".", "../existing_out"])?)
  assert file_exists(s, "existing_out/file")?
  assert file_exists(s, "existing_out/demo")?
}

# origin: uutils test_cp::test_cp_current_directory_with_symlinks
test test_uu_cp_cp_current_directory_with_symlinks { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "source_dir/file1.txt")?
  uu.symlink(s, uu.at(s, "file1.txt").display(), "source_dir/link1.txt")?
  uu.mkdir(s, "source_dir/subdir")?
  uu.touch(s, "source_dir/subdir/file2.txt")?
  uu.symlink(s, uu.at(s, "../file1.txt").display(), "source_dir/subdir/link2.txt")?
  uu.mkdir(s, "dest_dir")?
  let cwd = {ctx: ctx, root: uu.at(s, "source_dir")}
  uu.succeeds(uu.invoke(cwd, "cp", ["-r", ".", "../dest_dir"])?)
  assert file_exists(s, "dest_dir/file1.txt")?
  assert uu.is_symlink(s, "dest_dir/link1.txt")?
  assert uu.dir_exists(s, "dest_dir/subdir")?
  assert file_exists(s, "dest_dir/subdir/file2.txt")?
  assert uu.is_symlink(s, "dest_dir/subdir/link2.txt")?
}

