test test_fold_width { |ctx|
  let input = test.temp_file(ctx, name: "wide.txt", contents: b"abcdef\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -w 3 $input
  assert "abc" in output
  assert "def" in output
}

test test_fold_preserves_final_record_and_wraps_blanks { |ctx|
  let input = test.temp_file(ctx, name: "words", contents: b"ab cd ef")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -s -w 5 $input
  assert output == "ab \ncd ef"
}

test test_fold_unicode_display_and_character_width { |ctx|
  let input = test.temp_file(ctx, name: "wide", contents: b"\xe4\xb8\xad\xe4\xb8\xad\xe4\xb8\xad\n")?
  let columns = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -w 4 $input
  assert columns == "中中\n中\n"
  let chars = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -c -w 4 $input
  assert chars == "中中中\n"
}
