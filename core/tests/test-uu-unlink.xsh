##! Native ports of the uutils unlink integration tests.
use support.uu as uu

# origin: uutils test_unlink::test_invalid_arg
test test_uu_unlink_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "unlink", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_unlink::test_unlink_directory
test test_uu_unlink_unlink_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  let r = uu.invoke(s, "unlink", ["dir"])?
  uu.fails(r)
  assert r.stderr == b"unlink: cannot unlink 'dir': Is a directory\n" or r.stderr == b"unlink: cannot unlink 'dir': Permission denied\n"
}

# origin: uutils test_unlink::test_unlink_file
test test_uu_unlink_unlink_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_unlink_file")?
  let r = uu.invoke(s, "unlink", ["test_unlink_file"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! uu.exists(s, "test_unlink_file")?
}

# origin: uutils test_unlink::test_unlink_multiple_files
test test_uu_unlink_unlink_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_unlink_multiple_file_a")?
  uu.touch(s, "test_unlink_multiple_file_b")?
  let r = uu.invoke(s, "unlink", ["test_unlink_multiple_file_a", "test_unlink_multiple_file_b"])?
  uu.fails(r)
  uu.stderr_is(r, "unlink: extra operand 'test_unlink_multiple_file_b'\nTry 'unlink --help' for more information.\n")
}

# origin: uutils test_unlink::test_unlink_non_utf8_paths
test test_uu_unlink_unlink_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let target = uu.at_bytes(s, b"test_\xff\xfe.txt")?
  target.write("")?
  assert target.is_file()?
  let r = uu.invoke_paths(s, "unlink", [Path.parse_bytes(b"test_\xff\xfe.txt")?])?
  uu.succeeds(r)
  assert ! target.exists()?
}

# origin: uutils test_unlink::test_unlink_nonexistent
test test_uu_unlink_unlink_nonexistent { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unlink", ["test_unlink_nonexistent"])?
  uu.fails(r)
  uu.stderr_is(r, "unlink: cannot unlink 'test_unlink_nonexistent': No such file or directory\n")
}

# origin: uutils test_unlink::test_unlink_symlink
test test_uu_unlink_unlink_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, "foo", "bar")?
  let r = uu.invoke(s, "unlink", ["bar"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "foo")?
  assert ! uu.exists(s, "bar")?
}
