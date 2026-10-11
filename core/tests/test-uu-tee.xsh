use support.uu as uu

pure numbered_input(count: Int) -> Bytes {
  bytes.from_text([f"{i}\n" for i in range(1, count + 1)].join(""))
}

# Wait for the temporary reader to close before exec so stdout is already
# broken. Shell status preserves signal deaths as well as ordinary exits.
proc broken_stdout(s: uu.Scene, args: List[Str], input: Bytes, idle: Bool = false) [fs, process, env, error] -> Result[uu.Ran] {
  uu.mkfifo(s, "broken-stdout")?
  let words = uu.argv(s, "tee", [Path(word) for word in args])?
  let setup = if idle {
    uu.mkfifo(s, "idle-input")?
    r"""exec 3<>idle-input; (exec 4<broken-stdout) & reader=$!; exec 4>broken-stdout; wait "$reader"; exec "$@" <idle-input >&4 """
  } else {
    r"""(exec 4<broken-stdout) & reader=$!; exec 4>broken-stdout; wait "$reader"; exec "$@" >&4 """
  }
  let out = uu.at(s, "capture-out")
  let err = uu.at(s, "capture-err")
  let plan = process.command_argv(p"/bin/sh", [p"sh", p"-c", Path(setup), p"sh"].extend(words), s.root, {}, input, out, err, timeout: if idle { 1s } else { 10s })
  let status = process.run(plan)?
  Ok({util: "tee", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

proc check_file(s: uu.Scene, name: Str, input: Bytes, short: Bool = false) [fs, error] -> Result[Unit] {
  assert uu.file_exists(s, name)?
  let actual = uu.read(s, name)?
  if short {
    assert actual.len() < input.len()
    assert input.starts_with(actual)
  } else {
    assert actual == input
  }
  Ok()
}

# Poll before each nonblocking read so buffering regressions fail within the
# original deadline while the producer deliberately keeps stdin open.
proc read_exact(reader: Int, count: Int, deadline: Int) [process, error, fs, time] -> Result[Bytes] {
  var result = b""
  while result.len() < count {
    assert time.now() < deadline, "Nothing was received through output pipe"
    if "readable" in unix.poll_fd(reader, ["readable"], timeout_ms: 1)? {
      result = bytes.concat([result, unix.read_fd(reader, count - result.len())?])
    }
  }
  Ok(result)
}

proc streaming(s: uu.Scene, short_read: Bool) [fs, process, env, error, time] -> Result[Unit] {
  let deadline = time.now() + (if short_read { 5000 } else { 100 })
  uu.mkfifo(s, "stream-input")?
  uu.mkfifo(s, "stream-output")?
  let reader = unix.open_fd(uu.at(s, "stream-output"), nonblock: true)?
  defer unix.close_fd(reader)
  let file = if short_read { "tee_short_read_out" } else { "tee_file_out" }
  let words = uu.argv(s, "tee", [Path(file)])?
  let plan = process.command_argv(p"/bin/sh", [p"sh", p"-c", Path(r"""exec "$@" <stream-input >stream-output """), p"sh"].extend(words), s.root, {}, b"", uu.at(s, "stream-capture"), uu.at(s, "stream-errors"), timeout: 5s)
  let child = spawn plan?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  var writer: Int? = null
  while writer == null {
    assert time.now() < deadline, "tee did not open its input pipe"
    match unix.open_fd(uu.at(s, "stream-input"), write: true, nonblock: true) {
      Ok(fd) => writer = fd,
      Err(failure) => { assert failure.errno == 6, failure.message; time.sleep(1ms)? },
    }
  }
  let producer = writer ?? -1
  var producer_open = true
  defer { if producer_open { unix.close_fd(producer) } }
  if short_read {
    assert unix.write_fd(producer, b"first\n")? == 6
    assert read_exact(reader, 6, deadline)? == b"first\n"
    time.sleep(50ms)?
    assert unix.write_fd(producer, b"second\n")? == 7
    assert read_exact(reader, 7, deadline)? == b"second\n"
    unix.close_fd(producer)?
    producer_open = false
    let done = process.wait_timeout([child], 5s)?
    assert done != null, "tee did not complete within the timeout"
    assert done.status.exited_with(0)
    assert time.now() < deadline
    assert uu.read(s, file)? == b"first\nsecond\n"
  } else {
    assert unix.write_fd(producer, b"a")? == 1
    assert read_exact(reader, 1, deadline)? == b"a"
    time.sleep(1ms)?
    assert uu.read(s, file)? == b"a"
    assert time.now() < deadline
  }
  Ok()
}

# origin: uutils test_tee::linux_only::test_permission_denied_clean
test test_uu_tee_linux_only_permission_denied_clean { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tee", ["/dev/mem"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tee: /dev/mem: Permission denied\n")
}

# origin: uutils test_tee::linux_only::test_pipe_error_default
test test_uu_tee_linux_only_pipe_error_default { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["tee_file_out_a"], input)?
  uu.fails(r)
  uu.no_stderr(r)
  check_file(s, "tee_file_out_a", input, short: true)?
}

# origin: uutils test_tee::linux_only::test_pipe_error_exit
test test_uu_tee_linux_only_pipe_error_exit { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=exit", "tee_file_out_a"], input)?
  uu.fails(r)
  uu.stderr_contains(r, "Broken pipe")
  check_file(s, "tee_file_out_a", input, short: true)?
}

# origin: uutils test_tee::linux_only::test_pipe_error_exit_nopipe
test test_uu_tee_linux_only_pipe_error_exit_nopipe { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=exit-nopipe", "tee_file_out_a"], input)?
  uu.succeeds(r)
  uu.no_stderr(r)
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_pipe_error_exit_nopipe_shortcut
test test_uu_tee_linux_only_pipe_error_exit_nopipe_shortcut { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=exit-nop", "tee_file_out_a"], input)?
  uu.succeeds(r)
  uu.no_stderr(r)
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_pipe_error_warn
test test_uu_tee_linux_only_pipe_error_warn { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=warn", "tee_file_out_a"], input)?
  uu.fails(r)
  uu.stderr_contains(r, "Broken pipe")
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_pipe_error_warn_nopipe_1
test test_uu_tee_linux_only_pipe_error_warn_nopipe_1 { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["-p", "tee_file_out_a"], input)?
  uu.succeeds(r)
  uu.no_stderr(r)
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_pipe_error_warn_nopipe_2
test test_uu_tee_linux_only_pipe_error_warn_nopipe_2 { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error", "tee_file_out_a"], input)?
  uu.succeeds(r)
  uu.no_stderr(r)
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_pipe_error_warn_nopipe_3
test test_uu_tee_linux_only_pipe_error_warn_nopipe_3 { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=warn-nopipe", "tee_file_out_a"], input)?
  uu.succeeds(r)
  uu.no_stderr(r)
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_pipe_error_warn_nopipe_3_shortcut
test test_uu_tee_linux_only_pipe_error_warn_nopipe_3_shortcut { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=warn-", "tee_file_out_a"], input)?
  uu.succeeds(r)
  uu.no_stderr(r)
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_pipe_mode_broken_pipe_file
test test_uu_tee_linux_only_pipe_mode_broken_pipe_file { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["-p", "tee_file_out_a"], input)?
  uu.succeeds(r)
  uu.no_stderr(r)
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_pipe_mode_broken_pipe_only
test test_uu_tee_linux_only_pipe_mode_broken_pipe_only { |ctx|
  let s = uu.scene(ctx)?
  let r = broken_stdout(s, ["-p"], b"", idle: true)?
  uu.succeeds(r)
}

# origin: uutils test_tee::linux_only::test_space_error_default
test test_uu_tee_linux_only_space_error_default { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = uu.invoke(s, "tee", ["tee_file_out_a", "/dev/full"], stdin: input)?
  uu.fails(r)
  uu.stderr_contains(r, "No space left")
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_space_error_exit
test test_uu_tee_linux_only_space_error_exit { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=exit", "tee_file_out_a", "/dev/full"], input)?
  uu.fails(r)
  # GNU stops at the broken stdout before reaching /dev/full.
  uu.stderr_contains(r, "Broken pipe")
  check_file(s, "tee_file_out_a", input, short: true)?
}

# origin: uutils test_tee::linux_only::test_space_error_exit_nopipe
test test_uu_tee_linux_only_space_error_exit_nopipe { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=exit-nopipe", "tee_file_out_a", "/dev/full"], input)?
  uu.fails(r)
  uu.stderr_contains(r, "No space left")
  check_file(s, "tee_file_out_a", input, short: true)?
}

# origin: uutils test_tee::linux_only::test_space_error_warn
test test_uu_tee_linux_only_space_error_warn { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=warn", "tee_file_out_a", "/dev/full"], input)?
  uu.fails(r)
  uu.stderr_contains(r, "No space left")
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_space_error_warn_nopipe_1
test test_uu_tee_linux_only_space_error_warn_nopipe_1 { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["-p", "tee_file_out_a", "/dev/full"], input)?
  uu.fails(r)
  uu.stderr_contains(r, "No space left")
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_space_error_warn_nopipe_2
test test_uu_tee_linux_only_space_error_warn_nopipe_2 { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error", "tee_file_out_a", "/dev/full"], input)?
  uu.fails(r)
  uu.stderr_contains(r, "No space left")
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_space_error_warn_nopipe_3
test test_uu_tee_linux_only_space_error_warn_nopipe_3 { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(100000)
  let r = broken_stdout(s, ["--output-error=warn-nopipe", "tee_file_out_a", "/dev/full"], input)?
  uu.fails(r)
  uu.stderr_contains(r, "No space left")
  check_file(s, "tee_file_out_a", input, short: false)?
}

# origin: uutils test_tee::linux_only::test_tee_no_more_writeable_1
test test_uu_tee_linux_only_tee_no_more_writeable_1 { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(10)
  let r = uu.invoke(s, "tee", ["/dev/full", "tee_file_out"], stdin: input)?
  uu.fails(r)
  uu.stdout_contains(r, input.utf8()?)
  uu.stderr_is(r, "tee: /dev/full: No space left on device\n")
  assert uu.read(s, "tee_file_out")? == input
}

# origin: uutils test_tee::linux_only::test_tee_no_more_writeable_2
test test_uu_tee_linux_only_tee_no_more_writeable_2 { |ctx|
  let s = uu.scene(ctx)?
  let input = numbered_input(10)
  let r = uu.invoke(s, "tee", ["tee_file_out_a", "tee_file_out_b"], stdin: input, stdout: p"/dev/full", stdout_append: true)?
  uu.fails(r)
  assert uu.read(s, "tee_file_out_a")? == input
  assert uu.read(s, "tee_file_out_b")? == input
  uu.stderr_contains(r, "No space left on device")
}

# origin: uutils test_tee::test_broken_pipe_early_termination_stdout_only
test test_uu_tee_broken_pipe_early_termination_stdout_only { |ctx|
  let s = uu.scene(ctx)?
  let r = broken_stdout(s, [], bytes.concat([b"x" for _ in range(10000)]))?
  assert r.status >= 0, "process did not exit"
}

# origin: uutils test_tee::test_error_stdin_directory
test test_uu_tee_error_stdin_directory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_from_path(s, "tee", [], s.root)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tee: read error: Is a directory\n")
}

# origin: uutils test_tee::test_invalid_arg
test test_uu_tee_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "tee", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_tee::test_output_error_flag_without_value_defaults_warn_nopipe
test test_uu_tee_output_error_flag_without_value_defaults_warn_nopipe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tee", ["--output-error", "tee_output_error_default.txt"], stdin: b"abc")?
  uu.succeeds(r)
  uu.stdout_is(r, "abc")
  assert uu.file_exists(s, "tee_output_error_default.txt")?
  assert uu.read(s, "tee_output_error_default.txt")? == b"abc"
}

# origin: uutils test_tee::test_output_error_presence_only_broken_pipe_unix
test test_uu_tee_output_error_presence_only_broken_pipe_unix { |ctx|
  let s = uu.scene(ctx)?
  let r = broken_stdout(s, ["--output-error"], bytes.concat([b"x" for _ in range(10000)]))?
  assert r.status >= 0, "process did not exit"
}

# origin: uutils test_tee::test_readonly
test test_uu_tee_readonly { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "tee_file_out", "world")?
  uu.set_mode(s, "tee_file_out", 0o444)?
  let r = uu.invoke(s, "tee", ["tee_file_out", "tee_file_out2"], stdin: b"hello")?
  uu.fails(r)
  uu.stdout_is(r, "hello")
  assert "Permission denied" in r.stderr.utf8()? or "Access is denied" in r.stderr.utf8()?
  assert uu.read(s, "tee_file_out")? == b"world"
  assert uu.read(s, "tee_file_out2")? == b"hello"
}

# origin: uutils test_tee::test_tee_append
test test_uu_tee_tee_append { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "tee_out")?
  uu.write(s, "tee_out", "tee_sample_content")?
  assert uu.read(s, "tee_out")? == b"tee_sample_content"
  let r = uu.invoke(s, "tee", ["-a", "tee_out"], stdin: b"tee_sample_content")?
  uu.succeeds(r)
  uu.stdout_is(r, "tee_sample_content")
  assert uu.file_exists(s, "tee_out")?
  assert uu.read(s, "tee_out")? == b"tee_sample_contenttee_sample_content"
}

# origin: uutils test_tee::test_tee_continues_after_short_read
test test_uu_tee_tee_continues_after_short_read { |ctx|
  streaming(uu.scene(ctx)?, true)?
}

# origin: uutils test_tee::test_tee_multiple_append_flags
test test_uu_tee_tee_multiple_append_flags { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "log1", "existing1\n")?
  uu.write(s, "log2", "existing2\n")?
  let r = uu.invoke(s, "tee", ["-a", "log1", "-a", "log2"], stdin: b"don't fail me now rust")?
  uu.succeeds(r)
  uu.stdout_is(r, "don't fail me now rust")
  assert uu.file_exists(s, "log1")?
  assert uu.file_exists(s, "log2")?
  assert uu.read(s, "log1")? == b"existing1\ndon't fail me now rust"
  assert uu.read(s, "log2")? == b"existing2\ndon't fail me now rust"
}

# origin: uutils test_tee::test_tee_output_not_buffered
test test_uu_tee_tee_output_not_buffered { |ctx|
  streaming(uu.scene(ctx)?, false)?
}

# origin: uutils test_tee::test_tee_processing_multiple_operands
test test_uu_tee_tee_processing_multiple_operands { |ctx|
  for count in [1, 2, 12, 13] {
    let s = uu.scene(ctx)?
    let files = [f"{i}" for i in range(1, count + 1)]
    let r = uu.invoke(s, "tee", files, stdin: b"tee_sample_content")?
    uu.succeeds(r)
    uu.stdout_is(r, "tee_sample_content")
    for file in files {
      assert uu.file_exists(s, file)?
      assert uu.read(s, file)? == b"tee_sample_content"
    }
  }
}

# origin: uutils test_tee::test_tee_treat_minus_as_filename
test test_uu_tee_tee_treat_minus_as_filename { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tee", ["-"], stdin: b"tee_sample_content")?
  uu.succeeds(r)
  uu.stdout_is(r, "tee_sample_content")
  assert uu.file_exists(s, "-")?
  assert uu.read(s, "-")? == b"tee_sample_content"
}

# origin: uutils test_tee::test_write_failure_reports_error_and_nonzero_exit
test test_uu_tee_write_failure_reports_error_and_nonzero_exit { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "out_dir")?
  let r = uu.invoke(s, "tee", ["out_dir"], stdin: b"data")?
  uu.fails(r)
  assert ! r.stderr.is_empty()
}
