##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_dir.rs.

use support.uu as uu

# A FIFO preserves the upstream diagnostic's pipe descriptor while a bounded
# reader owns the bytes; regular-file capture would change this boundary.
proc pipe_diagnostic(s: uu.Scene, args: List[Str]) -> Result[uu.Ran, Error] {
  uu.mkfifo(s, ".diagnostic-pipe")?
  let pipe = uu.at(s, ".diagnostic-pipe")
  let output = uu.at(s, ".diagnostic-output")
  let reader_error = uu.at(s, ".diagnostic-reader-error")
  let cat = process.which("cat")?
  let plan = process.command_argv(cat, [cat, pipe], s.root, {}, b"", output, reader_error, timeout: 5s)
  let reader = spawn plan?
  defer reader.cancel(kill_after: 100ms)
  let r = uu.invoke(s, "dir", args, stderr: pipe, timeout: 5s)?
  assert (wait reader?).exited_with(0)
  assert reader_error.read_bytes()? == b""
  Ok({...r, stderr: output.read_bytes()?})
}

# origin: uutils test_dir::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_dir_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = pipe_diagnostic(s, ["--block-size=1fb"])?
  uu.fails_with_code(r, 2)
  uu.stderr_is(r, "dir: invalid suffix in --block-size argument '1fb'\n")
}

# origin: uutils test_dir::test_default_format_overrides
test test_uu_dir_default_format_overrides { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  for item in [{flag: "-1", expected: b"file\n"}, {flag: "--zero", expected: b"file\0"}] {
    let r = uu.invoke(s, "dir", [item.flag])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, item.expected)
  }
  for flag in ["--full-time", "--dired"] {
    let r = uu.invoke(s, "dir", [flag])?
    uu.succeeds(r)
    uu.stdout_contains(r, "total 0")
  }
}

# origin: uutils test_dir::test_default_output
test test_uu_dir_default_output { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some-dir1")?
  uu.touch(s, "some-file1")?
  let r = uu.invoke(s, "dir", [])?
  uu.succeeds(r)
  uu.stdout_contains(r, "some-file1")
  let again = uu.invoke(s, "dir", [])?
  uu.succeeds(again)
  assert ! regex.compile("[rwx-]{10}.*some-file1$")?.matches(again.stdout.utf8()?)
}

# origin: uutils test_dir::test_dir
test test_uu_dir_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "dir", [])?)
}

# origin: uutils test_dir::test_help_shows_dir_not_ls
test test_uu_dir_help_shows_dir_not_ls { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dir", ["--help"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "dir [OPTION]")
  assert ! ("ls [OPTION]" in r.stdout.utf8()?)
}

# origin: uutils test_dir::test_invalid_option_exit_code
test test_uu_dir_invalid_option_exit_code { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dir", ["-/"])?
  uu.fails_with_code(r, 2)
}

# origin: uutils test_dir::test_literal_quoting_on_terminal
test test_uu_dir_literal_quoting_on_terminal { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a\nb")?
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  let r = {
    defer unix.close_fd(pty.replica)
    uu.invoke(s, "dir", [], vars: {QUOTING_STYLE: "literal"}, stdout: Path(pty.name), timeout: 5s)?
  }
  uu.succeeds(r)
  uu.no_stderr(r)
  assert unix.read_fd(pty.master, 4096)? == b"a?b\r\n"
}

# origin: uutils test_dir::test_long_output
test test_uu_dir_long_output { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "some-dir1")?
  uu.touch(s, "some-file1")?
  let r = uu.invoke(s, "dir", ["-l"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "some-file1")
  let again = uu.invoke(s, "dir", ["-l"])?
  uu.succeeds(again)
  assert regex.compile("[rwx-]{10}.*some-file1\n$")?.matches(again.stdout.utf8()?)
}

# origin: uutils test_dir::test_quoting_defaults
test test_uu_dir_quoting_defaults { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a b")?
  let r = uu.invoke(s, "dir", [])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\\ b\n")
  let zero = uu.invoke(s, "dir", ["--zero"])?
  uu.succeeds(zero)
  uu.stdout_only_bytes(zero, b"a b\0")
  let literal = uu.invoke(s, "dir", [], vars: {QUOTING_STYLE: "literal"})?
  uu.succeeds(literal)
  uu.stdout_only(literal, "a b\n")
  let escaped = uu.invoke(s, "dir", ["-b"], vars: {QUOTING_STYLE: "literal"})?
  uu.succeeds(escaped)
  uu.stdout_only(escaped, "a\\ b\n")
  let dired = uu.invoke(s, "dir", ["--dired"])?
  uu.succeeds(dired)
  uu.stdout_contains(dired, " a\\ b\n")
}

# origin: uutils test_dir::test_write_error
test test_uu_dir_write_error { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let r = uu.invoke(s, "dir", ["file"], stdout: p"/dev/full")?
  uu.fails(r)
  uu.stderr_is(r, "dir: write error: No space left on device\n")
}
