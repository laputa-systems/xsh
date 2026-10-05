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
