##! Native ports of the uutils vdir integration tests.
use support.uu as uu

# Listing captures live outside the directory being listed.
proc invoke(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let captures = test.temp_dir(s.ctx, name: "vdir-capture")?
  let output = fp"{captures}/stdout"
  let errors = fp"{captures}/stderr"
  let r = uu.invoke(s, "vdir", args, vars:, stdout: output, stderr: errors, timeout: 5s)?
  Ok({...r, stdout: output.read_bytes()?, stderr: errors.read_bytes()?})
}

# A FIFO preserves the diagnostic's pipe descriptor, with a bounded reader
# capturing bytes outside the directory being listed.
proc pipe_diagnostic(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let captures = test.temp_dir(s.ctx, name: "vdir-diagnostic")?
  let pipe = fp"{captures}/pipe"
  fs.mkfifo(pipe, 0o644)
  let output = fp"{captures}/stderr"
  let reader_error = fp"{captures}/reader-error"
  let cat = process.which("cat")?
  let plan = process.command_argv(cat, [cat, pipe], s.root, {}, b"", output, reader_error, timeout: 5s)
  let reader = spawn plan?
  defer reader.cancel(kill_after: 100ms)
  let r = uu.invoke(s, "vdir", args, stdout: fp"{captures}/stdout", stderr: pipe, timeout: 5s)?
  assert (wait reader?).exited_with(0)
  assert reader_error.read_bytes()? == b""
  Ok({...r, stderr: output.read_bytes()?})
}

# Keep each terminal replica open until its queued output has been drained.
proc terminal_output(fd: Int) [process, net, error] -> Result[Bytes, Error] {
  var output = b""
  while "readable" in unix.poll_fd(fd, ["readable"], timeout_ms: 0)? {
    let chunk = unix.read_fd(fd, 8192)?
    break when chunk.is_empty()
    output = bytes.concat([output, chunk])
  }
  Ok(output)
}

proc terminal() [process, error] -> Result[UnixPty, Error] {
  let pair = unix.open_pty()?
  unix.set_window_size(30, 80, xpixel: 640, ypixel: 300, fd: pair.replica)
  Ok(pair)
}

proc invoke_terminal(s: uu.Scene, vars: Record) [fs, process, net, env, error] -> Result[uu.Ran, Error] {
  let input = terminal()?
  defer unix.close_fd(input.master)
  defer unix.close_fd(input.replica)
  let output = terminal()?
  defer unix.close_fd(output.master)
  defer unix.close_fd(output.replica)
  let errors = terminal()?
  defer unix.close_fd(errors.master)
  defer unix.close_fd(errors.replica)
  let argv = uu.argv(s, "vdir", [], vars)?
  let plan = process.command_argv(s.ctx.xsh_bin, argv, s.root, vars, Path(input.name), Path(output.name), Path(errors.name), timeout: 5s)
  let status = process.run(plan)?
  Ok({util: "vdir", args: [], status: status.exit_code()?, stdout: terminal_output(output.master)?, stderr: terminal_output(errors.master)?})
}

# origin: uutils test_vdir::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_vdir_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = pipe_diagnostic(s, ["--block-size=1fb"])?
  uu.fails_with_code(r, 2)
  uu.stderr_is(r, "vdir: invalid suffix in --block-size argument '1fb'\n")
}

# origin: uutils test_vdir::test_column_output
test test_uu_vdir_column_output { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some-dir1")
  uu.touch(s, "some-file1")
  let first = invoke(s, ["-C"])?
  uu.succeeds(first)
  uu.stdout_contains(first, "some-file1")
  let second = invoke(s, ["-C"])?
  uu.succeeds(second)
  assert ! rx"[rwx-]{10}.*some-file1$".matches(second.stdout as Str)
}

# origin: uutils test_vdir::test_default_format_overrides
test test_uu_vdir_default_format_overrides { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")
  for args in [["-1"], ["-C"], ["-x"], ["-m"], ["--format=single-column"], ["--zero"], ["-g", "-C"]] {
    let r = invoke(s, args, {TIME_STYLE: "invalid"})?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, if args == ["--zero"] { b"file\0" } else { b"file\n" })
  }
  for args in [["-l", "-1"], ["-g", "--zero"], ["-C", "-g"]] {
    let r = invoke(s, args)?
    uu.succeeds(r)
    uu.stdout_contains(r, "total 0")
  }
}

# origin: uutils test_vdir::test_default_output
test test_uu_vdir_default_output { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some-dir1")
  uu.touch(s, "some-file1")
  fs.set_times(uu.at(s, "some-file1"), mtime_sec: 978307200, mtime_nsec: 0)
  let first = invoke(s, [])?
  uu.succeeds(first)
  uu.stdout_contains(first, "some-file1")
  let second = invoke(s, [])?
  uu.succeeds(second)
  uu.stdout_contains(second, "Jan  1  2001 some-file1\n")
  let third = invoke(s, ["--time-style=long-iso"])?
  uu.succeeds(third)
  uu.stdout_contains(third, "2001-01-01 00:00 some-file1\n")
  let fourth = invoke(s, [], {TIME_STYLE: "long-iso"})?
  uu.succeeds(fourth)
  uu.stdout_contains(fourth, "2001-01-01 00:00 some-file1\n")
}

# origin: uutils test_vdir::test_help_shows_vdir_not_ls
test test_uu_vdir_help_shows_vdir_not_ls { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["--help"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "vdir [OPTION]")
  assert ! ("ls [OPTION]" in r.stdout as Str)
}

# origin: uutils test_vdir::test_invalid_option_exit_code
test test_uu_vdir_invalid_option_exit_code { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(invoke(s, ["-/"])?, 2)
}

# origin: uutils test_vdir::test_literal_quoting_on_terminal
test test_uu_vdir_literal_quoting_on_terminal { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a\nb")
  let r = invoke_terminal(s, {QUOTING_STYLE: "literal"})?
  uu.succeeds(r)
  uu.stdout_contains(r, " a?b\r\n")
}

# origin: uutils test_vdir::test_quoting_defaults
test test_uu_vdir_quoting_defaults { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a b")
  let first = invoke(s, [])?
  uu.succeeds(first)
  uu.stdout_contains(first, " a\\ b\n")
  let zero = invoke(s, ["--zero"])?
  uu.succeeds(zero)
  uu.stdout_only_bytes(zero, b"a b\0")
  let literal = invoke(s, [], {QUOTING_STYLE: "literal"})?
  uu.succeeds(literal)
  uu.stdout_contains(literal, " a b\n")
  let escape = invoke(s, ["-b"], {QUOTING_STYLE: "literal"})?
  uu.succeeds(escape)
  uu.stdout_contains(escape, " a\\ b\n")
  let dired = invoke(s, ["--dired"])?
  uu.succeeds(dired)
  uu.stdout_contains(dired, " a\\ b\n")
}

# origin: uutils test_vdir::test_time_selection_preserves_name_sort
test test_uu_vdir_time_selection_preserves_name_sort { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")
  uu.touch(s, "b")
  fs.set_times(uu.at(s, "a"), atime_sec: 978307200, atime_nsec: 0)
  fs.set_times(uu.at(s, "b"), atime_sec: 1009843200, atime_nsec: 0)
  for time_flag in ["-u", "--time=atime"] {
    for zero in [false, true] {
      let args = [time_flag, "--time-style=+%Y", "a", "b"].extend(if zero { ["--zero"] } else { [] })
      let r = invoke(s, args)?
      uu.succeeds(r)
      uu.no_stderr(r)
      if zero { uu.stdout_is_bytes(r, b"b\0a\0") } else { assert rx"(?s)2001 a\n.*2002 b\n".matches(r.stdout as Str) }
    }
  }
}

# origin: uutils test_vdir::test_vdir
test test_uu_vdir_vdir { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(invoke(s, [])?)
}
