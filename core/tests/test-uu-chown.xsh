##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_chown.rs.

use support.uu as uu

proc identity(s: uu.Scene, util: Str, args: List[Str]) [fs, process, env, error] -> Result[Str, Error] {
  let r = uu.invoke(s, util, args)?
  uu.succeeds(r)
  let name = r.stdout.utf8()?.trim()
  assert name != ""
  Ok(name)
}

# origin: uutils test_chown::test_invalid_option
test test_uu_chown_invalid_option { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "chown", ["-w", "-q", "/"])?
  uu.fails(r)
}

# origin: uutils test_chown::test_invalid_arg
test test_uu_chown_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "chown", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_chown::test_chown_only_owner
test test_uu_chown_chown_only_owner { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  uu.touch(s, "test_chown_file1")?
  let r = uu.invoke(s, "chown", [user_name, "--verbose", "test_chown_file1"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "retained as")
  uu.no_stderr(r)
  let root = uu.invoke(s, "chown", ["root", "--verbose", "test_chown_file1"])?
  uu.fails(root)
  uu.stderr_is(root, "chown: changing ownership of 'test_chown_file1': Operation not permitted\n")
}

# origin: uutils test_chown::test_chown_only_owner_colon
test test_uu_chown_chown_only_owner_colon { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  uu.touch(s, "test_chown_file1")?
  let r = uu.invoke(s, "chown", [f"{user_name}:", "--verbose", "test_chown_file1"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "retained as")
  uu.no_stderr(r)
  let dot = uu.invoke(s, "chown", [f"{user_name}.", "--verbose", "test_chown_file1"])?
  uu.succeeds(dot)
  uu.stdout_contains(dot, "retained as")
  uu.stderr_contains(dot, "warning: '.' should be ':'")
  let root = uu.invoke(s, "chown", ["root:", "--verbose", "test_chown_file1"])?
  uu.fails(root)
  uu.stderr_is(root, "chown: changing ownership of 'test_chown_file1': Operation not permitted\n")
}

# origin: uutils test_chown::test_chown_dot_separator_warning
test test_uu_chown_chown_dot_separator_warning { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  let group_name = identity(s, "id", ["-gn"])?
  uu.touch(s, "test_chown_dot_warn")?
  let r = uu.invoke(s, "chown", [f"{user_name}.", "test_chown_dot_warn"])?
  uu.succeeds(r)
  uu.stderr_contains(r, "warning: '.' should be ':'")
  let both = uu.invoke(s, "chown", [f"{user_name}.{group_name}", "--verbose", "test_chown_dot_warn"])?
  uu.stderr_contains(both, "warning: '.' should be ':'")
  let output = both.stdout.utf8()?
  assert "retained as" in output or "changed ownership" in output
  let colon = uu.invoke(s, "chown", [f"{user_name}:", "test_chown_dot_warn"])?
  uu.succeeds(colon)
  assert "warning" not in colon.stderr.utf8()?
}

# origin: uutils test_chown::test_chown_owner_group
test test_uu_chown_chown_owner_group { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  let group_name = identity(s, "id", ["-gn"])?
  uu.touch(s, "test_chown_file1")?
  let r = uu.invoke(s, "chown", [f"{user_name}:{group_name}", "--verbose", "test_chown_file1"])?
  uu.stdout_contains(r, "retained as")
  let colon = uu.invoke(s, "chown", ["root:root:root", "--verbose", "test_chown_file1"])?
  uu.fails(colon)
  uu.stderr_contains(colon, "invalid group")
  let dot = uu.invoke(s, "chown", ["root.root.root", "--verbose", "test_chown_file1"])?
  uu.fails(dot)
  uu.stderr_only(dot, "chown: invalid user: 'root.root.root'\n")
  let root = uu.invoke(s, "chown", ["root:root", "--verbose", "test_chown_file1"])?
  uu.fails(root)
  uu.stderr_is(root, "chown: changing ownership of 'test_chown_file1': Operation not permitted\n")
}

# origin: uutils test_chown::test_chown_various_input
test test_uu_chown_chown_various_input { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  let group_name = identity(s, "id", ["-gn"])?
  uu.touch(s, "test_chown_file1")?
  let r = uu.invoke(s, "chown", [f"{user_name}:{group_name}", "--verbose", "test_chown_file1"])?
  uu.stdout_contains(r, "retained as")
  let dot = uu.invoke(s, "chown", [f"{user_name}.{group_name}", "--verbose", "test_chown_file1"])?
  uu.stdout_contains(dot, "retained as")
  let invalid = uu.invoke(s, "chown", ["user.name:groupname", "--verbose", "test_chown_file1"])?
  uu.fails(invalid)
  uu.stderr_contains(invalid, "chown: invalid user: 'user.name:groupname'")
}

# origin: uutils test_chown::test_chown_only_group
test test_uu_chown_chown_only_group { |ctx|
  let s = uu.scene(ctx)?
  let group_name = identity(s, "id", ["-gn"])?
  uu.touch(s, "test_chown_file1")?
  let r = uu.invoke(s, "chown", [f":{group_name}", "--verbose", "test_chown_file1"])?
  uu.stdout_contains(r, "retained as")
  uu.succeeds(r)
  if group_name != "root" {
    let root = uu.invoke(s, "chown", [":root", "--verbose", "test_chown_file1"])?
    uu.fails(root)
    uu.stderr_is(root, "chown: changing group of 'test_chown_file1': Operation not permitted\n")
  }
}

# origin: uutils test_chown::test_chown_only_user_id
test test_uu_chown_chown_only_user_id { |ctx|
  let s = uu.scene(ctx)?
  let user_id = identity(s, "id", ["-u"])?
  uu.touch(s, "test_chown_file1")?
  let r = uu.invoke(s, "chown", [user_id, "--verbose", "test_chown_file1"])?
  uu.stdout_contains(r, "retained as")
  let root = uu.invoke(s, "chown", ["0", "--verbose", "test_chown_file1"])?
  uu.fails(root)
  uu.stderr_is(root, "chown: changing ownership of 'test_chown_file1': Operation not permitted\n")
}

# origin: uutils test_chown::test_chown_only_group_id
test test_uu_chown_chown_only_group_id { |ctx|
  let s = uu.scene(ctx)?
  let group_id = identity(s, "id", ["-g"])?
  uu.touch(s, "test_chown_file1")?
  let r = uu.invoke(s, "chown", [f":{group_id}", "--verbose", "test_chown_file1"])?
  uu.stdout_contains(r, "retained as")
  if group_id != "0" {
    let root = uu.invoke(s, "chown", [":0", "--verbose", "test_chown_file1"])?
    uu.fails(root)
    uu.stderr_is(root, "chown: changing group of 'test_chown_file1': Operation not permitted\n")
  }
}

# origin: uutils test_chown::test_chown_owner_group_id
test test_uu_chown_chown_owner_group_id { |ctx|
  let s = uu.scene(ctx)?
  let user_id = identity(s, "id", ["-u"])?
  let group_id = identity(s, "id", ["-g"])?
  uu.touch(s, "test_chown_file1")?
  let r = uu.invoke(s, "chown", [f"{user_id}:{group_id}", "--verbose", "test_chown_file1"])?
  uu.stdout_contains(r, "retained as")
  let dot = uu.invoke(s, "chown", [f"{user_id}.{group_id}", "--verbose", "test_chown_file1"])?
  uu.stdout_contains(dot, "retained as")
  let root = uu.invoke(s, "chown", ["0:0", "--verbose", "test_chown_file1"])?
  uu.fails(root)
  uu.stderr_is(root, "chown: changing ownership of 'test_chown_file1': Operation not permitted\n")
}

# origin: uutils test_chown::test_chown_owner_group_mix
test test_uu_chown_chown_owner_group_mix { |ctx|
  let s = uu.scene(ctx)?
  let user_id = identity(s, "id", ["-u"])?
  let group_name = identity(s, "id", ["-gn"])?
  uu.touch(s, "test_chown_file1")?
  let r = uu.invoke(s, "chown", [f"{user_id}:{group_name}", "--verbose", "test_chown_file1"])?
  uu.stdout_contains(r, "retained as")
  let root = uu.invoke(s, "chown", ["0:root", "--verbose", "test_chown_file1"])?
  uu.fails(root)
  uu.stderr_is(root, "chown: changing ownership of 'test_chown_file1': Operation not permitted\n")
}

# origin: uutils test_chown::test_chown_recursive
test test_uu_chown_chown_recursive { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  uu.mkdir_all(s, "a/b/c")?
  uu.mkdir(s, "z")?
  for file in ["a/a", "a/b/b", "a/b/c/c", "z/y"] { uu.touch(s, file)? }
  let r = uu.invoke(s, "chown", ["-R", "--verbose", user_name, "a", "z"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "ownership of 'a/a' retained as")
  uu.stdout_contains(r, "ownership of 'z/y' retained as")
}

# origin: uutils test_chown::test_root_preserve
test test_uu_chown_root_preserve { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  let r = uu.invoke(s, "chown", ["--preserve-root", "-R", user_name, "/"])?
  uu.fails(r)
  uu.stderr_contains(r, "chown: it is dangerous to operate recursively")
}

# origin: uutils test_chown::test_big_p
test test_uu_chown_big_p { |ctx|
  let s = uu.scene(ctx)?
  if unix.id()?.euid != 0 {
    let r = uu.invoke(s, "chown", ["-RP", "bin", "/proc/self/cwd"])?
    uu.fails(r)
    uu.stderr_contains(r, "chown: changing ownership of '/proc/self/cwd': ")
  }
}

# origin: uutils test_chown::test_chown_file_notexisting
test test_uu_chown_chown_file_notexisting { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  let r = uu.invoke(s, "chown", [user_name, "--verbose", "not_existing"])?
  uu.fails(r)
  uu.stdout_contains(r, f"failed to change ownership of 'not_existing' to {user_name}")
}

# origin: uutils test_chown::test_chown_no_change_to_user
test test_uu_chown_chown_no_change_to_user { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  let froms = ["42", ":42", "42:42"]
  for i in range(froms.len()) {
    let file = f"{i}"
    uu.touch(s, file)?
    let r = uu.invoke(s, "chown", ["-v", f"--from={froms[i]}", "43", file])?
    uu.succeeds(r)
    uu.stdout_only(r, f"ownership of '{file}' retained as {user_name}\n")
  }
}

# origin: uutils test_chown::test_chown_no_change_to_group
test test_uu_chown_chown_no_change_to_group { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  let group_name = identity(s, "id", ["-ng"])?
  let froms = ["42", ":42", "42:42"]
  for i in range(froms.len()) {
    let file = f"{i}"
    uu.touch(s, file)?
    let r = uu.invoke(s, "chown", ["-v", f"--from={froms[i]}", ":43", file])?
    uu.succeeds(r)
    uu.stdout_only(r, f"group of '{file}' retained as {group_name}\n")
  }
}

# origin: uutils test_chown::test_chown_no_change_to_user_group
test test_uu_chown_chown_no_change_to_user_group { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  let group_name = identity(s, "id", ["-ng"])?
  let froms = ["42", ":42", "42:42"]
  for i in range(froms.len()) {
    let file = f"{i}"
    uu.touch(s, file)?
    let r = uu.invoke(s, "chown", ["-v", f"--from={froms[i]}", "43:43", file])?
    uu.succeeds(r)
    uu.stdout_only(r, f"ownership of '{file}' retained as {user_name}:{group_name}\n")
  }
}

# origin: uutils test_chown::test_chown_reference_file
test test_uu_chown_chown_reference_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "chown", ["--verbose", "--reference", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "ownership of 'b' retained as")
  uu.no_stderr(r)
}

# origin: uutils test_chown::test_chown_reference_file_with_non_utf8_path
test test_uu_chown_chown_reference_file_with_non_utf8_path { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let reference = uu.at_bytes(s, b"reference_\xff\xfe")?
  reference.write(b"")?
  let r = uu.invoke_paths(s, "chown", [p"--verbose", p"--reference", Path.parse_bytes(b"reference_\xff\xfe")?, p"file"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "ownership of 'file' retained as")
  uu.no_stderr(r)
}

# origin: uutils test_chown::test_chown_no_dereference_symlink_to_dir
test test_uu_chown_chown_no_dereference_symlink_to_dir { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  uu.mkdir(s, "dir")?
  uu.symlink(s, uu.at(s, "dir").display(), "link_to_dir")?
  let link_before = fs.stat(uu.at(s, "link_to_dir"))?.ctime_ns
  let dir_before = fs.stat(uu.at(s, "dir"))?.ctime_ns
  let r = uu.invoke(s, "chown", ["--no-dereference", user_name, "link_to_dir"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "link_to_dir"))?.ctime_ns != link_before, "link's ctime should have advanced"
  assert fs.stat(uu.at(s, "dir"))?.ctime_ns == dir_before, "dir's ctime should not have changed"
}

# origin: uutils test_chown::test_chown_symlink_cycles
test test_uu_chown_chown_symlink_cycles { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  uu.mkdir_all(s, "a/b/c")?
  uu.symlink(s, uu.at(s, "a").display(), "a/b/c/d")?
  let r = uu.invoke(s, "chown", ["-vRL", user_name, "a"])?
  uu.succeeds(r)
  for file in ["a", "a/b", "a/b/c", "a/b/c/d"] {
    uu.stdout_contains(r, f"ownership of '{file}' retained as {user_name}")
  }
  for file in ["a/b/c/d/b", "a/b/c/d/b/c", "a/b/c/d/b/c/d"] {
    assert f"ownership of '{file}' retained as {user_name}" not in r.stdout.utf8()?
  }
}

# origin: uutils test_chown::test_chown_symlink_two_links_same_dir
test test_uu_chown_chown_symlink_two_links_same_dir { |ctx|
  let s = uu.scene(ctx)?
  let user_name = identity(s, "whoami", [])?
  uu.mkdir_all(s, "base/realdir")?
  uu.touch(s, "base/realdir/file")?
  uu.symlink(s, uu.at(s, "base/realdir").display(), "base/link1")?
  uu.symlink(s, uu.at(s, "base/realdir").display(), "base/link2")?
  let r = uu.invoke(s, "chown", ["-vRL", user_name, "base"])?
  uu.succeeds(r)
  for file in ["base/realdir/file", "base/link1/file", "base/link2/file"] {
    uu.stdout_contains(r, f"ownership of '{file}' retained as {user_name}")
  }
}

# origin: uutils test_chown::verbose_missing_file_write_error_is_reported_not_panic
test test_uu_chown_verbose_missing_file_write_error_is_reported_not_panic { |ctx|
  let s = uu.scene(ctx)?
  let uid = unix.id()?.euid
  let r = uu.invoke(s, "chown", ["--verbose", f"{uid}", "does-not-exist"], stdout: p"/dev/full")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "chown: write error: No space left on device")
}
