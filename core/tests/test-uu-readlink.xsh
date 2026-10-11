##! Native readlink cases transcribed from the uutils integration suite.

use support.uu as uu

# origin: uutils test_readlink::test_no_args
test test_uu_readlink_no_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "readlink", [])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "missing operand")
}

# origin: uutils test_readlink::test_invalid_arg
test test_uu_readlink_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "readlink", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_readlink::test_resolve
test test_uu_readlink_resolve { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, uu.at(s, "foo").display(), "bar")?
  let r = uu.invoke(s, "readlink", ["bar"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "foo\n")
}

# origin: uutils test_readlink::test_keeps_going_after_an_operand_that_cannot_be_read
test test_uu_readlink_keeps_going_after_an_operand_that_cannot_be_read { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, "foo", "bar")?
  uu.touch(s, "baz")?
  uu.symlink(s, "baz", "qux")?
  let r = uu.invoke(s, "readlink", ["bar", "nope", "qux"])?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "foo\nbaz\n")
  uu.no_stderr(r)
  let verbose = uu.invoke(s, "readlink", ["-v", "bar", "nope", "qux"])?
  uu.fails_with_code(verbose, 1)
  uu.stdout_is(verbose, "foo\nbaz\n")
  uu.stderr_contains(verbose, "nope: No such file or directory")
}

# origin: uutils test_readlink::test_canonicalize_existing_keeps_going_after_a_missing_operand
test test_uu_readlink_canonicalize_existing_keeps_going_after_a_missing_operand { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "c")?
  let root = s.root.resolve()?
  let r = uu.invoke(s, "readlink", ["-e", "a", "b", "c"])?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, f"{root}/a\n{root}/c\n")
}

# origin: uutils test_readlink::test_canonicalize
test test_uu_readlink_canonicalize { |ctx|
  let s = uu.scene(ctx)?
  let root = s.root.resolve()?
  let r = uu.invoke(s, "readlink", ["-f", "."])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{root}\n")
}

# origin: uutils test_readlink::test_canonicalize_existing
test test_uu_readlink_canonicalize_existing { |ctx|
  let s = uu.scene(ctx)?
  let root = s.root.resolve()?
  let r = uu.invoke(s, "readlink", ["-e", "."])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{root}\n")
}

# origin: uutils test_readlink::test_canonicalize_missing
test test_uu_readlink_canonicalize_missing { |ctx|
  let s = uu.scene(ctx)?
  let root = s.root.resolve()?
  let r = uu.invoke(s, "readlink", ["-m", "supercalifragilisticexpialidocious"])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{root}/supercalifragilisticexpialidocious\n")
}

# origin: uutils test_readlink::test_canonicalize_symlink_before_parentdir
test test_uu_readlink_canonicalize_symlink_before_parentdir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "real")?
  uu.mkdir(s, "real/sub")?
  uu.symlink(s, "real/sub", "link")?
  let root = s.root.resolve()?
  let r = uu.invoke(s, "readlink", ["-f", "link/.."])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{root}/real\n")
}

# origin: uutils test_readlink::test_long_redirection_to_current_dir
test test_uu_readlink_long_redirection_to_current_dir { |ctx|
  let s = uu.scene(ctx)?
  let dir = ["." for _ in range(128)].join("/")
  let root = s.root.resolve()?
  let r = uu.invoke(s, "readlink", ["-n", "-m", dir])?
  uu.succeeds(r)
  uu.stdout_is(r, root.display())
}

# origin: uutils test_readlink::test_long_redirection_to_root
test test_uu_readlink_long_redirection_to_root { |ctx|
  let s = uu.scene(ctx)?
  let dir = [".." for _ in range(85)].join("/")
  let r = uu.invoke(s, "readlink", ["-n", "-m", dir])?
  uu.succeeds(r)
  uu.stdout_is(r, "/")
}

# origin: uutils test_readlink::test_symlink_to_itself_verbose
test test_uu_readlink_symlink_to_itself_verbose { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "a", "a")?
  let r = uu.invoke(s, "readlink", ["-ev", "a"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "Too many levels of symbolic links")
}

# origin: uutils test_readlink::test_posixly_correct_regular_file
test test_uu_readlink_posixly_correct_regular_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "regfile")?
  for args in [["regfile"], ["-s", "regfile"], ["-q", "regfile"]] {
    let r = uu.invoke(s, "readlink", args, vars: {POSIXLY_CORRECT: "1"})?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "Invalid argument")
    uu.no_stdout(r)
  }
}

# origin: uutils test_readlink::test_trailing_slash_regular_file
test test_uu_readlink_trailing_slash_regular_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "regfile")?
  let r = uu.invoke(s, "readlink", ["-ev", "./regfile/"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "Not a directory")
  uu.no_stdout(r)
  let plain = uu.invoke(s, "readlink", ["-e", "./regfile"])?
  uu.succeeds(plain)
  uu.stdout_contains(plain, "regfile")
}

# origin: uutils test_readlink::test_trailing_slash_symlink_to_regular_file
test test_uu_readlink_trailing_slash_symlink_to_regular_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "regfile")?
  uu.symlink(s, "regfile", "link")?
  let r = uu.invoke(s, "readlink", ["-ev", "./link/"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "Not a directory")
  uu.no_stdout(r)
  let plain = uu.invoke(s, "readlink", ["-e", "./link"])?
  uu.succeeds(plain)
  uu.stdout_contains(plain, "regfile")
  let more = uu.invoke(s, "readlink", ["-e", "./link/more"])?
  uu.fails_with_code(more, 1)
  uu.no_stdout(more)
}

# origin: uutils test_readlink::test_trailing_slash_directory
test test_uu_readlink_trailing_slash_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "directory")?
  for query in ["./directory", "./directory/"] {
    let r = uu.invoke(s, "readlink", ["-e", query])?
    uu.succeeds(r)
    uu.stdout_contains(r, "directory")
  }
}

# origin: uutils test_readlink::test_trailing_slash_symlink_to_directory
test test_uu_readlink_trailing_slash_symlink_to_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "directory")?
  uu.symlink(s, "directory", "link")?
  for query in ["./link", "./link/"] {
    let r = uu.invoke(s, "readlink", ["-e", query])?
    uu.succeeds(r)
    uu.stdout_contains(r, "directory")
  }
  let more = uu.invoke(s, "readlink", ["-ev", "./link/more"])?
  uu.fails_with_code(more, 1)
  uu.stderr_contains(more, "No such file or directory")
  uu.no_stdout(more)
}

# origin: uutils test_readlink::test_trailing_slash_symlink_to_missing
test test_uu_readlink_trailing_slash_symlink_to_missing { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "subdir")?
  uu.symlink(s, "missing", "link")?
  uu.symlink(s, "subdir/missing", "link2")?
  for query in ["missing", "./missing/", "link", "./link/", "link/more", "link2", "./link2/", "link2/more"] {
    let r = uu.invoke(s, "readlink", ["-ev", query])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "No such file or directory")
    uu.no_stdout(r)
  }
}

# origin: uutils test_readlink::test_canonicalize_trailing_slash_regfile
test test_uu_readlink_canonicalize_trailing_slash_regfile { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "regfile")?
  uu.symlink(s, "regfile", "link1")?
  for name in ["regfile", "link1"] {
    let plain = uu.invoke(s, "readlink", ["-f", name])?
    uu.succeeds(plain)
    uu.stdout_contains(plain, "regfile")
    for query in [f"./{name}/", f"{name}/more", f"./{name}/more/"] {
      let r = uu.invoke(s, "readlink", ["-fv", query])?
      uu.fails_with_code(r, 1)
      uu.stderr_contains(r, "Not a directory")
      uu.no_stdout(r)
    }
  }
}

# origin: uutils test_readlink::test_canonicalize_trailing_slash_subdir
test test_uu_readlink_canonicalize_trailing_slash_subdir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "subdir")?
  uu.symlink(s, "subdir", "link2")?
  for name in ["subdir", "link2"] {
    for query in [name, f"./{name}/"] {
      let r = uu.invoke(s, "readlink", ["-f", query])?
      uu.succeeds(r)
      uu.stdout_contains(r, "subdir")
    }
    for query in [f"{name}/more", f"./{name}/more/"] {
      let r = uu.invoke(s, "readlink", ["-f", query])?
      uu.succeeds(r)
      uu.stdout_contains(r, "subdir/more")
    }
    for query in [f"{name}/more/more2", f"./{name}/more/more2/"] {
      let r = uu.invoke(s, "readlink", ["-f", query])?
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
    }
  }
}

# origin: uutils test_readlink::test_canonicalize_trailing_slash_missing
test test_uu_readlink_canonicalize_trailing_slash_missing { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "missing", "link3")?
  for name in ["missing", "link3"] {
    for query in [name, f"./{name}/"] {
      let r = uu.invoke(s, "readlink", ["-f", query])?
      uu.succeeds(r)
      uu.stdout_contains(r, "missing")
    }
    for query in [f"{name}/more", f"./{name}/more/"] {
      let r = uu.invoke(s, "readlink", ["-f", query])?
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
    }
  }
}

# origin: uutils test_readlink::test_canonicalize_trailing_slash_subdir_missing
test test_uu_readlink_canonicalize_trailing_slash_subdir_missing { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "subdir")?
  uu.symlink(s, "subdir/missing", "link4")?
  for query in ["link4", "./link4/"] {
    let r = uu.invoke(s, "readlink", ["-f", query])?
    uu.succeeds(r)
    uu.stdout_contains(r, "subdir/missing")
  }
  for query in ["link4/more", "./link4/more/"] {
    let r = uu.invoke(s, "readlink", ["-f", query])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
  }
}

# origin: uutils test_readlink::test_canonicalize_trailing_slash_symlink_loop
test test_uu_readlink_canonicalize_trailing_slash_symlink_loop { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "link5", "link5")?
  for query in ["link5", "./link5/", "link5/more", "./link5/more/"] {
    let r = uu.invoke(s, "readlink", ["-f", query])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
  }
}

# origin: uutils test_readlink::test_delimiters
test test_uu_readlink_delimiters { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "readlink", ["--zero", "-n", "-m", "/a"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "/a")
  let r1 = uu.invoke(s, "readlink", ["-n", "-m", "/a"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "/a")
  let r2 = uu.invoke(s, "readlink", ["--zero", "-m", "/a"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "/a\0")
  let r3 = uu.invoke(s, "readlink", ["-m", "/a"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "/a\n")
  let zero_multiple = uu.invoke(s, "readlink", ["--zero", "-n", "-m", "/a", "/a"])?
  uu.succeeds(zero_multiple)
  uu.stderr_contains(zero_multiple, "ignoring --no-newline with multiple arguments")
  uu.stdout_is(zero_multiple, "/a\0/a\0")
  let newline_multiple = uu.invoke(s, "readlink", ["-n", "-m", "/a", "/a"])?
  uu.succeeds(newline_multiple)
  uu.stderr_contains(newline_multiple, "ignoring --no-newline with multiple arguments")
  uu.stdout_is(newline_multiple, "/a\n/a\n")
}

# origin: uutils test_readlink::test_readlink_non_utf8_paths
test test_uu_readlink_readlink_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "target_file")?
  let name = b"symlink_\xff\xfe"
  uu.at_bytes(s, name)?.symlink(to: uu.at(s, "target_file"))?
  let r = uu.invoke_paths(s, "readlink", [Path.parse_bytes(name)?])?
  uu.succeeds(r)
  uu.stdout_contains(r, "target_file")
}

# origin: uutils test_readlink::test_verbose_or_silent
test test_uu_readlink_verbose_or_silent { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "regfile")?
  let r = uu.invoke(s, "readlink", ["regfile"])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
  let verbose = uu.invoke(s, "readlink", ["-v", "regfile"])?
  uu.fails_with_code(verbose, 1)
  uu.stderr_contains(verbose, "Invalid argument")
  uu.no_stdout(verbose)
  let silent = uu.invoke(s, "readlink", ["-vs", "regfile"])?
  uu.fails_with_code(silent, 1)
  uu.no_output(silent)
  let last_verbose = uu.invoke(s, "readlink", ["-sv", "regfile"])?
  uu.fails_with_code(last_verbose, 1)
  uu.stderr_contains(last_verbose, "Invalid argument")
  uu.no_stdout(last_verbose)
}
