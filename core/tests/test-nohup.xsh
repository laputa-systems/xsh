type Ran = {status: Int, stdout: Str, stderr: Str}

proc nohup_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "nohup")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/nohup.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(
    process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err),
  )?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

proc nohup_run_posix(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Int] {
  let root = test.temp_dir(ctx, name: "nohup-posix")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/nohup.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(
    process.command_argv(
      ctx.xsh_bin,
      argv,
      root,
      {LC_ALL: "C", POSIXLY_CORRECT: "1", XSH_EXECUTION_PHRASE: ""},
      b"",
      out,
      err,
    ),
  )?
  status.exit_code()?
}

test test_nohup_runs_command_and_returns_its_status { |ctx|
  assert nohup_run(ctx, ["true"])?.status == 0
  assert nohup_run(ctx, ["false"])?.status == 1
  assert nohup_run(ctx, ["sh", "-c", "exit 7"])?.status == 7
}

test test_nohup_command_arguments_are_passed_through { |ctx|
  let result = nohup_run(ctx, ["echo", "-n", "one", "two"])?
  assert result.status == 0
  assert result.stdout == "one two"
  assert result.stderr == ""
}

test test_nohup_missing_command_is_127 { |ctx|
  let result = nohup_run(ctx, ["xsh-no-such-command-compat"])?
  assert result.status == 127
  assert "failed to run command 'xsh-no-such-command-compat'" in result.stderr
}

test test_nohup_posix_error_statuses { |ctx|
  assert nohup_run_posix(ctx, [])? == 127
  assert nohup_run_posix(ctx, ["--invalid"])? == 127
}

test test_nohup_help_and_version { |ctx|
  let help = nohup_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.starts_with("Usage: nohup COMMAND [ARG]...")

  let version = nohup_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout == "nohup (XSH core) 0.0.1\n", version.stdout
}

test test_nohup_terminal_stdin_becomes_unreadable { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)

  let root = test.temp_dir(ctx, name: "nohup-tty")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/nohup.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "cat"]
  let status = process.run(
    process.command_argv(
      ctx.xsh_bin,
      argv,
      root,
      {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""},
      fp"{pty.name}",
      out,
      err,
    ),
  )?

  assert status.exit_code()? == 1
  assert "Bad file descriptor" in err.read_text()?
}
