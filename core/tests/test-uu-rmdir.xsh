##! Transcribed directory removal tests from the uutils coreutils suite.

use support.uu as uu

# A removed directory yields ENOENT, which the upstream existence predicate
# represents as false; other host failures remain errors.
proc dir_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  match fs.stat(uu.at(s, name), follow_symlinks: true) {
    Ok(meta) => Ok(meta.kind == "dir"),
    Err(failure) => { if failure.errno == 2 { Ok(false) } else { Err(failure) } },
  }
}

# origin: uutils test_rmdir::test_invalid_arg
test test_uu_rmdir_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "rmdir", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_rmdir::test_rmdir_empty_directory_no_parents
test test_uu_rmdir_rmdir_empty_directory_no_parents { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  let r = uu.invoke(s, "rmdir", ["dir"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! dir_exists(s, "dir")?
}

# origin: uutils test_rmdir::test_rmdir_nonempty_directory_no_parents
test test_uu_rmdir_rmdir_nonempty_directory_no_parents { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/file")?
  let r = uu.invoke(s, "rmdir", ["dir"])?
  uu.fails(r)
  uu.stderr_is(r, "rmdir: failed to remove 'dir': Directory not empty\n")
  assert dir_exists(s, "dir")?
}

# origin: uutils test_rmdir::test_rmdir_ignore_nonempty_directory_no_parents
test test_uu_rmdir_rmdir_ignore_nonempty_directory_no_parents { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/file")?
  let r = uu.invoke(s, "rmdir", ["--ignore-fail-on-non-empty", "dir"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert dir_exists(s, "dir")?
}

# origin: uutils test_rmdir::test_rmdir_empty_directory_with_parents
test test_uu_rmdir_rmdir_empty_directory_with_parents { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "dir/ect/ory")?
  let r = uu.invoke(s, "rmdir", ["-p", "dir/ect/ory"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! dir_exists(s, "dir/ect/ory")?
  assert ! dir_exists(s, "dir")?
}

# origin: uutils test_rmdir::test_rmdir_nonempty_directory_with_parents
test test_uu_rmdir_rmdir_nonempty_directory_with_parents { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "dir/ect/ory")?
  uu.touch(s, "dir/ect/ory/file")?
  let r = uu.invoke(s, "rmdir", ["-p", "dir/ect/ory"])?
  uu.fails(r)
  uu.stderr_is(r, "rmdir: failed to remove 'dir/ect/ory': Directory not empty\n")
  assert dir_exists(s, "dir/ect/ory")?
}

# origin: uutils test_rmdir::test_rmdir_ignore_nonempty_directory_with_parents
test test_uu_rmdir_rmdir_ignore_nonempty_directory_with_parents { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "dir/ect/ory")?
  uu.touch(s, "dir/ect/ory/file")?
  let r = uu.invoke(s, "rmdir", ["--ignore-fail-on-non-empty", "-p", "dir/ect/ory"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert dir_exists(s, "dir/ect/ory")?
}

# origin: uutils test_rmdir::test_rmdir_ignore_nonempty_no_permissions
test test_uu_rmdir_rmdir_ignore_nonempty_no_permissions { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "dir/ect/ory")?
  uu.touch(s, "dir/ect/ory/file")?
  uu.set_mode(s, "dir/ect", 0o555)?
  defer uu.set_mode(s, "dir/ect", 0o755)
  let r = uu.invoke(s, "rmdir", ["--ignore-fail-on-non-empty", "dir/ect/ory"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert dir_exists(s, "dir/ect/ory")?
  uu.set_mode(s, "dir/ect", 0o755)?
}

# origin: uutils test_rmdir::test_rmdir_not_a_directory
test test_uu_rmdir_rmdir_not_a_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let r = uu.invoke(s, "rmdir", ["--ignore-fail-on-non-empty", "file"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_is(r, "rmdir: failed to remove 'file': Not a directory\n")
}

# origin: uutils test_rmdir::test_verbose_single
test test_uu_rmdir_verbose_single { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  let r = uu.invoke(s, "rmdir", ["-v", "dir"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "rmdir: removing directory, 'dir'\n")
}

# origin: uutils test_rmdir::test_verbose_multi
test test_uu_rmdir_verbose_multi { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  let r = uu.invoke(s, "rmdir", ["-v", "does_not_exist", "dir"])?
  uu.fails(r)
  uu.stdout_is(r, "rmdir: removing directory, 'does_not_exist'\nrmdir: removing directory, 'dir'\n")
  uu.stderr_is(r, "rmdir: failed to remove 'does_not_exist': No such file or directory\n")
}

# origin: uutils test_rmdir::test_rmdir_remove_symlink_file
test test_uu_rmdir_rmdir_remove_symlink_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, uu.at(s, "file").display(), "fl")?
  let r = uu.invoke(s, "rmdir", ["fl/"])?
  uu.fails(r)
  uu.stderr_is(r, "rmdir: failed to remove 'fl/': Not a directory\n")
}

# origin: uutils test_rmdir::test_rmdir_remove_symlink_dir
test test_uu_rmdir_rmdir_remove_symlink_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.symlink(s, uu.at(s, "dir").display(), "dl")?
  let r = uu.invoke(s, "rmdir", ["dl/"])?
  uu.fails(r)
  uu.stderr_is(r, "rmdir: failed to remove 'dl/': Symbolic link not followed\n")
}

# origin: uutils test_rmdir::test_rmdir_remove_symlink_dangling
test test_uu_rmdir_rmdir_remove_symlink_dangling { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, uu.at(s, "dir").display(), "dl")?
  let r = uu.invoke(s, "rmdir", ["dl/"])?
  uu.fails(r)
  uu.stderr_is(r, "rmdir: failed to remove 'dl/': Symbolic link not followed\n")
}

# origin: uutils test_rmdir::test_rmdir_remove_symlink_dir_with_trailing_slashes
test test_uu_rmdir_rmdir_remove_symlink_dir_with_trailing_slashes { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.symlink(s, uu.at(s, "dir").display(), "dl")?
  let r = uu.invoke(s, "rmdir", ["dl////"])?
  uu.fails(r)
  uu.stderr_is(r, "rmdir: failed to remove 'dl////': Symbolic link not followed\n")
}
