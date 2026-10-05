type Ran = {status: Int, out: Bytes, text: Str, err: Str}

# Runs core/ls.xsh through a symlink named `applet` (the installed alias shape,
# so dir and vdir see their own names) with `lib` linked beside it, inside
# `work`, capturing both streams.
proc ls_in(
  ctx: TestContext,
  work: Path,
  args: List[Str],
  vars: Record = {LC_ALL: "C", TZ: "UTC"},
  name = "dir",
) [fs, process, error] -> Result[Ran] {
  let bin = fp"{work}/../bin"
  bin.mkdir()

  let script = fp"{bin}/{name}"

  if ! script.exists() {
    script.symlink(to: fp"{ctx.core_dir}/ls.xsh")
  }

  if ! fp"{bin}/lib".exists() {
    fp"{bin}/lib".symlink(to: fp"{ctx.core_dir}/lib")
  }

  let out = fp"{work}/../stdout"
  let err = fp"{work}/../stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, work, vars, b"", out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, out: raw, text: raw.utf8() ?? "", err: err.read_text()?})
}

proc sandbox(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "ls")?
  let work = fp"{root}/work"
  work.mkdir()
  Ok(work)
}

test test_dir_lists_in_columns_with_escaped_names { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/a b".write("")
  fp"{work}/c".write("")

  assert ls_in(ctx, work, [])?.text == "a\\ b  c\n"
  assert ls_in(ctx, work, ["-1"])?.text == "a\\ b\nc\n"
  assert ls_in(ctx, work, ["--zero"])?.out == b"a b\0c\0"
  assert ls_in(ctx, work, [], {LC_ALL: "C", TZ: "UTC", QUOTING_STYLE: "literal"})?.text == "a b  c\n"
  assert ls_in(ctx, work, ["-l", "--time-style=+T", "-og"])?.text == "total 0\n-rw-r--r-- 1 0 T a\\ b\n-rw-r--r-- 1 0 T c\n"
}

test test_dir_help_and_errors_name_dir { |ctx|
  let work = sandbox(ctx)?
  let help = ls_in(ctx, work, ["--help"])?
  assert "Usage: dir [OPTION]... [FILE]..." in help.text
  assert ! ("ls [OPTION]" in help.text)

  let bad = ls_in(ctx, work, ["-/"])?
  assert bad.status == 2
  assert bad.err == "dir: invalid option -- '/'\nTry 'dir --help' for more information.\n", bad.err
}
