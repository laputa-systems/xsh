##! Transcribed from the uutils coreutils install integration tests.

use support.uu as uu

# Scene symlinks have absolute targets unless the original used a relative link.
proc absolute_link(s: uu.Scene, target: Str, name: Str) [fs, error] -> Result[Unit, Error] {
  uu.symlink(s, uu.at(s, target).display(), name)?
  Ok()
}

# Upstream existence predicates return false for paths that are absent.
proc dir_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let target = uu.at(s, name)
  if target.exists()? { Ok(target.is_dir()?) } else { Ok(false) }
}

proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let target = uu.at(s, name)
  if target.exists()? { Ok(target.is_file()?) } else { Ok(false) }
}

# origin: uutils test_install::test_install_no_strip_with_program
test test_uu_install_install_no_strip_with_program { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "install", ["--strip-program", "false", ctx.xsh_bin.display(), "helloworld_installed"])?
  uu.succeeds(r)
  uu.stderr_only(r, "install: WARNING: ignoring --strip-program option as -s option was not specified\n")
}

# origin: uutils test_install::test_install_no_target_basic
test test_uu_install_install_no_target_basic { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.mkdir(s, "dir")?
  {
    let r = uu.invoke(s, "install", ["-T", "file", "dir/file"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert uu.file_exists(s, "file")?
  assert uu.file_exists(s, "dir/file")?
}

# origin: uutils test_install::test_install_no_target_directory_creating_leading_dirs_with_single_source_and_target_dir
test test_uu_install_install_no_target_directory_creating_leading_dirs_with_single_source_and_target_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let r = uu.invoke(s, "install", ["-TD", "file", f"{s.root}/missing_target_dir/"])?
  uu.fails(r)
  uu.stderr_contains(r, "missing_target_dir/': Not a directory")
  assert ! dir_exists(s, "missing_target_dir/")?
}

# origin: uutils test_install::test_install_no_target_directory_failing_cannot_overwrite
test test_uu_install_install_no_target_directory_failing_cannot_overwrite { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.mkdir(s, "dir")?
  {
    let r = uu.invoke(s, "install", ["-T", "file", "dir"])?
    uu.fails(r)
    uu.stderr_contains(r, "cannot overwrite directory 'dir' with non-directory")
  }
  assert ! dir_exists(s, "dir/file")?
}

# origin: uutils test_install::test_install_no_target_directory_failing_combine_with_target_directory
test test_uu_install_install_no_target_directory_failing_combine_with_target_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.mkdir(s, "dir1")?
  {
    let r = uu.invoke(s, "install", ["-T", "file", "-t", "dir1"])?
    uu.fails(r)
    uu.stderr_contains(r, "cannot combine --target-directory (-t) and --no-target-directory (-T)")
  }
}

# origin: uutils test_install::test_install_no_target_directory_failing_omitting_directory
test test_uu_install_install_no_target_directory_failing_omitting_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir2")?
  {
    let r = uu.invoke(s, "install", ["-T", "dir1", "dir2"])?
    uu.fails(r)
    uu.stderr_contains(r, "omitting directory 'dir1'")
  }
}

# origin: uutils test_install::test_install_no_target_directory_failing_usage_with_target_directory
test test_uu_install_install_no_target_directory_failing_usage_with_target_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  {
    let r = uu.invoke(s, "install", ["-T", "file", "-t"])?
    uu.fails(r)
    uu.stderr_contains(r, "option requires an argument -- 't'")
    uu.stderr_contains(r, "Try 'install --help' for more information.")
  }
}

# origin: uutils test_install::test_install_no_target_directory_overwrite_file
test test_uu_install_install_no_target_directory_overwrite_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  {
    let r = uu.invoke(s, "install", ["-T", "file", "dest"])?
    uu.succeeds(r)
  }
  {
    let r = uu.invoke(s, "install", ["-T", "file", "dest"])?
    uu.succeeds(r)
  }
  assert ! dir_exists(s, "dir/file")?
}

# origin: uutils test_install::test_install_no_target_multiple_sources_and_target_dir
test test_uu_install_install_no_target_multiple_sources_and_target_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file1")?
  uu.touch(s, "file2")?
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir2")?
  {
    let r = uu.invoke(s, "install", ["-T", "file1", "file2", "dir1"])?
    uu.fails(r)
    uu.stderr_contains(r, "extra operand 'dir1'")
    uu.stderr_contains(r, "Try 'install --help' for more information.")
  }
  {
    let r = uu.invoke(s, "install", ["-T", "file1", "file2", "dir1", "dir2"])?
    uu.fails(r)
    uu.stderr_contains(r, "extra operand 'dir1'")
    uu.stderr_contains(r, "Try 'install --help' for more information.")
  }
}

# origin: uutils test_install::test_install_non_utf8_dir
test test_uu_install_install_non_utf8_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "source")?
  let dir = uu.at_bytes(s, b"target_dir_\xFF\xFE")?
  dir.mkdir()?
  let r = uu.invoke_paths(s, "install", [p"source", p"--target-directory", dir])?
  uu.succeeds(r)
  uu.no_output(r)
  let installed = Path.parse_bytes(bytes.concat([dir.bytes(), b"/source"]))?
  assert installed.exists()?
}

# origin: uutils test_install::test_install_non_utf8_paths
test test_uu_install_install_non_utf8_paths { |ctx|
  {
    let s = uu.scene(ctx)?
    let source = uu.at_bytes(s, b"\xFF\xFE")?
    source.write(b"test content")?
    uu.mkdir(s, "target_dir")?
    uu.succeeds(uu.invoke_paths(s, "install", [source, p"target_dir"])?)
  }
  {
    let s = uu.scene(ctx)?
    uu.touch(s, "source.txt")?
    let target = Path.parse_bytes(b"\xFF\xFEdir/target.txt")?
    uu.succeeds(uu.invoke_paths(s, "install", [p"-D", p"source.txt", target])?)
  }
}

# origin: uutils test_install::test_install_normal_file_replaces_symlink
test test_uu_install_install_normal_file_replaces_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "source", "new content")?
  uu.write(s, "sensitive", "important data")?
  absolute_link(s, "sensitive", "dest")?
  uu.succeeds(uu.invoke(s, "install", ["source", "dest"])?)
  assert uu.file_exists(s, "dest")?
  uu.file_is(s, "dest", "new content")
  uu.file_is(s, "sensitive", "important data")
}

# origin: uutils test_install::test_install_on_invalid_link_at_destination
test test_uu_install_install_on_invalid_link_at_destination { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "src")?
  uu.mkdir(s, "dest")?
  uu.touch(s, "test.sh")?
  uu.symlink(s, "/opt/FakeDestination", "dest/test.sh")?
  uu.succeeds(uu.invoke(s, "chmod", ["+x", "test.sh"])?)
  absolute_link(s, "test.sh", "src/test.sh")?
  let inside: uu.Scene = {ctx: ctx, root: uu.at(s, "src")}
  let r = uu.invoke(inside, "install", [uu.at(s, "src/test.sh").display(), uu.at(s, "dest/test.sh").display()])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_install::test_install_on_invalid_link_at_destination_and_dev_null_at_source
test test_uu_install_install_on_invalid_link_at_destination_and_dev_null_at_source { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "src")?
  uu.mkdir(s, "dest")?
  uu.touch(s, "test.sh")?
  uu.symlink(s, "/opt/FakeDestination", "dest/test.sh")?
  uu.succeeds(uu.invoke(s, "chmod", ["+x", "test.sh"])?)
  absolute_link(s, "test.sh", "src/test.sh")?
  let inside: uu.Scene = {ctx: ctx, root: uu.at(s, "src")}
  let r = uu.invoke(inside, "install", ["/dev/null", uu.at(s, "dest/test.sh").display()])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_install::test_install_parent_directories
test test_uu_install_install_parent_directories { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "ancestor1")?
  {
    let r = uu.invoke(s, "install", ["-d", "ancestor1/ancestor2/target_dir"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert uu.dir_exists(s, "ancestor1/ancestor2")?
  assert uu.dir_exists(s, "ancestor1/ancestor2/target_dir")?
}

# origin: uutils test_install::test_install_preserve_timestamps
test test_uu_install_install_preserve_timestamps { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "source_file")?
  let before = fs.stat(uu.at(s, "source_file"))?
  let r = uu.invoke(s, "install", ["source_file", "target_file", "-p"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "source_file")?
  assert uu.file_exists(s, "target_file")?
  let source = fs.stat(uu.at(s, "source_file"))?
  let target = fs.stat(uu.at(s, "target_file"))?
  assert before.atime_ns == target.atime_ns
  assert source.mtime_ns == target.mtime_ns
}

# origin: uutils test_install::test_install_proc_self_mem_as_dst
test test_uu_install_install_proc_self_mem_as_dst { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "install", ["-g", "0", "/dev/full", "/proc/self/mem"])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot remove")
}

# origin: uutils test_install::test_install_replaces_special_target
test test_uu_install_install_replaces_special_target { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "source_file", "contents")?
  uu.mkfifo(s, "target_fifo")?
  let r = uu.invoke(s, "install", ["source_file", "target_fifo"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, "target_fifo")?
  uu.file_is(s, "target_fifo", "contents")
}

# origin: uutils test_install::test_install_root_combined
test test_uu_install_install_root_combined { |ctx|
  if user.current()?.uid != 0 { test.skip("requires root to set arbitrary uid and gid") }
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "c")?
  for row in [
    {args: ["-Cv", "-o1", "-g1", "a", "b"], target: "b", uid: 1, gid: 1},
    {args: ["-Cv", "-o2", "-g1", "a", "b"], target: "b", uid: 2, gid: 1},
    {args: ["-Cv", "-o2", "-g2", "a", "b"], target: "b", uid: 2, gid: 2},
    {args: ["-Cv", "-o2", "c", "d"], target: "d", uid: 2, gid: 0},
    {args: ["-Cv", "c", "d"], target: "d", uid: 0, gid: 0},
    {args: ["-Cv", "c", "d"], target: "d", uid: 0, gid: 0},
  ] {
    uu.succeeds(uu.invoke(s, "install", row.args)?)
    assert uu.file_exists(s, row.target)?
    let meta = fs.stat(uu.at(s, row.target))?
    assert meta.uid == row.uid
    assert meta.gid == row.gid
  }
}

# origin: uutils test_install::test_install_same_file
test test_uu_install_install_same_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  {
    let r = uu.invoke(s, "install", ["file", "."])?
    uu.fails(r)
    uu.stderr_contains(r, "'file' and './file' are the same file")
  }
}

# origin: uutils test_install::test_install_set_owner_nonexistent_uid_and_gid
test test_uu_install_install_set_owner_nonexistent_uid_and_gid { |ctx|
  if user.current()?.uid != 0 { test.skip("requires root to set unused uid and gid") }
  let records = p"/etc/passwd".read_text()?.lines()
  let used_uids = [fields[2].parse_int()? for fields in [line.split(":") for line in records] if fields.len() >= 4]
  let used_gids = [fields[3].parse_int()? for fields in [line.split(":") for line in records] if fields.len() >= 4]
  var uid = 60000
  var gid = 60000
  while uid in used_uids { uid += 1 }
  while gid in used_gids { gid += 1 }
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.succeeds(uu.invoke(s, "install", [f"-o{uid}", f"-g{gid}", "a", "b"])?)
  assert uu.file_exists(s, "b")?
  let meta = fs.stat(uu.at(s, "b"))?
  assert meta.uid == uid
  assert meta.gid == gid
}

# origin: uutils test_install::test_install_setuid_mode_applied_without_chown
test test_uu_install_install_setuid_mode_applied_without_chown { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "src")?
  uu.succeeds(uu.invoke(s, "install", ["-m", "4755", "src", "dst"])?)
  assert uu.mode(s, "dst")? == 0o4755
  uu.succeeds(uu.invoke(s, "install", ["-d", "-m", "2755", "newdir"])?)
  assert uu.mode(s, "newdir")? == 0o2755
}

# origin: uutils test_install::test_install_several_directories
test test_uu_install_install_several_directories { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "install", ["-d", "dir1", "dir2", "dir3"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert uu.dir_exists(s, "dir1")?
  assert uu.dir_exists(s, "dir2")?
  assert uu.dir_exists(s, "dir3")?
}

# origin: uutils test_install::test_install_suffix_without_backup_option
test test_uu_install_install_suffix_without_backup_option { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_install_backup_custom_suffix_file_a"
  let file_b = "test_install_backup_custom_suffix_file_b"
  let suffix = "super-suffix-of-the-century"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r = uu.invoke(s, "install", [f"--suffix={suffix}", file_a, file_b])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.file_exists(s, file_a)?
  assert uu.file_exists(s, file_b)?
  assert uu.file_exists(s, f"{file_b}{suffix}")?
}

# origin: uutils test_install::test_install_symlink_same_file
test test_uu_install_install_symlink_same_file { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "target_dir")?
  uu.touch(s, "target_dir/file")?
  absolute_link(s, "target_dir", "target_link")?
  let r = uu.invoke(s, "install", ["target_dir/file", "target_link"])?
  uu.fails(r)
  uu.stderr_contains(r, "'target_dir/file' and 'target_link/file' are the same file")
}

# origin: uutils test_install::test_install_target_dir_not_a_directory
test test_uu_install_install_target_dir_not_a_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "source", "new")?
  uu.write(s, "regular", "old")?
  absolute_link(s, "nowhere", "dangling")?
  for row in [
    {target: "regular", reason: "Not a directory"},
    {target: "dangling", reason: "No such file or directory"},
    {target: "missing", reason: "No such file or directory"},
  ] {
    let r = uu.invoke(s, "install", ["-t", row.target, "source"])?
    uu.fails(r)
    uu.stderr_only(r, f"install: failed to access '{row.target}': {row.reason}\n")
  }
  uu.file_is(s, "regular", "old")
  assert uu.is_symlink(s, "dangling")?
  assert ! file_exists(s, "missing")?
}

# origin: uutils test_install::test_install_target_file
test test_uu_install_install_target_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "source_file")?
  uu.touch(s, "target_file")?
  {
    let r = uu.invoke(s, "install", ["source_file", "target_file"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert uu.file_exists(s, "source_file")?
  assert uu.file_exists(s, "target_file")?
}

# origin: uutils test_install::test_install_target_file_dev_null
test test_uu_install_install_target_file_dev_null { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "install", ["/dev/null", "target_file"])?
    uu.succeeds(r)
  }
  assert uu.file_exists(s, "target_file")?
}

# origin: uutils test_install::test_install_target_new_file
test test_uu_install_install_target_new_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.mkdir(s, "target_dir")?
  {
    let r = uu.invoke(s, "install", ["file", "target_dir/file"])?
    uu.succeeds(r)
    uu.no_stderr(r)
  }
  assert uu.file_exists(s, "file")?
  assert uu.file_exists(s, "target_dir/file")?
}

# origin: uutils test_install::test_install_target_new_file_failing_nonexistent_parent
test test_uu_install_install_target_new_file_failing_nonexistent_parent { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "source_file")?
  {
    let r = uu.invoke(s, "install", ["source_file", "target_dir/target_file"])?
    uu.fails(r)
    uu.stderr_contains(r, "No such file or directory")
  }
}

# origin: uutils test_install::test_install_target_new_file_with_group
test test_uu_install_install_target_new_file_with_group { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.mkdir(s, "target_dir")?
  let id = group.current()?.gid
  let r = uu.invoke(s, "install", ["file", "--group", f"{id}", "target_dir/file"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "file")?
  assert uu.file_exists(s, "target_dir/file")?
}

# origin: uutils test_install::test_install_target_new_file_with_owner
test test_uu_install_install_target_new_file_with_owner { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.mkdir(s, "target_dir")?
  let id = user.current()?.uid
  let r = uu.invoke(s, "install", ["file", "--owner", f"{id}", "target_dir/file"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "file")?
  assert uu.file_exists(s, "target_dir/file")?
}

# origin: uutils test_install::test_install_target_without_splice_support
test test_uu_install_install_target_without_splice_support { |ctx|
  let s = uu.scene(ctx)?
  let strace = match process.which("strace") {
    Ok(tool) => tool,
    Err(_) => { test.skip("strace is not installed"); p"/usr/bin/strace" },
  }
  let source = ctx.xsh_bin
  let words = uu.argv(s, "install", [source, p"target_file"])?
  let args = [strace, p"-e", p"inject=splice:error=EINVAL:when=2"].extend(words)
  let _ = process.run(process.command_argv(strace, args, s.root, {}, b"", uu.at(s, "trace-stdout"), uu.at(s, "trace-stderr")))?
  assert source.read_bytes()? == uu.read(s, "target_file")?
}

# origin: uutils test_install::test_install_twice_dir
test test_uu_install_install_twice_dir { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "install", ["-d", "dir"])?
    uu.succeeds(r)
  }
  {
    let r = uu.invoke(s, "install", ["-d", "dir"])?
    uu.succeeds(r)
  }
  assert uu.dir_exists(s, "dir")?
}

# origin: uutils test_install::test_install_will_not_overwrite_just_created
test test_uu_install_install_will_not_overwrite_just_created { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.mkdir(s, "c")?
  uu.write(s, "a/f", "a")?
  uu.write(s, "b/f", "b")?
  {
    let r = uu.invoke(s, "install", ["a/f", "b/f", "c/"])?
    uu.fails(r)
    uu.stderr_contains(r, "will not overwrite just-created 'c/f' with 'b/f'")
  }
  uu.file_is(s, "c/f", "a")
}

# origin: uutils test_install::test_invalid_arg
test test_uu_install_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  {
    let r = uu.invoke(s, "install", ["--definitely-invalid"])?
    uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_install::test_multiple_mode_arguments_override_not_error
test test_uu_install_multiple_mode_arguments_override_not_error { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "source_file")?
  uu.mkdir(s, "source_dir")?
  let uid = user.current()?.uid
  let gid = group.current()?.gid
  for args in [
    ["source_file", "source_dir/source_file", "--owner=invalid_owner", "--owner", f"{uid}"],
    ["source_file", "source_dir/source_file", "-o invalid_owner", "-o", f"{uid}"],
    ["source_file", "source_dir/source_file", "--mode=999", "--mode=200"],
    ["source_file", "source_dir/source_file", "-m 999", "-m 200"],
    ["source_file", "source_dir/source_file", "--group=invalid_group", "--group", f"{gid}"],
    ["source_file", "source_dir/source_file", "-g invalid_group", "-g", f"{gid}"],
  ] {
    let r = uu.invoke(s, "install", args)?
    if args[2] == "-m 999" {
      uu.fails_with_code(r, 1)
      uu.stderr_is(r, "install: invalid mode ' 200'\n")
    } else {
      uu.succeeds(r)
      uu.no_stderr(r)
    }
  }
}

# origin: uutils test_install::test_t_exist_dir
test test_uu_install_t_exist_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.mkdir(s, "sub4/")?
  uu.touch(s, "sub4/file_exists")?
  {
    let r = uu.invoke(s, "install", ["-t", "sub4/file_exists", "-Dv", "file"])?
    uu.fails(r)
    uu.stderr_contains(r, "failed to access 'sub4/file_exists': Not a directory")
  }
}

# origin: uutils test_install::test_target_file_ends_with_slash
test test_uu_install_target_file_ends_with_slash { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "source_file")?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/target_file")?
  {
    let r = uu.invoke(s, "install", ["-t", "dir/target_file/", "-D", "source_file"])?
    uu.fails(r)
    uu.stderr_contains(r, "failed to access 'dir/target_file/': Not a directory")
  }
}

