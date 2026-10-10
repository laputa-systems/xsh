test test_pr_headerless_numbering { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -n:2 $input
  assert output == " 1:a\n 2:b\n"
  let backwards_compatible = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -b -t $input
  assert backwards_compatible == "a\nb\n"
}

test test_pr_across_and_partial_columns { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\nc\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -a -2 -s: $input
  assert output == "a                                  :b                                  \nc                                  \n"
  let compact_columns = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -W3 -t2 $input
  assert compact_columns == "a\tc\nb\n"
}

test test_pr_pages_with_literal_date_format { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\nc\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -l 12 -D DATE -h title -w 20 $input
  assert output == "\n\nDATE  title   Page 1\n\n\na\nb\n\n\n\n\n\n\n\nDATE  title   Page 2\n\n\nc\n\n\n\n\n\n\n"
}

test test_pr_tabs_and_formfeed_pages { |ctx|
  let input = test.temp_file(ctx, name: "tabs", contents: b"a\tb\n")?
  let expanded = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -e4 $input
  assert expanded == "a   b\n"
  let compressed = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -e4 -i4 $input
  assert compressed == "a\tb\n"
  let page = test.temp_file(ctx, name: "pages", contents: b"a\x0cb\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t $page
  assert output == "a\nb\n"
}

test test_pr_number_field_clips_digits_and_counted_columns_align { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\nc\nd\n")?
  let digits = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -n:1 -N10 $input
  assert digits == "0:a\n1:b\n2:c\n3:d\n"
  let columns = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -2 -n:1 -w20 $input
  assert columns == "1:a      \t3:c      \n2:b      \t4:d      \n"
}

test test_pr_numbered_offset_and_joined_columns { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\nc\nd\n")?
  let offset = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -2 -n -o7 -w20 $input
  assert offset == "           1\ta\t           3\tc\n           2\tb\t           4\td\n"
  let joined = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -2 -J $input
  assert joined == "ac\nbd\n"
}

test test_pr_rejects_across_merge_and_warns_for_unavailable_page { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\n")?
  let error = test.temp_path(ctx, name: "error")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -a -m $input 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == "pr: cannot specify both printing across and printing in parallel\n"
  let warning = test.temp_path(ctx, name: "warning")
  let page = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- --pages=2 $input 2> $warning
  assert page.exited_with(0)
  assert warning.read_text()? == "pr: starting page number 2 exceeds page count 1\n"
}

test test_pr_supports_large_number_fields { |ctx|
  let input = test.temp_file(ctx, name: "line", contents: b"x\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -n70000 $input
  assert output.byte_len() == 70003
  assert output.ends_with("1\tx\n")
}

test test_pr_streams_large_indents { |ctx|
  let input = test.temp_file(ctx, name: "line", contents: b"x\n")?
  # Leave room for the debug interpreter's preparation stack while bounding a regressed allocation.
  let command = "ulimit -v 524288; timeout 10 \"$1\" \"$2\" -- -t -o999999999 \"$3\" >/dev/null"
  let status = run.status sh -c $command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" $input
  assert status.exited_with(0)
}

test test_pr_optional_numeric_arguments_report_the_invalid_suffix { |ctx|
  let error = test.temp_path(ctx, name: "error")
  let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -efoo /dev/null 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == "pr: '-e' extra characters or invalid number in the argument: 'oo'\nTry 'pr --help' for more information.\n"
  let overflow = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -n:2147483648 /dev/null 2> $error
  assert overflow.exited_with(1)
  assert "'-n' extra characters or invalid number in the argument: '2147483648': Value too large for data type" in error.read_text()?

  let digit_suffix = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -e1a /dev/null 2> $error
  assert digit_suffix.exited_with(1)
  assert error.read_text()? == "pr: '-e' extra characters or invalid number in the argument: '1a'\nTry 'pr --help' for more information.\n"

  let negative_width = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -e=-1 /dev/null 2> $error
  assert negative_width.exited_with(1)
  assert error.read_text()? == "pr: '-e' extra characters or invalid number in the argument: '-1'\nTry 'pr --help' for more information.\n"
}

test test_pr_merge_numbering_reserves_one_page_prefix { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"a\n")?
  let second = test.temp_file(ctx, name: "second", contents: b"b\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -m -n -w20 $first $second
  var expected = "    1\ta\tb        \n"
  for _ in range(65) { expected += "         \t         \n" }
  assert output == expected
}

test test_pr_merge_preserves_positions_after_a_file_ends { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"a\nb\n")?
  let second = test.temp_file(ctx, name: "second", contents: b"x  \n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -m -n -w20 $first $second
  var expected = "    1\ta\tx        \n    2\tb\t         \n"
  for _ in range(64) { expected += "         \t         \n" }
  assert output == expected
}

test test_pr_page_range_errors_match_gnu { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\n")?
  let error = test.temp_path(ctx, name: "error")
  let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- --pages=20:5 $input 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == "pr: invalid page range '20:5'\n"

  let large = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- --pages=18446744073709551615 $input 2> $error
  assert large.exited_with(0)
  assert error.read_text()? == "pr: starting page number 18446744073709551615 exceeds page count 1\n"

  let leading_zeroes = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- --pages=+0002 $input 2> $error
  assert leading_zeroes.exited_with(0)
  assert error.read_text()? == "pr: starting page number 2 exceeds page count 1\n"

  let negative = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- --pages=-0 2> $error
  assert negative.exited_with(1)
  assert error.read_text()? == "pr: invalid --pages argument '-0'\n"

  let zero_operand = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- +0 2> $error
  assert zero_operand.exited_with(1)
  assert error.read_text()? == "pr: +0: No such file or directory\n"
}

test test_pr_integer_overflow_errors_match_gnu { |ctx|
  let error = test.temp_path(ctx, name: "error")
  let columns = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- --columns=9999999999999999999 2> $error
  assert columns.exited_with(1)
  assert error.read_text()? == "pr: invalid number of columns: '9999999999999999999': Value too large for defined data type\n"

  let width = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -w 18446744073709551615 -2 2> $error
  assert width.exited_with(1)
  assert error.read_text()? == "pr: '-w PAGE_WIDTH' invalid number of characters: '18446744073709551615': Value too large for defined data type\n"
}

test test_pr_invalid_indent_uses_gnu_wording { |ctx|
  let error = test.temp_path(ctx, name: "error")
  let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- --indent=-5 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == "pr: '-o MARGIN' invalid line offset: '-5'\n"
}

test test_pr_rejects_input_column_overflow_before_expanding_tabs { |ctx|
  let input = test.temp_file(ctx, name: "tabs", contents: b"\t\t")?
  let error = test.temp_path(ctx, name: "error")
  let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -e1073741824 $input 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == "pr: integer overflow\n"
}

test test_pr_zero_page_dimensions_report_range_error { |ctx|
  let error = test.temp_path(ctx, name: "error")
  let length = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -l0 2> $error
  assert length.exited_with(1)
  assert error.read_text()? == "pr: '-l PAGE_LENGTH' invalid number of lines: '0': Result not representable\n"

  let width = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -w0 2> $error
  assert width.exited_with(1)
  assert error.read_text()? == "pr: '-w PAGE_WIDTH' invalid number of characters: '0': Result not representable\n"

  let page_width = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -W0 2> $error
  assert page_width.exited_with(1)
  assert error.read_text()? == "pr: '-W PAGE_WIDTH' invalid number of characters: '0': Result not representable\n"
}

test test_pr_columns_pad_every_cell_and_separate_with_tab { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\nc\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -2 -w 20 $input
  assert output == "a        \tc        \nb        \n"
}

test test_pr_offset_indents_every_column { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -o 2 -2 -w 20 $input
  assert output == "  a        \t  b        \n"
}

test test_pr_quiet_suppresses_invalid_page_range_message { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\n")?
  let error = test.temp_path(ctx, name: "error")
  let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- --pages=20:5 -r $input 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == ""
}

test test_pr_expand_tab_overflow_message_has_no_overflow_note { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\n")?
  let error = test.temp_path(ctx, name: "error")
  let status = run.status env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -e2147483648 $input 2> $error
  assert status.exited_with(1)
  assert error.read_text()? == "pr: '-e' extra characters or invalid number in the argument: '2147483648'\nTry 'pr --help' for more information.\n"
}

test test_pr_posix_date_format_needs_posixly_correct_and_posix_time { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\n")?
  let posix = run.text env POSIXLY_CORRECT=1 LC_ALL=POSIX ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- $input
  assert rx"^[A-Z][a-z]{2} [ 0-9][0-9] [0-9]{2}:[0-9]{2} [0-9]{4} .*Page 1$".matches(posix.lines()[2])
  let plain = run.text env LC_TIME=POSIX ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- $input
  assert rx"^[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2} .*Page 1$".matches(plain.lines()[2])
}
