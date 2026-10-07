test test_nl_numbering_sections_and_negative_zero_padding { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\n\n\\:\\:\nb")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- -v -12 -n rz -w 6 $input
  assert output == "-00012\ta\n       \n\n-00012\tb\n"
}

test test_nl_groups_blank_lines { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\n\n\nb\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- -b a -l 2 -w 1 -s : $input
  assert output == "1:a\n  \n2:\n3:b\n"
}

test test_nl_separates_unterminated_file_records { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"a")?
  let second = test.temp_file(ctx, name: "second", contents: b"b")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- -w 1 -s : $first $second
  assert output == "1:a\n2:b\n"
}

test test_nl_numeric_errors_match_gnu { |ctx|
  let error = test.temp_path(ctx, name: "error")
  let zero_width = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- -w0 2> $error
  assert zero_width.exited_with(1)
  assert error.read_text()? == "nl: invalid line number field width: '0': Result not representable\n"

  let large_width = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- -w2147483648 2> $error
  assert large_width.exited_with(1)
  assert error.read_text()? == "nl: invalid line number field width: '2147483648': Value too large for data type\n"

  let invalid_blanks = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- -linvalid 2> $error
  assert invalid_blanks.exited_with(1)
  assert error.read_text()? == "nl: invalid line number of blank lines: 'invalid'\n"

  let invalid_increment = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- -i9223372036854775808 2> $error
  assert invalid_increment.exited_with(1)
  assert error.read_text()? == "nl: invalid line number increment: '9223372036854775808': Value too large for data type\n"
}
