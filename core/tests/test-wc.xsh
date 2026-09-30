proc normalized_counts(output: Str) [error] -> Str {
  return output.words().join(" ")
}

test test_wc_counts [fs, process, env, error] { |ctx|
  let input = test.temp_file(ctx, name: "words.txt", contents: b"one two\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lwc $input ?
  "2 3 14" in normalized_counts(output)
  "words.txt" in output
}

test test_wc_reads_stdin [fs, process, env, error] { |ctx|
  let input = test.temp_file(ctx, name: "stdin.txt", contents: b"one two\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lwc < ${input} ?
  normalized_counts(output) == "2 3 14"
}

test test_wc_counts_non_utf8_file_bytes_without_decoding [fs, process, env, error] { |ctx|
  let input = test.temp_file(ctx, name: "non-utf8.bin", contents: b"\xff\n\0x")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lc $input ?
  "2 4" in normalized_counts(output)
  "non-utf8.bin" in output
}

test test_wc_counts_non_utf8_stdin_bytes_without_decoding [fs, process, env, error] { |ctx|
  let input = test.temp_file(ctx, name: "stdin.bin", contents: b"\xff\n\0x")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lc < ${input} ?
  normalized_counts(output) == "2 4"
}

test test_wc_word_count_requires_utf8 [fs, process, env, error] { |ctx|
  let input = test.temp_file(ctx, name: "non-utf8.bin", contents: b"\xff\n")?
  let output = run.capture --text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -w $input ?
  output.status.exited_with(3)
  "invalid-utf8" in output.stderr
}
