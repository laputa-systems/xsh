##! Native ports of the uutils cp integration tests.

use support.uu as uu

proc fixtures(s: uu.Scene) [fs, error] -> Result[Unit, Error] {
  for name in ["hello_world.txt", "how_are_you.txt", "existing_file.txt"] {
    uu.fixture(s, "cp", name, name)?
  }
  uu.mkdir(s, "hello_dir")?
  uu.mkdir(s, "hello_dir_with_file")?
  uu.fixture(s, "cp", "hello_dir/hello.txt", "hello_dir/hello.txt")?
  uu.fixture(s, "cp", "hello_dir_with_file/hello_world.txt", "hello_dir_with_file/hello_world.txt")?
  Ok()
}

# origin: uutils test_cp::test_cp_no_preserve_timestamps
test test_uu_cp_cp_no_preserve_timestamps { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let previous = fs.stat(uu.at(s, "hello_world.txt"))?.mtime_ns - 3600000000000
  fs.set_times(uu.at(s, "hello_world.txt"), atime_ns: previous, mtime_ns: previous)
  time.sleep(100ms)
  let r = uu.invoke(s, "cp", ["hello_world.txt", "--no-preserve=timestamps", "how_are_you.txt"])?
  uu.succeeds(r)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  let source = fs.stat(uu.at(s, "hello_world.txt"))?.mtime_ns
  let destination = fs.stat(uu.at(s, "how_are_you.txt"))?.mtime_ns
  assert source != destination
  let difference = (destination - source) / 1000000000
  assert difference > 3595
  assert difference < 3605
}

# origin: uutils test_cp::test_cp_no_such
test test_uu_cp_cp_no_such { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cp", ["b", "no-such/"])?
  uu.fails(r)
  uu.stderr_is(r, "cp: cannot create regular file 'no-such/': Not a directory\n")
}

# origin: uutils test_cp::test_cp_numbered_if_existing_backup_existing
test test_uu_cp_cp_numbered_if_existing_backup_existing { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  uu.touch(s, "how_are_you.txt.~1~")?
  let r = uu.invoke(s, "cp", ["--backup=existing", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "how_are_you.txt")?
  assert uu.file_exists(s, "how_are_you.txt.~1~")?
  uu.file_is(s, "how_are_you.txt.~2~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_numbered_if_existing_backup_nil
test test_uu_cp_cp_numbered_if_existing_backup_nil { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  uu.touch(s, "how_are_you.txt.~1~")?
  let r = uu.invoke(s, "cp", ["--backup=nil", "hello_world.txt", "how_are_you.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "how_are_you.txt")?
  assert uu.file_exists(s, "how_are_you.txt.~1~")?
  uu.file_is(s, "how_are_you.txt.~2~", "How are you?\n")
}

# origin: uutils test_cp::test_cp_only_source_no_target
test test_uu_cp_cp_only_source_no_target { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["a"])?
  uu.fails(r)
  uu.stderr_contains(r, "missing destination file operand after 'a'")
}

# origin: uutils test_cp::test_cp_overriding_arguments
test test_uu_cp_cp_overriding_arguments { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file1")?
  for pair in [["--remove-destination", "--force"], ["--force", "--remove-destination"], ["--interactive", "--no-clobber"], ["--link", "--symbolic-link"], ["--symbolic-link", "--link"], ["--dereference", "--no-dereference"], ["--no-dereference", "--dereference"]] {
    let r = uu.invoke(s, "cp", pair.extend(["file1", "file2"]))?
    if "--link" in pair and "--symbolic-link" in pair {
      uu.fails_with_code(r, 1)
      uu.stderr_is(r, "cp: cannot make both hard and symbolic links\nTry 'cp --help' for more information.\n")
    } else {
      uu.succeeds(r)
      uu.remove(s, "file2")?
    }
  }
}

# origin: uutils test_cp::test_cp_p_does_not_preserve_xattr_by_default
test test_uu_cp_cp_p_does_not_preserve_xattr_by_default { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  fs.xattr_set(uu.at(s, "src"), "user.test_preserve_p", b"v")
  let r = uu.invoke(s, "cp", ["-p", uu.at(s, "src").display(), uu.at(s, "dst").display()])?
  uu.succeeds(r)
  assert fs.xattr_get(uu.at(s, "dst"), "user.test_preserve_p") is Err(_)
}

# origin: uutils test_cp::test_cp_p_preserves_posix_acls
test test_uu_cp_cp_p_preserves_posix_acls { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  # Linux POSIX ACL encoding: owner, named user bin, group, mask, other.
  let acl = bytes.concat([
    b"\x02\x00\x00\x00\x01\x00\x06\x00\xff\xff\xff\xff\x02\x00\x06\x00",
    bytes.pack_le(user.lookup("bin")?.uid, 4)?,
    b"\x04\x00\x04\x00\xff\xff\xff\xff\x10\x00\x06\x00\xff\xff\xff\xff\x20\x00\x04\x00\xff\xff\xff\xff",
  ])
  fs.xattr_set(uu.at(s, "src"), "system.posix_acl_access", acl)
  let r = uu.invoke(s, "cp", ["-p", "src", "dst"])?
  uu.succeeds(r)
  assert fs.xattr_get(uu.at(s, "src"), "system.posix_acl_access")? == fs.xattr_get(uu.at(s, "dst"), "system.posix_acl_access")?
}

# origin: uutils test_cp::test_cp_parents
test test_uu_cp_cp_parents { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["--parents", "hello_dir_with_file/hello_world.txt", "hello_dir/"])?
  uu.succeeds(r)
  uu.file_is(s, "hello_dir/hello_dir_with_file/hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_parents_2
test test_uu_cp_cp_parents_2 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.touch(s, "a/b/c")?
  uu.mkdir(s, "d")?
  let r = uu.invoke(s, "cp", ["--verbose", "--parents", "a/b/c", "d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a -> d/a\na/b -> d/a/b\n'a/b/c' -> 'd/a/b/c'\n")
  assert uu.file_exists(s, "d/a/b/c")?
}

# origin: uutils test_cp::test_cp_parents_2_deep_dir
test test_uu_cp_cp_parents_2_deep_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b/c")?
  uu.mkdir(s, "d/e")?
  let r = uu.invoke(s, "cp", ["--verbose", "-r", "--parents", "a/b/c", "d/e"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a -> d/e/a\na/b -> d/e/a/b\n'a/b/c' -> 'd/e/a/b/c'\n")
  assert uu.dir_exists(s, "d/e/a/b/c")?
}

# origin: uutils test_cp::test_cp_parents_2_dir
test test_uu_cp_cp_parents_2_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b/c")?
  uu.mkdir(s, "d")?
  let r = uu.invoke(s, "cp", ["--verbose", "-r", "--parents", "a/b/c", "d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a -> d/a\na/b -> d/a/b\n'a/b/c' -> 'd/a/b/c'\n")
  assert uu.dir_exists(s, "d/a/b/c")?
}

# origin: uutils test_cp::test_cp_parents_2_dirs
test test_uu_cp_cp_parents_2_dirs { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b/c")?
  uu.mkdir(s, "d")?
  let r = uu.invoke(s, "cp", ["-a", "--parents", "a/b/c", "d"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.dir_exists(s, "d/a/b/c")?
}

# origin: uutils test_cp::test_cp_parents_2_link
test test_uu_cp_cp_parents_2_link { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.touch(s, "a/b/c")?
  uu.mkdir(s, "d")?
  uu.symlink(s, "b", "a/link")?
  let r = uu.invoke(s, "cp", ["--verbose", "--parents", "a/link/c", "d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a -> d/a\na/link -> d/a/link\n'a/link/c' -> 'd/a/link/c'\n")
  assert uu.file_exists(s, "d/a/link/c")?
  assert uu.dir_exists(s, "d/a/link")?
  assert !uu.is_symlink(s, "d/a/link")?
}

# origin: uutils test_cp::test_cp_parents_absolute_path
test test_uu_cp_cp_parents_absolute_path { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.touch(s, "a/b/f")?
  uu.mkdir(s, "dest")?
  let r = uu.invoke(s, "cp", ["--parents", uu.at(s, "a/b/f").display(), "dest"])?
  uu.succeeds(r)
  assert uu.file_exists(s, f"dest{s.root.display()}/a/b/f")?
}

# origin: uutils test_cp::test_cp_parents_dest_not_directory
test test_uu_cp_cp_parents_dest_not_directory { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["--parents", "hello_dir_with_file/hello_world.txt", "copy_of_hello_world.txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "with --parents, the destination must be a directory")
}

# origin: uutils test_cp::test_cp_parents_multiple_files
test test_uu_cp_cp_parents_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["--parents", "hello_dir_with_file/hello_world.txt", "how_are_you.txt", "hello_dir/"])?
  uu.succeeds(r)
  uu.file_is(s, "hello_dir/hello_dir_with_file/hello_world.txt", "Hello, World!\n")
  uu.file_is(s, "hello_dir/how_are_you.txt", "How are you?\n")
}

# origin: uutils test_cp::test_cp_parents_recursive_source_ending_in_parent_dir
test test_uu_cp_cp_parents_recursive_source_ending_in_parent_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "src/sub")?
  uu.write(s, "src/sub/f", "x\n")?
  uu.mkdir(s, "d")?
  let r = uu.invoke(s, "cp", ["--parents", "-r", "src/sub/..", "d"])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot create directory 'd/src/sub/..'")
  uu.stderr_contains(r, "File exists")
  assert uu.dir_exists(s, "d/src/sub")?
  assert !uu.exists(s, "d/src/sub/f")?
}

# origin: uutils test_cp::test_cp_parents_symlink_permissions_dir
test test_uu_cp_cp_parents_symlink_permissions_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.set_mode(s, "a", 0o755)?
  uu.symlink(s, "a", "symlink")?
  uu.mkdir(s, "dest")?
  let r = uu.invoke(s, "cp", ["--parents", "-a", "symlink/b", "dest"])?
  uu.succeeds(r)
  assert uu.mode(s, "a")? == uu.mode(s, "dest/symlink")?
}

# origin: uutils test_cp::test_cp_parents_symlink_permissions_file
test test_uu_cp_cp_parents_symlink_permissions_file { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.touch(s, "a/file")?
  uu.set_mode(s, "a", 0o700)?
  uu.symlink(s, "a", "symlink")?
  uu.mkdir(s, "dest")?
  let r = uu.invoke(s, "cp", ["--parents", "-a", "symlink/file", "dest"])?
  uu.succeeds(r)
  assert uu.mode(s, "a")? == uu.mode(s, "dest/symlink")?
}

# origin: uutils test_cp::test_cp_path_ends_with_terminator
test test_uu_cp_cp_path_ends_with_terminator { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  let r = uu.invoke(s, "cp", ["-r", "-T", "a", "e/"])?
  uu.succeeds(r)
}

# origin: uutils test_cp::test_cp_preserve_all_context_fails_on_non_selinux
test test_uu_cp_cp_preserve_all_context_fails_on_non_selinux { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["hello_dir_with_file/hello_world.txt", "copy_of_hello_world.txt", "--preserve=all,context"])?
  uu.fails(r)
}

# origin: uutils test_cp::test_cp_preserve_directory_permissions_by_default
test test_uu_cp_cp_preserve_directory_permissions_by_default { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b/c/d")?
  uu.touch(s, "a/b/c/d/foo.txt")?
  for name in ["a", "a/b", "a/b/c", "a/b/c/d", "a/b/c/d/foo.txt"] { uu.set_mode(s, name, 0o555)? }
  uu.succeeds(uu.invoke(s, "cp", ["-r", "a", "b"])?)
  uu.succeeds(uu.invoke(s, "cp", ["-r", "a", "c"])?)
  for name in ["b", "b/b", "b/b/c", "b/b/c/d", "c", "c/b", "c/b/c", "c/b/c/d"] { assert fs.stat(uu.at(s, name))?.mode == 0o40555 }
}

# origin: uutils test_cp::test_cp_preserve_invalid_rejected
test test_uu_cp_cp_preserve_invalid_rejected { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["--preserve=invalid-value", "hello_dir_with_file/hello_world.txt", "copy_of_hello_world.txt"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
}

# origin: uutils test_cp::test_cp_preserve_link_parses
test test_uu_cp_cp_preserve_link_parses { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  for argument in ["--preserve=links", "--preserve=link", "--preserve=li", "--preserve=l"] {
    let r = uu.invoke(s, "cp", [argument, "hello_dir_with_file/hello_world.txt", "copy_of_hello_world.txt"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_cp::test_cp_preserve_links_case_1
test test_uu_cp_cp_preserve_links_case_1 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.hard_link(s, "a", "b")?
  uu.mkdir(s, "c")?
  let r = uu.invoke(s, "cp", ["-d", "a", "b", "c"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "c")?
  assert uu.exists(s, "c/a")?
  assert uu.exists(s, "c/b")?
  assert fs.stat(uu.at(s, "c/a"))?.ino == fs.stat(uu.at(s, "c/b"))?.ino
}

# origin: uutils test_cp::test_cp_preserve_links_case_2
test test_uu_cp_cp_preserve_links_case_2 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.symlink(s, "a", "b")?
  uu.mkdir(s, "c")?
  let r = uu.invoke(s, "cp", ["--preserve=links", "-R", "-H", "a", "b", "c"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "c")?
  assert uu.exists(s, "c/a")?
  assert uu.exists(s, "c/b")?
  assert fs.stat(uu.at(s, "c/a"))?.ino == fs.stat(uu.at(s, "c/b"))?.ino
}

# origin: uutils test_cp::test_cp_preserve_links_case_3
test test_uu_cp_cp_preserve_links_case_3 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "d/a")?
  uu.symlink(s, uu.at(s, "d/a").display(), "d/b")?
  let r = uu.invoke(s, "cp", ["--preserve=links", "-R", "-L", "d", "c"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "c")?
  assert uu.exists(s, "c/a")?
  assert uu.exists(s, "c/b")?
  assert fs.stat(uu.at(s, "c/a"))?.ino == fs.stat(uu.at(s, "c/b"))?.ino
}

# origin: uutils test_cp::test_cp_preserve_links_case_4
test test_uu_cp_cp_preserve_links_case_4 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "d/a")?
  uu.hard_link(s, "d/a", "d/b")?
  let r = uu.invoke(s, "cp", ["--preserve=links", "-R", "-L", "d", "c"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "c")?
  assert uu.exists(s, "c/a")?
  assert uu.exists(s, "c/b")?
  assert fs.stat(uu.at(s, "c/a"))?.ino == fs.stat(uu.at(s, "c/b"))?.ino
}

# origin: uutils test_cp::test_cp_preserve_links_case_5
test test_uu_cp_cp_preserve_links_case_5 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "d/a")?
  uu.hard_link(s, "d/a", "d/b")?
  let r = uu.invoke(s, "cp", ["-dR", "--no-preserve=links", "d", "c"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "c")?
  assert uu.exists(s, "c/a")?
  assert uu.exists(s, "c/b")?
  assert fs.stat(uu.at(s, "c/a"))?.ino != fs.stat(uu.at(s, "c/b"))?.ino
}

# origin: uutils test_cp::test_cp_preserve_links_case_6
test test_uu_cp_cp_preserve_links_case_6 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.hard_link(s, "a", "b")?
  uu.mkdir(s, "c")?
  let r = uu.invoke(s, "cp", ["-d", "a", "b", "c"])?
  uu.succeeds(r)
  assert uu.dir_exists(s, "c")?
  assert uu.exists(s, "c/a")?
  assert uu.exists(s, "c/b")?
  assert fs.stat(uu.at(s, "c/a"))?.ino == fs.stat(uu.at(s, "c/b"))?.ino
}

# origin: uutils test_cp::test_cp_preserve_links_case_7
test test_uu_cp_cp_preserve_links_case_7 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "src")?
  uu.touch(s, "src/f")?
  uu.hard_link(s, "src/f", "src/g")?
  uu.mkdir(s, "dest")?
  uu.touch(s, "dest/g")?
  let r = uu.invoke(s, "cp", ["-n", "--preserve=links", "--debug", "src/f", "src/g", "dest"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "skipped")
  assert uu.dir_exists(s, "dest")?
  assert uu.exists(s, "dest/f")?
  assert uu.exists(s, "dest/g")?
}

# origin: uutils test_cp::test_cp_preserve_setuid_when_chown_succeeds
test test_uu_cp_cp_preserve_setuid_when_chown_succeeds { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  uu.set_mode(s, "src", 0o4755)?
  let r = uu.invoke(s, "cp", ["-p", "src", "dst"])?
  uu.succeeds(r)
  assert uu.mode(s, "dst")?.bit_and(0o4000) == 0o4000
}

# origin: uutils test_cp::test_cp_preserve_timestamps
test test_uu_cp_cp_preserve_timestamps { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let previous = fs.stat(uu.at(s, "hello_world.txt"))?.mtime_ns - 3600000000000
  fs.set_times(uu.at(s, "hello_world.txt"), atime_ns: previous, mtime_ns: previous)
  let r = uu.invoke(s, "cp", ["hello_world.txt", "--preserve=timestamps", "how_are_you.txt"])?
  uu.succeeds(r)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  let source = fs.stat(uu.at(s, "hello_world.txt"))?.mtime_ns
  let destination = fs.stat(uu.at(s, "how_are_you.txt"))?.mtime_ns
  assert source == destination
}

# origin: uutils test_cp::test_cp_preserve_xattr
test test_uu_cp_cp_preserve_xattr { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.set_mode(s, "a", 0o500)?
  time.sleep(1s)
  let r = uu.invoke(s, "cp", ["a", "b", "--preserve=xattr"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "a"))?.mtime_ns / 1000000000 != fs.stat(uu.at(s, "b"))?.mtime_ns / 1000000000
}

# origin: uutils test_cp::test_cp_preserve_xattr_readonly_source
test test_uu_cp_cp_preserve_xattr_readonly_source { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  fs.xattr_set(uu.at(s, "a"), "user.test", b"value")
  assert "user.test" in fs.xattr_list(uu.at(s, "a"))?
  uu.set_mode(s, "a", uu.mode(s, "a")?.clear_bits(0o222))?
  assert uu.mode(s, "a")?.bit_and(0o222) == 0
  let r = uu.invoke(s, "cp", ["--preserve=xattr", uu.at(s, "a").display(), uu.at(s, "e").display()])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.mode(s, "e")?.bit_and(0o222) == 0
  assert fs.xattr_list(uu.at(s, "a"))? == fs.xattr_list(uu.at(s, "e"))?
  for key in fs.xattr_list(uu.at(s, "a"))? { assert fs.xattr_get(uu.at(s, "a"), key)? == fs.xattr_get(uu.at(s, "e"), key)? }
}

# origin: uutils test_cp::test_cp_r_symlink
test test_uu_cp_cp_r_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tmp")?
  uu.symlink(s, "doesnotexist", "tmp/symlink")?
  let source_gid = fs.stat(uu.at(s, "tmp/symlink"))?.gid
  var other_gid: Int? = null
  for gid in unix.id()?.supplementary {
    if gid != source_gid { other_gid = gid; break }
  }
  if let gid = other_gid { fs.set_owner(uu.at(s, "tmp/symlink"), gid: gid) }
  let r = uu.invoke(s, "cp", ["--preserve", "-r", "tmp", "tmp2"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.is_symlink(s, "tmp2/symlink")?
  assert uu.read_link(s, "tmp/symlink")? == uu.read_link(s, "tmp2/symlink")?
  assert fs.stat(uu.at(s, "tmp/symlink"))?.gid == fs.stat(uu.at(s, "tmp2/symlink"))?.gid
}

# origin: uutils test_cp::test_cp_readonly_dest_recursive
test test_uu_cp_cp_readonly_dest_recursive { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.mkdir(s, "dest_dir")?
  uu.write(s, "source_dir/file.txt", "source content")?
  uu.write(s, "dest_dir/file.txt", "original content")?
  uu.set_mode(s, "dest_dir/file.txt", uu.mode(s, "dest_dir/file.txt")?.clear_bits(0o222))?
  let r = uu.invoke(s, "cp", ["-r", "source_dir", "dest_dir"])?
  uu.succeeds(r)
  uu.file_is(s, "dest_dir/file.txt", "original content")
}

# origin: uutils test_cp::test_cp_readonly_dest_with_existing_file
test test_uu_cp_cp_readonly_dest_with_existing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "source.txt", "source content")?
  uu.write(s, "readonly_dest.txt", "original content")?
  uu.write(s, "other_file.txt", "other content")?
  uu.set_mode(s, "readonly_dest.txt", uu.mode(s, "readonly_dest.txt")?.clear_bits(0o222))?
  let r = uu.invoke(s, "cp", ["source.txt", "readonly_dest.txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "readonly_dest.txt")
  uu.stderr_contains(r, "denied")
  uu.file_is(s, "readonly_dest.txt", "original content")
  uu.file_is(s, "other_file.txt", "other content")
}

# origin: uutils test_cp::test_cp_readonly_dest_with_reflink
test test_uu_cp_cp_readonly_dest_with_reflink { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "source.txt", "source content")?
  for kind in ["auto", "always"] {
    let dest = f"readonly_dest_{kind}.txt"
    uu.write(s, dest, "original content")?
    uu.set_mode(s, dest, uu.mode(s, dest)?.clear_bits(0o222))?
    let r = uu.invoke(s, "cp", [f"--reflink={kind}", "source.txt", dest])?
    uu.fails(r)
    uu.stderr_contains(r, dest)
    uu.file_is(s, dest, "original content")
  }
}

# origin: uutils test_cp::test_cp_readonly_source
test test_uu_cp_cp_readonly_source { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "readonly_source.txt", "source content")?
  uu.write(s, "dest.txt", "dest content")?
  uu.set_mode(s, "readonly_source.txt", uu.mode(s, "readonly_source.txt")?.clear_bits(0o222))?
  let r = uu.invoke(s, "cp", ["readonly_source.txt", "dest.txt"])?
  uu.succeeds(r)
  uu.file_is(s, "dest.txt", "source content")
}

# origin: uutils test_cp::test_cp_readonly_source_and_dest
test test_uu_cp_cp_readonly_source_and_dest { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "readonly_source.txt", "source content")?
  uu.write(s, "readonly_dest.txt", "original content")?
  uu.set_mode(s, "readonly_source.txt", uu.mode(s, "readonly_source.txt")?.clear_bits(0o222))?
  uu.set_mode(s, "readonly_dest.txt", uu.mode(s, "readonly_dest.txt")?.clear_bits(0o222))?
  let r = uu.invoke(s, "cp", ["readonly_source.txt", "readonly_dest.txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "readonly_dest.txt")
  uu.stderr_contains(r, "denied")
  uu.file_is(s, "readonly_dest.txt", "original content")
}

# origin: uutils test_cp::test_cp_recurse
test test_uu_cp_cp_recurse { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["-r", "hello_dir_with_file/", "hello_dir_new"])?
  uu.succeeds(r)
  uu.file_is(s, "hello_dir_new/hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_recurse_several
test test_uu_cp_cp_recurse_several { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["-r", "-r", "hello_dir_with_file/", "hello_dir_new"])?
  uu.succeeds(r)
  uu.file_is(s, "hello_dir_new/hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_recurse_source_path_ends_with_slash_dot
test test_uu_cp_cp_recurse_source_path_ends_with_slash_dot { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "source_dir/file")?
  let r = uu.invoke(s, "cp", ["-r", "source_dir/.", "target_dir"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "target_dir/file")?
}

# origin: uutils test_cp::test_cp_recurse_verbose_output
test test_uu_cp_cp_recurse_verbose_output { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "source_dir/file")?
  let r = uu.invoke(s, "cp", ["source_dir", "target_dir", "-r", "--verbose"])?
  uu.succeeds(r)
  uu.stdout_only(r, "'source_dir' -> 'target_dir'\n'source_dir/file' -> 'target_dir/file'\n")
}

# origin: uutils test_cp::test_cp_recurse_verbose_output_with_symlink
test test_uu_cp_cp_recurse_verbose_output_with_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "file")?
  uu.symlink(s, uu.at(s, "file").display(), "source_dir/symlink")?
  let r = uu.invoke(s, "cp", ["source_dir", "target_dir", "-r", "--verbose"])?
  uu.succeeds(r)
  uu.stdout_only(r, "'source_dir' -> 'target_dir'\n'source_dir/symlink' -> 'target_dir/symlink'\n")
}

# origin: uutils test_cp::test_cp_recurse_verbose_output_with_symlink_already_exists
test test_uu_cp_cp_recurse_verbose_output_with_symlink_already_exists { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "file")?
  uu.symlink(s, uu.at(s, "file").display(), "source_dir/symlink")?
  uu.mkdir(s, "target_dir")?
  uu.symlink(s, uu.at(s, "file").display(), "target_dir/symlink")?
  let r = uu.invoke(s, "cp", ["source_dir", "target_dir", "-r", "--verbose", "-T"])?
  uu.succeeds(r)
  uu.stdout_only(r, "removed 'target_dir/symlink'\n'source_dir/symlink' -> 'target_dir/symlink'\n")
}

# origin: uutils test_cp::test_cp_recursive_char_device_copy_contents
test test_uu_cp_cp_recursive_char_device_copy_contents { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["-R", "--copy-contents", "/dev/null", "null2"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "null2")?
  uu.file_is(s, "null2", "")
}

# origin: uutils test_cp::test_cp_recursive_char_device_no_permission::case_1_recursive
test test_uu_cp_test_cp_recursive_char_device_no_permission_case_1_recursive { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["-R", "/dev/null", "null2"])?
  uu.fails(r)
  uu.stderr_is(r, "cp: cannot create special file 'null2': Operation not permitted\n")
}

# origin: uutils test_cp::test_cp_recursive_char_device_no_permission::case_2_archive
test test_uu_cp_test_cp_recursive_char_device_no_permission_case_2_archive { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["-a", "/dev/null", "null2"])?
  uu.fails(r)
  uu.stderr_is(r, "cp: cannot create special file 'null2': Operation not permitted\n")
}

# origin: uutils test_cp::test_cp_recursive_continues_after_skipped_file::case_1_no_clobber
test test_uu_cp_test_cp_recursive_continues_after_skipped_file_case_1_no_clobber { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source")?
  uu.mkdir(s, "destination/source")?
  uu.write(s, "source/first", "first contents")?
  uu.write(s, "source/second", "second contents")?
  let order = fs.children(uu.at(s, "source"), ordered: false)?.collect()
  let skipped = order[0].name
  let copied = order[1].name
  uu.write(s, f"destination/source/{skipped}", "old contents")?
  let r = uu.invoke(s, "cp", ["-R", "-n", "source", "destination"])?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, f"destination/source/{skipped}", "old contents")
  assert uu.read(s, f"destination/source/{copied}")? == uu.read(s, f"source/{copied}")?
}

# origin: uutils test_cp::test_cp_recursive_continues_after_skipped_file::case_2_update_none
test test_uu_cp_test_cp_recursive_continues_after_skipped_file_case_2_update_none { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source")?
  uu.mkdir(s, "destination/source")?
  uu.write(s, "source/first", "first contents")?
  uu.write(s, "source/second", "second contents")?
  let order = fs.children(uu.at(s, "source"), ordered: false)?.collect()
  let skipped = order[0].name
  let copied = order[1].name
  uu.write(s, f"destination/source/{skipped}", "old contents")?
  let r = uu.invoke(s, "cp", ["-R", "--update=none", "source", "destination"])?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, f"destination/source/{skipped}", "old contents")
  assert uu.read(s, f"destination/source/{copied}")? == uu.read(s, f"source/{copied}")?
}

# origin: uutils test_cp::test_cp_recursive_continues_after_skipped_file::case_3_archive_no_clobber
test test_uu_cp_test_cp_recursive_continues_after_skipped_file_case_3_archive_no_clobber { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source")?
  uu.mkdir(s, "destination/source")?
  uu.write(s, "source/first", "first contents")?
  uu.write(s, "source/second", "second contents")?
  let order = fs.children(uu.at(s, "source"), ordered: false)?.collect()
  let skipped = order[0].name
  let copied = order[1].name
  uu.write(s, f"destination/source/{skipped}", "old contents")?
  let r = uu.invoke(s, "cp", ["-R", "-an", "source", "destination"])?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, f"destination/source/{skipped}", "old contents")
  assert uu.read(s, f"destination/source/{copied}")? == uu.read(s, f"source/{copied}")?
}

# origin: uutils test_cp::test_cp_recursive_continues_after_skipped_file::case_4_declined_prompt
test test_uu_cp_test_cp_recursive_continues_after_skipped_file_case_4_declined_prompt { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source")?
  uu.mkdir(s, "destination/source")?
  uu.write(s, "source/first", "first contents")?
  uu.write(s, "source/second", "second contents")?
  let order = fs.children(uu.at(s, "source"), ordered: false)?.collect()
  let skipped = order[0].name
  let copied = order[1].name
  uu.write(s, f"destination/source/{skipped}", "old contents")?
  let r = uu.invoke(s, "cp", ["-R", "-i", "source", "destination"], stdin: b"n\n")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, f"cp: overwrite 'destination/source/{skipped}'? ")
  uu.file_is(s, f"destination/source/{skipped}", "old contents")
  assert uu.read(s, f"destination/source/{copied}")? == uu.read(s, f"source/{copied}")?
}

# origin: uutils test_cp::test_cp_recursive_dest_subdir_symlink_not_followed
test test_uu_cp_cp_recursive_dest_subdir_symlink_not_followed { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "src/hooks")?
  uu.write(s, "src/hooks/payload", "PAYLOAD")?
  uu.mkdir(s, "dst")?
  uu.mkdir(s, "outside")?
  uu.symlink(s, "../outside", "dst/hooks")?
  let r = uu.invoke(s, "cp", ["-a", "src/.", "dst"])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot overwrite non-directory")
  assert !uu.exists(s, "outside/payload")?
}

# origin: uutils test_cp::test_cp_recursive_dir_applies_umask
test test_uu_cp_cp_recursive_dir_applies_umask { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "src/dir")?
  uu.set_mode(s, "src/dir", 0o777)?
  let r = uu.invoke(s, "cp", ["-r", "src", "d"], umask: 0o077)?
  uu.succeeds(r)
  assert uu.mode(s, "d/dir")?.bit_and(0o777) == 0o700
}

# origin: uutils test_cp::test_cp_recursive_dir_drops_setuid_setgid
test test_uu_cp_cp_recursive_dir_drops_setuid_setgid { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tree")?
  for entry in [{name: "tree/setgid", mode: 0o2731}, {name: "tree/setuid", mode: 0o4713}, {name: "tree/sticky", mode: 0o1735}] {
    uu.mkdir(s, entry.name)?
    uu.set_mode(s, entry.name, entry.mode)?
  }
  let r = uu.invoke(s, "cp", ["-r", "tree", "plain"], umask: 0o026)?
  uu.succeeds(r)
  assert uu.mode(s, "plain/setgid")? == 0o711
  assert uu.mode(s, "plain/setuid")? == 0o711
  assert uu.mode(s, "plain/sticky")? == 0o1711
  uu.succeeds(uu.invoke(s, "cp", ["-r", "--preserve=mode", "tree", "kept"], umask: 0o026)?)
  assert uu.mode(s, "kept/setgid")? == 0o2731
  assert uu.mode(s, "kept/setuid")? == 0o4713
}

# origin: uutils test_cp::test_cp_recursive_files_ending_in_backslash
test test_uu_cp_cp_recursive_files_ending_in_backslash { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.touch(s, "a/foo\\")?
  let r = uu.invoke(s, "cp", ["-r", "a", "b"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "b/foo\\")?
}

# origin: uutils test_cp::test_cp_recursive_no_dereference_symlink_to_directory
test test_uu_cp_cp_recursive_no_dereference_symlink_to_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source_dir")?
  uu.touch(s, "source_dir/file.txt")?
  uu.symlink(s, "source_dir", "symlink_to_dir")?
  let r = uu.invoke(s, "cp", ["-r", "--no-dereference", "symlink_to_dir", "dest"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "dest")?
  assert uu.read_link(s, "dest")? == "source_dir"
}

# origin: uutils test_cp::test_cp_recursive_non_utf8_source
test test_uu_cp_cp_recursive_non_utf8_source { |ctx|
  let s = uu.scene(ctx)?
  uu.at_bytes(s, b"dir\x80")?.mkdir()?
  uu.mkdir(s, "dir2")?
  uu.at_bytes(s, b"dir\x80/a")?.write("")?
  let r = uu.invoke_paths(s, "cp", [p"-r", Path.parse_bytes(b"dir\x80/.")?, p"dir2"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.exists(s, "dir2/a")?
}

# origin: uutils test_cp::test_cp_recursive_symlink_preserves_target_mode
test test_uu_cp_cp_recursive_symlink_preserves_target_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "target_dir")?
  uu.touch(s, "target_dir/file.txt")?
  uu.set_mode(s, "target_dir/file.txt", 0o600)?
  uu.mkdir(s, "src")?
  uu.symlink(s, uu.at(s, "target_dir/file.txt").display(), "src/link")?
  let r = uu.invoke(s, "cp", ["-r", "src", "dst"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "dst/link")?
  assert uu.mode(s, "target_dir/file.txt")? == 0o600
}

# origin: uutils test_cp::test_cp_recursive_target_dir_symlink_still_allowed
test test_uu_cp_cp_recursive_target_dir_symlink_still_allowed { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "srcdir")?
  uu.write(s, "srcdir/f", "X")?
  uu.mkdir(s, "real")?
  uu.symlink(s, "real", "dstlink")?
  let r = uu.invoke(s, "cp", ["-r", "srcdir", "dstlink/"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "real/srcdir/f")?
}

# origin: uutils test_cp::test_cp_reflink_always
test test_uu_cp_cp_reflink_always { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["--reflink=always", "hello_world.txt", "existing_file.txt"])?
  if r.status == 0 { uu.file_is(s, "existing_file.txt", "Hello, World!\n") }
}

# origin: uutils test_cp::test_cp_reflink_always_failure
test test_uu_cp_cp_reflink_always_failure { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["--reflink=always", "/dev/null", "/dev/full"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Invalid argument")
  let target = uu.invoke(s, "cp", ["--reflink=always", "/dev/null", "target"])?
  uu.fails(target)
  uu.no_stdout(target)
  uu.stderr_contains(target, "ross-device link")
}

# origin: uutils test_cp::test_cp_reflink_always_failure_dest_cleanup
test test_uu_cp_cp_reflink_always_failure_dest_cleanup { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "payload.txt", "reflink me\n")?
  let fresh = uu.invoke(s, "cp", ["--reflink=always", "payload.txt", "fresh.txt"])?
  if fresh.status != 0 {
    assert !uu.exists(s, "fresh.txt")?
    uu.write(s, "kept.txt", "previous contents\n")?
    uu.fails(uu.invoke(s, "cp", ["--reflink=always", "payload.txt", "kept.txt"])?)
    assert uu.file_exists(s, "kept.txt")?
    uu.file_is(s, "kept.txt", "")
  }
}

# origin: uutils test_cp::test_cp_reflink_auto
test test_uu_cp_cp_reflink_auto { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["--reflink=auto", "hello_world.txt", "existing_file.txt"])?
  uu.succeeds(r)
  uu.file_is(s, "existing_file.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_reflink_never
test test_uu_cp_cp_reflink_never { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  for argument in ["--reflink=never", "--reflink=neve", "--reflink=n"] {
    uu.succeeds(uu.invoke(s, "cp", [argument, "hello_world.txt", "existing_file.txt"])?)
    uu.file_is(s, "existing_file.txt", "Hello, World!\n")
    let out = uu.at(s, "fragments")
    let tool = process.which("filefrag")?
    let _ = process.run(process.command_argv(tool, [tool, p"-v", p"existing_file.txt"], s.root, stdout: out))?
    assert !("shared" in out.read_text()?)
  }
}

# origin: uutils test_cp::test_cp_reflink_none
test test_uu_cp_cp_reflink_none { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["--reflink", "hello_world.txt", "existing_file.txt"])?
  if r.status == 0 { uu.file_is(s, "existing_file.txt", "Hello, World!\n") }
}

# origin: uutils test_cp::test_cp_remove_destination_symlink_applies_mode
test test_uu_cp_cp_remove_destination_symlink_applies_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  uu.set_mode(s, "src", 0o664)?
  uu.touch(s, "target")?
  uu.symlink(s, uu.at(s, "target").display(), "dst")?
  let r = uu.invoke(s, "cp", ["--remove-destination", "src", "dst"], umask: 0o022)?
  uu.succeeds(r)
  assert !uu.is_symlink(s, "dst")?
  assert uu.mode(s, "dst")? == 0o644
}

# origin: uutils test_cp::test_cp_same_file
test test_uu_cp_cp_same_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cp", ["a", "a"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "'a' and 'a' are the same file")
}

# origin: uutils test_cp::test_cp_seen_file
test test_uu_cp_cp_seen_file { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b", "c"] { uu.mkdir(s, name)? }
  uu.write(s, "a/f", "a")?
  uu.write(s, "b/f", "b")?
  let r = uu.invoke(s, "cp", ["a/f", "b/f", "c"])?
  uu.fails(r)
  uu.stderr_contains(r, "will not overwrite just-created 'c/f' with 'b/f'")
  assert uu.exists(s, "c/f")?
  uu.succeeds(uu.invoke(s, "cp", ["--backup=numbered", "a/f", "b/f", "c"])?)
  assert uu.exists(s, "c/f")?
  assert uu.exists(s, "c/f.~1~")?
}

# origin: uutils test_cp::test_cp_single_file
test test_uu_cp_cp_single_file { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)?
  let r = uu.invoke(s, "cp", ["hello_world.txt"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "missing destination file")
}

# origin: uutils test_cp::test_cp_socket
test test_uu_cp_cp_socket { |ctx|
  let s = uu.scene(ctx)?
  fs.mknod(uu.at(s, "socket"), "socket", 0o600)
  uu.set_mode(s, "socket", 0o731)?
  let r = uu.invoke(s, "cp", ["--preserve=mode", "-r", "socket", "socket2"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "socket2"))?.kind == "socket"
  assert uu.mode(s, "socket2")? == 0o731
}

