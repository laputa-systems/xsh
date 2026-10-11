##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_basename.rs.
##! Each test names its origin; expected values are the original assertions.

use support.uu as uu

# origin: uutils test_basename::test_directory
test test_uu_basename_directory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["/root/alpha/beta/gamma/delta/epsilon/omega/"])?
  uu.succeeds(r)
  uu.stdout_only(r, "omega\n")
}

# origin: uutils test_basename::test_file
test test_uu_basename_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["/etc/passwd"])?
  uu.succeeds(r)
  uu.stdout_only(r, "passwd\n")
}

# origin: uutils test_basename::test_trailing_separators
test test_uu_basename_trailing_separators { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "basename", ["foo/bar"])?
    uu.succeeds(r)
    uu.stdout_only(r, "bar\n")
  }
  {
    let r = uu.invoke(s, "basename", ["foo/bar/"])?
    uu.succeeds(r)
    uu.stdout_only(r, "bar\n")
  }
  {
    let r = uu.invoke(s, "basename", ["foo/bar///"])?
    uu.succeeds(r)
    uu.stdout_only(r, "bar\n")
  }
  {
    let r = uu.invoke(s, "basename", ["foo/./"])?
    uu.succeeds(r)
    uu.stdout_only(r, ".\n")
  }
  {
    let r = uu.invoke(s, "basename", ["foo/.//"])?
    uu.succeeds(r)
    uu.stdout_only(r, ".\n")
  }
  {
    let r = uu.invoke(s, "basename", ["foo/.//"])?
    uu.succeeds(r)
    uu.stdout_only(r, ".\n")
  }
  {
    let r = uu.invoke(s, "basename", ["/"])?
    uu.succeeds(r)
    uu.stdout_only(r, "/\n")
  }
  {
    let r = uu.invoke(s, "basename", ["//"])?
    uu.succeeds(r)
    uu.stdout_only(r, "/\n")
  }
  {
    let r = uu.invoke(s, "basename", ["///"])?
    uu.succeeds(r)
    uu.stdout_only(r, "/\n")
  }
}

# origin: uutils test_basename::test_remove_suffix
test test_uu_basename_remove_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["/usr/local/bin/reallylongexecutable.exe", ".exe"])?
  uu.succeeds(r)
  uu.stdout_only(r, "reallylongexecutable\n")
}

# origin: uutils test_basename::test_do_not_remove_suffix
test test_uu_basename_do_not_remove_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["/foo/bar/baz", "baz"])?
  uu.succeeds(r)
  uu.stdout_only(r, "baz\n")
}

# origin: uutils test_basename::test_multiple_param
test test_uu_basename_multiple_param { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-a", "--multiple", "--mul"] {
    let r = uu.invoke(s, "basename", [flag, "/foo/bar/baz", "/foo/bar/baz"])?
    uu.succeeds(r)
    uu.stdout_only(r, "baz\nbaz\n")
  }
}

# origin: uutils test_basename::test_suffix_param
test test_uu_basename_suffix_param { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-s", "--suffix", "--suf"] {
    let r = uu.invoke(s, "basename", [flag, ".exe", "/foo/bar/baz.exe", "/foo/bar/baz.exe"])?
    uu.succeeds(r)
    uu.stdout_only(r, "baz\nbaz\n")
  }
}

# origin: uutils test_basename::test_zero_param
test test_uu_basename_zero_param { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-z", "--zero", "--ze"] {
    let r = uu.invoke(s, "basename", [flag, "-a", "/foo/bar/baz", "/foo/bar/baz"])?
    uu.succeeds(r)
    uu.stdout_only(r, "baz\0baz\0")
  }
}

# origin: uutils test_basename::test_invalid_option
test test_uu_basename_invalid_option { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["-q", "/foo/bar/baz"])?
  uu.fails(r)
  uu.no_stdout(r)
  assert r.stderr.len() > 0
}

# origin: uutils test_basename::test_no_args
test test_uu_basename_no_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", [])?
  uu.fails(r)
  uu.no_stdout(r)
  assert r.stderr.len() > 0
}

# origin: uutils test_basename::test_too_many_args
test test_uu_basename_too_many_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["a", "b", "c"])?
  uu.fails(r)
  uu.no_stdout(r)
  assert r.stderr.len() > 0
}

# origin: uutils test_basename::test_no_args_output
test test_uu_basename_no_args_output { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", [])?
  uu.fails(r)
  uu.stderr_only(r, "basename: missing operand\nTry 'basename --help' for more information.\n")
}

# origin: uutils test_basename::test_too_many_args_output
test test_uu_basename_too_many_args_output { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["a", "b", "c"])?
  uu.fails(r)
  uu.stderr_only(r, "basename: extra operand 'c'\nTry 'basename --help' for more information.\n")
}

# origin: uutils test_basename::test_invalid_utf8_args
test test_uu_basename_invalid_utf8_args { |ctx|
  let s = uu.scene(ctx)?
  let param = Path.parse_bytes(b"/tmp/some-\xc0-file.k\xf3")?
  let r = uu.invoke_paths(s, "basename", [param])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"some-\xc0-file.k\xf3\n")
  let suffix = Path.parse_bytes(b".k\xf3")?
  let stripped = uu.invoke_paths(s, "basename", [param, suffix])?
  uu.succeeds(stripped)
  uu.stdout_is_bytes(stripped, b"some-\xc0-file\n")
}

# origin: uutils test_basename::test_double_slash
test test_uu_basename_double_slash { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "basename", ["//"])?
    uu.succeeds(r)
    uu.stdout_is(r, "/\n")
  }
  {
    let r = uu.invoke(s, "basename", ["//", "/"])?
    uu.succeeds(r)
    uu.stdout_is(r, "/\n")
  }
  {
    let r = uu.invoke(s, "basename", ["//", "//"])?
    uu.succeeds(r)
    uu.stdout_is(r, "/\n")
  }
}

# origin: uutils test_basename::test_trailing_dot
test test_uu_basename_trailing_dot { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "basename", ["/."])?
    uu.succeeds(r)
    uu.stdout_is(r, ".\n")
  }
  {
    let r = uu.invoke(s, "basename", ["hello/."])?
    uu.succeeds(r)
    uu.stdout_is(r, ".\n")
  }
  {
    let r = uu.invoke(s, "basename", ["/foo/bar/."])?
    uu.succeeds(r)
    uu.stdout_is(r, ".\n")
  }
}

# origin: uutils test_basename::test_simple_format
test test_uu_basename_simple_format { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "basename", ["a-a", "-a"])?
    uu.succeeds(r)
    uu.stdout_is(r, "a\n")
  }
  {
    let r = uu.invoke(s, "basename", ["a--help", "--help"])?
    uu.succeeds(r)
    uu.stdout_is(r, "a\n")
  }
  {
    let r = uu.invoke(s, "basename", ["a-h", "-h"])?
    uu.succeeds(r)
    uu.stdout_is(r, "a\n")
  }
  {
    let r = uu.invoke(s, "basename", ["f.s", ".s"])?
    uu.succeeds(r)
    uu.stdout_is(r, "f\n")
  }
  {
    let r = uu.invoke(s, "basename", ["a-s", "-s"])?
    uu.succeeds(r)
    uu.stdout_is(r, "a\n")
  }
  {
    let r = uu.invoke(s, "basename", ["a-z", "-z"])?
    uu.succeeds(r)
    uu.stdout_is(r, "a\n")
  }
  let r = uu.invoke(s, "basename", ["a", "b", "c"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "extra operand 'c'")
}

# origin: uutils test_basename::test_invalid_arg
test test_uu_basename_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_basename::test_repeated_multiple
test test_uu_basename_repeated_multiple { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["-aa", "-a", "foo"])?
  uu.succeeds(r)
  uu.stdout_is(r, "foo\n")
}

# origin: uutils test_basename::test_repeated_multiple_many
test test_uu_basename_repeated_multiple_many { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["-aa", "-a", "1/foo", "q/bar", "x/y/baz"])?
  uu.succeeds(r)
  uu.stdout_is(r, "foo\nbar\nbaz\n")
}

# origin: uutils test_basename::test_repeated_suffix_last
test test_uu_basename_repeated_suffix_last { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["-s", ".h", "-s", ".c", "foo.c"])?
  uu.succeeds(r)
  uu.stdout_is(r, "foo\n")
}

# origin: uutils test_basename::test_repeated_suffix_not_first
test test_uu_basename_repeated_suffix_not_first { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["-s", ".h", "-s", ".c", "foo.h"])?
  uu.succeeds(r)
  uu.stdout_is(r, "foo.h\n")
}

# origin: uutils test_basename::test_repeated_suffix_multiple
test test_uu_basename_repeated_suffix_multiple { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["-as", ".h", "-a", "-s", ".c", "foo.c", "bar.c", "bar.h"])?
  uu.succeeds(r)
  uu.stdout_is(r, "foo\nbar\nbar.h\n")
}

# origin: uutils test_basename::test_repeated_zero
test test_uu_basename_repeated_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["-zz", "-z", "foo/bar"])?
  uu.succeeds(r)
  uu.stdout_is(r, "bar\0")
}

# origin: uutils test_basename::test_zero_does_not_imply_multiple
test test_uu_basename_zero_does_not_imply_multiple { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["-z", "foo.c", "c"])?
  uu.succeeds(r)
  uu.stdout_is(r, "foo.\0")
}

# origin: uutils test_basename::test_suffix_implies_multiple
test test_uu_basename_suffix_implies_multiple { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", ["-s", ".c", "foo.c", "o.c"])?
  uu.succeeds(r)
  uu.stdout_is(r, "foo\no\n")
}

# origin: uutils test_basename::test_emoji_handling
test test_uu_basename_emoji_handling { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "basename", ["/path/to/🦀.txt"])?
    uu.succeeds(r)
    uu.stdout_only(r, "🦀.txt\n")
  }
  {
    let r = uu.invoke(s, "basename", ["/🌍/path/to/🚀.exe"])?
    uu.succeeds(r)
    uu.stdout_only(r, "🚀.exe\n")
  }
  {
    let r = uu.invoke(s, "basename", ["/path/to/file🎯.emoji", ".emoji"])?
    uu.succeeds(r)
    uu.stdout_only(r, "file🎯\n")
  }
}
