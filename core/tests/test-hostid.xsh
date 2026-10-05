type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/hostid.xsh by its real path (so the invoked name is hostid and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "hostid")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/hostid.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_hostid_prints_eight_lowercase_hex_digits { |ctx|
  let result = applet_run(ctx, [])?
  assert result.status == 0
  assert rx"^[0-9a-f]{8}\n$".matches(result.stdout), result.stdout
}

test test_hostid_help_version_and_errors { |ctx|
  assert "Print the numeric identifier (in hexadecimal) for the current host." in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("hostid ")

  let bad = applet_run(ctx, ["--invalid-argument"])?
  assert bad.status == 1
  assert bad.stdout == ""
  assert bad.stderr == "hostid: unrecognized option '--invalid-argument'\nTry 'hostid --help' for more information.\n", bad.stderr
}
