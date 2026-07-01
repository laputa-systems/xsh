pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_printf_strings_repeat_without_implicit_newline() [process, error] {
  let one = run.text xsh_bin() printf.xsh -- "%s" hello ?
  let lines = run.text xsh_bin() printf.xsh -- "%s\n" a b ?
  let pairs = run.text xsh_bin() printf.xsh -- "%s %s\n" hello xsh again ?
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

proc test_printf_escapes_and_usage(ctx: TestContext) [fs, process, error] {
  let escaped = run.text xsh_bin() printf.xsh -- "a\\tb\\n%%" ?

  test.eq(
    escaped,
    """a	b
%""",
  )?

  let err = test.temp_path(ctx, name: "printf.err")
  let status = run.status xsh_bin() printf.xsh 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "usage:")?
}
