test test_printf_strings_repeat_without_implicit_newline { |ctx|
  let one = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/printf.xsh" -- "%s" hello
  let lines = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/printf.xsh" -- "%s\n" a b
  let pairs = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/printf.xsh" -- "%s %s\n" hello xsh again
  assert one == "hello"

  assert lines == """a
b
"""

  assert pairs == """hello xsh
again 
"""
}

test test_printf_escapes_and_usage { |ctx|
  let escaped = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/printf.xsh" -- "a\\tb\\n%%"

  assert escaped == """a	b
%"""

  let err = test.temp_path(ctx, name: "printf.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/printf.xsh" 2> $err
  assert ! status.exited_with(0)
  assert "usage:" in err.read_text()?
}

type PrintfResult = {status: Int, stdout: Str, stderr: Str}

proc printf_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[PrintfResult] {
  let root = test.temp_dir(ctx, name: "printf-argv")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/printf.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display()].extend(args), root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  {status: status.exit_code()?, stdout: stdout.read_text()?, stderr: stderr.read_text()?}
}

test test_printf_only_leading_double_dash_ends_options { |ctx|
  for args in [["--", "%s\\n", "a"], ["%s\\n", "--"], ["--", "%s\\n", "--"]] {
    let output = printf_run(ctx, args)?
    assert output.status == 0, output.stderr
    let expected = if args[-1] == "a" { "a\n" } else { "--\n" }
    assert output.stdout == expected
    assert output.stderr == ""
  }
}

test test_printf_help_and_version_after_format_are_data { |ctx|
  for value in ["--help", "--version"] {
    let output = printf_run(ctx, ["%s", value])?
    assert output.status == 0, output.stderr
    assert output.stdout == value
    assert output.stderr == ""
  }
  let escaped_help = printf_run(ctx, ["--", "--help"])?
  assert escaped_help.status == 0, escaped_help.stderr
  assert escaped_help.stdout == "--help"
}

test test_printf_initial_help_version_and_empty_format { |ctx|
  let help = printf_run(ctx, ["--help"])?
  assert help.status == 0, help.stderr
  assert help.stdout.starts_with("Usage: printf ")
  let version = printf_run(ctx, ["--version"])?
  assert version.status == 0, version.stderr
  assert version.stdout.starts_with("printf (XSH core) ")
  let empty = printf_run(ctx, [""])?
  assert empty.status == 0, empty.stderr
  assert empty.stdout == ""
}
