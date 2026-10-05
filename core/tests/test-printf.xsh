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
