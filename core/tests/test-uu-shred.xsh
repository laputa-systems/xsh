##! Transcribed from the uutils coreutils integration tests for shred.

use support.uu as uu

# A missing file answers false; other metadata errors keep their cause.
proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  match uu.file_exists(s, name) {
    Ok(found) => Ok(found),
    Err(failure) => if failure.errno == 2 { Ok(false) } else { Err(failure) },
  }
}

# Keeping the replica open prevents terminal EOF while queued stderr is drained.
proc terminal_stderr(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let pair = unix.open_pty()?
  defer unix.close_fd(pair.master)?
  defer unix.close_fd(pair.replica)?
  unix.set_window_size(30, 80, xpixel: 640, ypixel: 300, fd: pair.replica)?
  let output = uu.at(s, "stdout")
  let argv = uu.argv(s, "shred", [Path(word) for word in args])?
  let plan = process.command_argv(s.ctx.xsh_bin, argv, s.root, {}, b"", output, Path(pair.name), timeout: 5s)
  let status = process.run(plan)?
  var errors = b""
  while "readable" in unix.poll_fd(pair.master, ["readable"], timeout_ms: 0)? {
    let chunk = unix.read_fd(pair.master, 8192)?
    break when chunk.is_empty()
    errors = bytes.concat([errors, chunk])
  }
  Ok({util: "shred", args: args, status: status.exit_code()?, stdout: output.read_bytes()?, stderr: errors})
}

# origin: uutils test_shred::diagnostics::test_snippet_points_at_the_unknown_unit
test test_uu_shred_diagnostics_snippet_points_at_the_unknown_unit { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "wipe_me")?
  let r = terminal_stderr(s, ["-s", "4vv", "wipe_me"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is_bytes(r, b"shred: invalid file size: '4vv'\r\n")
  assert file_exists(s, "wipe_me")?
}

# origin: uutils test_shred::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_shred_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "wipe_me")?
  let r = uu.invoke(s, "shred", ["-s", "4vv", "wipe_me"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "shred: invalid file size: '4vv'\n")
}

# origin: uutils test_shred::test_all_patterns_present
test test_uu_shred_all_patterns_present { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo.txt", "bar")?
  let r = uu.invoke(s, "shred", ["-vn25", "foo.txt"])?
  uu.succeeds(r)
  for pattern in ["000000", "ffffff", "555555", "aaaaaa", "249249", "492492", "6db6db", "924924", "b6db6d",
    "db6db6", "111111", "222222", "333333", "444444", "666666", "777777", "888888", "999999",
    "bbbbbb", "cccccc", "dddddd", "eeeeee",] {
    uu.stderr_contains(r, pattern)
  }
}

# origin: uutils test_shred::test_ambiguous_remove_arg
test test_uu_shred_ambiguous_remove_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shred", ["--remove=wip"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_shred::test_hex
test test_uu_shred_hex { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_hex")?
  let r = uu.invoke(s, "shred", ["--size=0x10", "test_hex"])?
  uu.succeeds(r)
}

# origin: uutils test_shred::test_invalid_arg
test test_uu_shred_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shred", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_shred::test_invalid_remove_arg
test test_uu_shred_invalid_remove_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shred", ["--remove=unknown"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_shred::test_random_source_dir
test test_uu_shred_random_source_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "source")?
  uu.write(s, "foo.txt", "a")?
  let r = uu.invoke(s, "shred", ["-v", "--random-source=source", "foo.txt"])?
  uu.fails(r)
  assert !("pass 2/3" in r.stderr.utf8()?)
  assert !("pass 3/3" in r.stderr.utf8()?)
}

# origin: uutils test_shred::test_random_source_open_error_includes_cause
test test_uu_shred_random_source_open_error_includes_cause { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "target")?
  let r = uu.invoke(s, "shred", ["--random-source=missing", "target"])?
  uu.fails(r)
  uu.stderr_only(r, "shred: missing: No such file or directory\n")
}

# origin: uutils test_shred::test_random_source_regular_file
test test_uu_shred_random_source_regular_file { |ctx|
  let s = uu.scene(ctx)?
  let many_bytes = bytes.concat([bytes.pack_le(i, 4)? for i in range(4096)])
  assert many_bytes.len() == 4096 * 4
  uu.write_bytes(s, "source_long", many_bytes)?
  uu.write(s, "foo.txt", "a")?
  let r = uu.invoke(s, "shred", ["-vn3", "--random-source=source_long", "foo.txt"])?
  uu.succeeds(r)
  uu.stderr_only(r, "shred: foo.txt: pass 1/3 (random)...\nshred: foo.txt: pass 2/3 (random)...\nshred: foo.txt: pass 3/3 (random)...\n")
  assert uu.read(s, "foo.txt")? == many_bytes[4096 * 2 + 3..4096 * 3 + 3]
}

# origin: uutils test_shred::test_shred
test test_uu_shred_shred { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_shred", "test_shred file content")?
  let r = uu.invoke(s, "shred", ["test_shred"])?
  uu.succeeds(r)
  assert file_exists(s, "test_shred")?
  assert uu.read(s, "test_shred")? != b"test_shred file content"
}

# origin: uutils test_shred::test_shred_empty
test test_uu_shred_shred_empty { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_shred_remove_a")?
  let empty = uu.invoke(s, "shred", ["-uv", "test_shred_remove_a"])?
  uu.succeeds(empty)
  assert !("1/3 (random)" in empty.stderr.utf8()?)
  assert !file_exists(s, "test_shred_remove_a")?
  uu.touch(s, "test_shred_remove_a")?
  uu.write(s, "test_shred_remove_a", "1")?
  let nonempty = uu.invoke(s, "shred", ["-uv", "test_shred_remove_a"])?
  uu.succeeds(nonempty)
  uu.stderr_contains(nonempty, "1/3 (random)")
  assert !file_exists(s, "test_shred_remove_a")?
}

# origin: uutils test_shred::test_shred_fail_no_perm
test test_uu_shred_shred_fail_no_perm { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/test_shred_remove_a")?
  let directory = uu.at(s, "dir")
  let mode = uu.mode(s, "dir")?
  uu.set_mode(s, "dir", mode.bit_and(0o555))?
  defer directory.chmod(mode)
  let r = uu.invoke(s, "shred", ["-uv", "dir/test_shred_remove_a"])?
  uu.fails(r)
  uu.stderr_contains(r, "failed to remove: Permission denied")
}

# origin: uutils test_shred::test_shred_force
test test_uu_shred_shred_force { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_shred_force")?
  assert file_exists(s, "test_shred_force")?
  let mode = uu.mode(s, "test_shred_force")?
  uu.set_mode(s, "test_shred_force", mode.bit_and(0o555))?
  let refused = uu.invoke(s, "shred", ["-u", "test_shred_force"])?
  uu.fails(refused)
  assert file_exists(s, "test_shred_force")?
  let forced = uu.invoke(s, "shred", ["-u", "-f", "test_shred_force"])?
  uu.succeeds(forced)
  assert !file_exists(s, "test_shred_force")?
}

# origin: uutils test_shred::test_shred_inaccessible_file_reports_real_error
test test_uu_shred_shred_inaccessible_file_reports_real_error { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "locked")?
  uu.touch(s, "locked/file")?
  let directory = uu.at(s, "locked")
  uu.set_mode(s, "locked", 0o000)?
  defer directory.chmod(0o755)
  let r = uu.invoke(s, "shred", ["locked/file"])?
  uu.fails(r)
  uu.stderr_contains(r, "Permission denied")
  assert !("No such file" in r.stderr.utf8()?)
}

# origin: uutils test_shred::test_shred_nineteen_passes_first_and_last_are_random
test test_uu_shred_shred_nineteen_passes_first_and_last_are_random { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "Us", bytes.from_ints([85 for _ in range(102400)])?)?
  uu.write(s, "f", "1")?
  let r = uu.invoke(s, "shred", ["-v", "-n19", "--random-source=Us", "f"])?
  uu.succeeds(r)
  for i in range(1, 20) { uu.stderr_contains(r, f"pass {i}/19") }
  uu.stderr_contains(r, "pass 1/19 (random)")
  uu.stderr_contains(r, "pass 19/19 (random)")
}

# origin: uutils test_shred::test_shred_non_utf8_paths
test test_uu_shred_shred_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let name = Path.parse_bytes(b"test_\xff\xfe.txt")?
  let file = uu.at_bytes(s, b"test_\xff\xfe.txt")?
  file.write("test content")?
  let r = uu.invoke_paths(s, "shred", [name])?
  uu.succeeds(r)
}

# origin: uutils test_shred::test_shred_remove
test test_uu_shred_shred_remove { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_shred_remove")?
  let r = uu.invoke(s, "shred", ["--remove", "test_shred_remove"])?
  uu.succeeds(r)
  assert !file_exists(s, "test_shred_remove")?
}

# origin: uutils test_shred::test_shred_remove_unlink
test test_uu_shred_shred_remove_unlink { |ctx|
  let s = uu.scene(ctx)?
  for argument in ["--remove=unlink", "--remove=unlin", "--remove=u"] {
    let scene = uu.scene(ctx)?
    uu.touch(scene, "test_shred_remove_unlink")?
    let r = uu.invoke(scene, "shred", [argument, "test_shred_remove_unlink"]) ?
    uu.succeeds(r)
    assert !file_exists(scene, "test_shred_remove_unlink")?
  }
}

# origin: uutils test_shred::test_shred_remove_unlink_relative_path
test test_uu_shred_shred_remove_unlink_relative_path { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1/dir2")?
  uu.write(s, "dir1/dir2/file1", "test data")?
  let r = uu.invoke(s, "shred", ["--remove=unlink", "dir1/dir2/file1"])?
  uu.succeeds(r)
  assert !file_exists(s, "dir1/dir2/file1")?
}

# origin: uutils test_shred::test_shred_remove_wipe
test test_uu_shred_shred_remove_wipe { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_shred_remove_wipe")?
  let r = uu.invoke(s, "shred", ["--remove=wipe", "test_shred_remove_wipe"])?
  uu.succeeds(r)
  assert !file_exists(s, "test_shred_remove_wipe")?
}

# origin: uutils test_shred::test_shred_remove_wipesync
test test_uu_shred_shred_remove_wipesync { |ctx|
  let s = uu.scene(ctx)?
  for argument in ["--remove=wipesync", "--remove=wipesyn", "--remove=wipes"] {
    let scene = uu.scene(ctx)?
    uu.touch(scene, "test_shred_remove_wipesync")?
    let r = uu.invoke(scene, "shred", [argument, "test_shred_remove_wipesync"]) ?
    uu.succeeds(r)
    assert !file_exists(scene, "test_shred_remove_wipesync")?
  }
}

# origin: uutils test_shred::test_shred_rename_exhaustion
test test_uu_shred_shred_rename_exhaustion { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test")?
  uu.touch(s, "000")?
  let r = uu.invoke(s, "shred", ["-vu", "test"])?
  uu.succeeds(r)
  uu.stderr_contains(r, "renamed to 0000")
  uu.stderr_contains(r, "renamed to 001")
  uu.stderr_contains(r, "renamed to 00")
  uu.stderr_contains(r, "removed")
  assert !file_exists(s, "test")?
}

# origin: uutils test_shred::test_shred_trailing_slash_on_dir
test test_uu_shred_shred_trailing_slash_on_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  let r = uu.invoke(s, "shred", ["d/"])?
  uu.fails(r)
  uu.stderr_contains(r, "Is a directory")
}

# origin: uutils test_shred::test_shred_trailing_slash_on_file
test test_uu_shred_shred_trailing_slash_on_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "shred", ["a/"])?
  uu.fails(r)
  uu.stderr_contains(r, "Not a directory")
}

# origin: uutils test_shred::test_shred_twenty_passes_with_known_random_source
test test_uu_shred_shred_twenty_passes_with_known_random_source { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "Us", bytes.from_ints([85 for _ in range(102400)])?)?
  uu.write(s, "f", "1")?
  let r = uu.invoke(s, "shred", ["-v", "-u", "-n20", "-s4096", "--random-source=Us", "f"])?
  uu.succeeds(r)
  for pass in ["pass 1/20 (random)",
        "pass 2/20 (ffffff)",
        "pass 3/20 (924924)",
        "pass 4/20 (888888)",
        "pass 5/20 (db6db6)",
        "pass 6/20 (777777)",
        "pass 7/20 (492492)",
        "pass 8/20 (bbbbbb)",
        "pass 9/20 (555555)",
        "pass 10/20 (aaaaaa)",
        "pass 11/20 (random)",
        "pass 12/20 (6db6db)",
        "pass 13/20 (249249)",
        "pass 14/20 (999999)",
        "pass 15/20 (111111)",
        "pass 16/20 (000000)",
        "pass 17/20 (b6db6d)",
        "pass 18/20 (eeeeee)",
        "pass 19/20 (333333)",
        "pass 20/20 (random)",] {
    uu.stderr_contains(r, pass)
  }
  uu.stderr_contains(r, "removing")
  uu.stderr_contains(r, "renamed to 0")
  uu.stderr_contains(r, "removed")
  assert !file_exists(s, "f")?
}

# origin: uutils test_shred::test_shred_u
test test_uu_shred_shred_u { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_shred_remove_a")?
  uu.touch(s, "test_shred_remove_b")?
  let r = uu.invoke(s, "shred", ["-u", "test_shred_remove_a"])?
  uu.succeeds(r)
  assert !file_exists(s, "test_shred_remove_a")?
  assert file_exists(s, "test_shred_remove_b")?
}

# origin: uutils test_shred::test_shred_verbose_no_padding_1
test test_uu_shred_shred_verbose_no_padding_1 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "non-empty")?
  let r = uu.invoke(s, "shred", ["-vn1", "foo"])?
  uu.succeeds(r)
  uu.stderr_only(r, "shred: foo: pass 1/1 (random)...\n")
}

# origin: uutils test_shred::test_shred_verbose_no_padding_10
test test_uu_shred_shred_verbose_no_padding_10 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "non-empty")?
  let r = uu.invoke(s, "shred", ["-vn10", "foo"])?
  uu.succeeds(r)
  uu.stderr_contains(r, "shred: foo: pass 1/10 (random)...\n")
}
