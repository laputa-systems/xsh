test test_fold_width { |ctx|
  let input = test.temp_file(ctx, name: "wide.txt", contents: b"abcdef\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -w 3 $input ?
  assert output == "abc\ndef\n"
}

test test_fold_soft_break_tabs_and_utf8 { |ctx|
  let words = test.temp_file(ctx, name: "words.txt", contents: b"ab cd ef\n")?
  let soft = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -s -w 5 $words ?
  assert soft == "ab \ncd ef\n"

  let wide = test.temp_file(ctx, name: "wide-char.txt", contents: b"界界界\n")?
  let columns = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -w 5 $wide ?
  assert columns == "界界\n界\n"
}
