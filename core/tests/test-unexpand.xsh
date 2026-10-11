test test_unexpand_initial_and_all { |ctx|
  let input = test.temp_file(ctx, name: "spaces", contents: b"        a       b\n")?
  let initial = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- $input
  assert initial == "\ta       b\n"
  let all = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -a $input
  assert all == "\ta\tb\n"
}

test test_unexpand_finite_stops_and_single_spaces { |ctx|
  let input = test.temp_file(ctx, name: "spaces", contents: b"      a b c\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -t 2,4 $input
  assert output == "\t\t  a b c\n"
}

test test_unexpand_unicode_display_columns { |ctx|
  let input = test.temp_file(ctx, name: "wide", contents: b"\xe4\xb8\xad      X")?
  let output = run.text LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -a $input
  assert output == "中\tX"
}

test test_unexpand_ideographic_blanks_preserve_unconverted_bytes { |ctx|
  let input = test.temp_file(ctx, name: "wide", contents: b"\xe3\x80\x80\xe3\x80\x80\xe3\x80\x80\xe3\x80\x80Z\na\xe3\x80\x80b\n")?
  let output = run.text LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -a $input
  assert output == "\tZ\na　b\n"
}

test test_unexpand_does_not_split_wide_blank_at_a_tab_stop { |ctx|
  let input = test.temp_file(ctx, name: "wide", contents: b"   \xe3\x80\x80X\ty\n")?
  let output = run.text LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -a -t4 $input
  assert output == "   \u{3000}X\ty\n"
}

test test_unexpand_accepts_non_utf8_path_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "unexpand-raw-path")?
  let input = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  input.write("a       b\n")
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -a $input
  assert output == "a\tb\n"
}

test test_unexpand_obsolete_tab_stop_overflow_omits_the_value { |ctx|
  let input = test.temp_file(ctx, name: "tabs", contents: b"a\n")?
  let error = test.temp_path(ctx, name: "tab-error")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -18446744073709551616 $input 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == "unexpand: tab stop is too large\n"
  let explicit = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -t 18446744073709551616 $input 2> $error
  assert explicit.exited_with(1)
  assert "unexpand: tab stop is too large " in error.read_text()?, "the -t form quotes the value"
}


test test_unexpand_first_only_has_no_short_alias { |ctx|
  let input = test.temp_file(ctx, name: "spaces", contents: b"        a       b\n")?
  let first = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -t8 --first-only $input
  assert first == "\ta       b\n"
  let invalid = run.capture --text LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -f $input
  assert invalid.status.exited_with(1)
  assert invalid.stderr == "unexpand: invalid option -- 'f'\nTry 'unexpand --help' for more information.\n"
}


test test_unexpand_c_locale_preserves_unicode_blanks { |ctx|
  let data = bytes.from_text("　　　　Z\n")
  let input = test.temp_file(ctx, name: "unicode-blanks", contents: data)?
  let output = run.capture --bytes LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -a $input
  assert output.status.exited_with(0)
  assert output.stdout == data
}

test test_unexpand_c_locale_counts_multibyte_text_as_bytes { |ctx|
  let data = bytes.from_text("1ΔΔΔ5   99999\n")
  let input = test.temp_file(ctx, name: "multibyte", contents: data)?
  let output = run.capture --bytes LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -a $input
  assert output.status.exited_with(0)
  assert output.stdout == data
}

test test_unexpand_c_locale_counts_wide_text_as_bytes { |ctx|
  let input = test.temp_file(ctx, name: "wide-text", contents: bytes.from_text("－      X\n"))?
  let output = run.capture --bytes LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -a $input
  assert output.status.exited_with(0)
  assert output.stdout == bytes.from_text("－\t X\n")
}


test test_unexpand_c_locale_preserves_binary_column_controls { |ctx|
  let input = test.temp_file(ctx, name: "binary-columns", contents: b"\0\xff\x08       X\n\x1b       Y\n")?
  let output = run.capture --bytes LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/unexpand.xsh" -- -a $input
  assert output.status.exited_with(0)
  assert output.stdout == b"\0\xff\x08       X\n\x1b\tY\n"
}
