proc xsh_bin() [env] -> Path {
  let bin = env.get("CARGO_BIN_EXE_xsh") ?? ""

  if bin != "" {
    return fp"${bin}"
  }

  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = env.get("XSH_CORE_DIR") ?? ""

  if dir != "" {
    return fp"${dir}/${name}"
  }

  return ../name
}

proc test_tr_translate_delete_squeeze_and_stdin(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "tr.txt", contents: b"abbc\n")?
  let translated = run.text xsh_bin() core_script("tr.xsh") -- a A $input ?
  let upper = run.text xsh_bin() core_script("tr.xsh") -- a-z A-Z $input ?
  let deleted = run.text xsh_bin() core_script("tr.xsh") -- -d b $input ?
  let digits = test.temp_file(ctx, name: "digits.txt", contents: b"a1-b2\n")?
  let only_digits = run.text xsh_bin() core_script("tr.xsh") -- -cd "[:digit:]" $digits ?
  let squeezed = run.text xsh_bin() core_script("tr.xsh") -- -s b B $input ?
  test.eq(translated.trim(), "Abbc")?
  test.eq(upper.trim(), "ABBC")?
  test.eq(deleted.trim(), "ac")?
  test.eq(only_digits.trim(), "12")?
  test.eq(squeezed.trim(), "aBc")?
  let script = core_script("tr.xsh")

  let command = f"""printf 'abc
' | ${xsh_bin().display()} ${script.display()} -- a A"""

  let stdin_output = run.text sh -c $command ?
  test.eq(stdin_output.trim(), "Abc")?
}

proc test_tr_rejects_bad_usage(ctx: TestContext) [fs, process, env, error] {
  let err = test.temp_path(ctx, name: "tr.err")
  let status = run.status xsh_bin() core_script("tr.xsh") -- a 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "usage:")?
}
