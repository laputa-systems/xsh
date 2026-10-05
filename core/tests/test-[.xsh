type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/test.xsh through a symlink named [ (the installed alias shape), with
# `lib` linked beside it so modules resolve, capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "[")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{root}/["
  fs.symlink(fp"{ctx.core_dir}/test.xsh", script)?
  fs.symlink(fp"{ctx.core_dir}/lib", fp"{root}/lib")?
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_bracket_requires_the_closing_bracket { |ctx|
  assert applet_run(ctx, ["1", "-eq", "1", "]"])?.status == 0
  assert applet_run(ctx, ["1", "-eq", "2", "]"])?.status == 1
  assert applet_run(ctx, ["]"])?.status == 1, "an empty expression is false"
  assert applet_run(ctx, ["x", "]"])?.status == 0
  assert applet_run(ctx, ["!", "]"])?.status == 0, "a lone ! is the string !"
}

test test_bracket_missing_closing_bracket_beats_other_errors { |ctx|
  for args in [[], ["1", "-eq"], ["a", "b"]] {
    let result = applet_run(ctx, args)?
    assert result.status == 2, args.join(" ")
    assert result.stderr == "[: missing ']'\n", args.join(" ")
  }
}

test test_bracket_errors_are_prefixed_with_its_name { |ctx|
  let result = applet_run(ctx, ["7", "-eq", "zap", "]"])?
  assert result.status == 2
  assert result.stderr == "[: invalid integer 'zap'\n", result.stderr
}

test test_bracket_help_and_version_only_as_the_sole_argument { |ctx|
  let help = applet_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.starts_with("Usage: test EXPRESSION\n")
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("[ (XSH core) ")
  assert applet_run(ctx, ["--help", "]"])?.status == 0, "--help before ] is the string --help"
  assert applet_run(ctx, ["--help", "]"])?.stdout == ""
}
