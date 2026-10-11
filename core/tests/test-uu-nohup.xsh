##! Transcribed from the uutils nohup integration tests.

use support.uu as uu

# Each standard stream gets its own terminal, matching isolated stream capture.
proc terminal() [process, error] -> Result[UnixPty, Error] {
  let pair = unix.open_pty()?
  unix.set_window_size(30, 80, xpixel: 640, ypixel: 300, fd: pair.replica)?
  Ok(pair)
}

# A live replica keeps terminal EOF from becoming EIO; poll drains only queued bytes.
proc terminal_output(fd: Int) [process, error] -> Result[Bytes, Error] {
  var output = b""
  while "readable" in unix.poll_fd(fd, ["readable"], timeout_ms: 0)? {
    let chunk = unix.read_fd(fd, 8192)?
    break when chunk.is_empty()
    output = bytes.concat([output, chunk])
  }
  Ok(output)
}

proc invoke_terminal(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let input = terminal()?
  defer unix.close_fd(input.master)?
  defer unix.close_fd(input.replica)?
  let output = terminal()?
  defer unix.close_fd(output.master)?
  defer unix.close_fd(output.replica)?
  let errors = terminal()?
  defer unix.close_fd(errors.master)?
  defer unix.close_fd(errors.replica)?
  let argv = uu.argv(s, "nohup", [Path(word) for word in args], vars)?
  let plan = process.command_argv(s.ctx.xsh_bin, argv, s.root, vars,
    Path(input.name), Path(output.name), Path(errors.name), timeout: 5s)
  let status = process.run(plan)?
  Ok({util: "nohup", args: args, status: status.exit_code()?,
    stdout: terminal_output(output.master)?, stderr: terminal_output(errors.master)?})
}

# origin: uutils test_nohup::test_nohup_exit_codes
test test_uu_nohup_nohup_exit_codes { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "nohup", [])?, 125)
  uu.fails_with_code(uu.invoke(s, "nohup", [], vars: {POSIXLY_CORRECT: "1"})?, 127)
  uu.fails_with_code(uu.invoke(s, "nohup", ["--invalid"])?, 125)
  uu.fails_with_code(uu.invoke(s, "nohup", ["--invalid"], vars: {POSIXLY_CORRECT: "1"})?, 127)
}

# origin: uutils test_nohup::test_nohup_multiple_args_and_flags
test test_uu_nohup_nohup_multiple_args_and_flags { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nohup", ["touch", "-t", "1006161200", "file1", "file2"])?
  uu.succeeds(r)
  time.sleep(10ms)?
  assert uu.file_exists(s, "file1")?
  assert uu.file_exists(s, "file2")?
}

# origin: uutils test_nohup::test_nohup_with_pseudo_terminal_emulation_on_stdin_stdout_stderr_get_replaced
test test_uu_nohup_nohup_with_pseudo_terminal_emulation_on_stdin_stdout_stderr_get_replaced { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "nohup", "is_a_tty.sh", "is_a_tty.sh")?
  let r = invoke_terminal(s, ["sh", "is_a_tty.sh"])?
  uu.succeeds(r)
  assert r.stderr.utf8()?.trim() == "nohup: ignoring input and appending output to 'nohup.out'"
  time.sleep(10ms)?
  uu.file_is(s, "nohup.out", "stdin is not a tty\nstdout is not a tty\nstderr is not a tty\n")?
}

# origin: uutils test_nohup::test_nohup_replaced_stdin_is_not_readable
test test_uu_nohup_nohup_replaced_stdin_is_not_readable { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke_terminal(s, ["cat"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "nohup: ignoring input and appending output to 'nohup.out'")
  time.sleep(10ms)?
  assert "Bad file descriptor" in uu.read_text(s, "nohup.out")?
}

# origin: uutils test_nohup::test_nohup_creates_output_in_cwd
test test_uu_nohup_nohup_creates_output_in_cwd { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke_terminal(s, ["echo", "test output"])?
  uu.succeeds(r)
  uu.stderr_contains(r, "nohup: ignoring input and appending output to 'nohup.out'")
  time.sleep(10ms)?
  assert uu.file_exists(s, "nohup.out")?
  assert "test output" in uu.read_text(s, "nohup.out")?
}

# origin: uutils test_nohup::test_nohup_appends_to_existing_file
test test_uu_nohup_nohup_appends_to_existing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "nohup.out", "existing content\n")?
  let r = invoke_terminal(s, ["echo", "new output"])?
  uu.succeeds(r)
  time.sleep(10ms)?
  let content = uu.read_text(s, "nohup.out")?
  assert "existing content" in content
  assert "new output" in content
}

# origin: uutils test_nohup::test_nohup_fallback_to_home
test test_uu_nohup_nohup_fallback_to_home { |ctx|
  if applet.current_euid() == 0 { test.skip("root bypasses the read-only directory") }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "home")?
  uu.mkdir(s, "readonly_dir")?
  uu.set_mode(s, "readonly_dir", 0o555)?
  defer uu.set_mode(s, "readonly_dir", 0o755)?
  let readonly: uu.Scene = {ctx: ctx, root: uu.at(s, "readonly_dir")}
  let home_dir = uu.at(s, "home").display()
  let r = invoke_terminal(readonly, ["echo", "fallback test"], {HOME: home_dir})?
  uu.set_mode(s, "readonly_dir", 0o755)?
  let home_nohup = f"{home_dir}/nohup.out"
  time.sleep(50ms)?
  assert home_nohup in r.stderr.utf8()? or Path(home_nohup).exists()?
}

# origin: uutils test_nohup::test_nohup_command_not_found
test test_uu_nohup_nohup_command_not_found { |ctx|
  let s = uu.scene(ctx)?
  let command = "this-command-definitely-does-not-exist-anywhere"
  let r = uu.invoke(s, "nohup", [command])?
  uu.fails(r)
  uu.stderr_contains(r, f"failed to run command '{command}'")
  assert r.status == 126 or r.status == 127
}

# origin: uutils test_nohup::test_nohup_stderr_to_stdout
test test_uu_nohup_nohup_stderr_to_stdout { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "both_streams.sh", "#!/bin/bash\necho 'stdout message'\necho 'stderr message' >&2")?
  uu.set_mode(s, "both_streams.sh", 0o755)?
  let r = invoke_terminal(s, ["sh", "both_streams.sh"])?
  uu.succeeds(r)
  time.sleep(10ms)?
  let content = uu.read_text(s, "nohup.out")?
  assert "stdout message" in content
  assert "stderr message" in content
}

# origin: uutils test_nohup::test_nohup_propagates_command_exit_code
test test_uu_nohup_nohup_propagates_command_exit_code { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "nohup", ["true"])?)
  uu.fails_with_code(uu.invoke(s, "nohup", ["sh", "-c", "exit 3"])?, 3)
}

# origin: uutils test_nohup::test_nohup_new_output_file_is_owner_only
test test_uu_nohup_nohup_new_output_file_is_owner_only { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke_terminal(s, ["echo", "secret output"])?
  uu.succeeds(r)
  time.sleep(10ms)?
  assert uu.mode(s, "nohup.out")?.bit_and(0o777) == 0o600
}

# origin: uutils test_nohup::test_nohup_existing_output_file_keeps_permissions
test test_uu_nohup_nohup_existing_output_file_keeps_permissions { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "nohup.out", "old content\n")?
  uu.set_mode(s, "nohup.out", 0o644)?
  let r = invoke_terminal(s, ["echo", "new content"])?
  uu.succeeds(r)
  time.sleep(10ms)?
  assert uu.mode(s, "nohup.out")?.bit_and(0o777) == 0o644
  let content = uu.read_text(s, "nohup.out")?
  assert "old content" in content
  assert "new content" in content
}
