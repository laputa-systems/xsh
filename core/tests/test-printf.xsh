proc printf_run(ctx: TestContext, args: List[Str]) [process, error] -> Result[Str] {
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/printf.xsh" @args
}

type PrintfResult = {status: Int, stdout: Bytes, stderr: Str}

proc printf_result(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[PrintfResult] {
  let root = test.temp_dir(ctx, name: "printf-result")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/printf.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_printf_strings_repeat_without_implicit_newline { |ctx|
  let one = printf_run(ctx, ["%s", "hello"])?
  let lines = printf_run(ctx, ["%s\n", "a", "b"])?
  let pairs = printf_run(ctx, ["%s %s\n", "hello", "xsh", "again"])?
  assert one == "hello"
  assert lines == """a
b
"""
  assert pairs == """hello xsh
again 
"""
}

test test_printf_leading_double_dash_ends_options { |ctx|
  assert printf_run(ctx, ["--", "%s\n", "value"])? == "value\n"
  assert printf_run(ctx, ["--", "--help"])? == "--help"
}

test test_printf_double_dash_after_format_is_data { |ctx|
  assert printf_run(ctx, ["%s\n", "--"])? == "--\n"
  assert printf_run(ctx, ["%s %s\n", "--help", "--version"])? == "--help --version\n"
  let double_dash_format = printf_result(ctx, ["--", "--", "extra"])?
  assert double_dash_format.status == 0
  assert double_dash_format.stdout == b"--"
  assert "warning: ignoring excess arguments" in double_dash_format.stderr
}

test test_printf_help_and_version_are_only_leading_options { |ctx|
  let help = printf_run(ctx, ["--help"])?
  let version = printf_run(ctx, ["--version"])?
  assert "Usage: printf FORMAT" in help
  assert version.starts_with("printf (XSH core)")
  assert printf_run(ctx, ["%s %s\n", "--help", "--version"])? == "--help --version\n"
}

test test_printf_integer_flags_width_precision_and_repeat { |ctx|
  assert printf_run(ctx, ["%+06d %#x %.4s\n", "42", "255", "hello"])? == "+00042 0xff hell\n"
  assert printf_run(ctx, ["%d,", "1", "2", "3"])? == "1,2,3,"
  assert printf_run(ctx, ["%*s", "-5", "x"])? == "x    "
}

test test_printf_float_string_and_character_conversions { |ctx|
  assert printf_run(ctx, ["%.2f %.2e %.3g\n", "1.25", "1000", "12.34"])? == "1.25 1.00e+03 12.3\n"
  assert printf_run(ctx, ["%c %c\n", "A", "66"])? == "A B\n"
  assert printf_run(ctx, ["%b\n", "a\\tb\\n"])? == "a\tb\n\n"
}

test test_printf_format_escapes_and_b_stop { |ctx|
  let escaped = printf_run(ctx, ["a\\tb\\n%%"])?
  assert escaped == """a	b
%"""
  assert printf_run(ctx, ["%bignored", "before\\cafter"])? == "before"
}

test test_printf_quote_hex_float_and_exact_decimal_precision { |ctx|
  assert printf_run(ctx, ["%q", "a b"])? == "'a b'"
  assert printf_run(ctx, ["%a %A", ".875", ".875"])? == "0xep-4 0XEP-4"
  assert printf_run(ctx, ["%.30f", "0.1"])? == "0.100000000000000000000000000000"
}

test test_printf_conversion_errors_keep_already_formatted_bytes { |ctx|
  let malformed_b = printf_result(ctx, ["prefix %7b", "value"])?
  assert malformed_b.status == 1
  assert malformed_b.stdout == b"prefix "
  assert malformed_b.stderr == "printf: %7b: invalid conversion specification\n"

  let malformed_c = printf_result(ctx, ["prefix %5.2c", "q"])?
  assert malformed_c.status == 1
  assert malformed_c.stdout == b"prefix "
  assert malformed_c.stderr == "printf: %5.2c: invalid conversion specification\n"

  let partial = printf_result(ctx, ["%d is %s", "42x23", "useful"])?
  assert partial.status == 1
  assert partial.stdout == b"42 is useful"
  assert partial.stderr == "printf: '42x23': value not completely converted\n"
}

test test_printf_invalid_formats_keep_gnu_plain_diagnostics { |ctx|
  let invalid_spec = printf_result(ctx, ["hello %s and %z", "world"])?
  assert invalid_spec.status == 1
  assert invalid_spec.stdout == b"hello world and "
  assert invalid_spec.stderr == "printf: %z: invalid conversion specification\n"

  let option_format = printf_result(ctx, ["-%z", "world"])?
  assert option_format.status == 1
  assert option_format.stdout == b"-"
  assert option_format.stderr == "printf: %z: invalid conversion specification\n"

  let collision = printf_result(ctx, ["╰%z"])?
  assert collision.status == 1
  assert collision.stdout == bytes.from_text("╰")
  assert collision.stderr == "printf: %z: invalid conversion specification\n"

  let bad_codepoint = printf_result(ctx, ["x\\ud800y"])?
  assert bad_codepoint.status == 1
  assert bad_codepoint.stdout == b"x"
  assert bad_codepoint.stderr == "printf: invalid universal character name \\ud800\n"

  let incomplete_hex = printf_result(ctx, ["a\\xzb", "q"])?
  assert incomplete_hex.status == 1
  assert incomplete_hex.stdout == b"a"
  assert incomplete_hex.stderr == "printf: missing hexadecimal number in escape\n"
}

test test_printf_partial_float_keeps_parsed_prefix { |ctx|
  let partial = printf_result(ctx, ["%.2f is %s", "42.03x", "a lot"])?
  assert partial.status == 1
  assert partial.stdout == b"42.03 is a lot"
  assert partial.stderr == "printf: '42.03x': value not completely converted\n"

  let exponent = printf_result(ctx, ["%f", "123e"])?
  assert exponent.status == 1
  assert exponent.stdout == b"123.000000"
  assert "value not completely converted" in exponent.stderr
}

test test_printf_float_accepts_character_constants_and_extreme_exponents { |ctx|
  assert printf_run(ctx, ["%f", "'á"])? == "225.000000"

  let overflow = printf_result(ctx, ["%f", "5e8123456789012345678"])?
  assert overflow.status == 1
  assert overflow.stdout == b"inf"
  assert "Numerical result out of range" in overflow.stderr

  let underflow = printf_result(ctx, ["%f", "7E-8123456789012345678"])?
  assert underflow.status == 1
  assert underflow.stdout == b"0.000000"
  assert "Numerical result out of range" in underflow.stderr
}

test test_printf_star_width_and_precision_extremes_do_not_panic { |ctx|
  let width = printf_result(ctx, ["|%*d|", "-9223372036854775808", "1"])?
  assert width.status == 1

  let precision = printf_result(ctx, ["|%.*d|", "-340282366920938463463374607431768211455", "10"])?
  assert precision.status == 1
  assert precision.stdout == b"|10|"
  assert "Numerical result out of range" in precision.stderr
}

test test_printf_missing_operand_uses_gnu_diagnostic { |ctx|
  let script = fp"{ctx.core_dir}/printf.xsh"
  let err = test.temp_path(ctx, name: "printf.err")
  let status = run.status ${ctx.xsh_bin} $script 2> $err
  assert status.exit_code()? == 1
  assert err.read_text()? == "printf: missing operand\nTry 'printf --help' for more information.\n"
}
