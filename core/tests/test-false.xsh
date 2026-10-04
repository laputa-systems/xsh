type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/false.xsh by its real path (so the invoked name is false and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C"}, stdin: Bytes = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "false")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/false.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_false_fails_and_ignores_arguments { |ctx|
  for args in [[], ["a", "b"], ["-h"], ["-V"], ["--help", "x"], ["--version", "x"], ["--help", "--version"], ["--he"]] {
    let result = applet_run(ctx, args)?
    assert result.status == 1, f"false {args.join(" ")}"
    assert result.stdout == "" and result.stderr == "", f"false {args.join(" ")}"
  }
}

test test_false_help_and_version_as_the_sole_argument_still_fail { |ctx|
  let help = applet_run(ctx, ["--help"])?
  assert help.status == 1
  assert help.stdout.starts_with("Usage: false [ignored command line arguments]\n")

  let version = applet_run(ctx, ["--version"])?
  assert version.status == 1
  assert version.stdout == "false (XSH core) 0.0.1\n"
}
