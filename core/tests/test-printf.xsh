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

proc test_printf_strings_repeat_without_implicit_newline() [process, env, error] {
  let one = run.text xsh_bin() core_script("printf.xsh") -- "%s" hello ?
  let lines = run.text xsh_bin() core_script("printf.xsh") -- "%s\n" a b ?
  let pairs = run.text xsh_bin() core_script("printf.xsh") -- "%s %s\n" hello xsh again ?
  test.eq(one, "hello")?

  test.eq(
    lines,
    """a
b
""",
  )?

  test.eq(
    pairs,
    """hello xsh
again 
""",
  )?
}

proc test_printf_escapes_and_usage(ctx: TestContext) [fs, process, env, error] {
  let escaped = run.text xsh_bin() core_script("printf.xsh") -- "a\\tb\\n%%" ?

  test.eq(
    escaped,
    """a	b
%""",
  )?

  let err = test.temp_path(ctx, name: "printf.err")
  let status = run.status xsh_bin() core_script("printf.xsh") 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "usage:")?
}
