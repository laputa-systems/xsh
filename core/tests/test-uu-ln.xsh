##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_ln.rs.

use support.uu as uu

# Scene setup symlinks name absolute targets, including dangling targets.
proc scene_symlink(s: uu.Scene, target: Str, name: Str) -> Result[Unit, Error] {
  uu.at(s, name).symlink(to: uu.at(s, target))?
  Ok()
}

# Absolute link text inside the scene is reported relative to the scene root;
# relative text is preserved exactly, including parent components.
proc resolve_link(s: uu.Scene, name: Str) -> Result[Str, Error] {
  let raw = uu.read_link(s, name)?
  let prefix = s.root.display() + "/"
  Ok(if raw.starts_with(prefix) { raw.byte_slice(prefix.byte_len()) } else { raw })
}

# Upstream file/dir probes follow links and treat only absence as false.
proc has_kind(s: uu.Scene, name: Str, kind: Str) -> Result[Bool, Error] {
  match fs.stat(uu.at(s, name), follow_symlinks: true) {
    Ok(metadata) => Ok(metadata.kind == kind),
    Err(failure) => if failure.errno == 2 or failure.errno == 20 { Ok(false) } else { Err(failure) },
  }
}

# origin: uutils test_ln::test_backup_existing_hard_linked_target_is_fifo
test test_uu_ln_backup_existing_hard_linked_target_is_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.hard_link(s, "a", "b")?
  uu.mkfifo(s, "b~")?
  let run1 = uu.invoke(s, "ln", ["--backup", "a", "b"] , timeout: 10s)?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, "a", "file")?
  assert has_kind(s, "b", "file")?
  assert !(fs.stat(uu.at(s, "b~"))?.kind == "fifo")
}

# origin: uutils test_ln::test_backup_existing_hard_linked_under_different_name
test test_uu_ln_backup_existing_hard_linked_under_different_name { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.hard_link(s, "a", "b")?
  let run1 = uu.invoke(s, "ln", ["--backup", "a", "b"])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, "a", "file")?
  assert has_kind(s, "b", "file")?
  assert has_kind(s, "b~", "file")?
}

# origin: uutils test_ln::test_backup_force
test test_uu_ln_backup_force { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\n")?
  uu.write(s, "b", "b2\n")?
  let run1 = uu.invoke(s, "ln", ["-s", "b", "b~"])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, "a", "file")?
  assert has_kind(s, "b", "file")?
  assert has_kind(s, "b~", "file")?
  let run2 = uu.invoke(s, "ln", ["-s", "-f", "--b=simple", "a", "b"])?
  uu.succeeds(run2)
  uu.no_stderr(run2)
  assert has_kind(s, "a", "file")?
  assert has_kind(s, "b", "file")?
  assert has_kind(s, "b~", "file")?
  assert uu.read_text(s, "a")? == "a\n"
  assert uu.read_text(s, "b")? == "a\n"
  assert uu.read_text(s, "b~")? == "b2\n"
}

# origin: uutils test_ln::test_backup_same_file
test test_uu_ln_backup_same_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file1")?
  let run1 = uu.invoke(s, "ln", ["--backup", "file1", "./file1"])?
  uu.fails(run1)
  uu.stderr_contains(run1, "n: 'file1' and './file1' are the same file")
}

# origin: uutils test_ln::test_force_replace_in_symlinked_directory
test test_uu_ln_force_replace_in_symlinked_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "real")?
  scene_symlink(s, "real", "dirlink")?
  scene_symlink(s, "old", "real/link")?
  let run1 = uu.invoke(s, "ln", ["-s", "-f", "new", "dirlink/link"])?
  uu.succeeds(run1)
  assert resolve_link(s, "real/link")? == "new"
}

# origin: uutils test_ln::test_force_same_file_detected_after_canonicalization
test test_uu_ln_force_same_file_detected_after_canonicalization { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "hello")?
  let run1 = uu.invoke(s, "ln", ["-f", "file", "./file"])?
  uu.fails_with_code(run1, 1)
  uu.stderr_contains(run1, "are the same file")
  assert has_kind(s, "file", "file")?
  assert uu.read_text(s, "file")? == "hello"
}

# origin: uutils test_ln::test_hard_link_force_failed_link_keeps_destination
test test_uu_ln_hard_link_force_failed_link_keeps_destination { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "dst", "keep\n")?
  let run1 = uu.invoke(s, "ln", ["-f", "no_such_source", "dst"])?
  uu.fails(run1)
  assert has_kind(s, "dst", "file")?
  assert uu.read_text(s, "dst")? == "keep\n"
}

# origin: uutils test_ln::test_invalid_arg
test test_uu_ln_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "ln", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_ln::test_ln_backup_no_path_traversal
test test_uu_ln_ln_backup_no_path_traversal { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  uu.mkdir(s, "b_")?
  let run1 = uu.invoke(s, "ln", ["-S", "_/../c", "-s", "a", "b"])?
  uu.succeeds(run1)
  assert !has_kind(s, "c", "file")?
  assert uu.is_symlink(s, "b")?
  assert has_kind(s, "b~", "file")?
  assert !uu.is_symlink(s, "b~")?
}

# origin: uutils test_ln::test_ln_backup_nonexistent_rollback
test test_uu_ln_ln_backup_nonexistent_rollback { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "dst")?
  let run1 = uu.invoke(s, "ln", ["--backup", "/non/existent/path", "dst"])?
  uu.fails(run1)
  assert !has_kind(s, "dst~", "file")?
  assert has_kind(s, "dst", "file")?
}

# origin: uutils test_ln::test_ln_hard_link_dir
test test_uu_ln_ln_hard_link_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  let run1 = uu.invoke(s, "ln", ["dir", "dir_link"])?
  uu.fails(run1)
  uu.stderr_contains(run1, "hard link not allowed for directory")
}

# origin: uutils test_ln::test_relative_dst_already_symlink
test test_uu_ln_relative_dst_already_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file1")?
  scene_symlink(s, "file1", "file2")?
  uu.succeeds(uu.invoke(s, "ln", ["-srf", "file1", "file2"])?)
  let linked = uu.is_symlink(s, "file2")?
}

# origin: uutils test_ln::test_relative_recursive
test test_uu_ln_relative_recursive { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  let run1 = uu.invoke(s, "ln", ["-sr", "dir", "dir/recursive"])?
  uu.succeeds(run1)
  assert resolve_link(s, "dir/recursive")? == "."
}

# origin: uutils test_ln::test_relative_requires_symbolic
test test_uu_ln_relative_requires_symbolic { |ctx|
  let s = uu.scene(ctx)?
  let run1 = uu.invoke(s, "ln", ["-r", "foo", "bar"])?
  uu.fails(run1)
}

# origin: uutils test_ln::test_relative_src_already_symlink
test test_uu_ln_relative_src_already_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file1")?
  scene_symlink(s, "file1", "file2")?
  uu.succeeds(uu.invoke(s, "ln", ["-sr", "file2", "file3"])?)
  assert resolve_link(s, "file3")?.ends_with("file1")
}

# origin: uutils test_ln::test_relative_target_with_no_parent
test test_uu_ln_relative_target_with_no_parent { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "src", "data")?
  let run1 = uu.invoke(s, "ln", ["-rsfT", "src", ""])?
  uu.fails_with_code(run1, 1)
}

# origin: uutils test_ln::test_symlink_backup_numbering
test test_uu_ln_symlink_backup_numbering { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_backup_numbering"
  let link = "test_symlink_backup_numbering_link"
  uu.touch(s, file)?
  scene_symlink(s, file, link)?
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
  let run1 = uu.invoke(s, "ln", ["-s", "--backup=t", file, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
  let backup = f"{link}.~1~"
  assert uu.is_symlink(s, backup)?
  assert resolve_link(s, backup)? == file
}

# origin: uutils test_ln::test_symlink_circular
test test_uu_ln_symlink_circular { |ctx|
  let s = uu.scene(ctx)?
  let link = "test_symlink_circular"
  let run1 = uu.invoke(s, "ln", ["-s", link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == link
}

# origin: uutils test_ln::test_symlink_custom_backup_suffix
test test_uu_ln_symlink_custom_backup_suffix { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_custom_backup_suffix"
  let link = "test_symlink_custom_backup_suffix_link"
  let suffix = "super-suffix-of-the-century"
  uu.touch(s, file)?
  scene_symlink(s, file, link)?
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
  let arg = f"--suffix={suffix}"
  let run1 = uu.invoke(s, "ln", ["-b", arg, "-s", file, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
  let backup = f"{link}{suffix}"
  assert uu.is_symlink(s, backup)?
  assert resolve_link(s, backup)? == file
}

# origin: uutils test_ln::test_symlink_custom_backup_suffix_hyphen_value
test test_uu_ln_symlink_custom_backup_suffix_hyphen_value { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_custom_backup_suffix"
  let link = "test_symlink_custom_backup_suffix_link"
  let suffix = "-v"
  uu.touch(s, file)?
  scene_symlink(s, file, link)?
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
  let arg = f"--suffix={suffix}"
  let run1 = uu.invoke(s, "ln", ["-b", arg, "-s", file, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
  let backup = f"{link}{suffix}"
  assert uu.is_symlink(s, backup)?
  assert resolve_link(s, backup)? == file
}

# origin: uutils test_ln::test_symlink_dangling_directory
test test_uu_ln_symlink_dangling_directory { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_symlink_dangling_dir"
  let link = "test_symlink_dangling_dir_link"
  let run1 = uu.invoke(s, "ln", ["-s", dir, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert !has_kind(s, dir, "dir")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == dir
}

# origin: uutils test_ln::test_symlink_dangling_file
test test_uu_ln_symlink_dangling_file { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_dangling_file"
  let link = "test_symlink_dangling_file_link"
  let run1 = uu.invoke(s, "ln", ["-s", file, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert !has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
}

# origin: uutils test_ln::test_symlink_do_not_overwrite
test test_uu_ln_symlink_do_not_overwrite { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_do_not_overwrite"
  let link = "test_symlink_do_not_overwrite_link"
  uu.touch(s, file)?
  uu.touch(s, link)?
  let run1 = uu.invoke(s, "ln", ["-s", file, link])?
  uu.fails(run1)
  assert has_kind(s, file, "file")?
  assert has_kind(s, link, "file")?
  assert !uu.is_symlink(s, link)?
}

# origin: uutils test_ln::test_symlink_error_includes_destination
test test_uu_ln_symlink_error_includes_destination { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_error_includes_destination"
  let link = "no_such_dir/test_symlink_error_includes_destination"
  uu.touch(s, file)?
  let run1 = uu.invoke(s, "ln", ["-s", file, link])?
  uu.fails(run1)
  uu.stderr_is(run1, f"ln: failed to create symbolic link '{link}': No such file or directory\n")
}

# origin: uutils test_ln::test_symlink_errors
test test_uu_ln_symlink_errors { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_symlink_errors_dir"
  let file_a = "test_symlink_errors_file_a"
  let file_b = "test_symlink_errors_file_b"
  uu.mkdir(s, dir)?
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let run1 = uu.invoke(s, "ln", ["-T", "-t", dir, file_a, file_b])?
  uu.fails(run1)
}

# origin: uutils test_ln::test_symlink_existing_backup
test test_uu_ln_symlink_existing_backup { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_existing_backup"
  let link = "test_symlink_existing_backup_link"
  let link_backup = "test_symlink_existing_backup_link.~1~"
  let resulting_backup = "test_symlink_existing_backup_link.~2~"
  uu.touch(s, file)?
  scene_symlink(s, file, link)?
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
  scene_symlink(s, file, link_backup)?
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link_backup)?
  assert resolve_link(s, link_backup)? == file
  let run1 = uu.invoke(s, "ln", ["-s", "--backup=nil", file, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link_backup)?
  assert resolve_link(s, link_backup)? == file
  assert uu.is_symlink(s, resulting_backup)?
  assert resolve_link(s, resulting_backup)? == file
}

# origin: uutils test_ln::test_symlink_existing_directory
test test_uu_ln_symlink_existing_directory { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_symlink_existing_dir"
  let link = "test_symlink_existing_dir_link"
  uu.mkdir(s, dir)?
  let run1 = uu.invoke(s, "ln", ["-s", dir, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, dir, "dir")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == dir
}

# origin: uutils test_ln::test_symlink_existing_file
test test_uu_ln_symlink_existing_file { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_existing_file"
  let link = "test_symlink_existing_file_link"
  uu.touch(s, file)?
  let run1 = uu.invoke(s, "ln", ["-s", file, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
}

# origin: uutils test_ln::test_symlink_implicit_target_dir
test test_uu_ln_symlink_implicit_target_dir { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_symlink_implicit_target_dir"
  let filename = "test_symlink_implicit_target_file"
  let source_path = f"{dir}/{filename}"
  let file = source_path
  uu.mkdir(s, dir)?
  uu.touch(s, source_path)?
  let run1 = uu.invoke(s, "ln", ["-s", file])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, filename, "file")?
  assert uu.is_symlink(s, filename)?
  assert resolve_link(s, filename)? == file
}

# origin: uutils test_ln::test_symlink_interactive
test test_uu_ln_symlink_interactive { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_interactive_file"
  let link = "test_symlink_interactive_file_link"
  uu.touch(s, file)?
  uu.touch(s, link)?
  let run1 = uu.invoke(s, "ln", ["-i", "-s", file, link] , stdin: b"n")?
  uu.fails(run1)
  uu.no_stdout(run1)
  assert has_kind(s, file, "file")?
  assert !uu.is_symlink(s, link)?
  let run2 = uu.invoke(s, "ln", ["-i", "-s", file, link] , stdin: b"Yesh")?
  uu.succeeds(run2)
  uu.no_stdout(run2)
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
}

# origin: uutils test_ln::test_symlink_interactive_overrides_force
test test_uu_ln_symlink_interactive_overrides_force { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_interactive_file"
  let link = "test_symlink_interactive_file_link"
  uu.touch(s, file)?
  uu.touch(s, link)?
  let run1 = uu.invoke(s, "ln", ["-f", "-i", "-s", file, link] , stdin: b"n")?
  uu.fails(run1)
  uu.no_stdout(run1)
  assert has_kind(s, file, "file")?
  assert !uu.is_symlink(s, link)?
  let run2 = uu.invoke(s, "ln", ["-f", "-i", "-s", file, link] , stdin: b"Yesh")?
  uu.succeeds(run2)
  uu.no_stdout(run2)
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
}

# origin: uutils test_ln::test_symlink_no_deref_dir
test test_uu_ln_symlink_no_deref_dir { |ctx|
  let s = uu.scene(ctx)?
  let dir1 = "foo"
  let dir2 = "bar"
  let link = "baz"
  uu.mkdir(s, dir1)?
  uu.mkdir(s, dir2)?
  let run1 = uu.invoke(s, "ln", ["-s", dir2, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, dir1, "dir")?
  assert has_kind(s, dir2, "dir")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == dir2
  let run2 = uu.invoke(s, "ln", ["-sf", dir1, link])?
  uu.succeeds(run2)
  uu.no_stderr(run2)
  assert has_kind(s, dir1, "dir")?
  assert has_kind(s, dir2, "dir")?
  assert uu.is_symlink(s, "baz/foo")?
  assert resolve_link(s, "baz/foo")? == dir1
  let run3 = uu.invoke(s, "ln", ["-sn", dir1, link])?
  uu.fails(run3)
  let run4 = uu.invoke(s, "ln", ["-sfn", dir1, link])?
  uu.succeeds(run4)
  uu.no_stderr(run4)
  assert has_kind(s, dir1, "dir")?
  assert has_kind(s, dir2, "dir")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == dir1
}

# origin: uutils test_ln::test_symlink_no_deref_file
test test_uu_ln_symlink_no_deref_file { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "foo"
  let file2 = "bar"
  let link = "baz"
  uu.touch(s, file1)?
  uu.touch(s, file2)?
  let run1 = uu.invoke(s, "ln", ["-s", file2, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, file1, "file")?
  assert has_kind(s, file2, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file2
  let run2 = uu.invoke(s, "ln", ["-sf", file1, link])?
  uu.succeeds(run2)
  uu.no_stderr(run2)
  assert has_kind(s, file1, "file")?
  assert has_kind(s, file2, "file")?
  assert uu.is_symlink(s, "baz")?
  assert resolve_link(s, "baz")? == file1
  let run3 = uu.invoke(s, "ln", ["-sn", file1, link])?
  uu.fails(run3)
  let run4 = uu.invoke(s, "ln", ["-sfn", file1, link])?
  uu.succeeds(run4)
  assert has_kind(s, file1, "file")?
  assert has_kind(s, file2, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file1
}

# origin: uutils test_ln::test_symlink_no_deref_file_in_destination_dir
test test_uu_ln_symlink_no_deref_file_in_destination_dir { |ctx|
  let s = uu.scene(ctx)?
  let file1 = "foo"
  let file2 = "bar"
  let dest = "baz"
  let link1 = "baz/foo"
  let link2 = "baz/bar"
  uu.touch(s, file1)?
  uu.touch(s, file2)?
  uu.mkdir(s, dest)?
  assert has_kind(s, file1, "file")?
  assert has_kind(s, file2, "file")?
  assert has_kind(s, dest, "dir")?
  let run1 = uu.invoke(s, "ln", ["-sn", file1, dest])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert uu.is_symlink(s, link1)?
  assert resolve_link(s, link1)? == file1
  let run2 = uu.invoke(s, "ln", ["-sf", file1, dest])?
  uu.succeeds(run2)
  uu.no_stderr(run2)
  assert uu.is_symlink(s, link1)?
  assert resolve_link(s, link1)? == file1
  let run3 = uu.invoke(s, "ln", ["-sn", file1, dest])?
  uu.fails(run3)
  let run4 = uu.invoke(s, "ln", ["-snf", file1, dest])?
  uu.succeeds(run4)
  uu.no_stderr(run4)
  assert uu.is_symlink(s, link1)?
  assert resolve_link(s, link1)? == file1
  let run5 = uu.invoke(s, "ln", ["-snf", file1, file2, dest])?
  uu.succeeds(run5)
  uu.no_stderr(run5)
  assert uu.is_symlink(s, link1)?
  assert resolve_link(s, link1)? == file1
  assert uu.is_symlink(s, link2)?
  assert resolve_link(s, link2)? == file2
}

# origin: uutils test_ln::test_symlink_overwrite_dir_fail
test test_uu_ln_symlink_overwrite_dir_fail { |ctx|
  let s = uu.scene(ctx)?
  let path_a = "test_symlink_overwrite_dir_a"
  let path_b = "test_symlink_overwrite_dir_b"
  uu.touch(s, path_a)?
  uu.mkdir(s, path_b)?
  let run1 = uu.invoke(s, "ln", ["-s", "-T", path_a, path_b])?
  uu.fails(run1)
  assert run1.stderr.len() > 0
}

# origin: uutils test_ln::test_symlink_overwrite_force
test test_uu_ln_symlink_overwrite_force { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_symlink_overwrite_force_a"
  let file_b = "test_symlink_overwrite_force_b"
  let link = "test_symlink_overwrite_force_link"
  scene_symlink(s, file_a, link)?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file_a
  let run1 = uu.invoke(s, "ln", ["--force", "-s", file_b, link])?
  uu.succeeds(run1)
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file_b
}

# origin: uutils test_ln::test_symlink_overwrite_force_overrides_interactive
test test_uu_ln_symlink_overwrite_force_overrides_interactive { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_symlink_overwrite_force_a"
  let file_b = "test_symlink_overwrite_force_b"
  let link = "test_symlink_overwrite_force_link"
  scene_symlink(s, file_a, link)?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file_a
  let run1 = uu.invoke(s, "ln", ["-i", "-f", "-s", file_b, link])?
  uu.succeeds(run1)
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file_b
}

# origin: uutils test_ln::test_symlink_relative
test test_uu_ln_symlink_relative { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_symlink_relative_a"
  let link = "test_symlink_relative_link"
  uu.touch(s, file_a)?
  let run1 = uu.invoke(s, "ln", ["-r", "-s", file_a, link])?
  uu.succeeds(run1)
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file_a
}

# origin: uutils test_ln::test_symlink_relative_dir
test test_uu_ln_symlink_relative_dir { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_symlink_existing_dir"
  let link = "test_symlink_existing_dir_link"
  uu.mkdir(s, dir)?
  let run1 = uu.invoke(s, "ln", ["-s", "-r", dir, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, dir, "dir")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == dir
}

# origin: uutils test_ln::test_symlink_remove_existing_same_src_and_dest
test test_uu_ln_symlink_remove_existing_same_src_and_dest { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.write(s, "a", "sample")?
  let run1 = uu.invoke(s, "ln", ["-sf", "a", "a"])?
  uu.fails_with_code(run1, 1)
  uu.stderr_contains(run1, "'a' and 'a' are the same file")
  assert has_kind(s, "a", "file")? and !uu.is_symlink(s, "a")?
  assert uu.read_text(s, "a")? == "sample"
}

# origin: uutils test_ln::test_symlink_simple_backup
test test_uu_ln_symlink_simple_backup { |ctx|
  let s = uu.scene(ctx)?
  let file = "test_symlink_simple_backup"
  let link = "test_symlink_simple_backup_link"
  uu.touch(s, file)?
  scene_symlink(s, file, link)?
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
  let run1 = uu.invoke(s, "ln", ["-b", "-s", file, link])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, file, "file")?
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file
  let backup = f"{link}~"
  assert uu.is_symlink(s, backup)?
  assert resolve_link(s, backup)? == file
}

# origin: uutils test_ln::test_symlink_suffix_without_backup_option
test test_uu_ln_symlink_suffix_without_backup_option { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\n")?
  uu.write(s, "b", "b2\n")?
  assert has_kind(s, "a", "file")?
  assert has_kind(s, "b", "file")?
  let suffix = ".sfx"
  let suffix_arg = f"--suffix={suffix}"
  let run1 = uu.invoke(s, "ln", ["-s", "-f", suffix_arg, "a", "b"])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, "a", "file")?
  assert has_kind(s, "b", "file")?
  assert uu.read_text(s, "a")? == "a\n"
  assert uu.read_text(s, "b")? == "a\n"
  assert uu.read_text(s, f"b{suffix}")? == "b2\n"
}

# origin: uutils test_ln::test_symlink_target_dir
test test_uu_ln_symlink_target_dir { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_ln_target_dir_dir"
  let file_a = "test_ln_target_dir_file_a"
  let file_b = "test_ln_target_dir_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  uu.mkdir(s, dir)?
  let run1 = uu.invoke(s, "ln", ["-s", "-t", dir, file_a, file_b])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  let file_a_link = f"{dir}/{file_a}"
  assert uu.is_symlink(s, file_a_link)?
  assert resolve_link(s, file_a_link)? == file_a
  let file_b_link = f"{dir}/{file_b}"
  assert uu.is_symlink(s, file_b_link)?
  assert resolve_link(s, file_b_link)? == file_b
}

# origin: uutils test_ln::test_symlink_target_dir_from_dir
test test_uu_ln_symlink_target_dir_from_dir { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_ln_target_dir_dir"
  let from_dir = "test_ln_target_dir_from_dir"
  let filename_a = "test_ln_target_dir_file_a"
  let filename_b = "test_ln_target_dir_file_b"
  let file_a = f"{from_dir}/{filename_a}"
  let file_b = f"{from_dir}/{filename_b}"
  uu.mkdir(s, from_dir)?
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  uu.mkdir(s, dir)?
  let run1 = uu.invoke(s, "ln", ["-s", "-t", dir, file_a, file_b])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  let file_a_link = f"{dir}/{filename_a}"
  assert uu.is_symlink(s, file_a_link)?
  assert resolve_link(s, file_a_link)? == file_a
  let file_b_link = f"{dir}/{filename_b}"
  assert uu.is_symlink(s, file_b_link)?
  assert resolve_link(s, file_b_link)? == file_b
}

# origin: uutils test_ln::test_symlink_target_only
test test_uu_ln_symlink_target_only { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_symlink_target_only"
  uu.mkdir(s, dir)?
  let run1 = uu.invoke(s, "ln", ["-s", "-t", dir])?
  uu.fails(run1)
  assert run1.stderr.len() > 0
}

# origin: uutils test_ln::test_symlink_to_dir_2args
test test_uu_ln_symlink_to_dir_2args { |ctx|
  let s = uu.scene(ctx)?
  let filename = "test_symlink_to_dir_2args_file"
  let from_file = uu.at(s, filename).display()
  let to_dir = "test_symlink_to_dir_2args_to_dir"
  let to_file = f"{to_dir}/{filename}"
  uu.mkdir(s, to_dir)?
  uu.at(s, filename).write("")?
  let run1 = uu.invoke(s, "ln", ["-s", from_file, to_dir])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  assert has_kind(s, to_file, "file")?
  assert uu.is_symlink(s, to_file)?
  assert resolve_link(s, to_file)? == filename
}

# origin: uutils test_ln::test_symlink_verbose
test test_uu_ln_symlink_verbose { |ctx|
  let s = uu.scene(ctx)?
  let file_a = "test_symlink_verbose_file_a"
  let file_b = "test_symlink_verbose_file_b"
  uu.touch(s, file_a)?
  let run1 = uu.invoke(s, "ln", ["-s", "-v", file_a, file_b])?
  uu.succeeds(run1)
  uu.stdout_only(run1, f"'{file_b}' -> '{file_a}'\n")
  uu.touch(s, file_b)?
  let run2 = uu.invoke(s, "ln", ["-s", "-v", "-b", file_a, file_b])?
  uu.succeeds(run2)
  uu.stdout_only(run2, f"'{file_b}~' ~ '{file_b}' -> '{file_a}'\n")
}

# origin: uutils test_ln::test_force_replace_same_inode_leaves_no_temp_file
test test_uu_ln_force_replace_same_inode_leaves_no_temp_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.hard_link(s, "a", "b")?
  uu.succeeds(uu.invoke(s, "ln", ["--force", "a", "b"])?)
  var leftovers: List[Str] = []
  for entry in fs.children(s.root)? {
    if entry.name not in ["a", "b"] { leftovers += [entry.name] }
  }
  assert leftovers.is_empty()
}

# origin: uutils test_ln::test_force_replace_hard_link_in_symlinked_directory
test test_uu_ln_force_replace_hard_link_in_symlinked_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "new")?
  uu.mkdir(s, "real")?
  scene_symlink(s, "real", "dirlink")?
  uu.touch(s, "real/link")?
  uu.succeeds(uu.invoke(s, "ln", ["-f", "new", "dirlink/link"])?)
  assert fs.stat(uu.at(s, "real/link"))?.ino == fs.stat(uu.at(s, "new"))?.ino
}

# origin: uutils test_ln::test_symlink_target_dir_non_utf8_source_name
test test_uu_ln_symlink_target_dir_non_utf8_source_name { |ctx|
  let s = uu.scene(ctx)?
  let target_dir = "test_symlink_target_dir_non_utf8"
  let source_bytes = b"source_\xff\xfe"
  uu.mkdir(s, target_dir)?
  let source = uu.at_bytes(s, source_bytes)?
  source.write("")?
  let run1 = uu.invoke_paths(s, "ln", [p"-s", p"-t", Path(target_dir), Path.parse_bytes(source_bytes)?])?
  uu.succeeds(run1)
  uu.no_stderr(run1)
  var entries: List[Path] = []
  for entry in fs.children(uu.at(s, target_dir))? { entries += [entry.path] }
  assert entries.len() == 1
  assert entries[0].components()[-1].bytes() == source_bytes
  let created_link = uu.at_bytes(s, bytes.concat([bytes.from_text(target_dir), b"/", source_bytes]))?
  assert created_link.readlink()?.bytes() == source_bytes
}

# origin: uutils test_ln::test_ln_non_utf8_paths
test test_uu_ln_ln_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let name = b"test_\xff\xfe.txt"
  let link_name = b"link_\xff\xfe.txt"
  uu.at_bytes(s, name)?.write("")?
  uu.succeeds(uu.invoke_paths(s, "ln", [Path.parse_bytes(name)?, Path.parse_bytes(link_name)?])?)
  assert uu.at_bytes(s, name)?.is_file()?
  assert uu.at_bytes(s, link_name)?.is_file()?
  let symlink_name = b"symlink_\xff\xfe.txt"
  uu.succeeds(uu.invoke_paths(s, "ln", [p"-s", Path.parse_bytes(name)?, Path.parse_bytes(symlink_name)?])?)
  assert uu.at_bytes(s, symlink_name)?.is_symlink()?
}

# origin: uutils test_ln::test_symlink_relative_path
test test_uu_ln_symlink_relative_path { |ctx|
  let s = uu.scene(ctx)?
  let dir = "test_symlink_existing_dir"
  let file_a = "test_symlink_relative_a"
  let link = "test_symlink_relative_link"
  let source = "test_symlink_existing_dir/../test_symlink_existing_dir/../test_symlink_existing_dir/../test_symlink_relative_a"
  uu.mkdir(s, dir)?
  let run1 = uu.invoke(s, "ln", ["-r", "-s", "-v", source, link])?
  uu.succeeds(run1)
  uu.stdout_only(run1, f"'{link}' -> '{file_a}'\n")
  assert uu.is_symlink(s, link)?
  assert resolve_link(s, link)? == file_a
  let s2 = uu.scene(ctx)?
  let unchanged = uu.invoke(s2, "ln", ["-s", "-v", source, link])?
  uu.succeeds(unchanged)
  uu.stdout_only(unchanged, f"'{link}' -> '{source}'\n")
  assert uu.is_symlink(s2, link)?
  assert resolve_link(s2, link)? == source
}

# origin: uutils test_ln::test_ln_no_dereference_symbolic
test test_uu_ln_ln_no_dereference_symbolic { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  scene_symlink(s, "a", "b")?
  uu.touch(s, "x")?
  let run1 = uu.invoke(s, "ln", ["-n", "x", "b"])?
  uu.fails(run1)
  uu.stderr_contains(run1, "failed to create hard link 'b'")
  uu.stderr_contains(run1, "File exists")
  assert ! has_kind(s, "a/x", "file")?
  uu.succeeds(uu.invoke(s, "ln", ["-bn", "x", "b"])?)
  assert ! has_kind(s, "a/x", "file")?
  assert has_kind(s, "b", "file")?
  assert uu.is_symlink(s, "b~")?
}

# origin: uutils test_ln::test_force_ln_existing_hard_link_entry
test test_uu_ln_force_ln_existing_hard_link_entry { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "hardlink\n")?
  uu.mkdir(s, "dir")?
  let first = uu.invoke(s, "ln", ["file", "dir"])?
  uu.succeeds(first)
  uu.no_stderr(first)
  assert has_kind(s, "dir/file", "file")?
  let forced = uu.invoke(s, "ln", ["-f", "file", "dir"])?
  uu.succeeds(forced)
  uu.no_stderr(forced)
  assert has_kind(s, "file", "file")?
  assert has_kind(s, "dir/file", "file")?
  uu.file_is(s, "file", "hardlink\n")
  uu.file_is(s, "dir/file", "hardlink\n")
  assert fs.stat(uu.at(s, "file"))?.ino == fs.stat(uu.at(s, "dir/file"))?.ino
}

# origin: uutils test_ln::test_hard_logical_non_exit_fail
test test_uu_ln_hard_logical_non_exit_fail { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "/no-such-dir", "no-such-dir")?
  let run1 = uu.invoke(s, "ln", ["-L", "no-such-dir", "hard-to-dangle"])?
  uu.fails(run1)
  uu.stderr_contains(run1, "failed to access 'no-such-dir'")
}

# origin: uutils test_ln::test_force_replace_never_leaves_the_destination_name_free
test test_uu_ln_force_replace_never_leaves_the_destination_name_free { |ctx|
  for symbolic in [true, false] {
    let s = uu.scene(ctx)?
    uu.touch(s, "a")?
    uu.touch(s, "b")?
    if symbolic { scene_symlink(s, "a", "link")? } else { uu.hard_link(s, "a", "link")? }
    let racer_source = """
proc main(...args: List[Str]) {
  let root = Path(args[0])
  while ! fp"{root}/.racer-stop".exists()? {
    match fp"{root}/link".symlink(to: p"claimed-by-attacker") {
      Ok(_) => {
        fp"{root}/.racer-claimed".write("")?
        return
      }
      Err(failure) => { assert failure.errno == 17 }
    }
    if ! fp"{root}/.racer-ready".exists()? { fp"{root}/.racer-ready".write("")? }
  }
}
"""
    let script = uu.at(s, ".racer.xsh")
    script.write(racer_source)?
    let output = uu.at(s, ".racer-output")
    let error_output = uu.at(s, ".racer-error")
    let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, script, s.root], s.root,
      {}, b"", output, error_output, timeout: 30s)
    let racer = spawn plan?
    defer racer.cancel(kill_after: 100ms)
    let deadline = time.now() + 5000
    while ! uu.exists(s, ".racer-ready")? {
      assert ! uu.exists(s, ".racer-claimed")?, "destination was claimed before replacements started"
      assert time.now() < deadline, f"racer did not reach its first conflicting create: {error_output.read_text()?}"
      time.sleep(1ms)
    }
    for _ in range(100) {
      let args = (if symbolic { ["--force", "-s"] } else { ["--force"] }).extend(["b", "link"])
      uu.succeeds(uu.invoke(s, "ln", args, timeout: 5s)?)
    }
    uu.touch(s, ".racer-stop")?
    assert (wait racer?).exited_with(0), error_output.read_text()?
    assert ! uu.exists(s, ".racer-claimed")?, f"destination name was unoccupied during forced replace (symbolic={symbolic})"
  }
}
