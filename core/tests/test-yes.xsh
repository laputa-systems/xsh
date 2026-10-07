type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/yes.xsh by its real path (so the invoked name is yes and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "yes")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/yes.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_yes_streams_past_the_old_output_cap { |ctx|
  let yes = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/yes.xsh".display()]
  let head = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/head.xsh".display(), "-c", "33554433"]
  let wc = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/wc.xsh".display(), "-c"]
  let count = run.text --accept=[0, 141] @yes | run @head | run @wc

  assert count == "33554433\n", count

  let odd = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/yes.xsh".display(), "abcdef"]
  let two = run.text --accept=[0, 141] @odd | run @([ctx.xsh_bin.display(), fp"{ctx.core_dir}/head.xsh".display(), "-n", "2"])
  assert two == "abcdef\nabcdef\n"
}

test test_yes_help_version_and_invalid_options { |ctx|
  let help = applet_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.starts_with("Usage: yes [STRING]...\n")
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("yes ")

  let bad = applet_run(ctx, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "yes: unrecognized option '--definitely-invalid'\nTry 'yes --help' for more information.\n", bad.stderr
}
