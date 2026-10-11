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
  assert status.exited_with(1)
  assert err.read_text()? == "printf: missing operand\nTry 'printf --help' for more information.\n"
}

type PrintfResult = {status: Int, stdout: Str, stderr: Str}

proc printf_run(ctx: TestContext, args: List[Str], posix: Bool = false) [fs, process, error] -> Result[PrintfResult] {
  let root = test.temp_dir(ctx, name: "printf-argv")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/printf.xsh"
  let command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display()].extend(args), root,
    {LC_ALL: "C"}, b"", stdout, stderr)
  let status = if posix {
    process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display()].extend(args), root,
      {LC_ALL: "C", POSIXLY_CORRECT: "1"}, b"", stdout, stderr))?
  } else { process.run(command)? }
  {status: status.exit_code()?, stdout: stdout.read_text()?, stderr: stderr.read_text()?}
}

type PrintfBytesResult = {status: Int, stdout: Bytes}

proc printf_bytes_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[PrintfBytesResult] {
  let root = test.temp_dir(ctx, name: "printf-bytes")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/printf.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display()].extend(args), root,
    {LC_ALL: "C.UTF-8"}, b"", stdout, stderr))?
  {status: status.exit_code()?, stdout: stdout.read_bytes()?}
}

test test_printf_preserves_output_bytes { |ctx|
  let character = printf_bytes_run(ctx, ["%c", "🙃"])?
  let format_escape = printf_bytes_run(ctx, ["\\xc2\\x81"])?
  let argument_escape = printf_bytes_run(ctx, ["%b", "\\xc2\\x81"])?

  assert character.status == 0
  assert character.stdout == b"\xf0"
  assert format_escape.status == 0
  assert format_escape.stdout == b"\xc2\x81"
  assert argument_escape.status == 0
  assert argument_escape.stdout == b"\xc2\x81"
}

test test_printf_flushes_stdout_and_reports_write_errors { |ctx|
  if ! p"/dev/full".exists()? { test.skip("requires /dev/full"); return }
  let root = test.temp_dir(ctx, name: "printf-write-error")?
  let script = fp"{ctx.core_dir}/printf.xsh"
  let error_path = fp"{root}/error"
  let failed = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "greeting"], root,
    {LC_ALL: "C"}, b"", p"/dev/full", error_path))?
  let empty_error = fp"{root}/empty-error"
  let empty = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), ""], root,
    {LC_ALL: "C"}, b"", p"/dev/full", empty_error))?

  assert failed.exit_code()? == 1
  assert error_path.read_text()? == "printf: write error: No space left on device\n"
  assert empty.exit_code()? == 0
  assert empty_error.read_text()? == ""
}

test test_printf_streams_large_field_widths { |ctx|
  let root = test.temp_dir(ctx, name: "printf-large-width")?
  let script = fp"{ctx.core_dir}/printf.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "A%1000001sB", "x"], root,
    {LC_ALL: "C"}, b"", stdout, stderr))?
  let data = stdout.read_bytes()?

  assert status.exit_code()? == 0
  assert data.len() == 1000003
  assert data.byte_at(0) == 65
  assert data.byte_at(1) == 32
  assert data.byte_at(1000001) == 120
  assert data.byte_at(1000002) == 66
  assert stderr.read_text()? == ""

  if ! p"/dev/full".exists()? { test.skip("requires /dev/full"); return }
  let full_error = fp"{root}/error-full"
  let full = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "%20000000f", "1"], root,
    {LC_ALL: "C"}, b"", p"/dev/full", full_error))?
  assert full.exit_code()? == 1
  assert full_error.read_text()? == "printf: write error: No space left on device\n"

  let overflow_error = fp"{root}/error-overflow"
  let overflow = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "%999999999999999999999999d", "1"], root,
    {LC_ALL: "C"}, b"", p"/dev/full", overflow_error))?
  assert overflow.exit_code()? == 1
  assert overflow_error.read_text()? == "printf: write error\n"
}

test test_printf_matches_c_field_width_limits { |ctx|
  if ! p"/dev/full".exists()? { test.skip("requires /dev/full"); return }
  let root = test.temp_dir(ctx, name: "printf-width-limits")?
  let script = fp"{ctx.core_dir}/printf.xsh"

  for width in ["-9223372036854775808", "2147483648", "9223372036854775808"] {
    let error_path = fp"{root}/dynamic-{width.byte_len()}"
    let result = process.run(process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin.display(), "--", script.display(), "%*d", width, "1"], root,
      {LC_ALL: "C"}, b"", p"/dev/full", error_path))?

    assert result.exit_code()? == 1
    let diagnostic = error_path.read_text()?
    assert "invalid field width" in diagnostic
    assert width in diagnostic
    if width == "9223372036854775808" {
      assert "Numerical result out of range" in diagnostic
    }
  }

  let overflow_error = fp"{root}/literal-overflow"
  let overflow = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), "--", script.display(), "%999999999999999999999999d", "1"], root,
    {LC_ALL: "C"}, b"", p"/dev/full", overflow_error))?
  assert overflow.exit_code()? == 1
  assert overflow_error.read_text()? == "printf: write error\n"
}

test test_printf_rejects_precision_above_printf_limit { |ctx|
  let result = printf_run(ctx, ["%.*d", "2147483648", "0"])?

  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "printf: invalid precision: '2147483648'\n"
}

test test_printf_rejects_zero_positional_index { |ctx|
  let result = printf_run(ctx, ["%0$d%d-", "5", "10", "6", "20"])?

  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "printf: %0$: invalid conversion specification\n"
}

test test_printf_warns_about_trailing_char_constant_after_failure { |ctx|
  let args = ["%d\n", "bad", "'ab"]
  let ordinary = printf_run(ctx, args)?
  let posix = printf_run(ctx, args, true)?

  assert ordinary.status == 1
  assert ordinary.stdout == "0\n97\n"
  assert ordinary.stderr == "printf: 'bad': expected a numeric value\nprintf: warning: b: character(s) following character constant have been ignored\n"
  assert posix.status == 1
  assert posix.stdout == "0\n97\n"
  assert posix.stderr == "printf: 'bad': expected a numeric value\n"
}

test test_printf_reports_empty_character_constant { |ctx|
  let result = printf_run(ctx, ["%d", "'"])?

  assert result.status == 1
  assert result.stdout == "0"
  assert result.stderr == "printf: '\\'': expected a numeric value\n"
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
  let apostrophe = printf_run(ctx, ["%q", "'"])?
  let leading_tilde = printf_run(ctx, ["%q", "~a"])?

  assert quoted.status == 0, quoted.stderr
  assert quoted.stdout == "test~|'a b'|''|'\"$test\"'"
  assert quoted.stderr == ""
  assert quote_then_literal.status == 0, quote_then_literal.stderr
  assert quote_then_literal.stdout == "'a b'd"
  assert quote_then_literal.stderr == ""
  assert apostrophe.stdout == "\"'\""
  assert leading_tilde.stdout == "'~a'"
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
  assert output.stdout == "[-0012][xy   ][abc][0x1a][6]"
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

test test_printf_rejects_flags_not_supported_by_conversion { |ctx|
  let alternate = printf_run(ctx, ["%#d", "0"])?
  let grouping = printf_run(ctx, ["%'s", "text"])?

  assert alternate.status == 1
  assert alternate.stdout == ""
  assert alternate.stderr == "printf: %#d: invalid conversion specification\n"
  assert grouping.status == 1
  assert grouping.stdout == ""
  assert grouping.stderr == "printf: %'s: invalid conversion specification\n"
}

test test_printf_reports_non_numeric_dynamic_width_and_precision { |ctx|
  let empty_width = printf_run(ctx, ["%*s", "", "empty width"])?
  let space_width = printf_run(ctx, ["%*s", " ", "space width"])?
  let empty_precision = printf_run(ctx, ["%.*sx", "", "empty precision"])?
  let space_precision = printf_run(ctx, ["%.*sx", " ", "space precision"])?

  assert empty_width.status == 1
  assert empty_width.stdout == "empty width"
  assert empty_width.stderr == "printf: '': expected a numeric value\n"
  assert space_width.status == 1
  assert space_width.stdout == "space width"
  assert space_width.stderr == "printf: ' ': expected a numeric value\n"
  assert empty_precision.status == 1
  assert empty_precision.stdout == "x"
  assert empty_precision.stderr == "printf: '': expected a numeric value\n"
  assert space_precision.status == 1
  assert space_precision.stdout == "x"
  assert space_precision.stderr == "printf: ' ': expected a numeric value\n"
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
    assert overflow.stderr == "printf: '5e8123456789012345678': Numerical result out of range\n"
    assert underflow.status == 1
    assert underflow.stdout == underflow_output
    assert underflow.stderr == "printf: '7E-8123456789012345678': Numerical result out of range\n"
  }
}

test test_printf_shell_quote_control_bytes_with_quotes { |ctx|
  let result = printf_run(ctx, ["%q", "\u{1}'\u{1}"])?

  assert result.status == 0
  assert result.stdout == "''$'\\001'\\'''$'\\001'"
  assert result.stderr == ""
}

test test_printf_zero_precision_and_zero_integer { |ctx|
  let output = printf_run(ctx, ["%.0d|%#.0o|%.*d", "0", "0", "-1", "0"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "|0|0"
  assert output.stderr == ""
}

test test_printf_hexadecimal_float_and_character_constant { |ctx|
  let output = printf_run(ctx, ["%a|%A|%i", ".875", ".875", "'a"])?
  let float_character = printf_run(ctx, ["%f", "'á"])?

  assert output.status == 0, output.stderr
  assert output.stdout == "0xep-4|0XEP-4|97"
  assert output.stderr == ""
  assert float_character.status == 0, float_character.stderr
  assert float_character.stdout == "225.000000"
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
  assert invalid_unicode.stderr == "printf: invalid universal character name \\ud9d0\n"
  assert bad_backslash_argument.status == 1
  assert bad_backslash_argument.stdout == "prefix"
  assert bad_backslash_argument.stderr == "printf: missing hexadecimal number in escape\n"
}

test test_printf_escape_sequences_and_backslash_b { |ctx|
  let escaped = printf_run(ctx, ["A\\101\\x42\\u0043\\n%%"])?
  let expanded = printf_run(ctx, ["[%b]", "x\\t\\101\\cignored"])?
  let formatted_zero = printf_run(ctx, ["\\0001_"])?
  let expanded_octal = printf_run(ctx, ["%b", "\\0001_"])?

  assert escaped.status == 0, escaped.stderr
  assert escaped.stdout == "AABC\n%"
  assert expanded.status == 0, expanded.stderr
  assert expanded.stdout == "[x\tA"
  assert formatted_zero.stdout == "\u{0}1_"
  assert expanded_octal.stdout == "\u{1}_"
}

test test_printf_exact_decimal_digits_beyond_double_precision { |ctx|
  let literal = printf_run(ctx, ["%.30f", "0.1"])?
  let negative = printf_run(ctx, ["%.20f", "-0.25"])?
  let rounded = printf_run(ctx, ["%.18f", "0.1234567890123456789012"])?
  let tie_to_even = printf_run(ctx, ["%.18f", "0.1234567890123456765"])?
  let tie_to_even_up = printf_run(ctx, ["%.18f", "0.1234567890123456775"])?

  assert literal.status == 0, literal.stderr
  assert literal.stdout == "0.100000000000000000000000000000"
  assert negative.status == 0, negative.stderr
  assert negative.stdout == "-0.25000000000000000000"
  assert rounded.stdout == "0.123456789012345679"
  assert tie_to_even.stdout == "0.123456789012345676"
  assert tie_to_even_up.stdout == "0.123456789012345678"
  assert literal.stderr == "" and rounded.stderr == ""
}

test test_printf_integer_minimum_is_representable { |ctx|
  let precision = printf_run(ctx, ["|%.*d|", "-9223372036854775808", "10"])?
  let value = printf_run(ctx, ["%d", "-9223372036854775808"])?
  let past_maximum = printf_run(ctx, ["%d", "9223372036854775808"])?

  assert precision.status == 0, precision.stderr
  assert precision.stdout == "|10|"
  assert precision.stderr == ""
  assert value.status == 0, value.stderr
  assert value.stdout == "-9223372036854775808"
  assert value.stderr == ""
  assert past_maximum.status == 1
  assert past_maximum.stdout == "9223372036854775807"
  assert past_maximum.stderr == "printf: '9223372036854775808': Numerical result out of range\n"
}

test test_printf_c_locale_unicode_escape_fallback { |ctx|
  let escaped = printf_run(ctx, ["\\u0125|\\U00000125"])?
  assert escaped.status == 0, escaped.stderr
  assert escaped.stdout == "\\u0125|\\u0125"
  let invalid = printf_run(ctx, ["\\U0000D8F9"])?
  assert invalid.status == 1
  assert invalid.stderr == "printf: invalid universal character name \\U0000d8f9\n"
}
