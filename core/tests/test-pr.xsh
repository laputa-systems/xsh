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
