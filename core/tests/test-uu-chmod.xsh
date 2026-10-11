##! Transcribed from the uutils coreutils chmod integration tests.

use support.uu as uu

proc make_file(s: uu.Scene, name: Str, mode: Int) [fs, error] -> Result[Unit, Error] {
  uu.touch(s, name)?
  uu.set_mode(s, name, mode.bit_and(0o7777))?
  Ok()
}

# Upstream scene symlinks use absolute targets; relative links are explicit.
proc absolute_link(s: uu.Scene, target: Str, name: Str) [fs, error] -> Result[Unit, Error] {
  uu.symlink(s, uu.at(s, target).display(), name)?
  Ok()
}

# script(1) supplies the stderr terminal; stdout stays in its own capture file.
proc tty_stderr(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let stdout = uu.at(s, ".tty-stdout")
  let stderr = uu.at(s, ".tty-stderr")
  let outer_error = uu.at(s, ".tty-wrapper-error")
  let words = uu.argv(s, "chmod", [Path(word) for word in args])?
  let quoted = ["'" + word.display().replace("'", with: "'\\''") + "'" for word in words]
  let command = quoted.join(" ") + " > '" + stdout.display().replace("'", with: "'\\''") + "'"
  let status = process.run(process.command_argv(p"/usr/bin/script", ["script", "-q", "-e", "-c", command, "/dev/null"], s.root, {}, b"", stderr, outer_error))?
  assert outer_error.read_bytes()? == b"", "PTY wrapper failed"
  Ok({util: "chmod", args: args, status: status.exit_code()?, stdout: stdout.read_bytes()?, stderr: stderr.read_bytes()?})
}

# origin: uutils test_chmod::diagnostics::test_snippet_marks_a_non_octal_mode
test test_uu_chmod_diagnostics_snippet_marks_a_non_octal_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "probe")?
  let r = tty_stderr(s, ["779", "probe"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "779")
}

# origin: uutils test_chmod::test_changes_from_identical_reference
test test_uu_chmod_changes_from_identical_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  {
    let r = uu.invoke(s, "chmod", ["-c", "--reference=file", "file"])?
    uu.succeeds(r)
    uu.no_stdout(r)
  }
}

# origin: uutils test_chmod::test_chmod_dangling_symlink_recursive_combos
test test_uu_chmod_chmod_dangling_symlink_recursive_combos { |ctx|
  for flags in [["-R"], ["-R", "-H"], ["-R", "-L"]] {
    let s = uu.scene(ctx)?
    absolute_link(s, "nonexistent_file", "symlink")?
    let r = uu.invoke(s, "chmod", flags.extend(["u+x", "symlink"]), umask: 0o022)?
    uu.fails(r)
    uu.stderr_is(r, "chmod: cannot operate on dangling symlink 'symlink'\n")
    assert fs.stat(uu.at(s, "symlink"), follow_symlinks: false)?.mode == 0o120777
  }
}

# origin: uutils test_chmod::test_chmod_dereference_symlink
test test_uu_chmod_chmod_dereference_symlink { |ctx|
  let s = uu.scene(ctx)?
  make_file(s, "file", 0o664)?
  absolute_link(s, "file", "symlink")?
  let r = uu.invoke(s, "chmod", ["--dereference", "u+x", "symlink"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(uu.at(s, "file"))?.mode == 0o100764
  assert fs.stat(uu.at(s, "symlink"), follow_symlinks: false)?.mode == 0o120777
}

# origin: uutils test_chmod::test_chmod_error_permissions
test test_uu_chmod_chmod_error_permissions { |ctx|
  let s = uu.scene(ctx)?
  make_file(s, "file", 0o777)?
  let r = uu.invoke(s, "chmod", ["-w", "file"], umask: 0o022)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "chmod: file: new permissions are r-xrwxrwx, not r-xr-xr-x\n")
  assert fs.stat(uu.at(s, "file"))?.mode == 0o100577
}

# origin: uutils test_chmod::test_chmod_file_after_non_existing_file
test test_uu_chmod_chmod_file_after_non_existing_file { |ctx|
  let s = uu.scene(ctx)?
  make_file(s, "file", 0o664)?
  make_file(s, "file2", 0o664)?
  let r = uu.invoke(s, "chmod", ["u+x", "does-not-exist", "file"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "chmod: cannot access 'does-not-exist': No such file or directory")
  assert fs.stat(uu.at(s, "file"))?.mode == 0o100764
  let quiet = uu.invoke(s, "chmod", ["u+x", "--q", "does-not-exist", "file2"])?
  uu.fails_with_code(quiet, 1)
  uu.no_stderr(quiet)
  assert fs.stat(uu.at(s, "file2"))?.mode == 0o100764
}

# origin: uutils test_chmod::test_chmod_file_symlink_after_non_existing_file
test test_uu_chmod_chmod_file_symlink_after_non_existing_file { |ctx|
  let s = uu.scene(ctx)?
  make_file(s, "file", 0o664)?
  let dangling = "test_chmod_symlink_non_existing_file_symlink"
  absolute_link(s, "test_chmod_symlink_non_existing_file", dangling)?
  absolute_link(s, "file", "file_symlink")?
  let r = uu.invoke(s, "chmod", ["u+x", "-v", dangling, "file_symlink"])?
  uu.fails_with_code(r, 1)
  uu.stdout_contains(r, f"'{dangling}' could not be accessed")
  uu.stderr_contains(r, f"cannot operate on dangling symlink '{dangling}'")
  assert fs.stat(uu.at(s, "file_symlink"), follow_symlinks: true)?.mode == 0o100764
}

# origin: uutils test_chmod::test_chmod_hyper_recursive_directory_tree_does_not_fail
test test_uu_chmod_chmod_hyper_recursive_directory_tree_does_not_fail { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, ["a/" for _ in range(400)].join(""))?
  uu.succeeds(uu.invoke(s, "chmod", ["-R", "777", "a"])?)
}

# origin: uutils test_chmod::test_chmod_inaccessible_file_reports_permission_denied
test test_uu_chmod_chmod_inaccessible_file_reports_permission_denied { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "locked")?
  make_file(s, "locked/file", 0o100644)?
  uu.set_mode(s, "locked", 0o000)?
  let r = uu.invoke(s, "chmod", ["644", "locked/file"])?
  uu.set_mode(s, "locked", 0o755)?
  uu.fails(r)
  uu.stderr_is(r, "chmod: cannot access 'locked/file': Permission denied\n")
}

# origin: uutils test_chmod::test_chmod_keep_setgid
test test_uu_chmod_chmod_keep_setgid { |ctx|
  for row in [
    {before: 0o7777, arg: "777", after: 0o46777},
    {before: 0o7777, arg: "=777", after: 0o40777},
    {before: 0o7777, arg: "0777", after: 0o46777},
    {before: 0o7777, arg: "=0777", after: 0o40777},
    {before: 0o7777, arg: "00777", after: 0o40777},
    {before: 0o2444, arg: "a+wx", after: 0o42777},
    {before: 0o2444, arg: "a=wx", after: 0o42333},
    {before: 0o1444, arg: "g+s", after: 0o43444},
    {before: 0o4444, arg: "u-s", after: 0o40444},
    {before: 0o7444, arg: "a-s", after: 0o41444},
  ] {
    let s = uu.scene(ctx)?
    uu.mkdir(s, "dir")?
    uu.set_mode(s, "dir", row.before)?
    uu.succeeds(uu.invoke(s, "chmod", [row.arg, "dir"])?)
    assert fs.stat(uu.at(s, "dir"))?.mode == row.after
  }
}

# origin: uutils test_chmod::test_chmod_many_options
test test_uu_chmod_chmod_many_options { |ctx|
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100444)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100444
    let r = uu.invoke(s, "chmod", ["-r,a+w", "file"], umask: 0)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100222
  }
}

# origin: uutils test_chmod::test_chmod_no_dereference_symlink
test test_uu_chmod_chmod_no_dereference_symlink { |ctx|
  let s = uu.scene(ctx)?
  make_file(s, "file", 0o664)?
  absolute_link(s, "file", "symlink")?
  let r = uu.invoke(s, "chmod", ["--no-dereference", "u+x", "symlink"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(uu.at(s, "file"))?.mode == 0o100664
  assert fs.stat(uu.at(s, "symlink"), follow_symlinks: false)?.mode == 0o120777
}

# origin: uutils test_chmod::test_chmod_non_existing_file
test test_uu_chmod_chmod_non_existing_file { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "chmod", ["-R", "-r,a+w", "does-not-exist"])?
    uu.fails(r)
    uu.stderr_contains(r, "cannot access 'does-not-exist': No such file or directory")
  }
}

# origin: uutils test_chmod::test_chmod_non_existing_file_silent
test test_uu_chmod_chmod_non_existing_file_silent { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "chmod", ["-R", "--quiet", "-r,a+w", "does-not-exist"])?
    uu.fails_with_code(r, 1)
    uu.no_stderr(r)
  }
}

# origin: uutils test_chmod::test_chmod_non_utf8_paths
test test_uu_chmod_chmod_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let file = uu.at_bytes(s, b"test_\xFF\xFE.txt")?
  file.write("")?
  file.chmod(0o644)?
  assert fs.stat(file)?.mode.bit_and(0o777) == 0o644
  let r = uu.invoke_paths(s, "chmod", [p"755", file])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(file)?.mode.bit_and(0o777) == 0o755
  let file2 = uu.at_bytes(s, b"file_\xC0\x80.dat")?
  file2.write("")?
  file2.chmod(0o666.clear_bits(fs.umask()?))?
  let both = uu.invoke_paths(s, "chmod", [p"644", file, file2])?
  uu.succeeds(both)
  uu.no_stderr(both)
  assert fs.stat(file)?.mode.bit_and(0o777) == 0o644
  assert fs.stat(file2)?.mode.bit_and(0o777) == 0o644
}

# origin: uutils test_chmod::test_chmod_octal
test test_uu_chmod_chmod_octal { |ctx|
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["0700", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100700
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["0070", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100070
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["0007", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100007
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100700)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100700
    let r = uu.invoke(s, "chmod", ["-0700", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100060)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100060
    let r = uu.invoke(s, "chmod", ["-0070", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100001)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100001
    let r = uu.invoke(s, "chmod", ["-0007", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100600)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100600
    let r = uu.invoke(s, "chmod", ["+0100", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100700
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100050)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100050
    let r = uu.invoke(s, "chmod", ["+0020", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100070
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100003)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100003
    let r = uu.invoke(s, "chmod", ["+0004", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100007
  }
}

# origin: uutils test_chmod::test_chmod_operator_only_still_calls_syscall
test test_uu_chmod_chmod_operator_only_still_calls_syscall { |ctx|
  if user.current()?.uid == 0 or fs.stat(p"/")?.uid != 0 { test.skip("requires a non-root caller and root-owned /") }
  let s = uu.scene(ctx)?
  for op in ["+", "-", "="] {
    let r = uu.invoke(s, "chmod", [op, "/"])?
    uu.fails(r)
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "changing permissions of '/'")
  }
}

# origin: uutils test_chmod::test_chmod_preserve_root
test test_uu_chmod_chmod_preserve_root { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "chmod", ["-R", "--preserve-root", "755", "/"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "chmod: it is dangerous to operate recursively on '/'")
  }
}

# origin: uutils test_chmod::test_chmod_preserve_root_symlink_during_recursion
test test_uu_chmod_chmod_preserve_root_symlink_during_recursion { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tree")?
  uu.symlink(s, "/", "tree/link")?
  let r = uu.invoke(s, "chmod", ["-R", "-L", "--preserve-root", "755", "tree"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "chmod: it is dangerous to operate recursively on 'tree/link' (same as '/')")
}

# origin: uutils test_chmod::test_chmod_preserve_root_with_paths_that_resolve_to_root
test test_uu_chmod_chmod_preserve_root_with_paths_that_resolve_to_root { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "chmod", ["-R", "--preserve-root", "755", "//"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "chmod: it is dangerous to operate recursively on '//' (same as '/')")
  }
}

# origin: uutils test_chmod::test_chmod_recursive_read_permission
test test_uu_chmod_chmod_recursive_read_permission { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.set_mode(s, "a/b", 0o311)?
  uu.set_mode(s, "a", 0o311)?
  let r = uu.invoke(s, "chmod", ["-R", "u+r", "a"])?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "a"))?.mode == 0o40711
  assert fs.stat(uu.at(s, "a/b"))?.mode == 0o40711
}

# origin: uutils test_chmod::test_chmod_recursive_reference_does_not_follow_inner_symlink
test test_uu_chmod_chmod_recursive_reference_does_not_follow_inner_symlink { |ctx|
  let s = uu.scene(ctx)?
  make_file(s, "outside_target", 0o600)?
  make_file(s, "ref", 0o745)?
  uu.mkdir(s, "tree")?
  absolute_link(s, "outside_target", "tree/link")?
  uu.succeeds(uu.invoke(s, "chmod", ["-R", "--reference=ref", "tree"])?)
  assert uu.mode(s, "outside_target")? == 0o600
  assert uu.mode(s, "tree")? == 0o745
}

# origin: uutils test_chmod::test_chmod_recursive_symlink_combinations
test test_uu_chmod_chmod_recursive_symlink_combinations { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.mkdir(s, "target_dir")?
  make_file(s, "target_file", 0o644)?
  make_file(s, "target_dir/file", 0o644)?
  absolute_link(s, "target_dir", "dir/link_dir")?
  absolute_link(s, "target_file", "dir/link_file")?
  let r = uu.invoke(s, "chmod", ["-R", "-L", "go-rwx", "dir"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(uu.at(s, "target_file"))?.mode == 0o100600
  assert fs.stat(uu.at(s, "target_dir/file"))?.mode == 0o100600
}

# origin: uutils test_chmod::test_chmod_recursive_symlink_during_traversal
test test_uu_chmod_chmod_recursive_symlink_during_traversal { |ctx|
  for row in [
    {flags: ["-R"], follow: false},
    {flags: ["-R", "-H"], follow: false},
    {flags: ["-R", "-L"], follow: true},
    {flags: ["-R", "-P"], follow: false},
  ] {
    let s = uu.scene(ctx)?
    uu.mkdir(s, "target_dir")?
    make_file(s, "target_dir/file_in_target", 0o644)?
    uu.mkdir(s, "dir")?
    absolute_link(s, "target_dir", "dir/link_dir")?
    let r = uu.invoke(s, "chmod", row.flags.extend(["go-rwx", "dir"]))?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert fs.stat(uu.at(s, "target_dir/file_in_target"))?.mode == (if row.follow { 0o100600 } else { 0o100644 })
  }
}

# origin: uutils test_chmod::test_chmod_recursive_symlink_option_like_mode
test test_uu_chmod_chmod_recursive_symlink_option_like_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d/sub")?
  uu.set_mode(s, "d", 0o755)?
  uu.set_mode(s, "d/sub", 0o755)?
  make_file(s, "d/sub/target", 0o644)?
  uu.symlink(s, "target", "d/sub/link")?
  let r = uu.invoke(s, "chmod", ["-R", "-w", "d"], umask: 0o022)?
  uu.succeeds(r)
  assert uu.mode(s, "d")? == 0o555
  assert uu.mode(s, "d/sub")? == 0o555
  assert uu.mode(s, "d/sub/target")? == 0o444
}

# origin: uutils test_chmod::test_chmod_recursive_symlink_to_directory_command_line
test test_uu_chmod_chmod_recursive_symlink_to_directory_command_line { |ctx|
  for row in [
    {flags: ["-R"], follow: true},
    {flags: ["-R", "-H"], follow: true},
    {flags: ["-R", "-L"], follow: true},
    {flags: ["-R", "-P"], follow: false},
  ] {
    let s = uu.scene(ctx)?
    uu.mkdir(s, "target_dir")?
    make_file(s, "target_dir/file_in_target", 0o644)?
    absolute_link(s, "target_dir", "link_dir")?
    let r = uu.invoke(s, "chmod", row.flags.extend(["go-rwx", "link_dir"]))?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert fs.stat(uu.at(s, "target_dir/file_in_target"))?.mode == (if row.follow { 0o100600 } else { 0o100644 })
  }
}

# origin: uutils test_chmod::test_chmod_reference_file
test test_uu_chmod_chmod_reference_file { |ctx|
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100070)?
    make_file(s, "reference", 0o247)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100070
    let r = uu.invoke(s, "chmod", ["--reference", "reference", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100247
  }
}

# origin: uutils test_chmod::test_chmod_symlink_cycles
test test_uu_chmod_chmod_symlink_cycles { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b/c")?
  absolute_link(s, "a", "a/b/c/d")?
  for name in ["a", "a/b", "a/b/c"] { uu.set_mode(s, name, 0o755)? }
  let r = uu.invoke(s, "chmod", ["-vRL", "+r", "a"])?
  let lines = r.stdout.utf8()?.lines()
  for name in ["a", "a/b", "a/b/c", "a/b/c/d"] {
    assert f"mode of '{name}' retained as 0755 (rwxr-xr-x)" in lines
  }
  for name in ["a/b/c/d/b", "a/b/c/d/b/c", "a/b/c/d/b/c/d"] {
    assert ! (f"mode of '{name}' retained as 0755 (rwxr-xr-x)" in r.stdout.utf8()?)
  }
}

# origin: uutils test_chmod::test_chmod_symlink_non_existing_file
test test_uu_chmod_chmod_symlink_non_existing_file { |ctx|
  let s = uu.scene(ctx)?
  let link = "test_chmod_symlink_non_existing_file_symlink"
  absolute_link(s, "test_chmod_symlink_non_existing_file", link)?
  let expected = f"'{link}' could not be accessed"
  let error = f"cannot operate on dangling symlink '{link}'"
  let verbose = uu.invoke(s, "chmod", ["755", "-v", link])?
  uu.fails_with_code(verbose, 1)
  uu.stdout_contains(verbose, expected)
  uu.stderr_contains(verbose, error)
  let quiet = uu.invoke(s, "chmod", ["755", "-v", "-f", link])?
  uu.fails_with_code(quiet, 1)
  uu.no_stderr(quiet)
  uu.stdout_contains(quiet, expected)
  let plain = uu.invoke(s, "chmod", ["755", link])?
  uu.fails_with_code(plain, 1)
  uu.no_stdout(plain)
  uu.stderr_contains(plain, error)
}

# origin: uutils test_chmod::test_chmod_symlink_non_existing_file_recursive
test test_uu_chmod_chmod_symlink_non_existing_file_recursive { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_chmod_symlink_non_existing_file_directory"
  let link = "test_chmod_symlink_non_existing_file_recursive_symlink"
  uu.mkdir(s, dir)?
  absolute_link(s, "test_chmod_symlink_non_existing_file_recursive", f"{dir}/{link}")?
  let plain = uu.invoke(s, "chmod", ["-R", "755", dir])?
  uu.succeeds(plain)
  uu.no_output(plain)
  let expected = f"mode of '{dir}' retained as 0755 (rwxr-xr-x)"
  let verbose = uu.invoke(s, "chmod", ["-R", "-v", "755", dir])?
  uu.succeeds(verbose)
  uu.stdout_contains(verbose, expected)
  uu.no_stderr(verbose)
  let quiet = uu.invoke(s, "chmod", ["-R", "-v", "-f", "755", dir])?
  uu.succeeds(quiet)
  uu.stdout_contains(quiet, expected)
  uu.no_stderr(quiet)
}

# origin: uutils test_chmod::test_chmod_symlink_recursive_final_traversal_flag
test test_uu_chmod_chmod_symlink_recursive_final_traversal_flag { |ctx|
  let s = uu.scene(ctx)?
  absolute_link(s, "nonexistent_file", "symlink")?
  let r = uu.invoke(s, "chmod", ["755", "-R", "-H", "-L", "-H", "-L", "-P", "symlink"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "symlink"), follow_symlinks: false)?.mode == 0o120777
}

# origin: uutils test_chmod::test_chmod_symlink_target_no_dereference
test test_uu_chmod_chmod_symlink_target_no_dereference { |ctx|
  let s = uu.scene(ctx)?
  make_file(s, "a", 0o644)?
  absolute_link(s, "a", "symlink")?
  let r = uu.invoke(s, "chmod", ["--no-dereference", "755", "symlink"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert fs.stat(uu.at(s, "a"))?.mode == 0o100644
}

# origin: uutils test_chmod::test_chmod_symlink_to_dangling_recursive_no_traverse
test test_uu_chmod_chmod_symlink_to_dangling_recursive_no_traverse { |ctx|
  let s = uu.scene(ctx)?
  absolute_link(s, "nonexistent_file", "symlink")?
  let r = uu.invoke(s, "chmod", ["755", "-R", "-P", "symlink"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert fs.stat(uu.at(s, "symlink"), follow_symlinks: false)?.mode == 0o120777
}

# origin: uutils test_chmod::test_chmod_symlink_to_dangling_target_dereference
test test_uu_chmod_chmod_symlink_to_dangling_target_dereference { |ctx|
  let s = uu.scene(ctx)?
  absolute_link(s, "nonexistent_file", "symlink")?
  let r = uu.invoke(s, "chmod", ["--dereference", "u+x", "symlink"])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot operate on dangling symlink 'symlink'")
}

# origin: uutils test_chmod::test_chmod_symlink_two_links_same_dir
test test_uu_chmod_chmod_symlink_two_links_same_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "base/realdir")?
  uu.touch(s, "base/realdir/file")?
  absolute_link(s, "base/realdir", "base/link1")?
  absolute_link(s, "base/realdir", "base/link2")?
  let r = uu.invoke(s, "chmod", ["-vRL", "+r", "base"])?
  for name in ["base/realdir/file", "base/link1/file", "base/link2/file"] {
    uu.stdout_contains(r, f"mode of '{name}'")
  }
}

# origin: uutils test_chmod::test_chmod_traverse_symlink_combo
test test_uu_chmod_chmod_traverse_symlink_combo { |ctx|
  for row in [
    {flags: ["-R"], after: 0o100664},
    {flags: ["-R", "-H"], after: 0o100664},
    {flags: ["-R", "-L"], after: 0o100764},
    {flags: ["-R", "-P"], after: 0o100664},
  ] {
    let s = uu.scene(ctx)?
    uu.mkdir(s, "dir")?
    make_file(s, "file", 0o664)?
    absolute_link(s, "file", "dir/symlink")?
    let r = uu.invoke(s, "chmod", row.flags.extend(["u+x", "dir"]), umask: 0o022)?
    uu.succeeds(r)
    uu.no_stderr(r)
    assert fs.stat(uu.at(s, "file"))?.mode == row.after
    assert fs.stat(uu.at(s, "dir/symlink"), follow_symlinks: false)?.mode == 0o120777
  }
}

# origin: uutils test_chmod::test_chmod_ugo_copy
test test_uu_chmod_chmod_ugo_copy { |ctx|
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100070)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100070
    let r = uu.invoke(s, "chmod", ["u=g", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100770
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100005)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100005
    let r = uu.invoke(s, "chmod", ["g=o", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100055
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100200)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100200
    let r = uu.invoke(s, "chmod", ["o=u", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100202
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100710)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100710
    let r = uu.invoke(s, "chmod", ["u-g", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100610
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100250)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100250
    let r = uu.invoke(s, "chmod", ["u+g", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100750
  }
}

# origin: uutils test_chmod::test_chmod_ugoa
test test_uu_chmod_chmod_ugoa { |ctx|
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["u=rwx", "file"], umask: 0)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100700
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["g=rwx", "file"], umask: 0)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100070
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["o=rwx", "file"], umask: 0)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100007
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["a=rwx", "file"], umask: 0)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100777
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100777)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100777
    let r = uu.invoke(s, "chmod", ["-r", "file"], umask: 0)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100333
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100777)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100777
    let r = uu.invoke(s, "chmod", ["-w", "file"], umask: 0)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100555
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100777)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100777
    let r = uu.invoke(s, "chmod", ["-x", "file"], umask: 0)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100666
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["u=rwx", "file"], umask: 0o022)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100700
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["g=rwx", "file"], umask: 0o022)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100070
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["o=rwx", "file"], umask: 0o022)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100007
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["a=rwx", "file"], umask: 0o022)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100777
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["+rw", "file"], umask: 0o022)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100644
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100000)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100000
    let r = uu.invoke(s, "chmod", ["=rwx", "file"], umask: 0o022)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100755
  }
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100777)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100777
    let r = uu.invoke(s, "chmod", ["-x", "file"], umask: 0o022)?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100666
  }
}

# origin: uutils test_chmod::test_gnu_invalid_mode
test test_uu_chmod_gnu_invalid_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  {
    let r = uu.invoke(s, "chmod", ["u+gr", "file"])?
    uu.fails(r)
  }
}

# origin: uutils test_chmod::test_gnu_options
test test_uu_chmod_gnu_options { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  {
    let r = uu.invoke(s, "chmod", ["-w", "file"])?
    uu.succeeds(r)
  }
  {
    let r = uu.invoke(s, "chmod", ["file", "-w"])?
    uu.succeeds(r)
  }
  {
    let r = uu.invoke(s, "chmod", ["-w", "--", "file"])?
    uu.succeeds(r)
  }
}

# origin: uutils test_chmod::test_gnu_repeating_options
test test_uu_chmod_gnu_repeating_options { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  {
    let r = uu.invoke(s, "chmod", ["-w", "-w", "file"])?
    uu.succeeds(r)
  }
  {
    let r = uu.invoke(s, "chmod", ["-w", "-w", "-w", "file"])?
    uu.succeeds(r)
  }
}

# origin: uutils test_chmod::test_gnu_special_filenames
test test_uu_chmod_gnu_special_filenames { |ctx|
  let s = uu.scene(ctx)?
  make_file(s, "--", 0o100640)?
  uu.succeeds(uu.invoke(s, "chmod", ["-w", "--", "--"])?)
  assert fs.stat(uu.at(s, "--"))?.mode == 0o100440
  uu.set_mode(s, "--", 0o640)?
  uu.succeeds(uu.invoke(s, "chmod", ["--", "-w", "--"])?)
  assert fs.stat(uu.at(s, "--"))?.mode == 0o100440
  uu.remove(s, "--")?
  make_file(s, "-w", 0o100640)?
  uu.succeeds(uu.invoke(s, "chmod", ["-w", "--", "-w"])?)
  assert fs.stat(uu.at(s, "-w"))?.mode == 0o100440
  uu.set_mode(s, "-w", 0o640)?
  uu.succeeds(uu.invoke(s, "chmod", ["--", "-w", "-w"])?)
  assert fs.stat(uu.at(s, "-w"))?.mode == 0o100440
}

# origin: uutils test_chmod::test_gnu_special_options
test test_uu_chmod_gnu_special_options { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  {
    let r = uu.invoke(s, "chmod", ["--", "--", "file"])?
    uu.succeeds(r)
  }
  {
    let r = uu.invoke(s, "chmod", ["--", "--"])?
    uu.fails(r)
  }
}

# origin: uutils test_chmod::test_gnu_usage_matrix
test test_uu_chmod_gnu_usage_matrix { |ctx|
  for row in [
    {args: ["--"], expected: []},
    {args: ["--", "--"], expected: []},
    {args: ["--", "--", "--", "f"], expected: ["--", "f"]},
    {args: ["--", "--", "-w", "f"], expected: ["-w", "f"]},
    {args: ["--", "--", "f"], expected: ["f"]},
    {args: ["--", "-w"], expected: []},
    {args: ["--", "-w", "--", "f"], expected: ["--", "f"]},
    {args: ["--", "-w", "-w", "f"], expected: ["-w", "f"]},
    {args: ["--", "-w", "f"], expected: ["f"]},
    {args: ["--", "f"], expected: []},
    {args: ["-w"], expected: []},
    {args: ["-w", "--"], expected: []},
    {args: ["-w", "--", "--", "f"], expected: ["--", "f"]},
    {args: ["-w", "--", "-w", "f"], expected: ["-w", "f"]},
    {args: ["-w", "--", "f"], expected: ["f"]},
    {args: ["-w", "-w"], expected: []},
    {args: ["-w", "-w", "--", "f"], expected: ["f"]},
    {args: ["-w", "-w", "-w", "f"], expected: ["f"]},
    {args: ["-w", "-w", "f"], expected: ["f"]},
    {args: ["-w", "f"], expected: ["f"]},
    {args: ["f"], expected: []},
    {args: ["f", "--"], expected: []},
    {args: ["f", "-w"], expected: ["f"]},
    {args: ["f", "f"], expected: []},
    {args: ["u+gr", "f"], expected: []},
    {args: ["ug,+x", "f"], expected: []},
  ] {
    let s = uu.scene(ctx)?
    for name in ["f", "--", "-w"] { make_file(s, name, 0o644)? }
    let r = uu.invoke(s, "chmod", ["-v"].extend(row.args))?
    let visited = [line.byte_slice(9).split("'")[0] for line in r.stdout.utf8()?.lines() if line.starts_with("mode of '")]
    assert visited == row.expected, f"{row.args.join(" ")}: visited {visited.join(",")}, expected {row.expected.join(",")}"
    assert r.status == 0 == ! row.expected.is_empty()
    for name in ["f", "--", "-w"] {
      if ! (name in row.expected) { assert uu.mode(s, name)? == 0o644 }
    }
  }
}

# origin: uutils test_chmod::test_invalid_arg
test test_uu_chmod_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "chmod", ["--definitely-invalid"])?
    uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_chmod::test_mode_after_dash_dash
test test_uu_chmod_mode_after_dash_dash { |ctx|
  {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o100777)?
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100777
    let r = uu.invoke(s, "chmod", ["--", "-r", "file"])?
    uu.succeeds(r)
    assert fs.stat(uu.at(s, "file"))?.mode == 0o100333
  }
}

# origin: uutils test_chmod::test_permission_denied
test test_uu_chmod_permission_denied { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d/no-x/y")?
  uu.succeeds(uu.invoke(s, "chmod", ["u=rw", "d/no-x"])?)
  let r = uu.invoke(s, "chmod", ["-R", "o=r", "d"])?
  uu.set_mode(s, "d/no-x", 0o755)?
  uu.fails(r)
  uu.stderr_is(r, "chmod: cannot access 'd/no-x/y': Permission denied\n")
}

# origin: uutils test_chmod::test_quiet_n_verbose_used_multiple_times
test test_uu_chmod_quiet_n_verbose_used_multiple_times { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  {
    let r = uu.invoke(s, "chmod", ["u+x", "--verbose", "--verbose", "file"])?
    uu.succeeds(r)
  }
  {
    let r = uu.invoke(s, "chmod", ["u+x", "--quiet", "--quiet", "file"])?
    uu.succeeds(r)
  }
}

# origin: uutils test_chmod::test_umask_conflict_reported_only_for_option_like_mode
test test_uu_chmod_umask_conflict_reported_only_for_option_like_mode { |ctx|
  for row in [
    {args: ["-w", "file"], after: 0o466, reported: true},
    {args: ["-w", "--", "file"], after: 0o466, reported: true},
    {args: ["file", "-w"], after: 0o466, reported: true},
    {args: ["-w", "-w", "--", "file"], after: 0o466, reported: true},
    {args: ["--", "-w", "file"], after: 0o466, reported: false},
    {args: ["--", "-rw", "file"], after: 0o022, reported: false},
    {args: ["u+x,-w", "file"], after: 0o566, reported: false},
    {args: ["-w,u+x", "file"], after: 0o566, reported: true},
    {args: ["--", "-w,u+x", "file"], after: 0o566, reported: false},
  ] {
    let s = uu.scene(ctx)?
    make_file(s, "file", 0o666)?
    let r = uu.invoke(s, "chmod", row.args, umask: 0o022)?
    assert uu.mode(s, "file")? == row.after
    if row.reported {
      uu.fails_with_code(r, 1)
      uu.stderr_contains(r, "new permissions are")
    } else {
      uu.succeeds(r)
      uu.no_stderr(r)
    }
  }
}

