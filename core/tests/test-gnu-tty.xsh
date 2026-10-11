use support.uu

# Descriptor closure belongs to the shell; the target argv stays under the
# shared launcher so the reference receives the same closed standard input.
proc closed_input(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let launch = uu.argv(s, "tty", [Path(arg) for arg in args])?
  let words = [p"/bin/sh", p"-c", p"exec \"$@\" 0<&-", p"tty-closed-input"].extend(launch)
  let out = uu.at(s, ".closed-out")
  let err = uu.at(s, ".closed-err")
  let status = process.run(process.command_argv(p"/bin/sh", words, s.root, {}, b"", out, err, timeout: 10s))?
  Ok({util: "tty", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: gnu tty/tty.log
test test_gnu_tty_tty_log { |ctx|
  let s = uu.scene(ctx)?
  let terminal = unix.open_pty()?
  defer unix.close_fd(terminal.master)?
  defer unix.close_fd(terminal.replica)?
  let input = Path(terminal.name)
  let named = uu.invoke_from_path(s, "tty", [], input)?
  uu.succeeds(named)
  uu.stdout_only(named, f"{terminal.name}\n")
  let quiet_terminal = uu.invoke_from_path(s, "tty", ["-s"], input)?
  uu.succeeds(quiet_terminal)
  uu.no_output(quiet_terminal)
  for args in [[], ["-s"]] {
    let null_input = uu.invoke_from_path(s, "tty", args, p"/dev/null")?
    uu.fails_with_code(null_input, 1)
    if args.is_empty() { uu.stdout_only(null_input, "not a tty\n") } else { uu.no_output(null_input) }
    let closed = closed_input(s, args)?
    uu.fails_with_code(closed, 1)
    if args.is_empty() { uu.stdout_is(closed, "not a tty\n") } else { uu.no_output(closed) }
  }
  for args in [["a"], ["-s", "a"]] {
    uu.fails_with_code(uu.invoke(s, "tty", args)?, 2)
  }
  for source in [input, p"/dev/null"] {
    let full = uu.invoke_from_path(s, "tty", [], source, stdout: p"/dev/full")?
    uu.fails_with_code(full, 3)
  }
}
