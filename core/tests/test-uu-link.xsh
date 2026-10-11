##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_link.rs.

use support.uu as uu

# Missing paths have no metadata; check existence before requesting file kind.
proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  Ok(uu.exists(s, name)? and uu.file_exists(s, name)?)
}

# origin: uutils test_link::test_invalid_arg
test test_uu_link_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "link", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_link::test_link_existing_file
test test_uu_link_link_existing_file { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_link_existing_file"
  let link = "test_link_existing_file_link"
  uu.touch(s, file)?
  uu.write(s, file, "foobar")?
  assert uu.file_exists(s, file)?
  let r = uu.invoke(s, "link", [file, link])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, file)?
  assert uu.file_exists(s, link)?
  assert uu.read(s, file)? == uu.read(s, link)?
}

# origin: uutils test_link::test_link_no_circular
test test_uu_link_link_no_circular { |ctx|
  let s = uu.scene(ctx)?
  let link = "test_link_no_circular"
  let r = uu.invoke(s, "link", [link, link])?
  uu.fails(r)
  uu.stderr_is(r, "link: cannot create link 'test_link_no_circular' to 'test_link_no_circular': No such file or directory\n")
  assert !file_exists(s, link)?
}

# origin: uutils test_link::test_link_nonexistent_file
test test_uu_link_link_nonexistent_file { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_link_nonexistent_file"
  let link = "test_link_nonexistent_file_link"
  let r = uu.invoke(s, "link", [file, link])?
  uu.fails(r)
  uu.stderr_only(r, "link: cannot create link 'test_link_nonexistent_file_link' to 'test_link_nonexistent_file': No such file or directory\n")
  assert !file_exists(s, file)?
  assert !file_exists(s, link)?
}

# GNU identifies the missing operand after the supplied file.
# origin: uutils test_link::test_link_one_argument
test test_uu_link_link_one_argument { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "link", ["test_link_argument"])?
  uu.fails(r)
  uu.stderr_contains(r, "missing operand after 'test_link_argument'")
}

# GNU identifies the first operand beyond the required pair.
# origin: uutils test_link::test_link_three_arguments
test test_uu_link_link_three_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "link", ["test_link_argument1", "test_link_argument2", "test_link_argument3"])?
  uu.fails(r)
  uu.stderr_contains(r, "extra operand 'test_link_argument3'")
}

# origin: uutils test_link::test_link_no_arguments
test test_uu_link_link_no_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "link", [])?
  uu.fails(r)
}

# origin: uutils test_link::test_link_dest_exists
test test_uu_link_link_dest_exists { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_link_dest_exists_src"
  let dest = "test_link_dest_exists_dst"
  uu.touch(s, file)?
  uu.touch(s, dest)?
  let r = uu.invoke(s, "link", [file, dest])?
  uu.fails(r)
  uu.stderr_contains(r, "exists")
}
