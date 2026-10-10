type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/tty.xsh by its real path (so the invoked name is tty and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "tty")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/tty.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_tty_reports_not_a_tty_for_a_non_terminal_stdin { |ctx|
  let result = applet_run(ctx, [])?
  assert result.status == 1
  assert result.stdout == "not a tty\n"

  for flag in ["-s", "--silent", "--quiet"] {
    let silent = applet_run(ctx, [flag])?
    assert silent.status == 1, flag
    assert silent.stdout == "" and silent.stderr == "", flag
  }
}

test test_tty_usage_errors_exit_with_status_two { |ctx|
  let extra = applet_run(ctx, ["a"])?
  assert extra.status == 2
  assert extra.stderr == "tty: extra operand 'a'\nTry 'tty --help' for more information.\n", extra.stderr

  let bad = applet_run(ctx, ["-x"])?
  assert bad.status == 2
  assert bad.stderr == "tty: invalid option -- 'x'\nTry 'tty --help' for more information.\n", bad.stderr
}

test test_tty_stdout_write_failure_exits_three { |ctx|
  if ! p"/dev/full".exists()? { test.skip("requires /dev/full"); return }

  let root = test.temp_dir(ctx, name: "tty-full")?
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/tty.xsh".display()]
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", p"/dev/full", err))?

  assert status.exit_code()? == 3
  assert err.read_text()? == "tty: write error: No space left on device\n", err.read_text()?
}

test test_tty_help_and_version { |ctx|
  let help = applet_run(ctx, ["--help"])?
  assert help.status == 0
  assert "Print the file name of the terminal connected to standard input." in help.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("tty ")
}
