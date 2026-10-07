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

test test_cut_utf8_character_positions { |ctx|
  let input = test.temp_file(ctx, name: "unicode", contents: b"\xc3\xa9Z\n")?
  let output = run.text env LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -c2 $input
  assert output == "Z\n"
}

test test_cut_rejects_repeated_modes_and_keeps_fields_abbreviation { |ctx|
  let input = test.temp_file(ctx, name: "table", contents: b"a:b\n")?
  let abbreviated = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- --fie=2 -d: $input
  assert abbreviated == "b\n"
  let error = test.temp_path(ctx, name: "error")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -f1 -f2 $input 2> $error
  assert status.exited_with(1)
  assert "only one list may be specified" in error.read_text()?
}

test test_cut_whitespace_delimiter_uses_unicode_blank_characters { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"one\xe3\x80\x80two three\n")?
  let output = run.text env LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -w -f2 $input
  assert output == "two\n"
}

test test_cut_preserves_gb18030_character_boundaries { |ctx|
  let input = test.temp_file(ctx, name: "gb", contents: b"\xb0\xa1w\xd6\xd0\n")?
  let output = run.text env LC_ALL=zh_CN.GB18030 ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -c2 $input
  assert output == "w\n"
  let selected = test.temp_path(ctx, name: "selected")
  let status = run.status env LC_ALL=zh_CN.GB18030 ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -b2 -n $input > $selected
  assert status.exited_with(0)
  assert selected.read_bytes()? == b"\xb0\xa1\n"
}

test test_cut_unicode_nobreak_spaces_are_not_blank_delimiters { |ctx|
  let input = test.temp_file(ctx, name: "spaces", contents: bytes.from_text("x\u{a0}y\nx\u{2007}y\nx\u{202f}y\n"))?
  let output = run.text env LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -- -s -w -f2 $input
  assert output == ""
}

test test_cut_accepts_non_utf8_path_and_delimiter_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "cut-raw-arguments")?
  let input = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  input.write(b"left\xadright\n")
  let output = test.temp_path(ctx, name: "output")
  let command = "delimiter=$(printf '\\255'); exec \"$1\" \"$2\" -- -d\"$delimiter\" -f2 \"$3\" > \"$4\""
  let status = run.status env LC_ALL=C sh -c $command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" $input $output
  assert status.exited_with(0)
  assert output.read_bytes()? == b"right\n"
}

test test_cut_delimiters_follow_utf8_and_gb18030_encoding { |ctx|
  let utf8 = test.temp_file(ctx, name: "utf8", contents: b"1\xe2\x82\xac2\xac3\n")?
  let utf8_output = test.temp_path(ctx, name: "utf8-output")
  let utf8_command = "delimiter=$(printf '\\254'); exec \"$1\" \"$2\" -- -d \"$delimiter\" -f2 \"$3\" > \"$4\""
  let utf8_status = run.status env LC_ALL=C.UTF-8 sh -c $utf8_command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" $utf8 $utf8_output
  assert utf8_status.exited_with(0)
  assert utf8_output.read_bytes()? == b"3\n"

  let gb = test.temp_file(ctx, name: "gb-delimiter", contents: b"red\xb0\xa1green\xb0\xa1blue\n")?
  let gb_output = test.temp_path(ctx, name: "gb-output")
  let gb_command = "delimiter=$(printf '\\260\\241'); exec \"$1\" \"$2\" -- -d \"$delimiter\" -f3 \"$3\" > \"$4\""
  let gb_status = run.status env LC_ALL=zh_CN.gb18030 sh -c $gb_command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" $gb $gb_output
  assert gb_status.exited_with(0)
  assert gb_output.read_bytes()? == b"blue\n"
}
