pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_tr_translate_delete_squeeze_and_stdin(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "tr.txt", contents: b"abbc\n")?
  let translated = run.text xsh_bin() tr.xsh -- a A $input ?
  let upper = run.text xsh_bin() tr.xsh -- a-z A-Z $input ?
  let deleted = run.text xsh_bin() tr.xsh -- -d b $input ?
  let digits = test.temp_file(ctx, name: "digits.txt", contents: b"a1-b2\n")?
  let only_digits = run.text xsh_bin() tr.xsh -- -cd "[:digit:]" $digits ?
  let squeezed = run.text xsh_bin() tr.xsh -- -s b B $input ?
  test.eq(translated.trim(), "Abbc")?
  test.eq(upper.trim(), "ABBC")?
  test.eq(deleted.trim(), "ac")?
  test.eq(only_digits.trim(), "12")?
  test.eq(squeezed.trim(), "aBc")?
  let script = p"tr.xsh"

  let command = f"""printf 'abc
' | ${xsh_bin().display()} ${script.display()} -- a A"""

  let stdin_output = run.text sh -c $command ?
  test.eq(stdin_output.trim(), "Abc")?
}

proc test_tr_rejects_bad_usage(ctx: TestContext) [fs, process, error] {
  let err = test.temp_path(ctx, name: "tr.err")
  let status = run.status xsh_bin() tr.xsh -- a 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "usage:")?
}
