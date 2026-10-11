##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_truncate.rs.

use support.uu as uu

# A bounded reader owns the pipe diagnostic bytes and reaps the child process.
proc pipe_diagnostic(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let capture = test.temp_dir(s.ctx, name: "diagnostic")?
  defer capture.remove()?
  let pipe = fp"{capture}/pipe"
  fs.mkfifo(pipe, 0o600)?
  let output = fp"{capture}/output"
  let errors = fp"{capture}/reader-error"
  let cat = process.which("cat")?
  let reader = spawn process.command_argv(cat, [cat, pipe], s.root, {}, b"", output, errors, timeout: 5s)?
  defer reader.cancel(kill_after: 100ms)?
  let r = uu.invoke(s, "truncate", args, stdout: fp"{capture}/stdout", stderr: pipe, timeout: 5s)?
  assert (wait reader?).exited_with(0)
  assert errors.read_bytes()? == b""
  Ok({...r, stdout: fp"{capture}/stdout".read_bytes()?, stderr: output.read_bytes()?})
}

# Keep the replica alive while draining the exact terminal stderr bytes.
proc terminal_diagnostic(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let pair = unix.open_pty()?
  defer unix.close_fd(pair.master)?
  defer unix.close_fd(pair.replica)?
  let capture = test.temp_dir(s.ctx, name: "terminal")?
  defer capture.remove()?
  let output = fp"{capture}/stdout"
  let argv = uu.argv(s, "truncate", [Path(word) for word in args])?
  let status = process.run(process.command_argv(s.ctx.xsh_bin, argv, s.root, {}, b"", output, Path(pair.name), timeout: 5s))?
  var diagnostic = b""
  while "readable" in unix.poll_fd(pair.master, ["readable"], timeout_ms: 0)? {
    let chunk = unix.read_fd(pair.master, 8192)?
    break when chunk.is_empty()
    diagnostic = bytes.concat([diagnostic, chunk])
  }
  Ok({util: "truncate", args: args, status: status.exit_code()?, stdout: output.read_bytes()?, stderr: diagnostic})
}

# origin: uutils test_truncate::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_truncate_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "probe")?
  let r = pipe_diagnostic(s, ["-s", "10fb", "probe"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "Invalid number: '10fb'")
}

# origin: uutils test_truncate::fifo::test_fifo_error_reference_and_size
test test_uu_truncate_fifo_fifo_error_reference_and_size { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  uu.touch(s, "reference_file")?
  let r = uu.invoke(s, "truncate", ["-r", "reference_file", "-s", "+0", "fifo"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "cannot open 'fifo' for writing: No such device or address")
}

# origin: uutils test_truncate::fifo::test_fifo_error_reference_file_only
test test_uu_truncate_fifo_fifo_error_reference_file_only { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  uu.touch(s, "reference_file")?
  let r = uu.invoke(s, "truncate", ["-r", "reference_file", "fifo"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "cannot open 'fifo' for writing: No such device or address")
}

# origin: uutils test_truncate::fifo::test_fifo_error_size_only
test test_uu_truncate_fifo_fifo_error_size_only { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  let r = uu.invoke(s, "truncate", ["-s", "0", "fifo"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "cannot open 'fifo' for writing: No such device or address")
}

# origin: uutils test_truncate::test_at_least_grows
test test_uu_truncate_at_least_grows { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890")?
  let r = uu.invoke(s, "truncate", ["--size", ">15", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 15
}

# origin: uutils test_truncate::test_at_least_no_change
test test_uu_truncate_at_least_no_change { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890")?
  let r = uu.invoke(s, "truncate", ["--size", ">4", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 10
}

# origin: uutils test_truncate::test_at_most_no_change
test test_uu_truncate_at_most_no_change { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890")?
  let r = uu.invoke(s, "truncate", ["--size", "<40", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 10
}

# origin: uutils test_truncate::test_at_most_shrinks
test test_uu_truncate_at_most_shrinks { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890")?
  let r = uu.invoke(s, "truncate", ["--size", "<4", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 4
}

# origin: uutils test_truncate::test_continue_after_error
test test_uu_truncate_continue_after_error { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  let r = uu.invoke(s, "truncate", ["-s", "0", "a", "dir", "b"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "dir")
  assert uu.at(s, "a").is_file()?
  assert uu.at(s, "b").is_file()?
}

# origin: uutils test_truncate::test_decrease_file_size
test test_uu_truncate_decrease_file_size { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890")?
  let r = uu.invoke(s, "truncate", ["--size=-4", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 6
}

# origin: uutils test_truncate::test_division_by_zero_reference_and_size
test test_uu_truncate_division_by_zero_reference_and_size { |ctx|
  let s = uu.scene(ctx)?
  {
    let fresh = uu.scene(ctx)?
    uu.touch(fresh, "truncate_test_1")?
    let r = uu.invoke(fresh, "truncate", ["-r", "truncate_test_1", "-s", "/0", "file"])?
    uu.fails(r)
    uu.no_stdout(r)
    uu.stderr_contains(r, "division by zero")
  }
  {
    let fresh = uu.scene(ctx)?
    uu.touch(fresh, "truncate_test_1")?
    let r = uu.invoke(fresh, "truncate", ["-r", "truncate_test_1", "-s", "%0", "file"])?
    uu.fails(r)
    uu.no_stdout(r)
    uu.stderr_contains(r, "division by zero")
  }
}

# origin: uutils test_truncate::test_division_by_zero_size_only
test test_uu_truncate_division_by_zero_size_only { |ctx|
  let s = uu.scene(ctx)?
  {
    let fresh = uu.scene(ctx)?
    let r = uu.invoke(fresh, "truncate", ["-s", "/0", "file"])?
    uu.fails(r)
    uu.no_stdout(r)
    uu.stderr_contains(r, "division by zero")
  }
  {
    let fresh = uu.scene(ctx)?
    let r = uu.invoke(fresh, "truncate", ["-s", "%0", "file"])?
    uu.fails(r)
    uu.no_stdout(r)
    uu.stderr_contains(r, "division by zero")
  }
}

# origin: uutils test_truncate::test_empty_size
test test_uu_truncate_empty_size { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s", "", "asd"])?
  uu.fails(r)
  uu.stderr_is(r, "truncate: Invalid number: ''\n")
}

# origin: uutils test_truncate::test_error_filename_only
test test_uu_truncate_error_filename_only { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["file"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "you must specify either '--size' or '--reference'")
}

# origin: uutils test_truncate::test_failed
test test_uu_truncate_failed { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", [])?
  uu.fails(r)
}

# origin: uutils test_truncate::test_failed_incorrect_arg
test test_uu_truncate_failed_incorrect_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s", "+5A", "truncate_test_1"])?
  uu.fails(r)
}

# origin: uutils test_truncate::test_increase_file_size
test test_uu_truncate_increase_file_size { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_1", "")?
  let r = uu.invoke(s, "truncate", ["-s", "+5K", "truncate_test_1"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_1")? == 5120
}

# origin: uutils test_truncate::test_increase_file_size_kb
test test_uu_truncate_increase_file_size_kb { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_1", "")?
  let r = uu.invoke(s, "truncate", ["-s", "+5KB", "truncate_test_1"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_1")? == 5000
}

# origin: uutils test_truncate::test_invalid_numbers
test test_uu_truncate_invalid_numbers { |ctx|
  let s = uu.scene(ctx)?
  {
    let fresh = uu.scene(ctx)?
    let r = uu.invoke(fresh, "truncate", ["-s", "0X", "file"])?
    uu.fails(r)
    uu.stderr_contains(r, "Invalid number: '" + "0X" + "'")
  }
  {
    let fresh = uu.scene(ctx)?
    let r = uu.invoke(fresh, "truncate", ["-s", "0XB", "file"])?
    uu.fails(r)
    uu.stderr_contains(r, "Invalid number: '" + "0XB" + "'")
  }
  {
    let fresh = uu.scene(ctx)?
    let r = uu.invoke(fresh, "truncate", ["-s", "0B", "file"])?
    uu.fails(r)
    uu.stderr_contains(r, "Invalid number: '" + "0B" + "'")
  }
  {
    let fresh = uu.scene(ctx)?
    let r = uu.invoke(fresh, "truncate", ["-s", "1b", "file"])?
    uu.fails(r)
    uu.stderr_contains(r, "Invalid number: '" + "1b" + "'")
  }
}

# origin: uutils test_truncate::test_invalid_option
test test_uu_truncate_invalid_option { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["--this-arg-does-not-exist"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_truncate::test_io_blocks_uses_file_block_size
test test_uu_truncate_io_blocks_uses_file_block_size { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_1", "x")?
  let block_size = fs.stat(uu.at(s, "truncate_test_1"))?.blksize
  let r = uu.invoke(s, "truncate", ["--io-blocks", "--size=1", "truncate_test_1"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.size(s, "truncate_test_1")? == 1 * block_size
}

# origin: uutils test_truncate::test_io_blocks_uses_parent_block_size_for_new_file
test test_uu_truncate_io_blocks_uses_parent_block_size_for_new_file { |ctx|
  let s = uu.scene(ctx)?
  let block_size = fs.stat(uu.at(s, "."))?.blksize
  let r = uu.invoke(s, "truncate", ["--io-blocks", "--size=2", "truncate_test_1"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.size(s, "truncate_test_1")? == 2 * block_size
}

# origin: uutils test_truncate::test_negative_size_with_space
test test_uu_truncate_negative_size_with_space { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s", "-1", "truncate_test_1"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.at(s, "truncate_test_1").is_file()?
  assert uu.read(s, "truncate_test_1")?.is_empty()
}

# origin: uutils test_truncate::test_new_file
test test_uu_truncate_new_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s", "8", "new_file_that_does_not_exist_yet"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.at(s, "new_file_that_does_not_exist_yet").is_file()?
  assert uu.read(s, "new_file_that_does_not_exist_yet")? == b"\x00\x00\x00\x00\x00\x00\x00\x00"
}

# origin: uutils test_truncate::test_new_file_no_create_reference_only
test test_uu_truncate_new_file_no_create_reference_only { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_1", "1234567890")?
  let r = uu.invoke(s, "truncate", ["-r", "truncate_test_1", "-c", "new_file_that_does_not_exist_yet"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert ! uu.exists(s, "new_file_that_does_not_exist_yet")?
}

# origin: uutils test_truncate::test_new_file_no_create_size_and_reference
test test_uu_truncate_new_file_no_create_size_and_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_1", "1234567890")?
  let r = uu.invoke(s, "truncate", ["-r", "truncate_test_1", "-s", "+8", "-c", "new_file_that_does_not_exist_yet"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert ! uu.exists(s, "new_file_that_does_not_exist_yet")?
}

# origin: uutils test_truncate::test_new_file_no_create_size_only
test test_uu_truncate_new_file_no_create_size_only { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s", "8", "-c", "new_file_that_does_not_exist_yet"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert ! uu.exists(s, "new_file_that_does_not_exist_yet")?
}

# origin: uutils test_truncate::test_new_file_reference
test test_uu_truncate_new_file_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_1", "1234567890")?
  let r = uu.invoke(s, "truncate", ["-r", "truncate_test_1", "new_file_that_does_not_exist_yet"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.at(s, "new_file_that_does_not_exist_yet").is_file()?
  assert uu.read(s, "new_file_that_does_not_exist_yet")? == b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
}

# origin: uutils test_truncate::test_new_file_size_and_reference
test test_uu_truncate_new_file_size_and_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_1", "1234567890")?
  let r = uu.invoke(s, "truncate", ["-s", "+3", "-r", "truncate_test_1", "new_file_that_does_not_exist_yet"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.at(s, "new_file_that_does_not_exist_yet").is_file()?
  assert uu.read(s, "new_file_that_does_not_exist_yet")? == b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
}

# origin: uutils test_truncate::test_no_such_dir
test test_uu_truncate_no_such_dir { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s", "0", "a/b"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "cannot open 'a/b' for writing: No such file or directory")
}

# origin: uutils test_truncate::test_reference
test test_uu_truncate_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "truncate_test_2")?
  let r = uu.invoke(s, "truncate", ["-s", "+5KB", "truncate_test_1"])?
  uu.succeeds(r)
  let r2 = uu.invoke(s, "truncate", ["--reference", "truncate_test_1", "truncate_test_2"])?
  uu.succeeds(r2)
  assert uu.size(s, "truncate_test_2")? == 5000
}

# origin: uutils test_truncate::test_reference_file_not_found
test test_uu_truncate_reference_file_not_found { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-r", "a", "b"])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot stat 'a': No such file or directory")
}

# origin: uutils test_truncate::test_reference_non_utf8_path
test test_uu_truncate_reference_non_utf8_path { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "truncate_test_2")?
  let reference = Path.parse_bytes(b"test_\xff\xfe.txt")?
  let r = uu.invoke_paths(s, "truncate", [p"-s", p"+5KB", reference])?
  uu.succeeds(r)
  let copied = uu.invoke_paths(s, "truncate", [p"--reference", reference, p"truncate_test_2"])?
  uu.succeeds(copied)
  assert uu.size(s, "truncate_test_2")? == 5000
}

# origin: uutils test_truncate::test_reference_with_size_file_not_found
test test_uu_truncate_reference_with_size_file_not_found { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-r", "a", "-s", "+1", "b"])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot stat 'a': No such file or directory")
}

# origin: uutils test_truncate::test_relative_size_overflow_preserves_file
test test_uu_truncate_relative_size_overflow_preserves_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_1", "x")?
  let r = uu.invoke(s, "truncate", ["--size=+18446744073709551615", "truncate_test_1"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "Value too large for defined data type")
  uu.file_is(s, "truncate_test_1", "x")
}

# origin: uutils test_truncate::test_repeated_size_takes_the_last
test test_uu_truncate_repeated_size_takes_the_last { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "repeated")?
  let r = uu.invoke(s, "truncate", ["-s", "1", "-s", "2", "repeated"])?
  uu.succeeds(r)
  assert uu.size(s, "repeated")? == 2
}

# origin: uutils test_truncate::test_round_down
test test_uu_truncate_round_down { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890")?
  let r = uu.invoke(s, "truncate", ["--size", "/4", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 8
}

# origin: uutils test_truncate::test_round_up
test test_uu_truncate_round_up { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890")?
  let r = uu.invoke(s, "truncate", ["--size", "%4", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 12
}

# origin: uutils test_truncate::test_round_up_already_aligned
test test_uu_truncate_round_up_already_aligned { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "12345678")?
  let r = uu.invoke(s, "truncate", ["--size", "%4", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 8
}

# origin: uutils test_truncate::test_round_up_file_smaller_than_size
test test_uu_truncate_round_up_file_smaller_than_size { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890")?
  let r = uu.invoke(s, "truncate", ["--size", "%4K", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 4096
}

# origin: uutils test_truncate::test_round_up_unaligned
test test_uu_truncate_round_up_unaligned { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890123")?
  let r = uu.invoke(s, "truncate", ["--size", "%8", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 16
}

# origin: uutils test_truncate::test_sign_as_a_size
test test_uu_truncate_sign_as_a_size { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s", "+", "asd"])?
  uu.fails(r)
  uu.stderr_is(r, "truncate: Invalid number: '+'\n")
}

# origin: uutils test_truncate::test_size_above_i64_max_is_rejected_without_creating_file
test test_uu_truncate_size_above_i64_max_is_rejected_without_creating_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s", "9223372036854775808", "new-file"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "Value too large for defined data type")
  assert ! uu.exists(s, "new-file")?
}

# origin: uutils test_truncate::test_size_and_reference
test test_uu_truncate_size_and_reference { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_1", "1234567890")?
  uu.touch(s, "truncate_test_2")?
  let r = uu.invoke(s, "truncate", ["--reference", "truncate_test_1", "--size", "+5", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 15
}

# origin: uutils test_truncate::test_space_in_size
test test_uu_truncate_space_in_size { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "truncate_test_2", "1234567890")?
  let r = uu.invoke(s, "truncate", ["--size", " 4", "truncate_test_2"])?
  uu.succeeds(r)
  assert uu.size(s, "truncate_test_2")? == 4
}

# origin: uutils test_truncate::test_truncate_bytes_size
test test_uu_truncate_truncate_bytes_size { |ctx|
  let s = uu.scene(ctx)?
  {
    let fresh = uu.scene(ctx)?
    let r = uu.invoke(fresh, "truncate", ["--no-create", "--size", "K", "file"])?
    uu.succeeds(r)
  }
  {
    let fresh = uu.scene(ctx)?
    let r = uu.invoke(fresh, "truncate", ["--size", "1024R", "file"])?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, "truncate: Invalid number: '1024R': Value too large for defined data type\n")
  }
  {
    let fresh = uu.scene(ctx)?
    let r = uu.invoke(fresh, "truncate", ["--size", "1Y", "file"])?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, "truncate: Invalid number: '1Y': Value too large for defined data type\n")
  }
}

# origin: uutils test_truncate::test_truncate_non_utf8_paths
test test_uu_truncate_truncate_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_��.txt", "test content")?
  let filename = Path.parse_bytes(b"test_\xff\xfe.txt")?
  let r = uu.invoke_paths(s, "truncate", [p"-s", p"10", filename])?
  uu.succeeds(r)
}

# origin: uutils test_truncate::test_underflow_relative_size
test test_uu_truncate_underflow_relative_size { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "truncate", ["-s-1", "truncate_test_1"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.at(s, "truncate_test_1").is_file()?
  assert uu.read(s, "truncate_test_1")?.is_empty()
}

# origin: uutils test_truncate::diagnostics::test_snippet_counts_the_mode_character_before_the_size
test test_uu_truncate_diagnostics_snippet_counts_the_mode_character_before_the_size { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "probe")?
  let r = terminal_diagnostic(s, ["--size=+2Zx", "probe"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "truncate: Invalid number: '+2Zx'\r\n")
}
