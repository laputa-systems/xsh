##! Transcribed from the uutils cp integration tests.

use support.uu as uu

# origin: uutils test_cp::test_cp_sparse_always_reflink_always
test test_uu_cp_cp_sparse_always_reflink_always { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src_file1")?
  let r = uu.invoke(s, "cp", ["--sparse=always", "--reflink=always", "src_file1", "dst_file"])?
  uu.fails(r)
}

# origin: uutils test_cp::test_cp_sparse_never_reflink_always
test test_uu_cp_cp_sparse_never_reflink_always { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src_file1")?
  let r = uu.invoke(s, "cp", ["--sparse=never", "--reflink=always", "src_file1", "dst_file"])?
  uu.fails(r)
}

# origin: uutils test_cp::test_cp_sparse_invalid_option
test test_uu_cp_cp_sparse_invalid_option { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src_file1")?
  let r = uu.invoke(s, "cp", ["--sparse=invalid", "src_file1", "dst_file"])?
  uu.fails(r)
}

# origin: uutils test_cp::test_cp_sparse_always_empty
test test_uu_cp_cp_sparse_always_empty { |ctx|
  for argument in ["--sparse=always", "--sparse=alway", "--sparse=al"] {
    let s = uu.scene(ctx)?
    let buf = bytes.zero(16384)?
    uu.write_bytes(s, "src_file1", buf)?
    let r = uu.invoke(s, "cp", [argument, "src_file1", "dst_file_sparse"])?
    uu.succeeds(r)
    assert uu.read(s, "dst_file_sparse")? == buf
    assert fs.stat(uu.at(s, "dst_file_sparse"))?.blocks_512 == 0
  }
}

# origin: uutils test_cp::test_cp_sparse_always_non_empty
test test_uu_cp_cp_sparse_always_non_empty { |ctx|
  let s = uu.scene(ctx)?
  let buf = bytes.concat([bytes.zero(21846)?, b"x", bytes.zero(21845)?, b"x", bytes.zero(21846)?])
  uu.write_bytes(s, "src_file1", buf)?
  let r = uu.invoke(s, "cp", ["--sparse=always", "src_file1", "dst_file_sparse"])?
  uu.succeeds(r)
  let metadata = fs.stat(uu.at(s, "dst_file_sparse"))?
  assert uu.read(s, "dst_file_sparse")? == buf
  assert metadata.blocks_512 == 2 * metadata.blksize / 512
}

# origin: uutils test_cp::test_cp_sparse_never_empty
test test_uu_cp_cp_sparse_never_empty { |ctx|
  let s = uu.scene(ctx)?
  let buf = bytes.zero(16384)?
  uu.write_bytes(s, "src_file1", buf)?
  let r = uu.invoke(s, "cp", ["--sparse=never", "src_file1", "dst_file_non_sparse"])?
  uu.succeeds(r)
  assert uu.read(s, "dst_file_non_sparse")? == buf
  assert fs.stat(uu.at(s, "dst_file_non_sparse"))?.blocks_512 * 512 == buf.len()
}

# origin: uutils test_cp::test_cp_stream_to_full
test test_uu_cp_cp_stream_to_full { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["/dev/zero", "/dev/full"], timeout: 10s)?
  uu.fails(r)
  uu.stderr_contains(r, "No space")
}

# origin: uutils test_cp::test_cp_strip_trailing_slashes
test test_uu_cp_cp_strip_trailing_slashes { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cp", "hello_world.txt", "hello_world.txt")?
  let r = uu.invoke(s, "cp", ["--strip-trailing-slashes", "hello_world.txt/", "copy_of_hello_world.txt"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "cp: cannot stat 'hello_world.txt/': Not a directory\n")
  assert ! uu.exists(s, "copy_of_hello_world.txt")?
}

# origin: uutils test_cp::test_cp_symbolic_link_loop
test test_uu_cp_cp_symbolic_link_loop { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, uu.at(s, "loop").display(), "loop")?
  uu.touch(s, "f")?
  let r = uu.invoke(s, "cp", ["-f", "f", "loop"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "loop")?
}

# origin: uutils test_cp::test_remove_destination_symbolic_link_loop
test test_uu_cp_remove_destination_symbolic_link_loop { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, uu.at(s, "loop").display(), "loop")?
  uu.touch(s, "f")?
  let r = uu.invoke(s, "cp", ["--remove-destination", "f", "loop"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "loop")?
}

# origin: uutils test_cp::test_cp_symlink_overwrite_detection
test test_uu_cp_cp_symlink_overwrite_detection { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "good")?
  uu.mkdir(s, "tmp")?
  uu.write(s, "README", "file1")?
  uu.write(s, "good/README", "file2")?
  uu.symlink(s, uu.at(s, "tmp/foo").display(), "tmp/README")?
  uu.touch(s, "tmp/foo")?
  let r = uu.invoke(s, "cp", ["README", "good/README", "tmp"])?
  uu.fails(r)
  uu.stderr_only(r, "cp: will not copy 'good/README' through just-created symlink 'tmp/README'\n")
  uu.file_is(s, "tmp/foo", "file1")
}

# origin: uutils test_cp::test_cp_symlink_permissions
test test_uu_cp_cp_symlink_permissions { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.set_mode(s, "a", 0o700)?
  uu.symlink(s, uu.at(s, "a").display(), "symlink")?
  uu.mkdir(s, "dest")?
  let r = uu.invoke(s, "cp", ["--preserve", "symlink", "dest"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "a"))?.mode == fs.stat(uu.at(s, "dest/symlink"))?.mode
}

# origin: uutils test_cp::test_cp_target_directory_is_file
test test_uu_cp_cp_target_directory_is_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cp", "hello_world.txt", "hello_world.txt")?
  uu.fixture(s, "cp", "how_are_you.txt", "how_are_you.txt")?
  let r = uu.invoke(s, "cp", ["-t", "how_are_you.txt", "hello_world.txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "target directory 'how_are_you.txt': Not a directory")
}

# origin: uutils test_cp::test_cp_target_file_dev_null
test test_uu_cp_cp_target_file_dev_null { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_cp_target_file_file_i2")?
  let r = uu.invoke(s, "cp", ["/dev/null", "test_cp_target_file_file_i2"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "test_cp_target_file_file_i2")?
}

# origin: uutils test_cp::test_cp_to_existing_file_permissions
test test_uu_cp_cp_to_existing_file_permissions { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  uu.touch(s, "dst")?
  uu.set_mode(s, "src", uu.mode(s, "src")?.clear_bits(0o222))?
  let before = uu.mode(s, "dst")?
  let r = uu.invoke(s, "cp", ["src", "dst"])?
  uu.succeeds(r)
  assert uu.mode(s, "dst")? == before
}

# origin: uutils test_cp::test_cp_update_none_interactive_prompt_no
test test_uu_cp_cp_update_none_interactive_prompt_no { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "old", "old content")?
  uu.write(s, "new", "new content")?
  let r = uu.invoke(s, "cp", ["-i", "--update=none", "new", "old"])?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, "old", "old content")
  uu.file_is(s, "new", "new content")
}



# origin: uutils test_cp::test_cp_verbose_preserved_link_to_dir
test test_uu_cp_cp_verbose_preserved_link_to_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.hard_link(s, "file", "hardlink")?
  uu.mkdir(s, "dir")?
  let r = uu.invoke(s, "cp", ["-d", "--verbose", "file", "hardlink", "dir"])?
  uu.succeeds(r)
  uu.stdout_is(r, "'file' -> 'dir/file'\n'hardlink' -> 'dir/hardlink'\n")
  assert uu.file_exists(s, "dir/file")?
  assert uu.file_exists(s, "dir/hardlink")?
  assert fs.stat(uu.at(s, "dir/file"))?.nlink == 2
  assert fs.stat(uu.at(s, "dir/hardlink"))?.nlink == 2
  assert fs.stat(uu.at(s, "dir/file"))?.ino == fs.stat(uu.at(s, "dir/hardlink"))?.ino
}

# origin: uutils test_cp::test_cp_verbose_write_error_is_reported
test test_uu_cp_cp_verbose_write_error_is_reported { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "source_file")?
  let r = uu.invoke(s, "cp", ["--verbose", "source_file", "dest_file"], stdout: p"/dev/full")?
  uu.fails(r)
  uu.stderr_is(r, "cp: write error: No space left on device\n")
  assert uu.file_exists(s, "dest_file")?
}

# origin: uutils test_cp::test_cp_with_dirs
test test_uu_cp_cp_with_dirs { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cp", "hello_world.txt", "hello_world.txt")?
  uu.mkdir(s, "hello_dir")?
  uu.mkdir(s, "hello_dir_with_file")?
  uu.fixture(s, "cp", "hello_dir_with_file/hello_world.txt", "hello_dir_with_file/hello_world.txt")?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "hello_dir/"])?
  uu.succeeds(r)
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
  let r2 = uu.invoke(s, "cp", ["hello_dir_with_file/hello_world.txt", "copy_of_hello_world.txt"])?
  uu.succeeds(r2)
  uu.file_is(s, "copy_of_hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_with_dirs_t
test test_uu_cp_cp_with_dirs_t { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cp", "hello_world.txt", "hello_world.txt")?
  uu.mkdir(s, "hello_dir")?
  let r = uu.invoke(s, "cp", ["-t", "hello_dir/", "hello_world.txt"])?
  uu.succeeds(r)
  uu.file_is(s, "hello_dir/hello_world.txt", "Hello, World!\n")
}

# origin: uutils test_cp::test_cp_with_options_backup_and_rem_when_dest_is_symlink
test test_uu_cp_cp_with_options_backup_and_rem_when_dest_is_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "xyz")?
  uu.mkdir(s, "inner_dir")?
  uu.write(s, "inner_dir/inner_file", "abc")?
  uu.symlink(s, "inner_file", "inner_dir/sl")?
  let r = uu.invoke(s, "cp", ["-b", "--rem", "file", "inner_dir/sl"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "inner_dir/inner_file")?
  uu.file_is(s, "inner_dir/inner_file", "abc")
  assert uu.is_symlink(s, "inner_dir/sl~")?
  assert ! uu.is_symlink(s, "inner_dir/sl")?
  uu.file_is(s, "inner_dir/sl", "xyz")
}

# origin: uutils test_cp::test_cp_writable_special_file_permissions
test test_uu_cp_cp_writable_special_file_permissions { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cp", ["/dev/null", "/dev/zero"])?
  uu.succeeds(r)
}

# origin: uutils test_cp::test_cp_zero_sized_virtual_file_contents
test test_uu_cp_cp_zero_sized_virtual_file_contents { |ctx|
  let expected = p"/proc/version".read_text()?
  assert expected != ""
  for extra in [[], ["--sparse=never"], ["--sparse=always"], ["--sparse=auto"], ["--reflink=never"], ["--reflink=never", "--sparse=always"]] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "cp", extra.extend(["/proc/version", "copied"]))?
    uu.succeeds(r)
    uu.no_output(r)
    uu.file_is(s, "copied", expected)
  }
}

# origin: uutils test_cp::test_dir_recursive_copy
test test_uu_cp_dir_recursive_copy { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "parent1/child")?
  uu.mkdir(s, "parent2/child1/child2/child3")?
  for pair in [["parent1", "parent1"], ["parent1", "parent1/child"]] {
    let r = uu.invoke(s, "cp", ["-R"].extend(pair))?
    uu.fails(r)
    uu.stderr_contains(r, "cannot copy a directory")
  }
  let r = uu.invoke(s, "cp", ["-R", "parent1/child", "parent2"])?
  uu.succeeds(r)
  let bad = uu.invoke(s, "cp", ["-R", "parent2/child1/", "parent2/child1/child2/child3"])?
  uu.fails(bad)
  uu.stderr_contains(bad, "cannot copy a directory")
}

# origin: uutils test_cp::test_hard_link_file
test test_uu_cp_hard_link_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  uu.touch(s, "dest")?
  let r = uu.invoke(s, "cp", ["-f", "--link", "src", "dest"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "src"))?.ino == fs.stat(uu.at(s, "dest"))?.ino
}

# origin: uutils test_cp::test_no_preserve_mode_with_later_preserve
test test_uu_cp_no_preserve_mode_with_later_preserve { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  uu.set_mode(s, "src", 0o755)?
  let r = uu.invoke(s, "cp", ["--no-preserve=mode", "src", "dst1"])?
  uu.succeeds(r)
  let first = uu.mode(s, "dst1")?.bit_and(0o777)
  assert first.bit_and(0o111) != 0o111
  for args in [
    ["--no-preserve=mode", "--preserve=timestamps", "src", "dst2"],
    ["--no-preserve=all", "--preserve=timestamps", "src", "dst3"],
    ["--preserve=timestamps", "--no-preserve=mode", "src", "dst4"]
  ] {
    uu.succeeds(uu.invoke(s, "cp", args)?)
    assert uu.mode(s, args[3])?.bit_and(0o777) == first
  }
  uu.succeeds(uu.invoke(s, "cp", ["--no-preserve=mode", "--preserve=mode", "src", "dst5"])?)
  assert uu.mode(s, "dst5")?.bit_and(0o777) == 0o755
}

# origin: uutils test_cp::test_non_utf8_src
test test_uu_cp_non_utf8_src { |ctx|
  let s = uu.scene(ctx)?
  let src = uu.at_bytes(s, b"\xff\xffsrc")?
  src.write("")?
  let r = uu.invoke_paths(s, "cp", [Path.parse_bytes(b"\xff\xffsrc")?, p"dest"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "dest")?
}

# origin: uutils test_cp::test_non_utf8_dest
test test_uu_cp_non_utf8_dest { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cp", "hello_world.txt", "hello_world.txt")?
  let name = Path.parse_bytes(b"\xff\xffdest")?
  let r = uu.invoke_paths(s, "cp", [p"hello_world.txt", name])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.at_bytes(s, b"\xff\xffdest")?.is_file()?
}

# origin: uutils test_cp::test_non_utf8_target
test test_uu_cp_non_utf8_target { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cp", "hello_world.txt", "hello_world.txt")?
  uu.at_bytes(s, b"\xff\xffdest")?.mkdir()?
  let r = uu.invoke_paths(s, "cp", [p"-t", Path.parse_bytes(b"\xff\xffdest")?, p"hello_world.txt"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.at_bytes(s, b"\xff\xffdest/hello_world.txt")?.is_file()?
}

# origin: uutils test_cp::test_preserve_attrs_overriding_1
test test_uu_cp_preserve_attrs_overriding_1 { |ctx|
  for flag in ["-d", "-a"] {
    let s = uu.scene(ctx)?
    uu.touch(s, "file")?
    uu.symlink(s, uu.at(s, "file").display(), "symlink")?
    uu.succeeds(uu.invoke(s, "cp", [flag, "--no-preserve=all", "symlink", "dest"])?)
    assert uu.is_symlink(s, "dest")?
  }
}

# origin: uutils test_cp::test_preserve_attrs_overriding_2
test test_uu_cp_preserve_attrs_overriding_2 { |ctx|
  for args in [
    ["-r", "--preserve=mode,link,timestamp", "--no-preserve=link"],
    ["-r", "--preserve=mode", "--preserve=link", "--preserve=timestamp", "--no-preserve=link"],
    ["-r", "--preserve=mode,link", "--no-preserve=link", "--preserve=timestamp"],
    ["-a", "--no-preserve=link"],
    ["-r", "--preserve", "--no-preserve=link"]
  ] {
    let s = uu.scene(ctx)?
    uu.mkdir(s, "folder")?
    uu.touch(s, "folder/file1")?
    uu.set_mode(s, "folder/file1", 0o775)?
    uu.hard_link(s, "folder/file1", "folder/file2")?
    let before = fs.stat(uu.at(s, "folder/file1"))?
    uu.succeeds(uu.invoke(s, "cp", args.extend(["folder", "dest"]))?)
    let first = fs.stat(uu.at(s, "dest/file1"))?
    let second = fs.stat(uu.at(s, "dest/file2"))?
    assert before.mtime_ns == first.mtime_ns
    assert before.mode == first.mode
    assert first.ino != second.ino
  }
}

# origin: uutils test_cp::test_preserve_hardlink_attributes_in_directory
test test_uu_cp_preserve_hardlink_attributes_in_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "src")?
  uu.touch(s, "src/f")?
  uu.hard_link(s, "src/f", "src/link")?
  uu.mkdir(s, "dest/src")?
  uu.touch(s, "dest/src/f")?
  let r = uu.invoke(s, "cp", ["-a", "src", "dest"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "dest/src/f"))?.ino == fs.stat(uu.at(s, "dest/src/link"))?.ino
}

# origin: uutils test_cp::test_preserve_mode
test test_uu_cp_preserve_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.set_mode(s, "file", 0o7777)?
  let r = uu.invoke(s, "cp", ["file", "dest", "-p"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.mode(s, "dest")? == 0o7777
}

# origin: uutils test_cp::test_reflink_never_sparse_always
test test_uu_cp_reflink_never_sparse_always { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  uu.truncate(s, "src", 1048576)?
  let r = uu.invoke(s, "cp", ["--reflink=never", "--sparse=always", "src", "dest"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "src"))?.blocks_512 == fs.stat(uu.at(s, "dest"))?.blocks_512
  assert uu.size(s, "dest")? == 1048576
}

# origin: uutils test_cp::test_remove_destination_with_destination_being_a_hardlink_to_source
test test_uu_cp_remove_destination_with_destination_being_a_hardlink_to_source { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.hard_link(s, "file", "hardlink")?
  let r = uu.invoke(s, "cp", ["--remove-destination", "file", "hardlink"])?
  uu.succeeds(r)
  assert ! uu.is_symlink(s, "hardlink")?
  assert uu.file_exists(s, "file")?
  assert uu.file_exists(s, "hardlink")?
}

# origin: uutils test_cp::test_remove_destination_with_destination_being_a_symlink_to_source
test test_uu_cp_remove_destination_with_destination_being_a_symlink_to_source { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, uu.at(s, "file").display(), "symlink")?
  let r = uu.invoke(s, "cp", ["--remove-destination", "file", "symlink"])?
  uu.succeeds(r)
  assert ! uu.is_symlink(s, "symlink")?
  assert uu.file_exists(s, "file")?
  assert uu.file_exists(s, "symlink")?
}

# origin: uutils test_cp::test_remove_destination_with_destination_being_relative_path_of_source
test test_uu_cp_remove_destination_with_destination_being_relative_path_of_source { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "hello")?
  let r = uu.invoke(s, "cp", ["--remove-destination", "a", "./a"])?
  uu.fails(r)
  uu.stderr_contains(r, "are the same file")
  assert uu.file_exists(s, "a")?
  uu.file_is(s, "a", "hello")
}

# origin: uutils test_cp::test_same_file_backup
test test_uu_cp_same_file_backup { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = uu.invoke(s, "cp", ["--backup", "f", "f"])?
  uu.fails(r)
  uu.stderr_only(r, "cp: 'f' and 'f' are the same file\n")
  assert ! uu.exists(s, "f~")?
}

# origin: uutils test_cp::test_same_file_force
test test_uu_cp_same_file_force { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = uu.invoke(s, "cp", ["--force", "f", "f"])?
  uu.fails(r)
  uu.stderr_only(r, "cp: 'f' and 'f' are the same file\n")
  assert ! uu.exists(s, "f~")?
}

# origin: uutils test_cp::test_same_file_force_backup
test test_uu_cp_same_file_force_backup { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = uu.invoke(s, "cp", ["--force", "--backup", "f", "f"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.file_exists(s, "f~")?
}

# origin: uutils test_cp::test_src_base_dot
test test_uu_cp_src_base_dot { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "x")?
  uu.mkdir(s, "y")?
  let nested = {...s, root: uu.at(s, "y")}
  let r = uu.invoke(nested, "cp", ["--verbose", "-r", "../x/.", "."])?
  uu.succeeds(r)
  uu.no_output(r)
  assert ! uu.exists(s, "y/x")?
}

# origin: uutils test_cp::test_symbolic_link_file
test test_uu_cp_symbolic_link_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  uu.touch(s, "dest")?
  let r = uu.invoke(s, "cp", ["-f", "--symbolic-link", "src", "dest"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.read_link(s, "dest")? == "src"
}

# origin: uutils test_cp::test_symlink_mode_overwrite
test test_uu_cp_symlink_mode_overwrite { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.write(s, "a/t", "hello")?
  uu.write(s, "b/t", "hello")?
  let r = uu.invoke(s, "cp", ["-s", "a/t", "b/t", "."])?
  uu.fails(r)
  uu.stderr_only(r, "cp: will not overwrite just-created './t' with 'b/t'\n")
  uu.file_is(s, "./t", "hello")
}

# origin: uutils test_cp::test_cp_umask_stripping_owner_write_bit
test test_uu_cp_cp_umask_stripping_owner_write_bit { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input.txt", "copied through the first fd\n")?
  uu.set_mode(s, "input.txt", 0o664)?
  let r = uu.invoke(s, "cp", ["input.txt", "output.txt"], umask: 0o333)?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, "output.txt", "copied through the first fd\n")
  assert uu.mode(s, "output.txt")?.bit_and(0o777) == 0o444
}

# origin: uutils test_cp::test_cp_umask_stripping_owner_write_bit_reflink_never
test test_uu_cp_cp_umask_stripping_owner_write_bit_reflink_never { |ctx|
  for sparse in ["--sparse=auto", "--sparse=always"] {
    let s = uu.scene(ctx)?
    uu.write(s, "input.txt", "written while still writable\n")?
    uu.set_mode(s, "input.txt", 0o664)?
    let r = uu.invoke(s, "cp", ["--reflink=never", sparse, "input.txt", "output.txt"], umask: 0o333)?
    uu.succeeds(r)
    uu.no_output(r)
    uu.file_is(s, "output.txt", "written while still writable\n")
    assert uu.mode(s, "output.txt")?.bit_and(0o777) == 0o444
  }
}

# origin: uutils test_cp::test_no_preserve_mode
test test_uu_cp_no_preserve_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.set_mode(s, "file", 0o7777)?
  let r = uu.invoke(s, "cp", ["file", "dest"], umask: 0o022)?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.mode(s, "dest")? == 0o755
}

# origin: uutils test_cp::test_dir_perm_race_with_preserve_mode_and_ownership
test test_uu_cp_dir_perm_race_with_preserve_mode_and_ownership { |ctx|
  for attr in ["mode", "ownership"] {
    let s = uu.scene(ctx)?
    uu.mkdir(s, "src")?
    uu.mkdir(s, "dest")?
    uu.set_mode(s, "src", 0o775)?
    uu.set_mode(s, "dest", 0o2775)?
    uu.mkfifo(s, "src/fifo")?
    let plan = uu.command(s, "cp", [f"--preserve={attr}", "-R", "--copy-contents", "--parents", "src", "dest"], umask: 0)?
    let child = spawn plan?
    defer child.cancel(signal: "TERM", kill_after: 0ms)
    let start = time.now()
    while ! uu.exists(s, "dest/src")? {
      assert time.now() - start < 10000000000, "timed out: cp took too long to create destination directory"
      time.sleep(100ms)?
    }
    let observed = fs.stat(uu.at(s, "dest/src"))?.mode
    let mask = if attr == "mode" { 0o022 } else { 0o077 }
    # Complete the FIFO copy before asserting, so a failed permission check cannot leave a child blocked.
    let writer = process.command_argv("sh", ["sh", "-c", "printf done > \"$1\"", "sh", uu.at(s, "src/fifo").display()], timeout: 10s)
    assert process.run(writer)?.exited_with(0)
    let finished = process.wait_timeout([child], 10s)?
    assert finished != null, "timed out waiting for FIFO copy"
    assert finished.status.exited_with(0)
    assert observed.bit_and(mask) == 0, f"unwanted permissions are present - {attr}"
  }
}

# origin: uutils test_cp::test_cp_xattr_enotsup_handling
test test_uu_cp_cp_xattr_enotsup_handling { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "src", "x")?
  if fs.xattr_set(uu.at(s, "src"), "user.t", b"v") is Err(_) {
    test.skip("source filesystem does not support user xattrs")
    return
  }
  if ! p"/dev/shm".exists()? { test.skip("/dev/shm is unavailable"); return }
  let base = s.root.basename()
  let probe = fp"/dev/shm/xattr_test_probe_{base}"
  probe.write("test")?
  defer probe.remove(missing_ok: true)
  if fs.xattr_set(probe, "user.t", b"v") is Ok(_) {
    test.skip("/dev/shm supports xattrs on this system")
    return
  }
  let t1 = fp"/dev/shm/t1_{base}"
  let t2 = fp"/dev/shm/t2_{base}"
  let t3 = fp"/dev/shm/t3_{base}"
  defer t1.remove(missing_ok: true)
  defer t2.remove(missing_ok: true)
  defer t3.remove(missing_ok: true)
  for item in [{flag: "-a", target: t1}, {flag: "--preserve=all", target: t2}] {
    let r = uu.invoke(s, "cp", [item.flag, uu.at(s, "src").display(), item.target.display()])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  let r = uu.invoke(s, "cp", ["--preserve=xattr", uu.at(s, "src").display(), t3.display()])?
  uu.fails(r)
  uu.stderr_contains(r, "setting attributes")
  uu.stderr_contains(r, "Operation not supported")
}

# origin: uutils test_cp::test_cp_xattr_failure_keeps_dest_contents
test test_uu_cp_cp_xattr_failure_keeps_dest_contents { |ctx|
  let s = uu.scene(ctx)?
  let base = s.root.basename()
  let source = fp"/dev/shm/cp_keep_dest_{base}"
  # The destination must live outside tmpfs to exercise its smaller xattr value limit.
  let destination = fp"/var/tmp/cp_keep_dest_{base}"
  if source.write("kept content") is Err(_) or destination.mkdir() is Err(_) {
    test.skip("no usable /dev/shm or disk-backed temporary directory")
    return
  }
  defer source.remove(missing_ok: true)
  defer destination.remove(missing_ok: true)
  let huge = bytes.from_ints([121 for _ in range(9100)])?
  let source_accepts = fs.xattr_set(source, "user.huge", huge) is Ok(_)
  let probe = fp"{destination}/probe"
  probe.write("x")?
  let destination_rejects = fs.xattr_set(probe, "user.huge", huge) is Err(_)
  if ! source_accepts or ! destination_rejects {
    test.skip("filesystem combination cannot produce xattr failure")
    return
  }
  let target = fp"{destination}/out"
  let r = uu.invoke(s, "cp", ["--preserve=xattr", source.display(), target.display()])?
  uu.fails(r)
  uu.stderr_contains(r, "setting attribute")
  assert target.read_text()? == "kept content"
  source.chmod(0o444)?
  let readonly = fp"{destination}/out_ro"
  let r2 = uu.invoke(s, "cp", ["--preserve=xattr", source.display(), readonly.display()])?
  uu.fails(r2)
  uu.stderr_contains(r2, "setting attribute")
  assert readonly.read_text()? == "kept content"
  assert fs.stat(readonly)?.mode.bit_and(0o777) == 0o444
}
