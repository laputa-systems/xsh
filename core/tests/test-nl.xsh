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

test test_nl_preserves_non_utf8_paths_and_number_separators { |ctx|
  let root = test.temp_dir(ctx, name: "nl-raw-arguments")?
  let input = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  input.write("test")
  let output = test.temp_path(ctx, name: "numbered")
  let command = "separator=$(printf '\\377\\376'); exec \"$1\" \"$2\" -- --number-separator=\"$separator\" \"$3\" > \"$4\""
  let status = run.status env LC_ALL=C sh -c $command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" $input $output
  assert status.exited_with(0)
  assert output.read_bytes()? == b"     1\xff\xfetest\n"
}

test test_nl_matches_raw_section_delimiters { |ctx|
  let pair_input = test.temp_file(ctx, name: "pair-sections", contents: b"a\n\xff\xfe\xff\xfe\xff\xfe\nb")?
  let pair_output = test.temp_path(ctx, name: "pair-numbered")
  let pair_command = "delimiter=$(printf '\\377\\376'); exec \"$1\" \"$2\" -- --section-delimiter=\"$delimiter\" \"$3\" > \"$4\""
  let pair_status = run.status env LC_ALL=C sh -c $pair_command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" $pair_input $pair_output
  assert pair_status.exited_with(0)
  assert pair_output.read_bytes()? == b"     1\ta\n\n       b\n"

  let byte_input = test.temp_file(ctx, name: "byte-sections", contents: b"a\n\xff:\xff:\xff:\nb")?
  let byte_output = test.temp_path(ctx, name: "byte-numbered")
  let byte_command = "delimiter=$(printf '\\377'); exec \"$1\" \"$2\" -- -d\"$delimiter\" \"$3\" > \"$4\""
  let byte_status = run.status env LC_ALL=C sh -c $byte_command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" $byte_input $byte_output
  assert byte_status.exited_with(0)
  assert byte_output.read_bytes()? == b"     1\ta\n\n       b\n"
}

test test_nl_treats_section_delimiters_as_bytes { |ctx|
  let input = test.temp_file(ctx, name: "sections", contents: bytes.from_text("a\nä:ä:ä:\nä\nb"))?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- "-dä" $input
  assert output == "     1\ta\n     2\tä:ä:ä:\n\n       b\n"
}

test test_nl_names_invalid_numbering_sections { |ctx|
  let error = test.temp_path(ctx, name: "error")
  for section in ["header", "body", "footer"] {
    let option = f"--{section}-numbering=invalid"
    let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- $option 2> $error
    assert status.exited_with(1)
    assert error.read_text()? == f"nl: invalid {section} numbering style: 'invalid'\nTry 'nl --help' for more information.\n"
  }
}

test test_nl_reports_invalid_regular_expressions { |ctx|
  let error = test.temp_path(ctx, name: "error")
  for section in ["header", "body", "footer"] {
    let option = f"--{section}-numbering=p["
    let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/nl.xsh" -- $option 2> $error
    assert status.exited_with(1)
    assert error.read_text()? == "nl: Invalid regular expression\n"
  }
}
