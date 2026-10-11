##! Transcribed from the MIT-licensed uutils mv integration tests.
use support.uu as uu

proc scene(ctx: TestContext) [fs, error] -> Result[uu.Scene] {
  let s = uu.scene(ctx)?
  uu.fixture(s, "mv", "hello_world.txt", "hello_world.txt")?
  Ok(s)
}

# Missing entries and missing symlink targets are false for Rust fixture predicates.
# Preserve other host errors instead of treating them as absent paths.
proc has_kind(s: uu.Scene, name: Str, kind: Str, follow: Bool) [fs, error] -> Result[Bool] {
  match fs.stat(uu.at(s, name), follow_symlinks: follow) {
    Ok(meta) => Ok(meta.kind == kind),
    Err(failure) => {
      if failure.errno == 2 or failure.errno == 20 { Ok(false) } else { Err(failure) }
    },
  }
}
proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool] { has_kind(s, name, "file", true) }
proc dir_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool] { has_kind(s, name, "dir", true) }
proc symlink_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool] { has_kind(s, name, "symlink", false) }

# origin: uutils test_mv::test_mv_file_into_dir_where_both_are_files
test test_uu_mv_mv_file_into_dir_where_both_are_files { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r1 = uu.invoke(s, "mv", ["a", "b/"])?
  uu.fails(r1)
  uu.stderr_only(r1, "mv: cannot stat 'b/': Not a directory\n")
}

# origin: uutils test_mv::test_mv_file_to_broken_symlink_directory
test test_uu_mv_mv_file_to_broken_symlink_directory { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, uu.at(s, "missing-target").display(), "broken")?
  let r1 = uu.invoke(s, "mv", ["file", "broken"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, "broken")?
  assert !symlink_exists(s, "broken")?
  assert !file_exists(s, "file")?
}

# origin: uutils test_mv::test_mv_file_to_broken_symlink_file
test test_uu_mv_mv_file_to_broken_symlink_file { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, uu.at(s, "missing-target").display(), "broken")?
  let r1 = uu.invoke(s, "mv", ["file", "broken"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, "broken")?
  assert !symlink_exists(s, "broken")?
  assert !file_exists(s, "file")?
}

# origin: uutils test_mv::test_mv_file_to_symlink_directory
test test_uu_mv_mv_file_to_symlink_directory { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.touch(s, "a/empty_file_a")?
  uu.touch(s, "b")?
  uu.symlink(s, uu.at(s, "a").display(), "symlink")?
  assert file_exists(s, "symlink/empty_file_a")?
  let r1 = uu.invoke(s, "mv", ["b", "symlink"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert dir_exists(s, "symlink")?
  assert symlink_exists(s, "symlink")?
  assert file_exists(s, "symlink/b")?
  assert !file_exists(s, "b")?
  assert dir_exists(s, "a")?
  assert file_exists(s, "a/b")?
}

# origin: uutils test_mv::test_mv_force_replace_file
test test_uu_mv_mv_force_replace_file { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_force_replace_file_a"
  let file_b = "test_mv_force_replace_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", ["--force", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !file_exists(s, file_a)?
  assert file_exists(s, file_b)?
}

# origin: uutils test_mv::test_mv_hardlink_preservation
test test_uu_mv_mv_hardlink_preservation { |ctx|
  let s = scene(ctx)?
  uu.write(s, "file1", "test content")?
  uu.hard_link(s, "file1", "file2")?
  uu.mkdir(s, "target")?
  let r1 = uu.invoke(s, "mv", ["file1", "file2", "target"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, "target/file1")?
  assert file_exists(s, "target/file2")?
}

# origin: uutils test_mv::test_mv_interactive
test test_uu_mv_mv_interactive { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_interactive_file_a"
  let file_b = "test_mv_interactive_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", ["-i", file_a, file_b], stdin: bytes.from_text("n"))?
  uu.fails(r1)
  uu.no_stdout(r1)
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  let r2 = uu.invoke(s, "mv", ["-i", file_a, file_b], stdin: bytes.from_text("Yesh"))?
  uu.succeeds(r2)
  uu.no_stdout(r2)
  assert !file_exists(s, file_a)?
  assert file_exists(s, file_b)?
}

# origin: uutils test_mv::test_mv_interactive_dir_to_file_not_affirmative
test test_uu_mv_mv_interactive_dir_to_file_not_affirmative { |ctx|
  let s = scene(ctx)?
  let dir = "test_mv_interactive_dir_to_file_not_affirmative_dir"
  let file = "test_mv_interactive_dir_to_file_not_affirmative_file"
  uu.mkdir(s, dir)?
  uu.touch(s, file)?
  let r1 = uu.invoke(s, "mv", [dir, file, "-i"], stdin: bytes.from_text("n"))?
  uu.fails(r1)
  uu.no_stdout(r1)
  assert dir_exists(s, dir)?
}

# origin: uutils test_mv::test_mv_interactive_no_clobber_force_last_arg_wins
test test_uu_mv_mv_interactive_no_clobber_force_last_arg_wins { |ctx|
  let s = scene(ctx)?
  let file_a = "a.txt"
  let file_b = "b.txt"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", [file_a, file_b, "-f", "-i", "-n", "--debug"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "skipped 'b.txt'")
  let r2 = uu.invoke(s, "mv", [file_a, file_b, "-n", "-f", "-i"])?
  uu.fails(r2)
  uu.stderr_only(r2, f"mv: overwrite '{file_b}'? ")
  uu.write(s, file_a, "aa")?
  let r3 = uu.invoke(s, "mv", [file_a, file_b, "-i", "-n", "-f"])?
  uu.succeeds(r3)
  uu.no_output(r3)
  assert !file_exists(s, file_a)?
  assert "aa" == uu.read_text(s, file_b)?
}

# origin: uutils test_mv::test_mv_interactive_with_dir_as_target
test test_uu_mv_mv_interactive_with_dir_as_target { |ctx|
  let s = scene(ctx)?
  let file = "test_mv_interactive_file"
  let target_dir = "target"
  uu.mkdir(s, target_dir)?
  uu.touch(s, file)?
  uu.touch(s, f"{target_dir}/{file}")?
  let r1 = uu.invoke(s, "mv", [file, target_dir, "-i"], stdin: bytes.from_text("n"))?
  uu.fails(r1)
  assert !("cannot move" in r1.stderr.utf8()?)
  uu.no_stdout(r1)
}

# origin: uutils test_mv::test_mv_into_self_data
test test_uu_mv_mv_into_self_data { |ctx|
  let s = scene(ctx)?
  let sub_dir = "sub_folder"
  let file1 = "t1.test"
  let file2 = "sub_folder/t2.test"
  let file1_result_location = "sub_folder/t1.test"
  uu.mkdir(s, sub_dir)?
  uu.touch(s, file1)?
  uu.touch(s, file2)?
  let r1 = uu.invoke(s, "mv", [file1, sub_dir, sub_dir])?
  uu.fails_with_code(r1, 1)
  assert dir_exists(s, sub_dir)?
  assert file_exists(s, file1_result_location)?
  assert file_exists(s, file2)?
  assert !file_exists(s, file1)?
}

# origin: uutils test_mv::test_mv_invalid_arg
test test_uu_mv_mv_invalid_arg { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "mv", ["--definitely-invalid"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_mv::test_mv_missing_dest
test test_uu_mv_mv_missing_dest { |ctx|
  let s = scene(ctx)?
  let dir = "dir"
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "mv", [dir])?
  uu.fails(r1)
}

# origin: uutils test_mv::test_mv_move_file_between_dirs
test test_uu_mv_mv_move_file_between_dirs { |ctx|
  let s = scene(ctx)?
  let dir1 = "test_mv_move_file_between_dirs_dir1"
  let dir2 = "test_mv_move_file_between_dirs_dir2"
  let file = "test_mv_move_file_between_dirs_file"
  uu.mkdir(s, dir1)?
  uu.mkdir(s, dir2)?
  uu.touch(s, f"{dir1}/{file}")?
  assert file_exists(s, f"{dir1}/{file}")?
  let r1 = uu.invoke(s, "mv", [f"{dir1}/{file}", dir2])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !file_exists(s, f"{dir1}/{file}")?
  assert file_exists(s, f"{dir2}/{file}")?
}

# origin: uutils test_mv::test_mv_move_file_into_dir
test test_uu_mv_mv_move_file_into_dir { |ctx|
  let s = scene(ctx)?
  let dir = "test_mv_move_file_into_dir_dir"
  let file = "test_mv_move_file_into_dir_file"
  uu.mkdir(s, dir)?
  uu.touch(s, file)?
  let r1 = uu.invoke(s, "mv", [file, dir])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, f"{dir}/{file}")?
}

# origin: uutils test_mv::test_mv_move_file_into_dir_with_target_arg
test test_uu_mv_mv_move_file_into_dir_with_target_arg { |ctx|
  let s = scene(ctx)?
  let dir = "test_mv_move_file_into_dir_with_target_arg_dir"
  let file = "test_mv_move_file_into_dir_with_target_arg_file"
  uu.mkdir(s, dir)?
  uu.touch(s, file)?
  let r1 = uu.invoke(s, "mv", ["--target", dir, file])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, f"{dir}/{file}")?
}

# origin: uutils test_mv::test_mv_move_file_into_file_with_target_arg
test test_uu_mv_mv_move_file_into_file_with_target_arg { |ctx|
  let s = scene(ctx)?
  let file1 = "test_mv_move_file_into_file_with_target_arg_file1"
  let file2 = "test_mv_move_file_into_file_with_target_arg_file2"
  uu.touch(s, file1)?
  uu.touch(s, file2)?
  let r1 = uu.invoke(s, "mv", ["--target", file1, file2])?
  uu.fails(r1)
  uu.stderr_only(r1, f"mv: target directory '{file1}': Not a directory\n")
  assert file_exists(s, file1)?
}

# origin: uutils test_mv::test_mv_move_multiple_files_into_file
test test_uu_mv_mv_move_multiple_files_into_file { |ctx|
  let s = scene(ctx)?
  let file1 = "test_mv_move_multiple_files_into_file1"
  let file2 = "test_mv_move_multiple_files_into_file2"
  let file3 = "test_mv_move_multiple_files_into_file3"
  uu.touch(s, file1)?
  uu.touch(s, file2)?
  uu.touch(s, file3)?
  let r1 = uu.invoke(s, "mv", [file1, file2, file3])?
  uu.fails(r1)
  uu.stderr_only(r1, f"mv: target '{file3}': Not a directory\n")
  assert file_exists(s, file1)?
  assert file_exists(s, file2)?
}

# origin: uutils test_mv::test_mv_multiple_files
test test_uu_mv_mv_multiple_files { |ctx|
  let s = scene(ctx)?
  let target_dir = "test_mv_multiple_files_dir"
  let file_a = "test_mv_multiple_file_a"
  let file_b = "test_mv_multiple_file_b"
  uu.mkdir(s, target_dir)?
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", [file_a, file_b, target_dir])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, f"{target_dir}/{file_a}")?
  assert file_exists(s, f"{target_dir}/{file_b}")?
}

# origin: uutils test_mv::test_mv_multiple_folders
test test_uu_mv_mv_multiple_folders { |ctx|
  let s = scene(ctx)?
  let target_dir = "test_mv_multiple_dirs_dir"
  let dir_a = "test_mv_multiple_dir_a"
  let dir_b = "test_mv_multiple_dir_b"
  uu.mkdir(s, target_dir)?
  uu.mkdir(s, dir_a)?
  uu.mkdir(s, dir_b)?
  let r1 = uu.invoke(s, "mv", [dir_a, dir_b, target_dir])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert dir_exists(s, f"{target_dir}/{dir_a}")?
  assert dir_exists(s, f"{target_dir}/{dir_b}")?
}

# origin: uutils test_mv::test_mv_no_clobber
test test_uu_mv_mv_no_clobber { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_no_clobber_file_a"
  let file_b = "test_mv_no_clobber_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", ["-n", file_a, file_b, "--debug"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "skipped 'test_mv_no_clobber_file_b")
  assert file_exists(s, file_a)?
  assert file_exists(s, file_b)?
}

# origin: uutils test_mv::test_mv_no_prompt_unwriteable_file_with_no_tty
test test_uu_mv_mv_no_prompt_unwriteable_file_with_no_tty { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "source_notty")?
  uu.touch(s, "target_notty")?
  uu.set_mode(s, "target_notty", 0o000)?
  let r1 = uu.invoke(s, "mv", ["source_notty", "target_notty"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !file_exists(s, "source_notty")?
  assert file_exists(s, "target_notty")?
}

# origin: uutils test_mv::test_mv_no_target_dir_with_dest_not_existing
test test_uu_mv_mv_no_target_dir_with_dest_not_existing { |ctx|
  let s = scene(ctx)?
  let dir_a = "a"
  let dir_b = "b"
  uu.mkdir(s, dir_a)?
  let r1 = uu.invoke(s, "mv", ["-T", dir_a, dir_b])?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert !dir_exists(s, dir_a)?
  assert dir_exists(s, dir_b)?
}

# origin: uutils test_mv::test_mv_no_target_dir_with_dest_not_existing_and_ending_with_slash
test test_uu_mv_mv_no_target_dir_with_dest_not_existing_and_ending_with_slash { |ctx|
  let s = scene(ctx)?
  let dir_a = "a"
  let dir_b = "b/"
  uu.mkdir(s, dir_a)?
  let r1 = uu.invoke(s, "mv", ["-T", dir_a, dir_b])?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert !dir_exists(s, dir_a)?
  assert dir_exists(s, dir_b)?
}

# origin: uutils test_mv::test_mv_numbered_if_existing_backup_existing
test test_uu_mv_mv_numbered_if_existing_backup_existing { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_backup_numbering_file_a"
  let file_b = "test_mv_backup_numbering_file_b"
  let file_b_backup = "test_mv_backup_numbering_file_b.~1~"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  uu.touch(s, file_b_backup)?
  let r1 = uu.invoke(s, "mv", ["--backup=existing", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_b)?
  assert file_exists(s, file_b_backup)?
  assert file_exists(s, f"{file_b}.~2~")?
}

# origin: uutils test_mv::test_mv_numbered_if_existing_backup_nil
test test_uu_mv_mv_numbered_if_existing_backup_nil { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_backup_numbering_file_a"
  let file_b = "test_mv_backup_numbering_file_b"
  let file_b_backup = "test_mv_backup_numbering_file_b.~1~"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  uu.touch(s, file_b_backup)?
  let r1 = uu.invoke(s, "mv", ["--backup=nil", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file_b)?
  assert file_exists(s, file_b_backup)?
  assert file_exists(s, f"{file_b}.~2~")?
}

# origin: uutils test_mv::test_mv_overwrite_dir
test test_uu_mv_mv_overwrite_dir { |ctx|
  let s = scene(ctx)?
  let dir_a = "test_mv_overwrite_dir_a"
  let dir_b = "test_mv_overwrite_dir_b"
  uu.mkdir(s, dir_a)?
  uu.mkdir(s, dir_b)?
  let r1 = uu.invoke(s, "mv", ["-T", dir_a, dir_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !dir_exists(s, dir_a)?
  assert dir_exists(s, dir_b)?
}

# origin: uutils test_mv::test_mv_overwrite_nonempty_dir
test test_uu_mv_mv_overwrite_nonempty_dir { |ctx|
  let s = scene(ctx)?
  let dir_a = "test_mv_overwrite_nonempty_dir_a"
  let dir_b = "test_mv_overwrite_nonempty_dir_b"
  let dummy = "test_mv_overwrite_nonempty_dir_b/file"
  uu.mkdir(s, dir_a)?
  uu.mkdir(s, dir_b)?
  uu.touch(s, dummy)?
  let r1 = uu.invoke(s, "mv", ["-vT", dir_a, dir_b])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot overwrite")
  assert dir_exists(s, dir_a)?
  assert dir_exists(s, dir_b)?
}

# origin: uutils test_mv::test_mv_overwrite_nonempty_dir_into_dir
test test_uu_mv_mv_overwrite_nonempty_dir_into_dir { |ctx|
  let s = scene(ctx)?
  let dir_a = "test_mv_overwrite_nonempty_dir_into_dir_a"
  let dir_b = "test_mv_overwrite_nonempty_dir_into_dir_b"
  let target_dir = f"{dir_b}/{dir_a}"
  let dummy = f"{target_dir}/file"
  uu.mkdir(s, dir_a)?
  uu.mkdir(s, dir_b)?
  uu.mkdir(s, target_dir)?
  uu.touch(s, dummy)?
  let r1 = uu.invoke(s, "mv", [dir_a, dir_b])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot overwrite")
  assert dir_exists(s, dir_a)?
  assert dir_exists(s, dir_b)?
}

# origin: uutils test_mv::test_mv_rename_dir
test test_uu_mv_mv_rename_dir { |ctx|
  let s = scene(ctx)?
  let dir1 = "test_mv_rename_dir"
  let dir2 = "test_mv_rename_dir2"
  uu.mkdir(s, dir1)?
  let r1 = uu.invoke(s, "mv", [dir1, dir2])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert dir_exists(s, dir2)?
}

# origin: uutils test_mv::test_mv_rename_file
test test_uu_mv_mv_rename_file { |ctx|
  let s = scene(ctx)?
  let file1 = "test_mv_rename_file"
  let file2 = "test_mv_rename_file2"
  uu.touch(s, file1)?
  let r1 = uu.invoke(s, "mv", [file1, file2])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, file2)?
}

# origin: uutils test_mv::test_mv_replace_file
test test_uu_mv_mv_replace_file { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_replace_file_a"
  let file_b = "test_mv_replace_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", [file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !file_exists(s, file_a)?
  assert file_exists(s, file_b)?
}

# origin: uutils test_mv::test_mv_replace_symlink_with_directory
test test_uu_mv_mv_replace_symlink_with_directory { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "a")?
  uu.mkdir(s, "b")?
  uu.touch(s, "b/empty_file_b")?
  uu.symlink(s, uu.at(s, "a").display(), "symlink")?
  let r1 = uu.invoke(s, "mv", ["-T", "b", "symlink"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot overwrite non-directory")
  uu.stderr_contains(r1, "with directory")
}

# origin: uutils test_mv::test_mv_replace_symlink_with_file
test test_uu_mv_mv_replace_symlink_with_file { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  uu.symlink(s, uu.at(s, "a").display(), "symlink")?
  let r1 = uu.invoke(s, "mv", ["-T", "b", "symlink"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, "symlink")?
  assert !symlink_exists(s, "symlink")?
  assert !file_exists(s, "b")?
  assert file_exists(s, "a")?
}

# origin: uutils test_mv::test_mv_replace_symlink_with_symlink
test test_uu_mv_mv_replace_symlink_with_symlink { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.touch(s, "a/empty_file_a")?
  uu.touch(s, "b/empty_file_b")?
  uu.symlink(s, uu.at(s, "a").display(), "symlink_a")?
  uu.symlink(s, uu.at(s, "b").display(), "symlink_b")?
  assert uu.read_text(s, "symlink_a/empty_file_a")? == ""
  let r1 = uu.invoke(s, "mv", ["-T", "symlink_b", "symlink_a"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert file_exists(s, "symlink_a/empty_file_b")?
  assert !file_exists(s, "symlink_a/empty_file_a")?
  assert !symlink_exists(s, "symlink_b")?
}

# origin: uutils test_mv::test_mv_same_broken_symlink
test test_uu_mv_mv_same_broken_symlink { |ctx|
  let s = scene(ctx)?
  uu.symlink(s, uu.at(s, "missing-target").display(), "broken")?
  let r1 = uu.invoke(s, "mv", ["broken", "broken"])?
  uu.fails(r1)
  uu.stderr_only(r1, "mv: 'broken' and 'broken' are the same file\n")
}

# origin: uutils test_mv::test_mv_same_file
test test_uu_mv_mv_same_file { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_same_file_a"
  uu.touch(s, file_a)?
  let r1 = uu.invoke(s, "mv", [file_a, file_a])?
  uu.fails(r1)
  uu.stderr_only(r1, f"mv: '{file_a}' and '{file_a}' are the same file\n")
}

# origin: uutils test_mv::test_mv_same_file_dot_dir
test test_uu_mv_mv_same_file_dot_dir { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "mv", [".", "."])?
  uu.fails(r1)
  uu.stderr_only(r1, "mv: '.' and './.' are the same file\n")
}

# origin: uutils test_mv::test_mv_same_file_not_dot_dir
test test_uu_mv_mv_same_file_not_dot_dir { |ctx|
  let s = scene(ctx)?
  let dir = "test_mv_errors_dir"
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "mv", [dir, dir])?
  uu.fails(r1)
  uu.stderr_only(r1, f"mv: cannot move '{dir}' to a subdirectory of itself, '{dir}/{dir}'\n")
}

# origin: uutils test_mv::test_mv_same_hardlink
test test_uu_mv_mv_same_hardlink { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_same_file_a"
  let file_b = "test_mv_same_file_b"
  uu.touch(s, file_a)?
  uu.hard_link(s, file_a, file_b)?
  uu.touch(s, file_a)?
  let r1 = uu.invoke(s, "mv", [file_a, file_b])?
  uu.fails(r1)
  uu.stderr_only(r1, f"mv: '{file_a}' and '{file_b}' are the same file\n")
}

# origin: uutils test_mv::test_mv_same_hardlink_backup_simple
test test_uu_mv_mv_same_hardlink_backup_simple { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_same_file_a"
  let file_b = "test_mv_same_file_b"
  uu.touch(s, file_a)?
  uu.hard_link(s, file_a, file_b)?
  let r1 = uu.invoke(s, "mv", [file_a, file_b, "--backup=simple"])?
  uu.succeeds(r1)
}

# origin: uutils test_mv::test_mv_same_hardlink_backup_simple_destroy
test test_uu_mv_mv_same_hardlink_backup_simple_destroy { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_same_file_a~"
  let file_b = "test_mv_same_file_a"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", [file_a, file_b, "--b=simple"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "backing up 'test_mv_same_file_a' might destroy source")
}

# origin: uutils test_mv::test_mv_simple_backup
test test_uu_mv_mv_simple_backup { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_simple_backup_file_a"
  let file_b = "test_mv_simple_backup_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", ["-b", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}~")?
}

# origin: uutils test_mv::test_mv_simple_backup_for_directory
test test_uu_mv_mv_simple_backup_for_directory { |ctx|
  let s = scene(ctx)?
  let dir_a = "test_mv_simple_backup_dir_a"
  let dir_b = "test_mv_simple_backup_dir_b"
  uu.mkdir(s, dir_a)?
  uu.mkdir(s, dir_b)?
  uu.touch(s, f"{dir_a}/file_a")?
  uu.touch(s, f"{dir_b}/file_b")?
  let r1 = uu.invoke(s, "mv", ["-T", "-b", dir_a, dir_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !dir_exists(s, dir_a)?
  assert dir_exists(s, dir_b)?
  assert dir_exists(s, f"{dir_b}~")?
  assert file_exists(s, f"{dir_b}/file_a")?
  assert file_exists(s, f"{dir_b}~/file_b")?
}

# origin: uutils test_mv::test_mv_simple_backup_with_file_extension
test test_uu_mv_mv_simple_backup_with_file_extension { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_simple_backup_file_a.txt"
  let file_b = "test_mv_simple_backup_file_b.txt"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", ["-b", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}~")?
}

# origin: uutils test_mv::test_mv_symlink_into_target
test test_uu_mv_mv_symlink_into_target { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.symlink(s, uu.at(s, "dir").display(), "dir-link")?
  let r1 = uu.invoke(s, "mv", ["dir-link", "dir"])?
  uu.succeeds(r1)
}

# origin: uutils test_mv::test_mv_target_dir
test test_uu_mv_mv_target_dir { |ctx|
  let s = scene(ctx)?
  let dir = "test_mv_target_dir_dir"
  let file_a = "test_mv_target_dir_file_a"
  let file_b = "test_mv_target_dir_file_b"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "mv", ["-t", dir, file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !file_exists(s, file_a)?
  assert !file_exists(s, file_b)?
  assert file_exists(s, f"{dir}/{file_a}")?
  assert file_exists(s, f"{dir}/{file_b}")?
}

# origin: uutils test_mv::test_mv_target_dir_single_source
test test_uu_mv_mv_target_dir_single_source { |ctx|
  let s = scene(ctx)?
  let dir = "test_mv_target_dir_single_source_dir"
  let file = "test_mv_target_dir_single_source_file"
  uu.touch(s, file)?
  uu.mkdir(s, dir)?
  let r1 = uu.invoke(s, "mv", ["-t", dir, file])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !file_exists(s, file)?
  assert file_exists(s, f"{dir}/{file}")?
}

# origin: uutils test_mv::test_mv_update_with_dest_ending_with_slash
test test_uu_mv_mv_update_with_dest_ending_with_slash { |ctx|
  let s = scene(ctx)?
  let source = "source"
  let dest = "destination/"
  uu.mkdir(s, "source")?
  let r1 = uu.invoke(s, "mv", ["--update", source, dest])?
  uu.succeeds(r1)
  assert !dir_exists(s, source)?
  assert dir_exists(s, dest)?
}

# origin: uutils test_mv::test_mv_verbose
test test_uu_mv_mv_verbose { |ctx|
  let s = scene(ctx)?
  let dir = "test_mv_verbose_dir"
  let file_a = "test_mv_verbose_file_a"
  let file_b = "test_mv_verbose_file_b"
  uu.mkdir(s, dir)?
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", ["-v", file_a, file_b])?
  uu.succeeds(r1)
  uu.stdout_only(r1, f"renamed '{file_a}' -> '{file_b}'\n")
  uu.touch(s, file_a)?
  let r2 = uu.invoke(s, "mv", ["-vb", file_a, file_b])?
  uu.succeeds(r2)
  uu.stdout_only(r2, f"renamed '{file_a}' -> '{file_b}' (backup: '{file_b}~')\n")
}

# origin: uutils test_mv::test_suffix_without_backup_option
test test_uu_mv_suffix_without_backup_option { |ctx|
  let s = scene(ctx)?
  let file_a = "test_mv_custom_backup_suffix_file_a"
  let file_b = "test_mv_custom_backup_suffix_file_b"
  let suffix = "super-suffix-of-the-century"
  uu.touch(s, file_a)?
  uu.touch(s, file_b)?
  let r1 = uu.invoke(s, "mv", [f"--suffix={suffix}", file_a, file_b])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  assert !file_exists(s, file_a)?
  assert file_exists(s, file_b)?
  assert file_exists(s, f"{file_b}{suffix}")?
}

# origin: uutils test_mv::test_mv_exchange_wrong_operand_count
test test_uu_mv_mv_exchange_wrong_operand_count { |ctx|
  let s = scene(ctx)?
  uu.write(s, "only", "x")?
  let one = uu.invoke(s, "mv", ["--exchange", "only"])?
  uu.fails_with_code(one, 1)
  uu.stderr_only(one, "mv: missing destination file operand after 'only'\nTry 'mv --help' for more information.\n")
  uu.write(s, "two", "y")?
  uu.write(s, "three", "z")?
  let three = uu.invoke(s, "mv", ["--exchange", "only", "two", "three"])?
  uu.fails_with_code(three, 1)
  uu.stderr_only(three, "mv: target 'three': Not a directory\n")
}

# origin: uutils test_mv::test_mv_force_no_prompt_unwriteable_file
test test_uu_mv_mv_force_no_prompt_unwriteable_file { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "source_f")?
  uu.touch(s, "target_f")?
  uu.set_mode(s, "target_f", 0o000)?
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let r = uu.invoke_from_path(s, "mv", ["-f", "source_f", "target_f"], stdin: fp"{pty.name}", timeout: 2s)?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert !file_exists(s, "source_f")?
  assert file_exists(s, "target_f")?
}

# origin: uutils test_mv::test_mv_hardlink_to_symlink
test test_uu_mv_mv_hardlink_to_symlink { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "file")?
  uu.symlink(s, uu.at(s, "file").display(), "symlink")?
  uu.hard_link(s, "symlink", "hardlink_to_symlink")?
  let first = uu.invoke(s, "mv", ["symlink", "hardlink_to_symlink"])?
  uu.fails(first)
  let second = scene(ctx)?
  uu.touch(second, "file")?
  uu.symlink(second, uu.at(second, "file").display(), "symlink")?
  uu.hard_link(second, "symlink", "hardlink_to_symlink")?
  let backup = uu.invoke(second, "mv", ["--backup", "symlink", "hardlink_to_symlink"])?
  uu.succeeds(backup)
  assert !symlink_exists(second, "symlink")?
  assert symlink_exists(second, "hardlink_to_symlink~")?
}

# origin: uutils test_mv::test_mv_interactive_error
test test_uu_mv_mv_interactive_error { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "test_mv_errors_dir")?
  uu.touch(s, "test_mv_errors_file_a")?
  let r = uu.invoke(s, "mv", ["-i", "test_mv_errors_dir", "test_mv_errors_file_a"], stdin: b"y")?
  uu.fails(r)
  assert r.stderr.len() != 0
}

# origin: uutils test_mv::test_mv_mixed_hardlinks_and_regular_files
test test_uu_mv_mv_mixed_hardlinks_and_regular_files { |ctx|
  let s = scene(ctx)?
  uu.write(s, "hardlink1", "hardlink content")?
  uu.hard_link(s, "hardlink1", "hardlink2")?
  uu.write(s, "regular1", "regular content")?
  uu.write(s, "regular2", "regular content 2")?
  uu.mkdir(s, "target")?
  let r = uu.invoke(s, "mv", ["hardlink1", "hardlink2", "regular1", "regular2", "target"])?
  uu.succeeds(r)
  for file in ["hardlink1", "hardlink2", "regular1", "regular2"] { assert file_exists(s, f"target/{file}")? }
  let first = fs.stat(uu.at(s, "target/hardlink1"), follow_symlinks: true)?
  let second = fs.stat(uu.at(s, "target/hardlink2"), follow_symlinks: true)?
  let regular1 = fs.stat(uu.at(s, "target/regular1"), follow_symlinks: true)?
  let regular2 = fs.stat(uu.at(s, "target/regular2"), follow_symlinks: true)?
  if first.dev == second.dev { assert first.ino == second.ino }
  assert regular1.ino != regular2.ino
}

# origin: uutils test_mv::test_mv_permission_error
test test_uu_mv_mv_permission_error { |ctx|
  let s = scene(ctx)?
  let first = uu.invoke(s, "mkdir", ["-m444", "bar"])?
  uu.succeeds(first)
  defer uu.set_mode(s, "bar", 0o755)?
  let second = uu.invoke(s, "mkdir", ["-m777", "foo"])?
  uu.succeeds(second)
  let r = uu.invoke(s, "mv", ["foo", "bar/foo"])?
  uu.fails(r)
  uu.stderr_contains(r, "Permission denied")
}

# origin: uutils test_mv::test_mv_prompt_unwriteable_file_when_using_tty
test test_uu_mv_mv_prompt_unwriteable_file_when_using_tty { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "source")?
  uu.touch(s, "target")?
  uu.set_mode(s, "target", 0o000)?
  defer uu.set_mode(s, "target", 0o644)?
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  assert unix.write_fd(pty.master, b"n\n")? == 2
  let r = uu.invoke_from_path(s, "mv", ["source", "target"], stdin: fp"{pty.name}", timeout: 2s)?
  uu.fails(r)
  uu.stderr_contains(r, "replace 'target', overriding mode 0000")
  assert file_exists(s, "source")?
}

# origin: uutils test_mv::test_mv_same_symlink
test test_uu_mv_mv_same_symlink { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "test_mv_same_file_a")?
  uu.symlink(s, uu.at(s, "test_mv_same_file_a").display(), "test_mv_same_file_b")?
  let first = uu.invoke(s, "mv", ["test_mv_same_file_b", "test_mv_same_file_a"])?
  uu.fails(first)
  uu.stderr_only(first, "mv: 'test_mv_same_file_b' and 'test_mv_same_file_a' are the same file\n")
  let second = scene(ctx)?
  uu.touch(second, "test_mv_same_file_a")?
  uu.symlink(second, uu.at(second, "test_mv_same_file_a").display(), "test_mv_same_file_b")?
  let a_to_b = uu.invoke(second, "mv", ["test_mv_same_file_a", "test_mv_same_file_b"])?
  uu.succeeds(a_to_b)
  assert file_exists(second, "test_mv_same_file_b")?
  assert !file_exists(second, "test_mv_same_file_a")?
  let third = scene(ctx)?
  uu.touch(third, "test_mv_same_file_a")?
  uu.symlink(third, uu.at(third, "test_mv_same_file_a").display(), "test_mv_same_file_b")?
  uu.symlink(third, uu.at(third, "test_mv_same_file_b").display(), "test_mv_same_file_c")?
  let c_to_b = uu.invoke(third, "mv", ["test_mv_same_file_c", "test_mv_same_file_b"])?
  uu.succeeds(c_to_b)
  assert !symlink_exists(third, "test_mv_same_file_c")?
  assert symlink_exists(third, "test_mv_same_file_b")?
  let fourth = scene(ctx)?
  uu.touch(fourth, "test_mv_same_file_a")?
  uu.symlink(fourth, uu.at(fourth, "test_mv_same_file_a").display(), "test_mv_same_file_b")?
  uu.symlink(fourth, uu.at(fourth, "test_mv_same_file_b").display(), "test_mv_same_file_c")?
  let c_to_a = uu.invoke(fourth, "mv", ["test_mv_same_file_c", "test_mv_same_file_a"])?
  uu.fails(c_to_a)
  uu.stderr_only(c_to_a, "mv: 'test_mv_same_file_c' and 'test_mv_same_file_a' are the same file\n")
}

# origin: uutils test_mv::test_mv_seen_file
test test_uu_mv_mv_seen_file { |ctx|
  let s = scene(ctx)?
  for dir in ["a", "b", "c"] { uu.mkdir(s, dir)? }
  uu.write(s, "a/f", "a")?
  uu.write(s, "b/f", "b")?
  let r = uu.invoke(s, "mv", ["a/f", "b/f", "c"])?
  uu.fails(r)
  uu.stderr_contains(r, "will not overwrite just-created 'c/f' with 'b/f'")
  assert file_exists(s, "c/f")?
  assert file_exists(s, "b/f")?
  assert !file_exists(s, "a/f")?
}

# origin: uutils test_mv::test_mv_seen_multiple_files_to_directory
test test_uu_mv_mv_seen_multiple_files_to_directory { |ctx|
  let s = scene(ctx)?
  for dir in ["a", "b", "c"] { uu.mkdir(s, dir)? }
  uu.write(s, "a/f", "a")?
  uu.write(s, "b/f", "b")?
  uu.write(s, "b/g", "g")?
  let r = uu.invoke(s, "mv", ["a/f", "b/f", "b/g", "c"])?
  uu.fails(r)
  uu.stderr_contains(r, "will not overwrite just-created 'c/f' with 'b/f'")
  assert file_exists(s, "c/f")?
  assert file_exists(s, "b/f")?
  assert !file_exists(s, "a/f")?
  assert !file_exists(s, "b/g")?
  assert file_exists(s, "c/g")?
}

# origin: uutils test_mv::test_mv_strip_slashes
test test_uu_mv_mv_strip_slashes { |ctx|
  let s = scene(ctx)?
  let dir = "test_mv_strip_slashes_dir"
  let file = "test_mv_strip_slashes_file"
  let source = f"{file}/"
  uu.mkdir(s, dir)?
  uu.touch(s, file)?
  let first = uu.invoke(s, "mv", [source, dir])?
  uu.fails(first)
  assert !file_exists(s, f"{dir}/{file}")?
  let second = uu.invoke(s, "mv", ["--strip-trailing-slashes", source, dir])?
  uu.succeeds(second)
  uu.no_stderr(second)
  assert file_exists(s, f"{dir}/{file}")?
}

# origin: uutils test_mv::test_mv_update_option
test test_uu_mv_mv_update_option { |ctx|
  let s = scene(ctx)?
  let a = "test_mv_update_option_file_a"
  let b = "test_mv_update_option_file_b"
  uu.touch(s, a)?
  uu.touch(s, b)?
  let now = time.now() * 1000000
  fs.set_times(uu.at(s, a), atime_ns: now, mtime_ns: now)?
  fs.set_times(uu.at(s, b), atime_ns: now, mtime_ns: now + 3600000000000)?
  let first = uu.invoke(s, "mv", ["--update", a, b])?
  uu.succeeds(first)
  assert file_exists(s, a)?
  assert file_exists(s, b)?
  let second = uu.invoke(s, "mv", ["--update", b, a])?
  uu.succeeds(second)
  uu.no_stderr(second)
  assert file_exists(s, a)?
  assert !file_exists(s, b)?
}

# origin: uutils test_mv::test_mv_verbose_directory_recursive
test test_uu_mv_mv_verbose_directory_recursive { |ctx|
  let s = scene(ctx)?
  for dir in ["mv-dir", "mv-dir/a", "mv-dir/a/b", "mv-dir/a/b/c", "mv-dir/d", "mv-dir/d/e", "mv-dir/d/e/f"] { uu.mkdir(s, dir)? }
  uu.touch(s, "mv-dir/a/b/c/file1")?
  uu.touch(s, "mv-dir/d/e/f/file2")?
  let target_root = fs.tempdir_in(p"/dev/shm")?
  defer target_root.close()
  let target = target_root.host_path()?
  assert fs.stat(s.root)?.dev != fs.stat(target)?.dev
  let r = uu.invoke(s, "mv", ["--verbose", "mv-dir", target.display()])?
  uu.succeeds(r)
  assert !dir_exists(s, "mv-dir")?
  for name in ["mv-dir", "mv-dir/a", "mv-dir/a/b", "mv-dir/a/b/c", "mv-dir/d", "mv-dir/d/e", "mv-dir/d/e/f", "mv-dir/a/b/c/file1", "mv-dir/d/e/f/file2"] { assert fp"{target}/{name}".exists()? }
  for name in ["mv-dir/a", "mv-dir/a/b", "mv-dir/a/b/c", "mv-dir/d", "mv-dir/d/e", "mv-dir/d/e/f"] { uu.stdout_contains(r, f"created directory '{target}/{name}'") }
  for name in ["mv-dir/a/b/c/file1", "mv-dir/d/e/f/file2"] { uu.stdout_contains(r, f"copied '{name}' -> '{target}/{name}'") }
}

# origin: uutils test_mv::test_mv_with_source_file_opened_and_target_file_exists
test test_uu_mv_mv_with_source_file_opened_and_target_file_exists { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "source_file_opened")?
  let fd = unix.open_fd(uu.at(s, "source_file_opened"), write: true)?
  defer unix.close_fd(fd)
  uu.touch(s, "target_file_exists")?
  let r = uu.invoke(s, "mv", ["source_file_opened", "target_file_exists"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_mv::test_mv_xattr_enotsup_silent
test test_uu_mv_mv_xattr_enotsup_silent { |ctx|
  let s = scene(ctx)?
  uu.write(s, "src", "x")?
  fs.xattr_set(uu.at(s, "src"), "user.t", b"v")?
  let target = p"/dev/shm/mv_test"
  defer target.remove(missing_ok: true)?
  let r = uu.invoke(s, "mv", [uu.at(s, "src").display(), target.display()])?
  uu.succeeds(r)
  uu.no_stderr(r)
}
