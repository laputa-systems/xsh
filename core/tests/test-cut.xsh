test test_cut_fields { |ctx|
  let input = test.temp_file(ctx, name: "table.txt", contents: b"a,b,c\n1,2,3\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -d , -f 2 $input
  assert "b" in output
  assert "2" in output
}

test test_cut_reads_files_and_stdin_in_operand_order { |ctx|
  let first = test.temp_file(ctx, name: "first.txt", contents: b"a,b\n")?
  let middle = test.temp_file(ctx, name: "middle.txt", contents: b"c,d\n")?
  let last = test.temp_file(ctx, name: "last.txt", contents: b"e,f\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -d , -f 2 $first - $last < ${middle}
  assert output == """b
d
f
"""
}

test test_cut_complement_and_empty_selected_field { |ctx|
  let input = test.temp_file(ctx, name: "empty.csv", contents: b"a,,c\nplain\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -d , -f 2 -s $input
  assert output == "\n"
  let complement = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -b 2 --complement $input
  assert complement == "a,c\npain\n"
}

test test_cut_newline_delimiter_and_multibyte_tail { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\nc\n")?
  let fields = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -d "\n" -f 2 $input
  assert fields == "b\n"
  let unicode = test.temp_file(ctx, name: "unicode", contents: b"\xc3\xbcZ\n")?
  let output = run.text env LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -b2- -n $unicode
  assert output == "üZ\n"
}

test test_cut_keeps_adjacent_range_boundaries { |ctx|
  let input = test.temp_file(ctx, name: "bytes", contents: b"abcd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -b 1-2,3-4 --output-delimiter : $input
  assert output == "ab:cd\n"
}

test test_cut_whitespace_and_merged_fields { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"  one\ttwo   three  \n")?
  let trimmed = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- --whitespace-delimited=trimmed -f 1,3 $input
  assert trimmed == "one\tthree\n"
  let merged = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -F 1,3 -O : $input
  assert merged == "one:three\n"
}

test test_cut_unicode_field_delimiter { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"\xe4\xb8\xad\xf0\x9f\x97\xbfX\n")?
  let output = run.text env LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -d "🗿" -f 2 $input
  assert output == "X\n"
}
