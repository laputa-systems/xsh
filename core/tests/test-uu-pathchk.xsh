##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_pathchk.rs.

use support.uu as uu

# Linux musl defines both PATH_MAX and FILENAME_MAX as 4096.
pure long_path() -> Str {
  ["dir" for _ in range(4097)].join("")
}

pure long_filename() -> Str {
  "dir/" + ["file" for _ in range(4097)].join("")
}

# origin: uutils test_pathchk::test_no_args
test test_uu_pathchk_no_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pathchk", [])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "missing operand")
}

# origin: uutils test_pathchk::test_invalid_arg
test test_uu_pathchk_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "pathchk", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_pathchk::test_default_mode
test test_uu_pathchk_default_mode { |ctx|
  let s = uu.scene(ctx)?
  for name in ["dir/file", "dir#/$file"] {
    let r = uu.invoke(s, "pathchk", [name])?
    uu.succeeds(r)
    uu.no_stdout(r)
  }
  let empty = uu.invoke(s, "pathchk", [""])?
  uu.fails(empty)
  uu.stderr_only(empty, "pathchk: '': No such file or directory\n")
  let twice = uu.invoke(s, "pathchk", ["", ""])?
  uu.fails(twice)
  uu.stderr_only(twice, "pathchk: '': No such file or directory\npathchk: '': No such file or directory\n")
  for name in [long_path(), long_filename()] {
    let r = uu.invoke(s, "pathchk", [name])?
    uu.fails(r)
    uu.no_stdout(r)
  }
}

# origin: uutils test_pathchk::test_posix_mode
test test_uu_pathchk_posix_mode { |ctx|
  let s = uu.scene(ctx)?
  let good = uu.invoke(s, "pathchk", ["-p", "dir/file"])?
  uu.succeeds(good)
  uu.no_stdout(good)
  for name in [long_path(), long_filename(), "dir#/$file"] {
    let r = uu.invoke(s, "pathchk", ["-p", name])?
    uu.fails(r)
    uu.no_stdout(r)
  }
}

# origin: uutils test_pathchk::test_posix_special
test test_uu_pathchk_posix_special { |ctx|
  let s = uu.scene(ctx)?
  for name in ["dir/file", "dir#/$file", "dir/file-name"] {
    let r = uu.invoke(s, "pathchk", ["-P", name])?
    uu.succeeds(r)
    uu.no_stdout(r)
  }
  for name in [long_path(), long_filename(), "dir/-file", ""] {
    let r = uu.invoke(s, "pathchk", ["-P", name])?
    uu.fails(r)
    uu.no_stdout(r)
  }
}

# origin: uutils test_pathchk::test_posix_all
test test_uu_pathchk_posix_all { |ctx|
  let s = uu.scene(ctx)?
  for name in ["dir/file", "dir/file-name"] {
    let r = uu.invoke(s, "pathchk", ["-p", "-P", name])?
    uu.succeeds(r)
    uu.no_stdout(r)
  }
  for name in [long_path(), long_filename(), "dir#/$file", "dir/-file"] {
    let r = uu.invoke(s, "pathchk", ["-p", "-P", name])?
    uu.fails(r)
    uu.no_stdout(r)
  }
  let empty = uu.invoke(s, "pathchk", ["-p", "-P", ""])?
  uu.fails(empty)
  uu.stderr_only(empty, "pathchk: empty file name\n")
}

# origin: uutils test_pathchk::test_empty_path_portability_message
test test_uu_pathchk_empty_path_portability_message { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-p", "-P", "--portability"] {
    let r = uu.invoke(s, "pathchk", [flag, ""])?
    uu.fails(r)
    uu.stderr_only(r, "pathchk: empty file name\n")
  }
}

# origin: uutils test_pathchk::test_pathchk_non_utf8_paths
test test_uu_pathchk_pathchk_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let filename = Path.parse_bytes(b"\xff\xfe")?
  uu.succeeds(uu.invoke_paths(s, "pathchk", [filename])?)
}

# origin: uutils test_pathchk::test_not_a_directory_clean
test test_uu_pathchk_not_a_directory_clean { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pathchk", ["/dev/full/"])?
  uu.fails(r)
  uu.stderr_is(r, "pathchk: /dev/full/: Not a directory\n")
}
