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

test test_yes_repeats_a_bounded_number_of_lines { |ctx|
  let plain = applet_run(ctx, [])?
  assert plain.status == 0
  assert plain.stdout.starts_with("y\ny\ny\n")
  assert plain.bytes.len() == 33554432, "output is bounded to 32 MiB of whole lines"

  let words = applet_run(ctx, ["a", "bar", "c"])?
  assert words.stdout.starts_with("a bar c\na bar c\n")
  assert words.bytes.len() % 8 == 0
}

test test_yes_odd_length_lines_repeat_whole { |ctx|
  let result = applet_run(ctx, ["abcdef"])?
  assert result.stdout.starts_with("abcdef\nabcdef\n")
  assert result.bytes.len() % 7 == 0
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
