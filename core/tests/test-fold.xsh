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
  let columns = run.text LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -w 4 $input
  assert columns == "中中\n中\n"
  let chars = run.text LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -c -w 4 $input
  assert chars == "中中中\n"
}

test test_fold_starts_columns_again_for_each_unterminated_file { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"a")?
  let second = test.temp_file(ctx, name: "second", contents: b"b")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -w1 $first $second
  assert output == "ab"
}

test test_fold_preserves_a_character_split_between_input_chunks { |ctx|
  let prefix = ["a" for _ in range(65535)].join("")
  let input = test.temp_file(ctx, name: "unicode", contents: bytes.concat([bytes.from_text(prefix), b"\xe4\xb8\xadb\n"]))?
  let output = run.text LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -c -w65536 $input
  assert output == prefix + "中\nb\n"
}


test test_fold_c_locale_counts_unicode_bytes { |ctx|
  let input = test.temp_file(ctx, name: "unicode-bytes", contents: b"\xe4\xb8\xad\xe4\xb8\xad\n")?
  let output = run.capture --bytes LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -w2 $input
  assert output.status.exited_with(0)
  assert output.stdout == b"\xe4\xb8\n\xad\xe4\n\xb8\xad\n"
}

test test_fold_unicode_blank_wrap { |ctx|
  let input = test.temp_file(ctx, name: "unicode-blanks", contents: b"abcdefghijklmnop\xe2\x80\x82qrstuvwxyz\n")?
  let output = run.text LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -sw10 $input
  assert output == "abcdefghij\nklmnop\u{2002}\nqrstuvwxyz\n"
}


test test_fold_zero_width_buffer_flush_preserves_columns { |ctx|
  let zeroes = bytes.from_ints([0 for _ in range(262144)])?
  let data = bytes.concat([b"a ", zeroes, b"bcde"])
  let input = test.temp_file(ctx, name: "zero-width", contents: data)?
  let output = run.capture --bytes LC_ALL=C.UTF-8 ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -- -sw4 $input
  assert output.status.exited_with(0)
  assert output.stdout == bytes.concat([b"a ", zeroes, b"bc\nde"])
}
