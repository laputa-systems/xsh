##! Transcribed from the uutils coreutils rm integration tests.

use support.uu as uu

# Dot operands may name two ancestors. Keep both ancestors inside this test's
# owned scratch tree even if a guard regresses.
proc safe_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let outer = uu.scene(ctx)?
  let working = uu.at(outer, "fixture/working")
  working.mkdir()?
  Ok({ctx: ctx, root: working})
}

proc absolute_link(s: uu.Scene, target: Str, name: Str) [fs, error] -> Result[Unit, Error] {
  let to = if target.starts_with("/") { Path(target) } else { uu.at(s, target) }
  uu.symlink(s, to.display(), name)?
  Ok()
}

proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let target = uu.at(s, name)
  if target.exists()? { Ok(target.is_file()?) } else { Ok(false) }
}

proc dir_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let target = uu.at(s, name)
  if target.exists()? { Ok(target.is_dir()?) } else { Ok(false) }
}

proc is_symlink(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  match fs.stat(uu.at(s, name)) {
    Ok(meta) => Ok(meta.kind == "symlink"),
    Err(failure) => if failure.errno == 2 { Ok(false) } else { Err(failure) },
  }
}

# origin: uutils test_rm::no_preserve_root_may_not_be_abbreviated
test test_uu_rm_no_preserve_root_may_not_be_abbreviated { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_file_123")?
  for arg in ["--n", "--no-pre", "--no-preserve-ro"] {
    let r = uu.invoke(s, "rm", [arg, "test_file_123"])?
    uu.fails(r)
    uu.stderr_contains(r, "you may not abbreviate the --no-preserve-root option")
  }
  assert file_exists(s, "test_file_123")?
}

# origin: uutils test_rm::test_current_or_parent_dir_rm4
test test_uu_rm_current_or_parent_dir_rm4 { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "file1")?
  uu.touch(s, "d/file2")?
  let answers = [
    "rm: refusing to remove '.' or '..' directory: skipping 'd/.'",
    "rm: refusing to remove '.' or '..' directory: skipping 'd/./'",
    "rm: refusing to remove '.' or '..' directory: skipping 'd/./'",
    "rm: refusing to remove '.' or '..' directory: skipping 'd/..'",
    "rm: refusing to remove '.' or '..' directory: skipping 'd/../'",
    "rm: refusing to remove '.' or '..' directory: skipping '.'",
    "rm: refusing to remove '.' or '..' directory: skipping './'",
    "rm: refusing to remove '.' or '..' directory: skipping '../'",
    "rm: refusing to remove '.' or '..' directory: skipping '..'",
  ]
  let r = uu.invoke(s, "rm", ["-rf", "d/.", "d/./", "d/.////", "d/..", "d/../", ".", "./", "../", ".."])?
  uu.fails(r)
  let lines = r.stderr.utf8()?.lines()
  for idx in range(lines.len()) { assert lines[idx] == answers[idx] }
  assert dir_exists(s, "d")?
  assert file_exists(s, "file1")?
  assert file_exists(s, "d/file2")?
}

# origin: uutils test_rm::test_dash_hint_absent_without_matching_file
test test_uu_rm_dash_hint_absent_without_matching_file { |ctx|
  let s = safe_scene(ctx)?
  let r1 = uu.invoke(s, "rm", ["-q"])?
  uu.fails_with_code(r1, 1)
  assert ! ("to remove the file" in r1.stderr.utf8()?)
}

# origin: uutils test_rm::test_dash_hint_is_shell_escaped
test test_uu_rm_dash_hint_is_shell_escaped { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "-a\tb'c")?
  let r1 = uu.invoke(s, "rm", ["-a\tb'c"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "./'-a'$'\\t''b'\\''c'' to remove the file '-a'$'\\t''b'\\''c'.")
}

# origin: uutils test_rm::test_dash_hint_shown_for_existing_dash_file
test test_uu_rm_dash_hint_shown_for_existing_dash_file { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "-z")?
  let r = uu.invoke(s, "rm", ["-z"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "./-z' to remove the file '-z'.")
  uu.stderr_contains(r, "--help' for more information.")
  assert file_exists(s, "-z")?
}

# origin: uutils test_rm::test_descend_directory
test test_uu_rm_descend_directory { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir_all(s, "a/b/")?
  uu.touch(s, "a/at.txt")?
  uu.touch(s, "a/b/bt.txt")?
  let _ = uu.invoke(s, "rm", ["-ri", "a"], stdin: b"y\ny\ny\ny\ny\ny\n")?
  assert ! dir_exists(s, "a/b")?
  assert ! dir_exists(s, "a")?
  assert ! file_exists(s, "a/at.txt")?
  assert ! file_exists(s, "a/b/bt.txt")?
}

# origin: uutils test_rm::test_directory_without_flag
test test_uu_rm_directory_without_flag { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "test_rm_directory_without_flag_dir")?
  let r1 = uu.invoke(s, "rm", ["test_rm_directory_without_flag_dir"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot remove 'test_rm_directory_without_flag_dir': Is a directory")
}

# origin: uutils test_rm::test_dot_protection_multiple_slashes
test test_uu_rm_dot_protection_multiple_slashes { |ctx|
  for operand in ["./", ".//", ".///", ".//./", "../", "..//", "..//../"] {
    let s = safe_scene(ctx)?
    uu.mkdir(s, "d")?
    uu.touch(s, "file1")?
    uu.touch(s, "d/file2")?
    let r = uu.invoke(s, "rm", ["-rf", operand])?
    uu.fails(r)
    let message = r.stderr.utf8()?
    assert "refusing to remove" in message or "dangerous to operate" in message
    assert dir_exists(s, "d")?
    assert file_exists(s, "file1")?
    assert file_exists(s, "d/file2")?
  }
}

# origin: uutils test_rm::test_dot_protection_nested_paths
test test_uu_rm_dot_protection_nested_paths { |ctx|
  for operand in ["a/b/c/.", "a/b/c/./", "a/b/c/..", "a/b/c/../", "a/b/.", "a/b/../"] {
    let s = safe_scene(ctx)?
    uu.mkdir_all(s, "a/b/c")?
    uu.touch(s, "a/b/c/file")?
    uu.touch(s, "rootfile")?
    let r = uu.invoke(s, "rm", ["-rf", operand])?
    uu.fails(r)
    uu.stderr_contains(r, "refusing to remove")
    assert dir_exists(s, "a/b/c")?
    assert file_exists(s, "a/b/c/file")?
    assert file_exists(s, "rootfile")?
  }
}

# origin: uutils test_rm::test_empty_directory
test test_uu_rm_empty_directory { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "test_rm_empty_directory")?
  let r1 = uu.invoke(s, "rm", ["-d", "test_rm_empty_directory"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert ! dir_exists(s, "test_rm_empty_directory")?
}

# origin: uutils test_rm::test_empty_directory_verbose
test test_uu_rm_empty_directory_verbose { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "test_rm_empty_directory_verbose")?
  let r1 = uu.invoke(s, "rm", ["-d", "-v", "test_rm_empty_directory_verbose"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "removed directory 'test_rm_empty_directory_verbose'\n")
  assert ! dir_exists(s, "test_rm_empty_directory_verbose")?
}

# origin: uutils test_rm::test_failed
test test_uu_rm_failed { |ctx|
  let s = safe_scene(ctx)?
  let r1 = uu.invoke(s, "rm", ["test_rm_one_file"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot remove 'test_rm_one_file': No such file or directory")
}

# origin: uutils test_rm::test_fifo_removal
test test_uu_rm_fifo_removal { |ctx|
  let s = safe_scene(ctx)?
  uu.mkfifo(s, "some_fifo")?
  let r1 = uu.invoke(s, "rm", ["some_fifo"], timeout: 2s)?
  uu.succeeds(r1)
}

# origin: uutils test_rm::test_force
test test_uu_rm_force { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_force_a")?
  uu.touch(s, "test_rm_force_b")?
  assert file_exists(s, "test_rm_force_a")?
  assert file_exists(s, "test_rm_force_b")?
  let r1 = uu.invoke(s, "rm", ["-f", "test_rm_force_a", "test_rm_force_b"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert ! file_exists(s, "test_rm_force_a")?
  assert ! file_exists(s, "test_rm_force_b")?
}

# origin: uutils test_rm::test_force_multiple
test test_uu_rm_force_multiple { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_force_a")?
  uu.touch(s, "test_rm_force_b")?
  assert file_exists(s, "test_rm_force_a")?
  assert file_exists(s, "test_rm_force_b")?
  let r1 = uu.invoke(s, "rm", ["-f", "-f", "-f", "test_rm_force_a", "test_rm_force_b"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert ! file_exists(s, "test_rm_force_a")?
  assert ! file_exists(s, "test_rm_force_b")?
}

# origin: uutils test_rm::test_force_no_operand
test test_uu_rm_force_no_operand { |ctx|
  let s = safe_scene(ctx)?
  let r1 = uu.invoke(s, "rm", ["-f"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
}

# origin: uutils test_rm::test_force_prompts_order
test test_uu_rm_force_prompts_order { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "empty")?
  let prompted = uu.invoke(s, "rm", ["-fi", "empty"], stdin: b"y\n")?
  uu.stderr_only(prompted, "rm: remove regular empty file 'empty'? ")
  assert ! file_exists(s, "empty")?
  uu.touch(s, "empty")?
  let forced = uu.invoke(s, "rm", ["-if", "empty"])?
  uu.succeeds(forced)
  uu.no_stderr(forced)
  assert ! file_exists(s, "empty")?
}

# origin: uutils test_rm::test_inaccessible_dir
test test_uu_rm_inaccessible_dir { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.set_mode(s, "dir", 0o0333)?
  let r1 = uu.invoke(s, "rm", ["-d", "dir"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert ! dir_exists(s, "dir")?
  if dir_exists(s, "dir")? { uu.set_mode(s, "dir", 0o755)? }
}

# origin: uutils test_rm::test_inaccessible_dir_interactive
test test_uu_rm_inaccessible_dir_interactive { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.set_mode(s, "dir", 0)?
  let r1 = uu.invoke(s, "rm", ["-i", "-d", "dir"], stdin: b"y\n")?
  uu.succeeds(r1)
  uu.stderr_only(r1, "rm: attempt removal of inaccessible directory 'dir'? ")
  assert ! dir_exists(s, "dir")?
  if dir_exists(s, "dir")? { uu.set_mode(s, "dir", 0o755)? }
}

# origin: uutils test_rm::test_inaccessible_dir_nonempty
test test_uu_rm_inaccessible_dir_nonempty { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/f")?
  uu.set_mode(s, "dir", 0o0333)?
  let r1 = uu.invoke(s, "rm", ["-d", "dir"])?
  uu.fails(r1)
  uu.stderr_only(r1, "rm: cannot remove 'dir': Directory not empty\n")
  assert file_exists(s, "dir/f")?
  assert dir_exists(s, "dir")?
  if dir_exists(s, "dir")? { uu.set_mode(s, "dir", 0o755)? }
}

# origin: uutils test_rm::test_inaccessible_dir_recursive
test test_uu_rm_inaccessible_dir_recursive { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "a/unreadable")?
  uu.set_mode(s, "a/unreadable", 0o0333)?
  let r1 = uu.invoke(s, "rm", ["-r", "-f", "a"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert ! dir_exists(s, "a/unreadable")?
  assert ! dir_exists(s, "a")?
  if dir_exists(s, "a/unreadable")? { uu.set_mode(s, "a/unreadable", 0o755)? }
}

# origin: uutils test_rm::test_interactive
test test_uu_rm_interactive { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_interactive_file_a")?
  uu.touch(s, "test_rm_interactive_file_b")?
  assert file_exists(s, "test_rm_interactive_file_a")?
  assert file_exists(s, "test_rm_interactive_file_b")?
  let r1 = uu.invoke(s, "rm", ["-i", "test_rm_interactive_file_a", "test_rm_interactive_file_b"], stdin: b"n")?
  uu.succeeds(r1)
  assert file_exists(s, "test_rm_interactive_file_a")?
  assert file_exists(s, "test_rm_interactive_file_b")?
  let r2 = uu.invoke(s, "rm", ["-i", "test_rm_interactive_file_a", "test_rm_interactive_file_b"], stdin: b"Yesh")?
  uu.succeeds(r2)
  assert ! file_exists(s, "test_rm_interactive_file_a")?
  assert file_exists(s, "test_rm_interactive_file_b")?
}

# origin: uutils test_rm::test_interactive_always
test test_uu_rm_interactive_always { |ctx|
  let s = safe_scene(ctx)?
  for arg in ["-i", "--interactive", "--interactive=always", "--interactive=yes"] {
    uu.touch(s, "a")?
    uu.touch(s, "b")?
    let r = uu.invoke(s, "rm", [arg, "a", "b"], stdin: b"y\ny")?
    uu.succeeds(r)
    uu.no_stdout(r)
    assert ! file_exists(s, "a")?
    assert ! file_exists(s, "b")?
  }
}

# origin: uutils test_rm::test_interactive_never
test test_uu_rm_interactive_never { |ctx|
  let s = safe_scene(ctx)?
  for arg in ["never", "no", "none"] {
    uu.touch(s, "a")?
    uu.succeeds(uu.invoke(s, "chmod", ["0", "a"])?)
    let r = uu.invoke(s, "rm", [f"--interactive={arg}", "a"])?
    uu.succeeds(r)
    uu.no_output(r)
    assert ! file_exists(s, "a")?
  }
}

# origin: uutils test_rm::test_interactive_once_prompt
test test_uu_rm_interactive_once_prompt { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_interactive_once_recursive_prompt_file1")?
  uu.touch(s, "test_rm_interactive_once_recursive_prompt_file2")?
  uu.touch(s, "test_rm_interactive_once_recursive_prompt_file3")?
  uu.touch(s, "test_rm_interactive_once_recursive_prompt_file4")?
  let r1 = uu.invoke(s, "rm", ["--interactive=once", "test_rm_interactive_once_recursive_prompt_file1", "test_rm_interactive_once_recursive_prompt_file2", "test_rm_interactive_once_recursive_prompt_file3", "test_rm_interactive_once_recursive_prompt_file4"], stdin: b"y")?
  uu.succeeds(r1)
  uu.stderr_contains(r1, "remove 4 arguments?")
  assert ! file_exists(s, "test_rm_interactive_once_recursive_prompt_file1")?
  assert ! file_exists(s, "test_rm_interactive_once_recursive_prompt_file2")?
  assert ! file_exists(s, "test_rm_interactive_once_recursive_prompt_file3")?
  assert ! file_exists(s, "test_rm_interactive_once_recursive_prompt_file4")?
}

# origin: uutils test_rm::test_interactive_once_recursive_prompt
test test_uu_rm_interactive_once_recursive_prompt { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_interactive_once_recursive_prompt_file1")?
  let r1 = uu.invoke(s, "rm", ["--interactive=once", "-r", "test_rm_interactive_once_recursive_prompt_file1"], stdin: b"y")?
  uu.succeeds(r1)
  uu.stderr_contains(r1, "remove 1 argument recursively?")
  assert ! file_exists(s, "test_rm_interactive_once_recursive_prompt_file1")?
}

# origin: uutils test_rm::test_invalid_arg
test test_uu_rm_invalid_arg { |ctx|
  let s = safe_scene(ctx)?
  let r1 = uu.invoke(s, "rm", ["--definitely-invalid"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_rm::test_invalid_symlink
test test_uu_rm_invalid_symlink { |ctx|
  let s = safe_scene(ctx)?
  absolute_link(s, "test_rm_invalid_symlink", "test_rm_invalid_symlink")?
  let r1 = uu.invoke(s, "rm", ["test_rm_invalid_symlink"])?
  uu.succeeds(r1)
}

# origin: uutils test_rm::test_mixed_valid_and_dot_arguments
test test_uu_rm_mixed_valid_and_dot_arguments { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "deleteme")?
  uu.touch(s, "deleteme/file")?
  uu.mkdir(s, "d")?
  uu.touch(s, "keepme")?
  let r1 = uu.invoke(s, "rm", ["-rf", "deleteme", "d/.", "keepme"])?
  uu.fails(r1)
  assert ! dir_exists(s, "deleteme")?
  uu.stderr_contains(r1, "refusing to remove")
  assert ! file_exists(s, "keepme")?
}

# origin: uutils test_rm::test_multiple_files
test test_uu_rm_multiple_files { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_multiple_file_a")?
  uu.touch(s, "test_rm_multiple_file_b")?
  let r1 = uu.invoke(s, "rm", ["test_rm_multiple_file_a", "test_rm_multiple_file_b"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert ! file_exists(s, "test_rm_multiple_file_a")?
  assert ! file_exists(s, "test_rm_multiple_file_b")?
}

# origin: uutils test_rm::test_no_operand
test test_uu_rm_no_operand { |ctx|
  let s = safe_scene(ctx)?
  let r1 = uu.invoke(s, "rm", [])?
  uu.fails(r1)
  uu.stderr_only(r1, "rm: missing operand\nTry 'rm --help' for more information.\n")
}

# origin: uutils test_rm::test_non_empty_directory
test test_uu_rm_non_empty_directory { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "test_rm_non_empty_dir")?
  uu.touch(s, "test_rm_non_empty_dir/test_rm_non_empty_file_a")?
  let r1 = uu.invoke(s, "rm", ["-d", "test_rm_non_empty_dir"])?
  uu.fails(r1)
  uu.stderr_only(r1, "rm: cannot remove 'test_rm_non_empty_dir': Directory not empty\n")
  assert file_exists(s, "test_rm_non_empty_dir/test_rm_non_empty_file_a")?
  assert dir_exists(s, "test_rm_non_empty_dir")?
}

# origin: uutils test_rm::test_non_utf8_paths
test test_uu_rm_non_utf8_paths { |ctx|
  let s = safe_scene(ctx)?
  let file = uu.at_bytes(s, b"test_\xFF\xFE.txt")?
  file.write("")?
  assert file.is_file()?
  uu.succeeds(uu.invoke_paths(s, "rm", [file])?)
  assert ! file.exists()?
  let dir = uu.at_bytes(s, b"test_dir_\xFF\xFE")?
  dir.mkdir()?
  assert dir.is_dir()?
  uu.succeeds(uu.invoke_paths(s, "rm", [p"-r", dir])?)
  assert ! dir.exists()?
}

# origin: uutils test_rm::test_normal_files_with_dots_not_protected
test test_uu_rm_normal_files_with_dots_not_protected { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, ".hidden")?
  uu.touch(s, "..double_hidden")?
  uu.touch(s, "file.txt")?
  uu.mkdir(s, "dir.with.dots")?
  uu.touch(s, "dir.with.dots/file")?
  let r1 = uu.invoke(s, "rm", ["-f", ".hidden"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "rm", ["-f", "..double_hidden"])?
  uu.succeeds(r2)
  let r3 = uu.invoke(s, "rm", ["-f", "file.txt"])?
  uu.succeeds(r3)
  let r4 = uu.invoke(s, "rm", ["-rf", "dir.with.dots"])?
  uu.succeeds(r4)
  assert ! file_exists(s, ".hidden")?
  assert ! file_exists(s, "..double_hidden")?
  assert ! file_exists(s, "file.txt")?
  assert ! dir_exists(s, "dir.with.dots")?
}

# origin: uutils test_rm::test_one_file
test test_uu_rm_one_file { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_one_file")?
  let r1 = uu.invoke(s, "rm", ["test_rm_one_file"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert ! file_exists(s, "test_rm_one_file")?
}

# origin: uutils test_rm::test_one_file_system_same_device
test test_uu_rm_one_file_system_same_device { |ctx|
  let s = safe_scene(ctx)?
  let dir = "test_rm_one_file_system_dir"
  uu.mkdir(s, dir)?
  uu.mkdir(s, f"{dir}/subdir")?
  uu.touch(s, f"{dir}/subdir/file")?
  let r = uu.invoke(s, "rm", ["--one-file-system", "-rf", dir])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! file_exists(s, f"{dir}/subdir/file")?
  assert ! dir_exists(s, f"{dir}/subdir")?
  assert ! dir_exists(s, dir)?
}

# origin: uutils test_rm::test_only_first_error_recursive
test test_uu_rm_only_first_error_recursive { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "a/b")?
  uu.touch(s, "a/b/file")?
  uu.set_mode(s, "a/b", 0o0555)?
  let r1 = uu.invoke(s, "rm", ["-r", "-f", "a"])?
  uu.fails(r1)
  uu.stderr_only(r1, "rm: cannot remove 'a/b/file': Permission denied\n")
  assert file_exists(s, "a/b/file")?
  assert dir_exists(s, "a/b")?
  assert dir_exists(s, "a")?
  if dir_exists(s, "a/b")? { uu.set_mode(s, "a/b", 0o755)? }
}

# origin: uutils test_rm::test_preserve_root_all_same_device
test test_uu_rm_preserve_root_all_same_device { |ctx|
  let s = safe_scene(ctx)?
  let dir = "test_rm_preserve_root_all_dir"
  uu.mkdir(s, dir)?
  uu.mkdir(s, f"{dir}/subdir")?
  uu.touch(s, f"{dir}/subdir/file")?
  let r = uu.invoke(s, "rm", ["--preserve-root=all", "-rf", dir])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! dir_exists(s, dir)?
}

# origin: uutils test_rm::test_preserve_root_symlink_removal_without_trailing_slash
test test_uu_rm_preserve_root_symlink_removal_without_trailing_slash { |ctx|
  let s = safe_scene(ctx)?
  absolute_link(s, "/", "rootlink")?
  let r1 = uu.invoke(s, "rm", ["--preserve-root", "rootlink"])?
  uu.succeeds(r1)
  assert ! is_symlink(s, "rootlink")?
}






# origin: uutils test_rm::test_prompt_write_protected_no
test test_uu_rm_prompt_write_protected_no { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_prompt_write_protected_2")?
  uu.succeeds(uu.invoke(s, "chmod", ["0", "test_rm_prompt_write_protected_2"])?)
  let r = uu.invoke(s, "rm", ["---presume-input-tty", "test_rm_prompt_write_protected_2"], stdin: b"n")?
  uu.succeeds(r)
  uu.stderr_contains(r, "rm: remove write-protected regular empty file")
  assert file_exists(s, "test_rm_prompt_write_protected_2")?
}

# origin: uutils test_rm::test_prompt_write_protected_yes
test test_uu_rm_prompt_write_protected_yes { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_prompt_write_protected_1")?
  uu.succeeds(uu.invoke(s, "chmod", ["0", "test_rm_prompt_write_protected_1"])?)
  let r = uu.invoke(s, "rm", ["---presume-input-tty", "test_rm_prompt_write_protected_1"], stdin: b"y")?
  uu.succeeds(r)
  uu.stderr_contains(r, "rm: remove write-protected regular empty file")
  assert ! file_exists(s, "test_rm_prompt_write_protected_1")?
}

# origin: uutils test_rm::test_prompts
test test_uu_rm_prompts { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "a/")?
  uu.touch(s, "a/empty")?
  uu.touch(s, "a/empty-no-write")?
  uu.write(s, "a/f-no-write", "not-empty")?
  absolute_link(s, "a/empty-f", "a/slink")?
  absolute_link(s, ".", "a/slink-dot")?
  uu.mkdir(s, "a/b/")?
  uu.mkdir(s, "a/b-no-write/")?
  uu.succeeds(uu.invoke(s, "chmod", ["u-w", "a/f-no-write", "a/b-no-write/", "a/empty-no-write"])?)
  let r = uu.invoke(s, "rm", ["-ri", "a"], stdin: b"y\ny\ny\ny\ny\ny\ny\ny\ny\n")?
  let actual = [f"rm: {piece}".trim() for piece in r.stderr.utf8()?.split("rm: ") if piece != ""] |> sort |> collect()
  let expected = [
    "rm: descend into directory 'a'?",
    "rm: remove write-protected regular empty file 'a/empty-no-write'?",
    "rm: remove symbolic link 'a/slink'?",
    "rm: remove symbolic link 'a/slink-dot'?",
    "rm: remove write-protected regular file 'a/f-no-write'?",
    "rm: remove regular empty file 'a/empty'?",
    "rm: remove directory 'a/b'?",
    "rm: remove write-protected directory 'a/b-no-write'?",
    "rm: remove directory 'a'?",
  ] |> sort |> collect()
  assert actual.len() == expected.len()
  for idx in range(actual.len()) { assert actual[idx] == expected[idx] }
  assert ! dir_exists(s, "a")?
}

# origin: uutils test_rm::test_prompts_no_tty
test test_uu_rm_prompts_no_tty { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "a/")?
  uu.touch(s, "a/empty")?
  uu.touch(s, "a/empty-no-write")?
  uu.write(s, "a/f-no-write", "not-empty")?
  absolute_link(s, "a/empty-f", "a/slink")?
  absolute_link(s, ".", "a/slink-dot")?
  uu.mkdir(s, "a/b/")?
  uu.mkdir(s, "a/b-no-write/")?
  uu.succeeds(uu.invoke(s, "chmod", ["u-w", "a/f-no-write", "a/b-no-write/", "a/empty-no-write"])?)
  let r = uu.invoke(s, "rm", ["-r", "a"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert ! dir_exists(s, "a")?
}

# origin: uutils test_rm::test_recursive
test test_uu_rm_recursive { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "test_rm_recursive_directory")?
  uu.touch(s, "test_rm_recursive_directory/test_rm_recursive_file_a")?
  uu.touch(s, "test_rm_recursive_directory/test_rm_recursive_file_b")?
  let r1 = uu.invoke(s, "rm", ["-r", "test_rm_recursive_directory"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert ! dir_exists(s, "test_rm_recursive_directory")?
  assert ! file_exists(s, "test_rm_recursive_directory/test_rm_recursive_file_a")?
  assert ! file_exists(s, "test_rm_recursive_directory/test_rm_recursive_file_b")?
}

# origin: uutils test_rm::test_recursive_interactive
test test_uu_rm_recursive_interactive { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "a/b")?
  let r = uu.invoke(s, "rm", ["-i", "-r", "a"], stdin: b"y\ny\ny\n")?
  uu.succeeds(r)
  uu.stderr_only(r, "rm: descend into directory 'a'? rm: remove directory 'a/b'? rm: remove directory 'a'? ")
  assert ! dir_exists(s, "a/b")?
  assert ! dir_exists(s, "a")?
}

# origin: uutils test_rm::test_recursive_long_filepath
test test_uu_rm_recursive_long_filepath { |ctx|
  let s = safe_scene(ctx)?
  let mkdir = ["test_rm_recursive_directory/" for _ in range(35)].join("")
  let file = mkdir + "test_rm_recursive_file_a"
  assert file.byte_len() > 1000
  uu.mkdir_all(s, mkdir)?
  uu.touch(s, file)?
  let r = uu.invoke(s, "rm", ["-r", "test_rm_recursive_directory"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert ! dir_exists(s, "test_rm_recursive_directory")?
  assert ! file_exists(s, file)?
}

# origin: uutils test_rm::test_recursive_multiple
test test_uu_rm_recursive_multiple { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "test_rm_recursive_directory")?
  uu.touch(s, "test_rm_recursive_directory/test_rm_recursive_file_a")?
  uu.touch(s, "test_rm_recursive_directory/test_rm_recursive_file_b")?
  let r1 = uu.invoke(s, "rm", ["-r", "-r", "-r", "test_rm_recursive_directory"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert ! dir_exists(s, "test_rm_recursive_directory")?
  assert ! file_exists(s, "test_rm_recursive_directory/test_rm_recursive_file_a")?
  assert ! file_exists(s, "test_rm_recursive_directory/test_rm_recursive_file_b")?
}

# origin: uutils test_rm::test_recursive_remove_unreadable_subdir
test test_uu_rm_recursive_remove_unreadable_subdir { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir_all(s, "foo/bar")?
  uu.touch(s, "foo/bar/baz")?
  uu.set_mode(s, "foo/bar", 0o0000)?
  let r1 = uu.invoke(s, "rm", ["-r", "-f", "foo"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "Permission denied")
  uu.stderr_contains(r1, "foo/bar")
  uu.set_mode(s, "foo/bar", 0o0755)?
  if dir_exists(s, "foo/bar")? { uu.set_mode(s, "foo/bar", 0o755)? }
}

# origin: uutils test_rm::test_recursive_symlink_loop
test test_uu_rm_recursive_symlink_loop { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "d")?
  uu.symlink(s, ".", "d/link")?
  let r1 = uu.invoke(s, "rm", ["-i", "-r", "d"], stdin: b"y\ny\ny\n")?
  uu.succeeds(r1)
  uu.stderr_only(r1, "rm: descend into directory 'd'? rm: remove symbolic link 'd/link'? rm: remove directory 'd'? ")
  assert ! is_symlink(s, "d/link")?
  assert ! dir_exists(s, "d")?
}

# origin: uutils test_rm::test_remove_inaccessible_dir
test test_uu_rm_remove_inaccessible_dir { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "test_rm_protected")?
  let r1 = uu.invoke(s, "chmod", ["0", "test_rm_protected"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "rm", ["-rf", "test_rm_protected"])?
  uu.succeeds(r2)
  assert ! dir_exists(s, "test_rm_protected")?
}

# origin: uutils test_rm::test_rm_directory_not_executable
test test_uu_rm_rm_directory_not_executable { |ctx|
  let s = safe_scene(ctx)?
  for name in ["a/0", "a/1/2", "a/2", "a/3", "b/3"] { uu.mkdir_all(s, name)? }
  uu.succeeds(uu.invoke(s, "chmod", ["u-x", "a/1"])?)
  uu.succeeds(uu.invoke(s, "chmod", ["u-x", "b"])?)
  let r = uu.invoke(s, "rm", ["-rf", "a", "b"])?
  uu.fails(r)
  uu.stderr_contains(r, "rm: cannot remove 'a/1/2': Permission denied")
  uu.stderr_contains(r, "rm: cannot remove 'b/3': Permission denied")
  assert ! dir_exists(s, "a/0")?
  assert dir_exists(s, "a/1")?
  assert ! dir_exists(s, "a/2")?
  assert ! dir_exists(s, "a/3")?
  uu.succeeds(uu.invoke(s, "chmod", ["u+x", "b"])?)
  assert dir_exists(s, "b/3")?
  uu.set_mode(s, "a/1", 0o755)?
}

# origin: uutils test_rm::test_rm_directory_not_writable
test test_uu_rm_rm_directory_not_writable { |ctx|
  let s = safe_scene(ctx)?
  for name in ["b/a/p", "b/c", "b/d"] { uu.mkdir_all(s, name)? }
  uu.succeeds(uu.invoke(s, "chmod", ["ug-w", "b/a"])?)
  let r = uu.invoke(s, "rm", ["-rf", "b"])?
  uu.fails(r)
  uu.stderr_only(r, "rm: cannot remove 'b/a/p': Permission denied\n")
  assert dir_exists(s, "b/a/p")?
  assert ! dir_exists(s, "b/c")?
  assert ! dir_exists(s, "b/d")?
  uu.set_mode(s, "b/a", 0o755)?
}

# origin: uutils test_rm::test_rm_recursive_long_path_safe_traversal
test test_uu_rm_rm_recursive_long_path_safe_traversal { |ctx|
  let s = safe_scene(ctx)?
  var deep = "rm_deep"
  uu.mkdir(s, deep)?
  for i in range(12) {
    let name = ["z" for _ in range(80)].join("") + f"{i}"
    deep = f"{deep}/{name}"
    uu.mkdir_all(s, deep)?
  }
  uu.write(s, "rm_deep/test1.txt", "content1")?
  uu.write(s, f"{deep}/test2.txt", "content2")?
  uu.succeeds(uu.invoke(s, "rm", ["-rf", "rm_deep"])?)
  assert ! dir_exists(s, "rm_deep")?
}

# origin: uutils test_rm::test_rm_recursive_very_deep_hierarchy
test test_uu_rm_rm_recursive_very_deep_hierarchy { |ctx|
  test.timeout(ctx, 300s)
  let s = safe_scene(ctx)?
  uu.mkdir(s, "deep")?
  var fd = unix.open_fd(uu.at(s, "deep"), flags: ["directory"])?
  var depth = 0
  while depth < 32768 {
    let parent = fs.open_root(fp"/proc/self/fd/{fd}")?
    let made = parent.mkdir(p"a", mode: 0o755)
    parent.close()?
    if let Err(failure) = made {
      if failure.errno == 36 { break } else { unix.close_fd(fd)?; return Err(failure) }
    }
    let opened = unix.open_fd(fp"/proc/self/fd/{fd}/a", flags: ["directory"])
    if let Err(failure) = opened {
      if failure.errno == 36 { break } else { unix.close_fd(fd)?; return Err(failure) }
    }
    let next = opened?
    unix.close_fd(fd)?
    fd = next
    depth += 1
  }
  unix.close_fd(fd)?
  let r = uu.invoke(s, "rm", ["-rf", "deep"], timeout: 240s)?
  uu.succeeds(r)
  if depth < 32768 { test.skip(f"filesystem stops nesting at {depth} levels") }
  uu.no_output(r)
  assert ! dir_exists(s, "deep")?
}

# origin: uutils test_rm::test_silently_accepts_presume_input_tty2
test test_uu_rm_silently_accepts_presume_input_tty2 { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_silently_accepts_presume_input_tty2")?
  let r1 = uu.invoke(s, "rm", ["---presume-input-tty", "test_rm_silently_accepts_presume_input_tty2"])?
  uu.succeeds(r1)
  assert ! file_exists(s, "test_rm_silently_accepts_presume_input_tty2")?
}

# origin: uutils test_rm::test_symlink_dir
test test_uu_rm_symlink_dir { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "test_rm_symlink_dir_directory")?
  absolute_link(s, "test_rm_symlink_dir_directory", "test_rm_symlink_dir_link")?
  let r1 = uu.invoke(s, "rm", ["test_rm_symlink_dir_link"])?
  uu.succeeds(r1)
}

# origin: uutils test_rm::test_symlink_to_dot_protection
test test_uu_rm_symlink_to_dot_protection { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "subdir")?
  uu.touch(s, "subdir/file")?
  uu.touch(s, "topfile")?
  absolute_link(s, ".", "subdir/dot_link")?
  absolute_link(s, "..", "subdir/dotdot_link")?
  let r1 = uu.invoke(s, "rm", ["-f", "subdir/dot_link"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "rm", ["-f", "subdir/dotdot_link"])?
  uu.succeeds(r2)
  assert ! is_symlink(s, "subdir/dot_link")?
  assert ! is_symlink(s, "subdir/dotdot_link")?
  assert dir_exists(s, "subdir")?
  assert file_exists(s, "subdir/file")?
  assert file_exists(s, "topfile")?
}

# origin: uutils test_rm::test_symlink_to_readonly_no_prompt
test test_uu_rm_symlink_to_readonly_no_prompt { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "foo")?
  uu.set_mode(s, "foo", 0o444)?
  absolute_link(s, "foo", "bar")?
  let r1 = uu.invoke(s, "rm", ["---presume-input-tty", "bar"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert ! is_symlink(s, "bar")?
  if dir_exists(s, "foo")? { uu.set_mode(s, "foo", 0o755)? }
}

# origin: uutils test_rm::test_uchild_when_run_no_wait_with_a_blocking_command
test test_uu_rm_uchild_when_run_no_wait_with_a_blocking_command { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "a")?
  uu.touch(s, "a/empty")?
  uu.mkfifo(s, "input")?
  let input = uu.at(s, "input")
  let output = uu.at(s, "output")
  let wrapper_error = uu.at(s, "wrapper-error")
  let words = uu.argv(s, "rm", [p"-riv", p"a"])?
  let argv = ["sh", "-c", r"""fifo=$1; shift; exec "$@" < "$fifo" 2>&1""", "rm-stdio", input.display()].extend([word.display() for word in words])
  let child = spawn process.command_argv(p"/bin/sh", argv, s.root, {}, b"", output, wrapper_error) ?
  defer child.cancel(kill_after: 0ms)
  let writer = unix.open_fd(input, write: true)?
  defer unix.close_fd(writer)
  time.sleep(1000ms)?
  assert process.wait_timeout([child], 0ms)? == null
  assert output.read_bytes()? == b"rm: descend into directory 'a'? "
  assert unix.write_fd(writer, b"y\n")? == 2
  time.sleep(1000ms)?
  assert process.wait_timeout([child], 0ms)? == null
  let second = output.read_bytes()?
  assert second == b"rm: descend into directory 'a'? rm: remove regular empty file 'a/empty'? "
  assert unix.write_fd(writer, b"y\n")? == 2
  time.sleep(1000ms)?
  assert process.wait_timeout([child], 0ms)? == null
  let third = output.read_bytes()?
  assert third[second.len()..] == b"rm: remove directory 'a'? "
  assert third.len() - second.len() == 26
  assert wrapper_error.read_bytes()? == b""
  assert unix.write_fd(writer, b"y\n")? == 2
  let waited = process.wait_any([child])?
  assert waited.status.exited_with(0)
  assert output.read_bytes()?[third.len()..] == b"removed 'a/empty'\nremoved directory 'a'\n"
  assert wrapper_error.read_bytes()? == b""
}

# origin: uutils test_rm::test_unreadable_and_nonempty_dir
test test_uu_rm_unreadable_and_nonempty_dir { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir_all(s, "a/b")?
  uu.set_mode(s, "a", 0o0333)?
  let r1 = uu.invoke(s, "rm", ["-r", "-f", "a"])?
  uu.fails(r1)
  uu.stderr_only(r1, "rm: cannot remove 'a': Permission denied\n")
  assert dir_exists(s, "a/b")?
  assert dir_exists(s, "a")?
  if dir_exists(s, "a")? { uu.set_mode(s, "a", 0o755)? }
}

# origin: uutils test_rm::test_verbose
test test_uu_rm_verbose { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_verbose_file_a")?
  uu.touch(s, "test_rm_verbose_file_b")?
  let r1 = uu.invoke(s, "rm", ["-v", "test_rm_verbose_file_a", "test_rm_verbose_file_b"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "removed 'test_rm_verbose_file_a'\nremoved 'test_rm_verbose_file_b'\n")
}

# origin: uutils test_rm::test_verbose_slash
test test_uu_rm_verbose_slash { |ctx|
  let s = safe_scene(ctx)?
  let dir = "test_rm_verbose_slash_directory"
  let file = "test_rm_verbose_slash_directory/test_rm_verbose_slash_file_a"
  uu.mkdir(s, dir)?
  uu.touch(s, file)?
  let r = uu.invoke(s, "rm", ["-r", "-f", "-v", f"{dir}///"])?
  uu.succeeds(r)
  uu.stdout_only(r, f"removed '{file}'\nremoved directory '{dir}/'\n")
  assert ! dir_exists(s, dir)?
  assert ! file_exists(s, file)?
}

# origin: uutils test_rm::test_verbose_write_error_does_not_panic
test test_uu_rm_verbose_write_error_does_not_panic { |ctx|
  let s = safe_scene(ctx)?
  uu.touch(s, "test_rm_verbose_write_error_file")?
  let r1 = uu.invoke(s, "rm", ["-v", "test_rm_verbose_write_error_file"], stdout: p"/dev/full")?
  uu.fails(r1)
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "No space left on device")
  assert ! ("panicked" in r1.stderr.utf8()?)
  assert ! file_exists(s, "test_rm_verbose_write_error_file")?
}

# origin: uutils test_rm::test_verbose_write_error_does_not_panic_dir
test test_uu_rm_verbose_write_error_does_not_panic_dir { |ctx|
  let s = safe_scene(ctx)?
  uu.mkdir(s, "test_rm_verbose_write_error_dir")?
  uu.touch(s, "test_rm_verbose_write_error_dir/a")?
  let r1 = uu.invoke(s, "rm", ["-r", "-v", "test_rm_verbose_write_error_dir"], stdout: p"/dev/full")?
  uu.fails(r1)
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "No space left on device")
  assert ! ("panicked" in r1.stderr.utf8()?)
  assert ! file_exists(s, "test_rm_verbose_write_error_dir/a")?
  assert ! dir_exists(s, "test_rm_verbose_write_error_dir")?
}

# origin: uutils test_rm::test_verbose_write_error_reported_once
test test_uu_rm_verbose_write_error_reported_once { |ctx|
  let s = safe_scene(ctx)?
  let dir = "test_rm_verbose_write_error_once"
  uu.mkdir(s, dir)?
  for name in ["alpha", "bravo", "charlie", "delta"] { uu.touch(s, f"{dir}/{name}")? }
  let r = uu.invoke(s, "rm", ["-r", "-v", dir], stdout: p"/dev/full")?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  assert r.stderr.utf8()?.split("No space left on device").len() - 1 == 1
  assert ! dir_exists(s, dir)?
}

