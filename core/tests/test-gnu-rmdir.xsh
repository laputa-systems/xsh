use support.uu as uu

# origin: gnu rmdir/fail-perm.log
test test_gnu_rmdir_fail_perm_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d/e/f")?
  uu.set_mode(s, "d", 0o555)?
  defer uu.set_mode(s, "d", 0o755)?
  let result = uu.invoke(s, "rmdir", ["-p", "d", "d/e/f"], stderr: p"/dev/null")?
  uu.fails_with_code(result, 1)
}

# origin: gnu rmdir/ignore.log
test test_gnu_rmdir_ignore_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b/c")?
  uu.mkdir(s, "a/x")?
  let absolute = uu.at(s, "a/b/c").display()
  uu.succeeds(uu.invoke(s, "rmdir", ["-p", "--ignore-fail-on-non-empty", absolute])?)
  assert uu.dir_exists(s, "a/x")?
  assert ! uu.exists(s, "a/b")?
  assert ! uu.exists(s, "a/b/c")?

  uu.mkdir(s, "x/y")?
  uu.set_mode(s, "x", 0o555)?
  defer uu.set_mode(s, "x", 0o755)?
  uu.fails_with_code(uu.invoke(s, "rmdir", ["--ignore-fail-on-non-empty", "x/y"])?, 1)
  assert uu.dir_exists(s, "x/y")?
  uu.touch(s, "x/y/z")?
  uu.succeeds(uu.invoke(s, "rmdir", ["--ignore-fail-on-non-empty", "x/y"])?)
  assert uu.dir_exists(s, "x/y")?
  uu.remove(s, "x/y/z")?
  uu.set_mode(s, "x/y", 0o311)?
  defer uu.set_mode(s, "x/y", 0o755)?
  uu.fails_with_code(uu.invoke(s, "rmdir", ["--ignore-fail-on-non-empty", "x/y"])?, 1)
  assert uu.dir_exists(s, "x/y")?
}

# origin: gnu rmdir/symlink-errors.log
test test_gnu_rmdir_symlink_errors_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.symlink(s, "dir", "sl")?
  uu.symlink(s, "missing", "dl")?
  uu.touch(s, "file")?
  uu.symlink(s, "file", "fl")?
  let file_link = uu.invoke(s, "rmdir", ["fl/"])?
  uu.fails_with_code(file_link, 1)
  uu.stderr_is(file_link, "rmdir: failed to remove 'fl/': Not a directory\n")
  uu.mkdir(s, "dir/dir2")?
  let ancestor = uu.invoke(s, "rmdir", ["-p", "sl/dir2"])?
  uu.fails_with_code(ancestor, 1)
  uu.stderr_is(ancestor, "rmdir: failed to remove 'sl': Not a directory\n")
  let probe = uu.invoke(s, "rmdir", ["sl/"], stderr: p"/dev/null")?
  if probe.status != 0 {
    for name in ["sl/", "dl/"] {
      let result = uu.invoke(s, "rmdir", [name])?
      uu.fails_with_code(result, 1)
      uu.stderr_is(result, f"rmdir: failed to remove '{name}': Symbolic link not followed\n")
    }
  }
}

# origin: gnu rmdir/t-slash.log
test test_gnu_rmdir_t_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.succeeds(uu.invoke(s, "rmdir", ["-p", "dir/"])?)
}
