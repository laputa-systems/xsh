##! Native ports of the uutils tty integration tests.
use support.uu as uu

type ClosedOutput = {status: Status, stderr: Bytes}

# Empty byte input owns a pipe writer that closes after spawn, matching a
# child whose parent's stdin handle is dropped without writing any bytes.
proc closed_input(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let plan = uu.command(s, "tty", args, stdin: b"", timeout: 5s)?
  let child = spawn plan?
  let status = wait child?
  Ok({util: "tty", args: args, status: status.exit_code()?, stdout: uu.read(s, ".uu-stdout")?, stderr: uu.read(s, ".uu-stderr")?})
}

# Close the only pipe reader after spawn, before the delayed exec writes.
# The FIFO supplies the same EPIPE boundary while its child status stays typed.
proc closed_output(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[ClosedOutput, Error] {
  uu.mkfifo(s, "stdout-pipe")?
  let reader = unix.open_fd(uu.at(s, "stdout-pipe"), nonblock: true)?
  let target_argv = uu.argv(s, "tty", [Path(word) for word in args])?
  let command = [p"sh", p"-c", Path(r"""sleep 0.2; exec "$@"
"""), p"tty-pipe"].extend(target_argv)
  let plan = process.command_argv(p"sh", command, s.root, {}, b"", uu.at(s, "stdout-pipe"), uu.at(s, "stderr"), timeout: 5s)
  let child = spawn plan?
  unix.close_fd(reader)?
  let status = wait child?
  Ok({status: status, stderr: uu.read(s, "stderr")?})
}

# origin: uutils test_tty::test_close_stdin
test test_uu_tty_close_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r = closed_input(s, [])?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "not a tty\n")
}

# origin: uutils test_tty::test_close_stdin_silent
test test_uu_tty_close_stdin_silent { |ctx|
  let s = uu.scene(ctx)?
  let r = closed_input(s, ["-s"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
}

# origin: uutils test_tty::test_close_stdin_silent_alias
test test_uu_tty_close_stdin_silent_alias { |ctx|
  let s = uu.scene(ctx)?
  let r = closed_input(s, ["--quiet"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
}

# origin: uutils test_tty::test_close_stdin_silent_long
test test_uu_tty_close_stdin_silent_long { |ctx|
  let s = uu.scene(ctx)?
  let r = closed_input(s, ["--silent"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
}

# origin: uutils test_tty::test_dev_null
test test_uu_tty_dev_null { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_from_path(s, "tty", [], p"/dev/null")?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "not a tty\n")
}

# origin: uutils test_tty::test_dev_null_silent
test test_uu_tty_dev_null_silent { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_from_path(s, "tty", ["-s"], p"/dev/null")?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: uutils test_tty::test_help
test test_uu_tty_help { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "tty", ["--help"])?)
}

# origin: uutils test_tty::test_stdout_fail
test test_uu_tty_stdout_fail { |ctx|
  let s = uu.scene(ctx)?
  let r = closed_output(s, [])?
  assert r.status.signaled()
  assert r.status.signal_number()? == 13
}

# origin: uutils test_tty::test_version_pipe_no_stderr
test test_uu_tty_version_pipe_no_stderr { |ctx|
  let s = uu.scene(ctx)?
  assert closed_output(s, ["--version"])?.stderr == b""
}

# origin: uutils test_tty::test_wrong_argument
test test_uu_tty_wrong_argument { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "tty", ["a"])?, 2)
}
