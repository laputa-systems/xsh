type Ran = {status: Int, out: Bytes, text: Str, err: Str}

# Runs core/ls.xsh through a symlink named `applet` (the installed alias shape,
# so dir and vdir see their own names) with `lib` linked beside it, inside
# `work`, capturing both streams.
proc ls_in(
  ctx: TestContext,
  work: Path,
  args: List[Str],
  vars: Record = {LC_ALL: "C", TZ: "UTC"},
  name = "vdir",
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

test test_vdir_lists_long_by_default { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/a b".write("")
  fp"{work}/c".write("")

  let long = ls_in(ctx, work, ["--time-style=+T", "-og"])?
  assert long.text == "total 0\n-rw-r--r-- 1 0 T a\\ b\n-rw-r--r-- 1 0 T c\n", long.text
  assert ls_in(ctx, work, ["-C"])?.text == "a\\ b  c\n"
  assert ls_in(ctx, work, ["-1"])?.text == "a\\ b\nc\n"
  assert ls_in(ctx, work, ["--zero"])?.out == b"a b\0c\0", "--zero replaces the default long format"
  assert "total 0" in ls_in(ctx, work, ["-g", "--zero"])?.text
  assert ls_in(ctx, work, ["-C"], {LC_ALL: "C", TZ: "UTC", TIME_STYLE: "invalid"})?.status == 0, "the time style is only checked for long listings"
}

test test_vdir_explicit_literal_style_preserves_newlines { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/a\nb".write("")
  let result = ls_in(ctx, work, [], {LC_ALL: "C", TZ: "UTC", QUOTING_STYLE: "literal"})?
  assert " a\nb\n" in result.text, result.text
}

test test_vdir_time_selection_keeps_name_order { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/a".write("")
  fp"{work}/b".write("")
  fs.set_times(fp"{work}/a", 1000000000000000000, 1000000000000000000)
  fs.set_times(fp"{work}/b", 1100000000000000000, 1100000000000000000)

  let by_name = ls_in(ctx, work, ["-u", "--time-style=+%Y", "-og"])?
  assert by_name.text == "total 0\n-rw-r--r-- 1 0 2001 a\n-rw-r--r-- 1 0 2004 b\n", by_name.text
  assert ls_in(ctx, work, ["-u", "--zero"])?.out == b"b\0a\0", "without a long format -u sorts by access time"
}
