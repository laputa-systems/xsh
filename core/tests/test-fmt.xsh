test test_fmt_joins_paragraphs_and_quick_wraps { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"one\ntwo\nthree\n\nfour five\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -q -w 8 $input
  assert output == "one two\nthree\n\nfour\nfive\n"
}

test test_fmt_prefix_leaves_other_lines { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"# one\n# two\nplain\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -q -p "# " $input
  assert output == "# one two\nplain\n"
}

test test_fmt_balances_paragraph_lines { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"aa bb cc dd ee")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -w 7 $input
  assert output == "aa\nbb cc\ndd ee\n"
}

test test_fmt_preserves_invalid_utf8_bytes { |ctx|
  let input = test.temp_file(ctx, name: "raw", contents: b"=\xa0=")?
  let output = test.temp_path(ctx, name: "formatted")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -s -w1 $input > $output
  assert status.exited_with(0)
  assert output.read_bytes()? == b"=\xa0=\n"
}

test test_fmt_avoids_false_sentence_breaks_and_sentence_widows { |ctx|
  let initials = test.temp_file(ctx, name: "initials", contents: b"Donald E. Knuth and Michael F. Plass wrote this paragraph formatting algorithm.")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -g25 -w30 $initials
  assert output == "Donald E. Knuth and Michael\nF. Plass wrote this paragraph\nformatting algorithm.\n"
  let sentences = test.temp_file(ctx, name: "sentences", contents: b"One short sentence.  A slightly longer sentence follows the first sentence.")?
  let balanced = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fmt.xsh" -- -g25 -w30 $sentences
  assert balanced == "One short sentence.\nA slightly longer sentence\nfollows the first sentence.\n"
}
