test test_fmt_reflows_paragraphs_and_preserves_blank_lines { |ctx|
  let input = test.temp_file(ctx, name: "paragraph.txt", contents: b"this\nis\na\nfile\nwith\none\nword\nper\nline\n\nsecond\nparagraph\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -w 10 $input ?
  assert output == "this is a\nfile with\none word\nper line\n\nsecond\nparagraph\n"
}

test test_fmt_split_only_does_not_reflow { |ctx|
  let input = test.temp_file(ctx, name: "split.txt", contents: b"one\nword\nper\nline\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -s $input ?
  assert output == "one\nword\nper\nline\n"
}
