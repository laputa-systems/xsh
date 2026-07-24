proc test_printf_strings_repeat_without_implicit_newline(ctx: TestContext) [process, env, error] {
  let one = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/printf.xsh" -- "%s" hello ?
  let lines = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/printf.xsh" -- "%s\n" a b ?
  let pairs = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/printf.xsh" -- "%s %s\n" hello xsh again ?
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
  let escaped = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/printf.xsh" -- "a\\tb\\n%%" ?

  test.eq(
    escaped,
    """a	b
%""",
  )?

  let err = test.temp_path(ctx, name: "printf.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/printf.xsh" 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "usage:")?
}
