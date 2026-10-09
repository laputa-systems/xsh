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

test test_fmt_width_max_display_width { |ctx|
  let input = test.temp_file(ctx, name: "width.txt", contents: b"aa bb cc dd ee")?
  let width_eight = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -w 8 $input ?
  assert width_eight == "aa bb cc\ndd ee\n"
  let width_seven = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -w 7 $input ?
  assert width_seven == "aa\nbb cc\ndd ee\n"
}

test test_fmt_knuth_plass_line_breaking { |ctx|
  let input = test.temp_file(ctx, name: "knuth-plass.txt", contents:
    bytes.concat([
      b"@command{fmt} prefers breaking lines at the end of a sentence, and tries to\n",
      b"avoid line breaks after the first word of a sentence or before the last word\n",
      b"of a sentence.  A @dfn{sentence break} is defined as either the end of a\n",
      b"paragraph or a word ending in any of @samp{.?!}, followed by two spaces or end\n",
      b"of line, ignoring any intervening parentheses or quotes.  Like @TeX{},\n",
      b"@command{fmt} reads entire ''paragraphs'' before choosing line breaks; the\n",
      b"algorithm is a variant of that given by\n",
      b"Donald E. Knuth and Michael F. Plass\n",
      b"in ''Breaking Paragraphs Into Lines'',\n",
      b"@cite{Software---Practice & Experience}\n",
      b"@b{11}, 11 (November 1981), 1119--1184.",
    ]))?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -g 60 -w 72 $input ?
  let expected = "@command{fmt} prefers breaking lines at the end of a sentence,\n" +
    "and tries to avoid line breaks after the first word of a sentence\n" +
    "or before the last word of a sentence.  A @dfn{sentence break}\n" +
    "is defined as either the end of a paragraph or a word ending\n" +
    "in any of @samp{.?!}, followed by two spaces or end of line,\n" +
    "ignoring any intervening parentheses or quotes.  Like @TeX{},\n" +
    "@command{fmt} reads entire ''paragraphs'' before choosing line\n" +
    "breaks; the algorithm is a variant of that given by Donald\n" +
    "E. Knuth and Michael F. Plass in ''Breaking Paragraphs Into\n" +
    "Lines'', @cite{Software---Practice & Experience} @b{11}, 11\n" +
    "(November 1981), 1119--1184.\n"
  assert output == expected
}
