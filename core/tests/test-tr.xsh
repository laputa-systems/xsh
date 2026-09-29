proc test_tr_translate_delete_squeeze_and_stdin(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "tr.txt", contents: b"abbc\n")?
  let translated = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tr.xsh" -- a A $input ?
  let upper = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tr.xsh" -- a-z A-Z $input ?
  let deleted = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tr.xsh" -- -d b $input ?
  let digits = test.temp_file(ctx, name: "digits.txt", contents: b"a1-b2\n")?
  let only_digits = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tr.xsh" -- -cd "[:digit:]" $digits ?
  let squeezed = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tr.xsh" -- -s b B $input ?
  translated.trim() == "Abbc"
  upper.trim() == "ABBC"
  deleted.trim() == "ac"
  only_digits.trim() == "12"
  squeezed.trim() == "aBc"
  let script = fp"${ctx.core_dir}/tr.xsh"

  let command = f"""printf 'abc
' | ${ctx.xsh_bin.display()} ${script.display()} -- a A"""

  let stdin_output = run.text sh -c $command ?
  stdin_output.trim() == "Abc"
}

proc test_tr_rejects_bad_usage(ctx: TestContext) [fs, process, env, error] {
  let err = test.temp_path(ctx, name: "tr.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/tr.xsh" -- a 2> $err
  ! status.exited_with(0)
  "usage:" in err.read_text()?
}
