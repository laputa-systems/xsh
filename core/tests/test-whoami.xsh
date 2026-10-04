type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/whoami.xsh by its real path (so the invoked name is whoami and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C"}, stdin: Bytes = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "whoami")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/whoami.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_whoami_prints_the_effective_user_name { |ctx|
  let expected = user.by_uid(unix.id()?.euid)?.name
  let result = applet_run(ctx, [])?
  assert result.status == 0
  assert result.stdout == f"{expected}\n"
  assert result.stderr == ""
}

test test_whoami_help_version_and_errors { |ctx|
  assert "associated with the current effective user ID" in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("whoami ")

  let extra = applet_run(ctx, ["x"])?
  assert extra.status == 1
  assert extra.stderr == "whoami: extra operand 'x'\nTry 'whoami --help' for more information.\n", extra.stderr
}
