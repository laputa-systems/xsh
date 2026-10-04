type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/logname.xsh by its real path (so the invoked name is logname and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C"}, stdin: Bytes = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "logname")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/logname.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_logname_without_a_login_session_reports_no_login_name { |ctx|
  let result = applet_run(ctx, [])?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "logname: no login name\n"
}

test test_logname_help_version_and_errors { |ctx|
  assert "Print the user's login name." in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("logname ")

  let extra = applet_run(ctx, ["x"])?
  assert extra.status == 1
  assert extra.stderr == "logname: extra operand 'x'\nTry 'logname --help' for more information.\n", extra.stderr
}
