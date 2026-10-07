test test_pr_headerless_numbering { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -n:2 $input
  assert output == " 1:a\n 2:b\n"
}

test test_pr_across_and_partial_columns { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\nc\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -a -2 -s: $input
  assert output == "a:b\nc\n"
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
  assert columns == "1:a\t  3:c\n2:b\t  4:d\n"
}

test test_pr_numbered_offset_and_joined_columns { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\nc\nd\n")?
  let offset = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -2 -n -o7 -w20 $input
  assert offset == "       \t   1   a     3\t c\n       \t   2   b     4\t d\n"
  let joined = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -2 -J $input
  assert joined == "a\tc\nb\td\n"
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
}

test test_pr_merge_numbering_reserves_one_page_prefix { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"a\n")?
  let second = test.temp_file(ctx, name: "second", contents: b"b\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -m -n -w20 $first $second
  assert output == "    1\ta     b\n"
}

test test_pr_merge_preserves_positions_after_a_file_ends { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"a\nb\n")?
  let second = test.temp_file(ctx, name: "second", contents: b"x  \n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pr.xsh" -- -t -m -n -w20 $first $second
  assert output == "    1\ta     x\n    2\tb     \n"
}
