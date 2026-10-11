##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_dirname.rs.
##! Each test names its origin; expected values are the original assertions.

use support.uu as uu

# origin: uutils test_dirname::test_invalid_arg
test test_uu_dirname_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_dirname::test_missing_operand
test test_uu_dirname_missing_operand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", [])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_dirname::test_path_with_trailing_slashes
test test_uu_dirname_path_with_trailing_slashes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["/root/alpha/beta/gamma/delta/epsilon/omega//"])?
  uu.succeeds(r)
  uu.stdout_is(r, "/root/alpha/beta/gamma/delta/epsilon\n")
}

# origin: uutils test_dirname::test_path_without_trailing_slashes
test test_uu_dirname_path_without_trailing_slashes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["/root/alpha/beta/gamma/delta/epsilon/omega"])?
  uu.succeeds(r)
  uu.stdout_is(r, "/root/alpha/beta/gamma/delta/epsilon\n")
}

# origin: uutils test_dirname::test_path_without_trailing_slashes_and_zero
test test_uu_dirname_path_without_trailing_slashes_and_zero { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "dirname", ["-z", "/root/alpha/beta/gamma/delta/epsilon/omega"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "/root/alpha/beta/gamma/delta/epsilon\0")
  let r2 = uu.invoke(s, "dirname", ["--zero", "/root/alpha/beta/gamma/delta/epsilon/omega"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "/root/alpha/beta/gamma/delta/epsilon\0")
}

# origin: uutils test_dirname::test_repeated_zero
test test_uu_dirname_repeated_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["--zero", "--zero", "foo/bar"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\0")
}

# origin: uutils test_dirname::test_root
test test_uu_dirname_root { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["/"])?
  uu.succeeds(r)
  uu.stdout_is(r, "/\n")
}

# origin: uutils test_dirname::test_pwd
test test_uu_dirname_pwd { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["."])?
  uu.succeeds(r)
  uu.stdout_is(r, ".\n")
}

# origin: uutils test_dirname::test_empty
test test_uu_dirname_empty { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", [""])?
  uu.succeeds(r)
  uu.stdout_is(r, ".\n")
}

# origin: uutils test_dirname::test_emoji_handling
test test_uu_dirname_emoji_handling { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "dirname", ["/🌍/path/to/🦀.txt"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "/🌍/path/to\n")
  let r2 = uu.invoke(s, "dirname", ["/🎉/path/to/🚀/"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "/🎉/path/to\n")
  let r3 = uu.invoke(s, "dirname", ["-z", "/🌟/emoji/path/🦋.file"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "/🌟/emoji/path\0")
}

# origin: uutils test_dirname::test_trailing_dot
test test_uu_dirname_trailing_dot { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "dirname", ["/home/dos/."])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "/home/dos\n")
  let r2 = uu.invoke(s, "dirname", ["/."])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "/\n")
  let r3 = uu.invoke(s, "dirname", ["hello/."])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "hello\n")
  let r4 = uu.invoke(s, "dirname", ["/foo/bar/baz/."])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "/foo/bar/baz\n")
}

# origin: uutils test_dirname::test_trailing_dot_with_zero_flag
test test_uu_dirname_trailing_dot_with_zero_flag { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "dirname", ["-z", "/home/dos/."])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "/home/dos\0")
  let r2 = uu.invoke(s, "dirname", ["--zero", "/."])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "/\0")
}

# origin: uutils test_dirname::test_trailing_dot_multiple_paths
test test_uu_dirname_trailing_dot_multiple_paths { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["/home/dos/.", "/var/log", "/tmp/."])?
  uu.succeeds(r)
  uu.stdout_is(r, "/home/dos\n/var\n/tmp\n")
}

# origin: uutils test_dirname::test_trailing_dot_edge_cases
test test_uu_dirname_trailing_dot_edge_cases { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "dirname", ["/home/dos//."])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "/home/dos\n")
  let r2 = uu.invoke(s, "dirname", ["/path/./to/file"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "/path/./to\n")
}

# origin: uutils test_dirname::test_trailing_dot_emoji
test test_uu_dirname_trailing_dot_emoji { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "dirname", ["/🌍/path/."])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "/🌍/path\n")
  let r2 = uu.invoke(s, "dirname", ["/🎉/🚀/."])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "/🎉/🚀\n")
}

# origin: uutils test_dirname::test_existing_behavior_preserved
test test_uu_dirname_existing_behavior_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "dirname", ["/home/dos"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "/home\n")
  let r2 = uu.invoke(s, "dirname", ["/home/dos/"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "/home\n")
  let r3 = uu.invoke(s, "dirname", ["/home/dos/.."])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "/home/dos\n")
}

# origin: uutils test_dirname::test_multiple_paths_comprehensive
test test_uu_dirname_multiple_paths_comprehensive { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["/home/dos/.", "/var/log", ".", "/tmp/.", "", "/", "relative/path"])?
  uu.succeeds(r)
  uu.stdout_is(r, "/home/dos\n/var\n.\n/tmp\n.\n/\nrelative\n")
}

# origin: uutils test_dirname::test_all_dot_slash_variations
test test_uu_dirname_all_dot_slash_variations { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "dirname", ["foo//."])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "foo\n")
  let r2 = uu.invoke(s, "dirname", ["foo///."])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "foo\n")
  let r3 = uu.invoke(s, "dirname", ["foo/./"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "foo\n")
  let r4 = uu.invoke(s, "dirname", ["foo/bar/./"])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "foo/bar\n")
  let r5 = uu.invoke(s, "dirname", ["foo/./bar"])?
  uu.succeeds(r5)
  uu.stdout_is(r5, "foo/.\n")
}

# origin: uutils test_dirname::test_dot_slash_component_preservation
test test_uu_dirname_dot_slash_component_preservation { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "dirname", ["a/./b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a/.\n")
  let r2 = uu.invoke(s, "dirname", ["a/./b/./c"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "a/./b/.\n")
  let r3 = uu.invoke(s, "dirname", ["foo/./bar/baz"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "foo/./bar\n")
  let r4 = uu.invoke(s, "dirname", ["/path/./to/file"])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "/path/./to\n")
}

# origin: uutils test_dirname::test_dirname_non_utf8_paths
test test_uu_dirname_dirname_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "dirname", [Path.parse_bytes(b"test_\xff\xfe/file.txt")?])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"test_\xff\xfe\n")
  let output = Path.parse_bytes(r.stdout)?.display()
  assert !output.is_empty()
  assert "test_" in output
}

# origin: uutils test_dirname::test_trailing_dot_non_utf8
test test_uu_dirname_trailing_dot_non_utf8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "dirname", [Path.parse_bytes(b"/test_\xff\xfe/.")?])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"/test_\xff\xfe\n")
  let output = Path.parse_bytes(r.stdout)?.display()
  assert !output.is_empty()
  assert "test_" in output
  assert !output.trim().ends_with(".")
}
