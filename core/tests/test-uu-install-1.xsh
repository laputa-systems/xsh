##! Transcribed from the uutils install integration tests.

use support.uu as uu

# Missing paths return false; metadata errors on existing paths remain errors.
proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let entry = uu.at(s, name)
  Ok(entry.exists()? and entry.is_file()?)
}

proc dir_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let entry = uu.at(s, name)
  Ok(entry.exists()? and entry.is_dir()?)
}

proc usage_error(r: uu.Ran, message: Str) {
  uu.stderr_only(r, f"install: {message}\nTry 'install --help' for more information.\n")
}

# origin: uutils test_install::test_install_ancestors_directories
test test_uu_install_install_ancestors_directories { |ctx|
  let s = uu.scene(ctx)?
  let ancestor1 = "ancestor1"
  let ancestor2 = "ancestor1/ancestor2"
  let target_dir = "ancestor1/ancestor2/target_dir"
  let directories_arg = "-d"
  let r1 = uu.invoke(s, "install", [directories_arg, target_dir])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert dir_exists(s, ancestor1)?
  assert dir_exists(s, ancestor2)?
  assert dir_exists(s, target_dir)?
}

# origin: uutils test_install::test_install_ancestors_mode_directories
test test_uu_install_install_ancestors_mode_directories { |ctx|
  let s = uu.scene(ctx)?
  let ancestor1 = "ancestor1"
  let ancestor2 = "ancestor1/ancestor2"
  let target_dir = "ancestor1/ancestor2/target_dir"
  let directories_arg = "-d"
  let mode_arg = "--mode=200"
  let probe = "probe"
  uu.mkdir(s, probe)?
  let default_perms = fs.stat(uu.at(s, probe), follow_symlinks: true)?.mode
  let r1 = uu.invoke(s, "install", [mode_arg, directories_arg, target_dir])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert dir_exists(s, ancestor1)?
  assert dir_exists(s, ancestor2)?
  assert dir_exists(s, target_dir)?
  assert default_perms == fs.stat(uu.at(s, ancestor1), follow_symlinks: true)?.mode
  assert default_perms == fs.stat(uu.at(s, ancestor2), follow_symlinks: true)?.mode
  assert 0o40200 == fs.stat(uu.at(s, target_dir), follow_symlinks: true)?.mode
}

# origin: uutils test_install::test_install_ancestors_mode_directories_with_file
test test_uu_install_install_ancestors_mode_directories_with_file { |ctx|
  let s = uu.scene(ctx)?
  let ancestor1 = "ancestor1"
  let ancestor2 = "ancestor1/ancestor2"
  let target_file = "ancestor1/ancestor2/target_file"
  let directories_arg = "-D"
  let mode_arg = "--mode=200"
  let file = "file"
  let probe = "probe"
  uu.mkdir(s, probe)?
  let default_perms = fs.stat(uu.at(s, probe), follow_symlinks: true)?.mode
  uu.touch(s, file)?
  let r1 = uu.invoke(s, "install", [mode_arg, directories_arg, file, target_file])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert dir_exists(s, ancestor1)?
  assert dir_exists(s, ancestor2)?
  assert file_exists(s, target_file)?
  assert default_perms == fs.stat(uu.at(s, ancestor1), follow_symlinks: true)?.mode
  assert default_perms == fs.stat(uu.at(s, ancestor2), follow_symlinks: true)?.mode
  assert 0o100200 == fs.stat(uu.at(s, target_file), follow_symlinks: true)?.mode
}

# origin: uutils test_install::test_install_backup_allows_hardlink_under_another_name
test test_uu_install_install_backup_allows_hardlink_under_another_name { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.write(s, "a~", "source content")?
  uu.hard_link(s, "a~", "other")?
  let r1 = uu.invoke(s, "install", ["--backup=simple", "other", "a"])?
  uu.succeeds(r1)
  assert uu.read_text(s, "a")? == "source content"
}

# origin: uutils test_install::test_install_backup_custom_suffix_refuses
test test_uu_install_install_backup_custom_suffix_refuses { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.write(s, "a.bak", "source content")?
  let r1 = uu.invoke(s, "install", ["-b", "--suffix=.bak", "a.bak", "a"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "might destroy source")
  assert uu.read_text(s, "a.bak")? == "source content"
}

# origin: uutils test_install::test_install_backup_custom_suffix_via_env
test test_uu_install_install_backup_custom_suffix_via_env { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_custom_suffix_file_a"
  let file_b = "test_install_backup_custom_suffix_file_b"
  let suffix = "super-suffix-of-the-century"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["-b", file_a, file_b], vars: {SIMPLE_BACKUP_SUFFIX: suffix})?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}{suffix}")?
}

# origin: uutils test_install::test_install_backup_existing
test test_uu_install_install_backup_existing { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["--backup=existing", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}~")?
}

# origin: uutils test_install::test_install_backup_long_no_args_file_to_dir
test test_uu_install_install_backup_long_no_args_file_to_dir { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_install_simple_backup_file_a"
  let dest_dir = "test_install_dest/"
  let expect = f"{dest_dir}{file}"
  uu.touch(s, file)?
  uu.mkdir(s, dest_dir)?
  uu.touch(s, expect)?
  let r1 = uu.invoke(s, "install", ["--backup", file, dest_dir])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file)?
  assert file_exists(s, expect)?
  assert file_exists(s, f"{expect}~")?
}

# origin: uutils test_install::test_install_backup_long_no_args_files
test test_uu_install_install_backup_long_no_args_files { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_simple_backup_file_a"
  let file_b = "test_install_simple_backup_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["--backup", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}~")?
}

# origin: uutils test_install::test_install_backup_never
test test_uu_install_install_backup_never { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["--backup=never", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}~")?
}

# origin: uutils test_install::test_install_backup_nil
test test_uu_install_install_backup_nil { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["--backup=nil", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}~")?
}

# origin: uutils test_install::test_install_backup_nil_same_file
test test_uu_install_install_backup_nil_same_file { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_install_backup_numbering_file"
  uu.write(s, file, "content")?
  let methods = [
  "none", "off", "numbered", "t", "existing", "nil", "simple", "never",
  ]
  for method in methods {
    let r1 = uu.invoke(s, "install", [
    f"--backup={method}",
    file,
    file,
    ])?
    uu.fails(r1)
    uu.stderr_contains(r1, "are the same file")
    assert uu.read_text(s, file)? == "content"
  }
}

# origin: uutils test_install::test_install_backup_none
test test_uu_install_install_backup_none { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["--backup=none", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert !file_exists(s, f"{file_b}~")?
}

# origin: uutils test_install::test_install_backup_numbered_allows_source_named_like_backup
test test_uu_install_install_backup_numbered_allows_source_named_like_backup { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.write(s, "a~", "source content")?
  let r1 = uu.invoke(s, "install", ["--backup=numbered", "a~", "a"])?
  uu.succeeds(r1)
  assert uu.read_text(s, "a~")? == "source content"
  assert uu.read_text(s, "a")? == "source content"
}

# origin: uutils test_install::test_install_backup_numbered_if_existing_backup_existing
test test_uu_install_install_backup_numbered_if_existing_backup_existing { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  let file_b_backup = "test_install_backup_numbering_file_b.~1~"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  uu.touch(s, file_b_backup)?
  let r1 = uu.invoke(s, "install", ["--backup=existing", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, file_b_backup)?
  assert file_exists(s, f"{file_b}.~2~")?
}

# origin: uutils test_install::test_install_backup_numbered_if_existing_backup_nil
test test_uu_install_install_backup_numbered_if_existing_backup_nil { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  let file_b_backup = "test_install_backup_numbering_file_b.~1~"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  uu.touch(s, file_b_backup)?
  let r1 = uu.invoke(s, "install", ["--backup=nil", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, file_b_backup)?
  assert file_exists(s, f"{file_b}.~2~")?
}

# origin: uutils test_install::test_install_backup_numbered_with_numbered
test test_uu_install_install_backup_numbered_with_numbered { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["--backup=numbered", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}.~1~")?
}

# origin: uutils test_install::test_install_backup_numbered_with_t
test test_uu_install_install_backup_numbered_with_t { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["--backup=t", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}.~1~")?
}

# origin: uutils test_install::test_install_backup_off
test test_uu_install_install_backup_off { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["--backup=off", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert !file_exists(s, f"{file_b}~")?
}

# origin: uutils test_install::test_install_backup_refuses_regardless_of_spelling
test test_uu_install_install_backup_refuses_regardless_of_spelling { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.write(s, "a~", "source content")?
  let r1 = uu.invoke(s, "install", ["--backup=simple", "./a~", "a"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "might destroy source")
  assert uu.read_text(s, "a~")? == "source content"
}

# origin: uutils test_install::test_install_backup_refuses_when_source_is_the_backup
test test_uu_install_install_backup_refuses_when_source_is_the_backup { |ctx|
  for mode in ["simple", "existing"] {
    let s = uu.scene(ctx)?
    uu.touch(s, "a")?
    uu.write(s, "a~", "source content")?
    let r1 = uu.invoke(s, "install", [f"--backup={mode}", "a~", "a"])?
    uu.fails(r1)
    uu.stderr_is(r1, "install: backing up 'a' might destroy source;  'a~' not copied\n")
    assert uu.read_text(s, "a~")? == "source content"
  }
}

# origin: uutils test_install::test_install_backup_short_custom_suffix
test test_uu_install_install_backup_short_custom_suffix { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_custom_suffix_file_a"
  let file_b = "test_install_backup_custom_suffix_file_b"
  let suffix = "super-suffix-of-the-century"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["-b", f"--suffix={suffix}", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}{suffix}")?
}

# origin: uutils test_install::test_install_backup_short_custom_suffix_hyphen_value
test test_uu_install_install_backup_short_custom_suffix_hyphen_value { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_custom_suffix_file_a"
  let file_b = "test_install_backup_custom_suffix_file_b"
  let suffix = "-v"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["-b", f"--suffix={suffix}", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}{suffix}")?
}

# origin: uutils test_install::test_install_backup_short_no_args_file_to_dir
test test_uu_install_install_backup_short_no_args_file_to_dir { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_install_simple_backup_file_a"
  let dest_dir = "test_install_dest/"
  let expect = f"{dest_dir}{file}"
  uu.touch(s, file)?
  uu.mkdir(s, dest_dir)?
  uu.touch(s, expect)?
  let r1 = uu.invoke(s, "install", ["-b", file, dest_dir])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file)?
  assert file_exists(s, expect)?
  assert file_exists(s, f"{expect}~")?
}

# origin: uutils test_install::test_install_backup_short_no_args_files
test test_uu_install_install_backup_short_no_args_files { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_simple_backup_file_a"
  let file_b = "test_install_simple_backup_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["-b", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}~")?
}

# origin: uutils test_install::test_install_backup_simple
test test_uu_install_install_backup_simple { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_numbering_file_a"
  let file_b = "test_install_backup_numbering_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "install", ["--backup=simple", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}~")?
}

# origin: uutils test_install::test_install_chown_directory_invalid
test test_uu_install_install_chown_directory_invalid { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "install", ["-o", "test_invalid_user", "-d", "dir1/dir2"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "install: invalid user 'test_invalid_user'")
  let r2 = uu.invoke(s, "install", ["-g", "test_invalid_group", "-d", "dir1/dir2"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "install: invalid group 'test_invalid_group'")
  let r3 = uu.invoke(s, "install", ["-o", "test_invalid_user", "-g", "test_invalid_group", "-d", "dir1/dir2"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "install: invalid user 'test_invalid_user'")
  let r4 = uu.invoke(s, "install", ["-g", "test_invalid_group", "-o", "test_invalid_user", "-d", "dir1/dir2"])?
  uu.fails(r4)
  uu.stderr_contains(r4, "install: invalid user 'test_invalid_user'")
}

# origin: uutils test_install::test_install_chown_file_invalid
test test_uu_install_install_chown_file_invalid { |ctx|
  let s = uu.scene(ctx)?
  let file_1 = "source_file1"
  uu.touch(s, file_1)?
  let r1 = uu.invoke(s, "install", ["-o", "test_invalid_user", file_1, "target_file1"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "install: invalid user 'test_invalid_user'")
  let r2 = uu.invoke(s, "install", ["-g", "test_invalid_group", file_1, "target_file1"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "install: invalid group 'test_invalid_group'")
  let r3 = uu.invoke(s, "install", ["-o", "test_invalid_user", "-g", "test_invalid_group", file_1, "target_file1"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "install: invalid user 'test_invalid_user'")
  let r4 = uu.invoke(s, "install", ["-g", "test_invalid_group", "-o", "test_invalid_user", file_1, "target_file1"])?
  uu.fails(r4)
  uu.stderr_contains(r4, "install: invalid user 'test_invalid_user'")
}

# origin: uutils test_install::test_install_compare_basic
test test_uu_install_install_compare_basic { |ctx|
  let s = uu.scene(ctx)?
  let source = "source_file"
  let dest = "dest_file"
  uu.write(s, source, "test content")?
  let r1 = uu.invoke(s, "install", ["-Cv", "-m644", source, dest])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, f"'{source}' -> '{dest}'")
  let r2 = uu.invoke(s, "install", ["-Cv", "-m644", source, dest])?
  uu.succeeds(r2)
  uu.no_stdout(r2)
  let source2 = "source2"
  uu.write(s, source2, "different content")?
  let r3 = uu.invoke(s, "install", ["-Cv", "-m644", source2, dest])?
  uu.succeeds(r3)
  uu.stdout_contains(r3, "removed")
  uu.stdout_contains(r3, f"'{source2}' -> '{dest}'")
  let r4 = uu.invoke(s, "install", ["-Cv", "-m644", source2, dest])?
  uu.succeeds(r4)
  uu.no_stdout(r4)
}

# origin: uutils test_install::test_install_compare_option
test test_uu_install_install_compare_option { |ctx|
  let s = uu.scene(ctx)?
  let first = "a"
  let second = "b"
  uu.touch(s, first)?
  let r1 = uu.invoke(s, "install", ["-Cv", first, second])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, f"'{first}' -> '{second}'")
  let r2 = uu.invoke(s, "install", ["-Cv", first, second])?
  uu.succeeds(r2)
  uu.no_stdout(r2)
  let r3 = uu.invoke(s, "install", ["-Cv", "-m0644", first, second])?
  uu.succeeds(r3)
  uu.stdout_contains(r3, f"removed '{second}'\n'{first}' -> '{second}'")
  let r4 = uu.invoke(s, "install", ["-Cv", first, second])?
  uu.succeeds(r4)
  uu.stdout_contains(r4, f"removed '{second}'\n'{first}' -> '{second}'")
  let r5 = uu.invoke(s, "install", ["-C", "--preserve-timestamps", first, second])?
  uu.succeeds(r5)
  uu.no_output(r5)
  let r6 = uu.invoke(s, "install", ["-C", "--strip", "--strip-program=echo", first, second])?
  uu.fails_with_code(r6, 1)
  uu.stderr_contains(r6, "options --compare (-C) and --strip are mutually exclusive")
}

# origin: uutils test_install::test_install_compare_special_mode_bits
test test_uu_install_install_compare_special_mode_bits { |ctx|
  let s = uu.scene(ctx)?
  let source = "source_file"
  let dest = "dest_file"
  uu.write(s, source, "test content")?
  let r1 = uu.invoke(s, "install", ["-Cv", "-m2755", source, dest])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, f"'{source}' -> '{dest}'")
  let r2 = uu.invoke(s, "install", ["-Cv", "-m2755", source, dest])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "removed")
  uu.stdout_contains(r2, f"'{source}' -> '{dest}'")
  let r3 = uu.invoke(s, "install", ["-Cv", "-m4755", source, dest])?
  uu.succeeds(r3)
  uu.stdout_contains(r3, "removed")
  uu.stdout_contains(r3, f"'{source}' -> '{dest}'")
  let r4 = uu.invoke(s, "install", ["-Cv", "-m4755", source, dest])?
  uu.succeeds(r4)
  uu.stdout_contains(r4, "removed")
  uu.stdout_contains(r4, f"'{source}' -> '{dest}'")
  let r5 = uu.invoke(s, "install", ["-Cv", "-m1755", source, dest])?
  uu.succeeds(r5)
  uu.stdout_contains(r5, "removed")
  uu.stdout_contains(r5, f"'{source}' -> '{dest}'")
  let r6 = uu.invoke(s, "install", ["-Cv", "-m1755", source, dest])?
  uu.succeeds(r6)
  uu.stdout_contains(r6, "removed")
  uu.stdout_contains(r6, f"'{source}' -> '{dest}'")
  let r7 = uu.invoke(s, "install", ["-Cv", "-m644", source, dest])?
  uu.succeeds(r7)
  uu.stdout_contains(r7, "removed")
  uu.stdout_contains(r7, f"'{source}' -> '{dest}'")
  let r8 = uu.invoke(s, "install", ["-Cv", "-m644", source, dest])?
  uu.succeeds(r8)
  uu.no_stdout(r8)
}

# origin: uutils test_install::test_install_compare_symlink_handling
test test_uu_install_install_compare_symlink_handling { |ctx|
  let s = uu.scene(ctx)?
  let source = "source_file"
  let symlink_dest = "symlink_dest"
  let target_file = "target_file"
  uu.write(s, source, "test content")?
  uu.write(s, target_file, "test content")?
  uu.symlink(s, target_file, symlink_dest)?
  let r1 = uu.invoke(s, "install", ["-Cv", "-m644", source, symlink_dest])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "removed")
  uu.stdout_contains(r1, f"'{source}' -> '{symlink_dest}'")
  let r2 = uu.invoke(s, "install", ["-Cv", "-m644", source, symlink_dest])?
  uu.succeeds(r2)
  uu.no_stdout(r2)
}

# origin: uutils test_install::test_install_copy_file
test test_uu_install_install_copy_file { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "source_file"
  let file2 = "target_file"
  uu.touch(s, file1)?
  let r1 = uu.invoke(s, "install", [file1, file2])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file1)?
  assert file_exists(s, file2)?
}

# origin: uutils test_install::test_install_copy_then_compare_file
test test_uu_install_install_copy_then_compare_file { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "test_install_copy_then_compare_file_a1"
  let file2 = "test_install_copy_then_compare_file_a2"
  uu.touch(s, file1)?
  let r1 = uu.invoke(s, "install", ["-C", file1, file2])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  var file2_meta = fs.stat(uu.at(s, file2), follow_symlinks: true)?
  let before = file2_meta.mtime_ns
  let r2 = uu.invoke(s, "install", ["-C", file1, file2])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  file2_meta = fs.stat(uu.at(s, file2), follow_symlinks: true)?
  let after = file2_meta.mtime_ns
  assert before == after
}

# origin: uutils test_install::test_install_copy_then_compare_file_with_extra_mode
test test_uu_install_install_copy_then_compare_file_with_extra_mode { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "test_install_copy_then_compare_file_with_extra_mode_a1"
  let file2 = "test_install_copy_then_compare_file_with_extra_mode_a2"
  uu.touch(s, file1)?
  let r1 = uu.invoke(s, "install", ["-C", file1, file2])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  var file2_meta = fs.stat(uu.at(s, file2), follow_symlinks: true)?
  let before = file2_meta.mtime_ns
  time.sleep(100ms)?
  let r2 = uu.invoke(s, "install", ["-C", file1, file2, "-m", "1644"])?
  uu.succeeds(r2)
  uu.stderr_contains(r2, "the --compare (-C) option is ignored when you specify a mode with non-permission bits")
  file2_meta = fs.stat(uu.at(s, file2), follow_symlinks: true)?
  let after_install_sticky = file2_meta.mtime_ns
  assert before != after_install_sticky
  time.sleep(100ms)?
  let r3 = uu.invoke(s, "install", ["-C", file1, file2])?
  uu.succeeds(r3)
  uu.no_stderr(r3)
  file2_meta = fs.stat(uu.at(s, file2), follow_symlinks: true)?
  let after_install_sticky_again = file2_meta.mtime_ns
  assert after_install_sticky != after_install_sticky_again
}

# origin: uutils test_install::test_install_creating_leading_dirs
test test_uu_install_install_creating_leading_dirs { |ctx|
  let s = uu.scene(ctx)?
  let source = "create_leading_test_file"
  let target = "dir1/dir2/dir3/test_file"
  uu.touch(s, source)?
  let r1 = uu.invoke(s, "install", ["-D", source, uu.at(s, target).display()])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, target)?
}

# origin: uutils test_install::test_install_creating_leading_dirs_with_multiple_sources_and_target_dir
test test_uu_install_install_creating_leading_dirs_with_multiple_sources_and_target_dir { |ctx|
  let s = uu.scene(ctx)?
  let source1 = "source_file_1"
  let source2 = "source_file_2"
  let target_dir = "missing_target_dir"
  uu.touch(s, source1)?
  uu.touch(s, source2)?
  let r1 = uu.invoke(s, "install", ["-D", source1, source2, uu.at(s, target_dir).display()])?
  uu.fails(r1)
  uu.stderr_contains(r1, "missing_target_dir': No such file or directory")
  assert !dir_exists(s, target_dir)?
  let r2 = uu.invoke(s, "install", ["-D", source1, source2, "-t", uu.at(s, target_dir).display()])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  assert dir_exists(s, target_dir)?
}

# origin: uutils test_install::test_install_creating_leading_dirs_with_single_source_and_target_dir
test test_uu_install_install_creating_leading_dirs_with_single_source_and_target_dir { |ctx|
  let s = uu.scene(ctx)?
  let source1 = "source_file_1"
  let target_dir = "missing_target_dir/"
  uu.touch(s, source1)?
  let r1 = uu.invoke(s, "install", ["-D", source1, uu.at(s, target_dir).display()])?
  uu.fails(r1)
  uu.stderr_contains(r1, "missing_target_dir/': Not a directory")
  assert !dir_exists(s, target_dir)?
  let r2 = uu.invoke(s, "install", ["-D", source1, "-t", uu.at(s, target_dir).display()])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  assert file_exists(s, f"{target_dir}/{source1}")?
}

# origin: uutils test_install::test_install_dev_full_as_dst
test test_uu_install_install_dev_full_as_dst { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "install", ["/dev/null", "/dev/full"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot remove '/dev/full'")
}

# origin: uutils test_install::test_install_dir
test test_uu_install_install_dir { |ctx|
  let s = uu.scene(ctx)?
  let dir = "target_dir"
  let file1 = "source_file1"
  let file2 = "source_file2"
  uu.touch(s, file1)?
  uu.touch(s, file2)?
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "install", [file1, file2, f"--target-directory={dir}"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file1)?
  assert file_exists(s, file2)?
  assert file_exists(s, f"{dir}/{file1}")?
  assert file_exists(s, f"{dir}/{file2}")?
}

# origin: uutils test_install::test_install_dir_dot
test test_uu_install_install_dir_dot { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "install", ["-d", "dir1/."])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "install", ["-d", "dir2/.."])?
  uu.succeeds(r2)
  let r3 = uu.invoke(s, "install", ["-d", "dir3/.", "-v"])?
  uu.succeeds(r3)
  uu.stdout_contains(r3, "creating directory 'dir3'")
  let r4 = uu.invoke(s, "install", ["-d", "dir4/./cal", "-v"])?
  uu.succeeds(r4)
  uu.stdout_contains(r4, "creating directory 'dir4/./cal'")
  let r5 = uu.invoke(s, "install", ["-d", "dir5/./cali/.", "-v"])?
  uu.succeeds(r5)
  uu.stdout_contains(r5, "creating directory 'dir5/./cali'")
  let r6 = uu.invoke(s, "install", ["-d", "dir6/./", "-v"])?
  uu.succeeds(r6)
  uu.stdout_contains(r6, "creating directory 'dir6'")
  assert dir_exists(s, "dir1")?
  assert dir_exists(s, "dir2")?
  assert dir_exists(s, "dir3")?
  assert dir_exists(s, "dir4/cal")?
  assert dir_exists(s, "dir5/cali")?
  assert dir_exists(s, "dir6")?
}

# origin: uutils test_install::test_install_dir_req_verbose
test test_uu_install_install_dir_req_verbose { |ctx|
  let s = uu.scene(ctx)?
  let file_1 = "source_file1"
  uu.touch(s, file_1)?
  let r1 = uu.invoke(s, "install", ["-Dv", file_1, "sub3/a/b/c/file"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "install: creating directory 'sub3'\ninstall: creating directory 'sub3/a'\ninstall: creating directory 'sub3/a/b'\ninstall: creating directory 'sub3/a/b/c'\n'source_file1' -> 'sub3/a/b/c/file'")
  let r2 = uu.invoke(s, "install", ["-t", "sub4/a", "-Dv", file_1])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "install: creating directory 'sub4'\ninstall: creating directory 'sub4/a'\n'source_file1' -> 'sub4/a/source_file1'")
  uu.mkdir(s, "sub5")?
  let r3 = uu.invoke(s, "install", ["-Dv", file_1, "sub5/a/b/c/file"])?
  uu.succeeds(r3)
  uu.stdout_contains(r3, "install: creating directory 'sub5/a'\ninstall: creating directory 'sub5/a/b'\ninstall: creating directory 'sub5/a/b/c'\n'source_file1' -> 'sub5/a/b/c/file'")
}

# origin: uutils test_install::test_install_dir_with_existing_file
test test_uu_install_install_dir_with_existing_file { |ctx|
  let s = uu.scene(ctx)?
  let newdir1 = "newdir1"
  let existing_file = "existing_file"
  let newdir2 = "newdir2"
  uu.touch(s, existing_file)?
  let r1 = uu.invoke(s, "install", ["-d", newdir1, existing_file, newdir2])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot create directory 'existing_file': File exists")
  assert dir_exists(s, newdir1)?
  assert !dir_exists(s, existing_file)?
  assert dir_exists(s, newdir2)?
}

# origin: uutils test_install::test_install_dir_with_multiple_existing_files
test test_uu_install_install_dir_with_multiple_existing_files { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "file1"
  let file2 = "file2"
  uu.touch(s, file1)?
  uu.touch(s, file2)?
  let r1 = uu.invoke(s, "install", ["-d", file1, file2])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot create directory 'file1': File exists")
  uu.stderr_contains(r1, "cannot create directory 'file2': File exists")
  assert file_exists(s, file1)?
  assert file_exists(s, file2)?
}

# origin: uutils test_install::test_install_failing_copy_file_to_target_contain_subdir_with_same_name
test test_uu_install_install_failing_copy_file_to_target_contain_subdir_with_same_name { |ctx|
  let s = uu.scene(ctx)?
  let file = "file"
  let dir1 = "dir1"
  uu.touch(s, file)?
  uu.mkdir_all(s, f"{dir1}/{file}")?
  let r1 = uu.invoke(s, "install", [file, dir1])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot overwrite directory")
}

# origin: uutils test_install::test_install_failing_no_such_file
test test_uu_install_install_failing_no_such_file { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "source_file"
  let file2 = "inexistent_file"
  let dir1 = "target_dir"
  uu.mkdir(s, dir1)?
  uu.touch(s, file1)?
  let r1 = uu.invoke(s, "install", [file1, file2, dir1])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "No such file or directory")
}

# origin: uutils test_install::test_install_failing_not_dir
test test_uu_install_install_failing_not_dir { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "file1"
  let file2 = "file2"
  let file3 = "file3"
  uu.touch(s, file1)?
  uu.touch(s, file2)?
  uu.touch(s, file3)?
  let r1 = uu.invoke(s, "install", [file1, file2, file3])?
  uu.fails(r1)
  uu.stderr_contains(r1, "Not a directory")
}

# origin: uutils test_install::test_install_failing_omitting_directory
test test_uu_install_install_failing_omitting_directory { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "file1"
  let dir1 = "dir1"
  let no_dir2 = "no-dir2"
  let dir3 = "dir3"
  uu.mkdir(s, dir1)?
  uu.mkdir(s, dir3)?
  uu.touch(s, file1)?
  let r1 = uu.invoke(s, "install", [file1, dir1, no_dir2])?
  uu.fails(r1)
  uu.stderr_contains(r1, "target 'no-dir2': No such file or directory")
  let r2 = uu.invoke(s, "install", [file1, dir1, dir3])?
  uu.fails_with_code(r2, 1)
  uu.stderr_contains(r2, "omitting directory")
  assert file_exists(s, f"{dir3}/{file1}")?
  let r3 = uu.invoke(s, "install", [dir1, dir3])?
  uu.fails_with_code(r3, 1)
  uu.stderr_contains(r3, "omitting directory")
}

# origin: uutils test_install::test_install_from_stdin
test test_uu_install_install_from_stdin { |ctx|
  let s = uu.scene(ctx)?
  let target = "target"
  let test_string = "Hello, World!\n"
  let r1 = uu.invoke(s, "install", ["/dev/fd/0", target], stdin: bytes.from_text(test_string))?
  uu.succeeds(r1)
  assert file_exists(s, target)?
  assert uu.read_text(s, target)? == test_string
}

# origin: uutils test_install::test_install_missing_arguments
test test_uu_install_install_missing_arguments { |ctx|
  let s = uu.scene(ctx)?
  let no_target_dir = "no-target_dir"
  let r1 = uu.invoke(s, "install", [])?
  uu.fails_with_code(r1, 1)
  usage_error(r1, "missing file operand")
  let r2 = uu.invoke(s, "install", ["-D", f"-t {no_target_dir}"])?
  uu.fails(r2)
  usage_error(r2, "missing file operand")
  assert !dir_exists(s, no_target_dir)?
}

# origin: uutils test_install::test_install_missing_destination
test test_uu_install_install_missing_destination { |ctx|
  let s = uu.scene(ctx)?
  let file_1 = "source_file1"
  let dir_1 = "source_dir1"
  uu.touch(s, file_1)?
  uu.mkdir(s, dir_1)?
  let r1 = uu.invoke(s, "install", [file_1])?
  uu.fails(r1)
  usage_error(r1, f"missing destination file operand after '{file_1}'")
  let r2 = uu.invoke(s, "install", [dir_1])?
  uu.fails(r2)
  usage_error(r2, f"missing destination file operand after '{dir_1}'")
}

# origin: uutils test_install::test_install_missing_source_reports_cannot_stat_with_path
test test_uu_install_install_missing_source_reports_cannot_stat_with_path { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "install", ["missing_source", "target_file"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "cannot stat 'missing_source': No such file or directory")
}

# origin: uutils test_install::test_install_mode_comma_separated
test test_uu_install_install_mode_comma_separated { |ctx|
  let s = uu.scene(ctx)?
  let dir = "target_dir"
  let file = "source_file"
  let mode_arg = "--mode=ug+rwX,o+rX"
  uu.touch(s, file)?
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "install", [file, dir, mode_arg])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  let dest_file = f"{dir}/{file}"
  assert file_exists(s, file)?
  assert file_exists(s, dest_file)?
  let permissions = fs.stat(uu.at(s, dest_file), follow_symlinks: true)?
  assert 0o100664 == permissions.mode
}

# origin: uutils test_install::test_install_mode_comma_separated_directory
test test_uu_install_install_mode_comma_separated_directory { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_dir"
  let mode_arg = "--mode=ug+rwX,o+rX"
  let r1 = uu.invoke(s, "install", ["-d", dir, mode_arg])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert dir_exists(s, dir)?
  let permissions = fs.stat(uu.at(s, dir), follow_symlinks: true)?
  assert 0o040775 == permissions.mode
}

# origin: uutils test_install::test_install_mode_directories
test test_uu_install_install_mode_directories { |ctx|
  let s = uu.scene(ctx)?
  let component = "component"
  let directories_arg = "-d"
  let mode_arg = "--mode=333"
  let r1 = uu.invoke(s, "install", [directories_arg, component, mode_arg])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert dir_exists(s, component)?
  let permissions = fs.stat(uu.at(s, component), follow_symlinks: true)?
  assert 0o040333 == permissions.mode
}

# origin: uutils test_install::test_install_mode_failing
test test_uu_install_install_mode_failing { |ctx|
  let s = uu.scene(ctx)?
  let dir = "target_dir"
  let file = "source_file"
  let mode_arg = "--mode=999"
  uu.touch(s, file)?
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "install", [file, dir, mode_arg])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid mode '999'")
  let dest_file = f"{dir}/{file}"
  assert file_exists(s, file)?
  assert !file_exists(s, dest_file)?
}

# origin: uutils test_install::test_install_mode_numeric
test test_uu_install_install_mode_numeric { |ctx|
  let s = uu.scene(ctx)?
  let dir = "dir1"
  let dir2 = "dir2"
  let file = "file"
  let mode_arg = "--mode=333"
  uu.touch(s, file)?
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "install", [file, dir, mode_arg])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  let dest_file = f"{dir}/{file}"
  assert file_exists(s, file)?
  assert file_exists(s, dest_file)?
  let permissions = fs.stat(uu.at(s, dest_file), follow_symlinks: true)?
  assert 0o100333 == permissions.mode
  let mode_arg2 = "-m 0333"
  uu.mkdir(s, dir2)?
  let r2 = uu.invoke(s, "install", [mode_arg2, file, dir2])?
  uu.fails_with_code(r2, 1)
  uu.stderr_is(r2, "install: invalid mode ' 0333'\n")
  assert file_exists(s, file)?
  assert !file_exists(s, f"{dir2}/{file}")?
}

# origin: uutils test_install::test_install_mode_symbolic
test test_uu_install_install_mode_symbolic { |ctx|
  let s = uu.scene(ctx)?
  let dir = "target_dir"
  let file = "source_file"
  let mode_arg = "--mode=o+wx"
  uu.touch(s, file)?
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "install", [file, dir, mode_arg])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  let dest_file = f"{dir}/{file}"
  assert file_exists(s, file)?
  assert file_exists(s, dest_file)?
  let permissions = fs.stat(uu.at(s, dest_file), follow_symlinks: true)?
  assert 0o100003 == permissions.mode
}

# origin: uutils test_install::test_install_mode_symbolic_ignore_umask
test test_uu_install_install_mode_symbolic_ignore_umask { |ctx|
  let s = uu.scene(ctx)?
  let dir = "target_dir"
  let file = "source_file"
  let mode_arg = "--mode=+w"
  uu.touch(s, file)?
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "install", [file, dir, mode_arg], umask: 0o022)?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  let dest_file = f"{dir}/{file}"
  assert file_exists(s, file)?
  assert file_exists(s, dest_file)?
  let permissions = fs.stat(uu.at(s, dest_file), follow_symlinks: true)?
  assert 0o100222 == permissions.mode
}

# origin: uutils test_install::test_install_nested_paths_copy_file
test test_uu_install_install_nested_paths_copy_file { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "source_file"
  let dir1 = "source_dir"
  let dir2 = "target_dir"
  uu.mkdir(s, dir1)?
  uu.mkdir(s, dir2)?
  uu.touch(s, f"{dir1}/{file1}")?
  let r1 = uu.invoke(s, "install", [f"{dir1}/{file1}", dir2])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, f"{dir2}/{file1}")?
}

# The script must reach disk before another process executes it.
proc write_strip(s: uu.Scene, name: Str, content: Str) [fs, error] -> Result[Unit, Error] {
  uu.write(s, name, content)?
  fs.fsync(uu.at(s, name))?
  uu.set_mode(s, name, 0o755)?
  Ok()
}

# origin: uutils test_install::test_install_and_strip
test test_uu_install_install_and_strip { |ctx|
  let s = uu.scene(ctx)?
  write_strip(s, "strip", "#!/bin/sh\n: > \"$1\"\n")?
  uu.write(s, "source", "file contents")?
  let r = uu.invoke(s, "install", ["-s", "source", "helloworld_installed"], vars: {PATH: f"{s.root}:{env.get("PATH")?}"})?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.size(s, "helloworld_installed")? == 0
}

# origin: uutils test_install::test_install_and_strip_with_program
test test_uu_install_install_and_strip_with_program { |ctx|
  let s = uu.scene(ctx)?
  write_strip(s, "strip-program", "#!/bin/sh\n: > \"$1\"\n")?
  uu.write(s, "source", "file contents")?
  let r = uu.invoke(s, "install", ["-s", "--strip-program", "./strip-program", "source", "helloworld_installed"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.size(s, "helloworld_installed")? == 0
}

# origin: uutils test_install::test_install_and_strip_with_program_hyphen
test test_uu_install_install_and_strip_with_program_hyphen { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "no-hyphen", "#!/bin/sh\n    printf -- '%s\\n' \"$1\" | grep '^[^-]'\n    ")?
  uu.succeeds(uu.invoke(s, "chmod", ["+x", "no-hyphen"])?)
  uu.touch(s, "src")?
  let first = uu.invoke(s, "install", ["-s", "--strip-program", "./no-hyphen", "--", "src", "-dest"])?
  uu.succeeds(first)
  uu.no_stderr(first)
  uu.stdout_is(first, "./-dest\n")
  let second = uu.invoke(s, "install", ["-s", "--strip-program", "./no-hyphen", "--", "src", "./-dest"])?
  uu.succeeds(second)
  uu.no_stderr(second)
  uu.stdout_is(second, "./-dest\n")
}

# origin: uutils test_install::test_install_and_strip_with_invalid_program
test test_uu_install_install_and_strip_with_invalid_program { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "install", ["-s", "--strip-program", "/bin/date", ctx.xsh_bin.display(), "helloworld_installed"])?
  uu.fails(r)
  uu.stderr_contains(r, "strip process terminated abnormally")
  assert !file_exists(s, "helloworld_installed")?
}

# origin: uutils test_install::test_install_and_strip_with_signal_terminated_program
test test_uu_install_install_and_strip_with_signal_terminated_program { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "src.sh", "kill -9 $$\n")?
  let r = uu.invoke(s, "install", ["-s", "--strip-program", "/bin/sh", "src.sh", "helloworld_installed"])?
  uu.fails(r)
  uu.stderr_only(r, "install: strip process terminated abnormally\n")
  assert !file_exists(s, "helloworld_installed")?
}

# origin: uutils test_install::test_install_and_strip_with_non_existent_program
test test_uu_install_install_and_strip_with_non_existent_program { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "install", ["-s", "--strip-program", "/usr/bin/non_existent_program", ctx.xsh_bin.display(), "helloworld_installed"])?
  uu.fails(r)
  uu.stderr_only(r, "install: cannot run strip program '/usr/bin/non_existent_program': No such file or directory\n")
  assert !file_exists(s, "helloworld_installed")?
}

# origin: uutils test_install::test_install_compare_preserve_timestamps
test test_uu_install_install_compare_preserve_timestamps { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "source_file", "data")?
  uu.write(s, "dest_file", "data")?
  fs.set_times(uu.at(s, "source_file"), mtime_sec: 1000000)?
  let r = uu.invoke(s, "install", ["-C", "--preserve-timestamps", "source_file", "dest_file"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "source_file"))?.mtime_ns == fs.stat(uu.at(s, "dest_file"))?.mtime_ns
}

# origin: uutils test_install::test_install_creating_leading_dirs_verbose
test test_uu_install_install_creating_leading_dirs_verbose { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "create_leading_test_file")?
  uu.mkdir(s, "dir1")?
  let r = uu.invoke(s, "install", ["-Dv", "create_leading_test_file", uu.at(s, "dir1/no-dir2/no-dir3/test_file").display()])?
  uu.succeeds(r)
  assert regex.compile("(?m)^install: creating directory.*no-dir[23]'$")?.matches(r.stdout.utf8()?)
  assert !regex.compile("(?m)^install: creating directory.*dir1'$")?.matches(r.stdout.utf8()?)
  uu.no_stderr(r)
  assert file_exists(s, "dir1/no-dir2/no-dir3/test_file")?
}

# origin: uutils test_install::test_install_creating_leading_dir_fails_on_long_name
test test_uu_install_install_creating_leading_dir_fails_on_long_name { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "create_leading_test_file")?
  let target = ["d" for _ in range(4097)].join("") + "/test_file"
  let r = uu.invoke(s, "install", ["-D", "create_leading_test_file", uu.at(s, target).display()])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot create directory")
}

# origin: uutils test_install::test_install_compare_group_ownership
test test_uu_install_install_compare_group_ownership { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "source_file", "test content")?
  let group_result = uu.invoke(s, "id", ["-nrg"])?
  uu.succeeds(group_result)
  let user_group = group_result.stdout.utf8()?.trim()
  let first = uu.invoke(s, "install", ["-Cv", "-m664", "-g", user_group, "source_file", "dest_file"])?
  uu.succeeds(first)
  uu.stdout_contains(first, "'source_file' -> 'dest_file'")
  let second = uu.invoke(s, "install", ["-Cv", "-m664", "source_file", "dest_file"])?
  uu.succeeds(second)
  uu.no_stdout(second)
}

# origin: uutils test_install::test_install_compare_with_mode_bits
test test_uu_install_install_compare_with_mode_bits { |ctx|
  for mode in ["4755", "2755", "1755", "7755", "755"] {
    let s = uu.scene(ctx)?
    let source = f"source_file_{mode}"
    let dest = f"dest_file_{mode}"
    uu.write(s, source, "test content")?
    let r = uu.invoke(s, "install", ["-C", f"--mode={mode}", source, dest])?
    uu.succeeds(r)
    if mode != "755" {
      uu.stderr_contains(r, "the --compare (-C) option is ignored when you specify a mode with non-permission bits")
    } else {
      uu.no_stderr(r)
      let second = uu.invoke(s, "install", ["-C", f"--mode={mode}", source, dest])?
      uu.succeeds(second)
      uu.no_stderr(second)
    }
    assert file_exists(s, dest)?
  }
}

# origin: uutils test_install::test_install_failed_chown_does_not_leave_setuid
test test_uu_install_install_failed_chown_does_not_leave_setuid { |ctx|
  if applet.current_euid() == 0 { test.skip("chown must be able to fail") }
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  let file = uu.invoke(s, "install", ["-m", "4755", "-o", "root", "src", "dst"])?
  uu.fails(file)
  if file_exists(s, "dst")? { assert uu.mode(s, "dst")?.bit_and(0o6000) == 0 }
  let directory = uu.invoke(s, "install", ["-d", "-m", "4755", "-o", "root", "newdir"])?
  uu.fails(directory)
  if dir_exists(s, "newdir")? { assert uu.mode(s, "newdir")?.bit_and(0o6000) == 0 }
}

# origin: uutils test_install::test_install_d_symlink_in_path_is_followed
test test_uu_install_install_d_symlink_in_path_is_followed { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "target")?
  uu.write(s, "source_file", "test content")?
  uu.mkdir_all(s, "testdir/a")?
  uu.at(s, "testdir/a/b").symlink(to: uu.at(s, "target"))?
  let r = uu.invoke(s, "install", ["-D", uu.at(s, "source_file").display(), uu.at(s, "testdir/a/b/c/file").display()])?
  uu.succeeds(r)
  assert uu.exists(s, "target/c/file")?
  uu.file_is(s, "target/c/file", "test content")?
  assert uu.is_symlink(s, "testdir/a/b")?
}

# origin: uutils test_install::test_install_d_follows_symlink_prefix
test test_uu_install_install_d_follows_symlink_prefix { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "target")?
  uu.at(s, "link").symlink(to: uu.at(s, "target"))?
  uu.write(s, "file.txt", "hello")?
  let r = uu.invoke(s, "install", ["-D", "-m", "644", uu.at(s, "file.txt").display(), uu.at(s, "link/subdir/file.txt").display()])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "link")?
  assert uu.exists(s, "target/subdir/file.txt")?
  uu.file_is(s, "target/subdir/file.txt", "hello")?
}

# origin: uutils test_install::test_install_d_dangling_symlink_in_path_errors
test test_uu_install_install_d_dangling_symlink_in_path_errors { |ctx|
  let s = uu.scene(ctx)?
  uu.at(s, "dangling").symlink(to: uu.at(s, "nonexistent"))?
  assert uu.is_symlink(s, "dangling")?
  uu.write(s, "file.txt", "hello")?
  let r = uu.invoke(s, "install", ["-D", "-m", "644", uu.at(s, "file.txt").display(), uu.at(s, "dangling/subdir/file.txt").display()])?
  uu.fails(r)
  assert uu.is_symlink(s, "dangling")?
  assert !uu.exists(s, "nonexistent")?
}

# origin: uutils test_install::test_install_d_parallel_mkdir_race
test test_uu_install_install_d_parallel_mkdir_race { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "s")?
  for round in range(10) {
    var children: List[ProcessHandle] = []
    for k in range(32) {
      let command = uu.command(s, "install", ["-D", "s", f"o{round}/q/f{k}"],
        stdout: uu.at(s, f"out{k}"), stderr: uu.at(s, f"err{k}"), timeout: 10s)?
      children += [spawn command?]
    }
    for k in range(32) {
      assert (wait children[k]?).exited_with(0)
      assert uu.read(s, f"err{k}")? == b""
      assert file_exists(s, f"o{round}/q/f{k}")?
    }
  }
}

# origin: uutils test_install::test_install_from_fifo
test test_uu_install_install_from_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "pipe")?
  assert fs.stat(uu.at(s, "pipe"))?.kind == "fifo"
  let command = uu.command(s, "install", ["pipe", "target"], timeout: 5s)?
  let child = spawn command?
  defer child.cancel(signal: "KILL", kill_after: 0ms)?
  # Opening the writer supplies exactly one stream and closes it at completion.
  let writer_command = process.command_argv(p"/bin/sh",
    ["sh", "-c", "printf '%s' \"$1\" > \"$2\"", "writer", "Hello, world!\n", uu.at(s, "pipe").display()],
    s.root, {}, b"", uu.at(s, "writer-out"), uu.at(s, "writer-err"), timeout: 5s)
  let writer = spawn writer_command?
  defer writer.cancel(signal: "KILL", kill_after: 0ms)?
  let _ = wait child?
  let _ = wait writer?
  assert file_exists(s, "target")?
  uu.file_is(s, "target", "Hello, world!\n")?
}

# origin: uutils test_install::test_install_backup_error_includes_cause
test test_uu_install_install_backup_error_includes_cause { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "source")?
  uu.touch(s, "target")?
  uu.mkdir(s, "target.backup")?
  let r = uu.invoke(s, "install", ["--backup", "source", "target"], vars: {SIMPLE_BACKUP_SUFFIX: ".backup"})?
  uu.fails(r)
  uu.stderr_is(r, "install: cannot backup 'target': Is a directory\n")
}

# origin: uutils test_install::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_install_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "source")?
  let r = uu.invoke(s, "install", ["-m", "u+rw?x", "source", "dest"])?
  uu.fails_with_code(r, 1)
  let stderr = r.stderr.utf8()?
  assert stderr.starts_with("install: ")
  assert !(":1:" in stderr)
}
