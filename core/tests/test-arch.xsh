type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/arch.xsh by its real path (so the invoked name is arch and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C"}, stdin: Bytes = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "arch")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/arch.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_arch_prints_the_machine_name { |ctx|
  let machine = run.text uname -m ?
  let result = applet_run(ctx, [])?
  assert result.status == 0
  assert result.stdout == machine
}

test test_arch_help_version_and_errors { |ctx|
  assert "Print machine architecture." in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("arch ")

  let bad = applet_run(ctx, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "arch: unrecognized option '--definitely-invalid'\nTry 'arch --help' for more information.\n", bad.stderr

  let extra = applet_run(ctx, ["x"])?
  assert extra.status == 1
  assert extra.stderr == "arch: extra operand 'x'\nTry 'arch --help' for more information.\n", extra.stderr
}
