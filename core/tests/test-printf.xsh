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

test test_printf_warns_about_arguments_after_literal_format { |ctx|
  let literal = printf_run(ctx, ["a", "b"])?
  let repeated = printf_run(ctx, ["%s", "a", "b"])?
  let stopped = printf_run(ctx, ["A%sC\\cD%sF", "B", "E"])?

  assert literal.status == 0
  assert literal.stdout == "a"
  assert literal.stderr == "printf: warning: ignoring excess arguments, starting with 'b'\n"
  assert repeated.status == 0
  assert repeated.stdout == "ab"
  assert repeated.stderr == ""
  assert stopped.status == 0
  assert stopped.stdout == "ABC"
  assert stopped.stderr == ""
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

test test_printf_shell_quote_conversion { |ctx|
  let quoted = printf_run(ctx, ["%q|%q|%q|%q", "test~", "a b", "", "\"$test\""])?
  let quote_then_literal = printf_run(ctx, ["%qd", "a b"])?

  assert quoted.status == 0, quoted.stderr
  assert quoted.stdout == "test~|'a b'|''|'\"$test\"'"
  assert quoted.stderr == ""
  assert quote_then_literal.status == 0, quote_then_literal.stderr
  assert quote_then_literal.stdout == "'a b'd"
  assert quote_then_literal.stderr == ""
}

test test_printf_rejects_width_for_shell_quote_conversion { |ctx|
  let output = printf_run(ctx, ["prefix%7q", "world"])?

  assert output.status == 1
  assert output.stdout == "prefix"
  assert output.stderr == "printf: %7q: invalid conversion specification\n"
}

test test_printf_rejects_field_parameters_for_escape_conversion { |ctx|
  let output = printf_run(ctx, ["prefix%7b", "world"])?

  assert output.status == 1
  assert output.stdout == "prefix"
  assert output.stderr == "printf: %7b: invalid conversion specification\n"
}

test test_printf_numeric_string_and_character_conversions { |ctx|
  let output = printf_run(ctx, ["[%05d][%-5s][%.3s][%#x][%c]", "-12", "xy", "abcdef", "26", "65"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "[-0012][xy   ][abc][0x1a][A]"
  assert output.stderr == ""
}

test test_printf_rejects_invalid_string_and_character_specifiers { |ctx|
  for format in ["%0c", "%0s", "%5.2c", "%.0c", "%-5.2c"] {
    let output = printf_run(ctx, [format, "3"])?

    assert output.status == 1, output.stderr
    assert output.stdout == ""
    assert output.stderr == f"printf: {format}: invalid conversion specification\n"
  }
}

test test_printf_dynamic_width_precision_and_repeated_format { |ctx|
  let output = printf_run(ctx, ["%*.*s|", "-6", "3", "abcdef", "4", "2", "xy"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "abc   |  xy|"
  assert output.stderr == ""
}

test test_printf_positional_arguments { |ctx|
  let output = printf_run(ctx, ["%2$s:%1$04d", "7", "item"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "item:0007"
  assert output.stderr == ""
}

test test_printf_repeats_positional_format_from_next_argument_set { |ctx|
  let output = printf_run(ctx, ["%1$s%1$s\\n", "1", "2"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "11\n22\n"
  assert output.stderr == ""
}

test test_printf_indexed_argument_cursor_and_bounds { |ctx|
  let mixed = printf_run(ctx, ["%s %3$s %s\\n", "A", "B", "C", "D"])?
  let width_precision = printf_run(ctx, ["%1$*2$.*3$d\\n", "1", "3", "2"])?
  let large_position = printf_run(ctx, ["empty%18446744073709551616$s\\n", "foo"])?

  assert mixed.status == 0, mixed.stderr
  assert mixed.stdout == "A C B\nD  \n"
  assert mixed.stderr == ""
  assert width_precision.status == 0, width_precision.stderr
  assert width_precision.stdout == " 01\n"
  assert width_precision.stderr == ""
  assert large_position.status == 0, large_position.stderr
  assert large_position.stdout == "empty\n"
  assert large_position.stderr == ""
}

test test_printf_float_conversions_and_precision { |ctx|
  let output = printf_run(ctx, ["%.2f|%.2e|%.3g", "3.14159", "3.14159", "3.14159"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "3.14|3.14e+00|3.14"
  assert output.stderr == ""
}

test test_printf_float_precision_above_formatter_limit { |ctx|
  let output = printf_run(ctx, ["%.70123f", "3.25"])?

  assert output.status == 0, output.stderr
  assert output.stdout.byte_len() == 70125
  assert output.stdout.starts_with("3.25")
  assert output.stdout.byte_slice(4).replace("0", with: "") == ""
  assert output.stderr == ""
}

test test_printf_general_float_trims_zeroes_before_exponent { |ctx|
  let output = printf_run(ctx, ["%g|%g", "0.0001", "0.00001"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "0.0001|1e-05"
  assert output.stderr == ""
}

test test_printf_numeric_defaults_and_float_whitespace { |ctx|
  let missing = printf_run(ctx, ["%-o"])?
  let leading_space = printf_run(ctx, ["%f", " \r\t\n0.000001"])?
  let trailing_space = printf_run(ctx, ["%f", "0.1 "])?
  let invalid_negative_dot = printf_run(ctx, ["%f", "-."])?
  let unicode_space = printf_run(ctx, ["%f", "\u{2029}0.1"])?

  assert missing.status == 0, missing.stderr
  assert missing.stdout == "0"
  assert missing.stderr == ""
  assert leading_space.status == 0, leading_space.stderr
  assert leading_space.stdout == "0.000001"
  assert trailing_space.status == 1
  assert trailing_space.stdout == "0.100000"
  assert trailing_space.stderr == "printf: '0.1 ': value not completely converted\n"
  assert invalid_negative_dot.status == 1
  assert invalid_negative_dot.stdout == "0.000000"
  assert invalid_negative_dot.stderr == "printf: '-.': expected a numeric value\n"
  assert unicode_space.status == 1
  assert unicode_space.stdout == "0.000000"
  assert unicode_space.stderr == "printf: '\\342\\200\\2510.1': expected a numeric value\n"
}

test test_printf_preserves_negative_float_zero_sign { |ctx|
  let output = printf_run(ctx, ["%+06.2f", "-0"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "-00.00"
  assert output.stderr == ""
}

test test_printf_non_finite_float_casing_and_padding { |ctx|
  let output = printf_run(ctx, ["%f|%f|%F|%05.2f|%05.2f|%05.2f|%05.2f", "nan", "-nan", "nan", "inf", "-inf", "nan", "-nan"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "nan|-nan|NAN|  inf| -inf|  nan| -nan"
  assert output.stderr == ""
}

test test_printf_reports_float_overflow_and_underflow { |ctx|
  for spec in ["%a", "%e", "%g", "%f"] {
    let overflow = printf_run(ctx, [spec, "5e8123456789012345678"])?
    let underflow = printf_run(ctx, [spec, "7E-8123456789012345678"])?
    let underflow_output = if spec == "%a" { "0x0p+0" } else if spec == "%e" { "0.000000e+00" } else if spec == "%g" { "0" } else { "0.000000" }

    assert overflow.status == 1
    assert overflow.stdout == "inf"
    assert overflow.stderr == "printf: '5e8123456789012345678': Result not representable\n"
    assert underflow.status == 1
    assert underflow.stdout == underflow_output
    assert underflow.stderr == "printf: '7E-8123456789012345678': Result not representable\n"
  }
}

test test_printf_zero_precision_and_zero_integer { |ctx|
  let output = printf_run(ctx, ["%.0d|%#.0o|%.*d", "0", "0", "-1", "0"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "|0|0"
  assert output.stderr == ""
}

test test_printf_hexadecimal_float_and_character_constant { |ctx|
  let output = printf_run(ctx, ["%a|%A|%i", ".875", ".875", "'a"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "0xep-4|0XEP-4|97"
  assert output.stderr == ""
}

test test_printf_reports_partial_numeric_conversion_after_output { |ctx|
  let output = printf_run(ctx, ["%d:%s", "42x", "done"])?

  assert output.status == 1
  assert output.stdout == "42:done"
  assert output.stderr == "printf: '42x': value not completely converted\n"
}

test test_printf_hex_float_input_and_unsigned_wraparound { |ctx|
  let output = printf_run(ctx, ["%f|%x|%u", "0xF1.1F", "-0x100", "-100"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "241.121094|ffffffffffffff00|18446744073709551516"
  assert output.stderr == ""
}

test test_printf_reports_malformed_hex_and_unicode_escapes { |ctx|
  let missing_hex = printf_run(ctx, ["prefix\\x"])?
  let missing_unicode = printf_run(ctx, ["\\uabc"])?
  let invalid_unicode = printf_run(ctx, ["\\uD9D0"])?
  let bad_backslash_argument = printf_run(ctx, ["prefix%b", "\\x"])?

  assert missing_hex.status == 1
  assert missing_hex.stdout == "prefix"
  assert missing_hex.stderr == "printf: missing hexadecimal number in escape\n"
  assert missing_unicode.status == 1
  assert missing_unicode.stdout == ""
  assert missing_unicode.stderr == "printf: missing hexadecimal number in escape\n"
  assert invalid_unicode.status == 1
  assert invalid_unicode.stdout == ""
  assert invalid_unicode.stderr == "printf: invalid universal character name \\uD9D0\n"
  assert bad_backslash_argument.status == 1
  assert bad_backslash_argument.stdout == "prefix"
  assert bad_backslash_argument.stderr == "printf: missing hexadecimal number in escape\n"
}

test test_printf_escape_sequences_and_backslash_b { |ctx|
  let escaped = printf_run(ctx, ["A\\101\\x42\\u0043\\n%%"])?
  let expanded = printf_run(ctx, ["[%b]", "x\\t\\101\\cignored"])?

  assert escaped.status == 0, escaped.stderr
  assert escaped.stdout == "AABC\n%"
  assert expanded.status == 0, expanded.stderr
  assert expanded.stdout == "[x\tA"
}
