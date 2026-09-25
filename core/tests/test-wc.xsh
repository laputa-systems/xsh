proc normalized_counts(output: Str) [error] -> Str {
  return output.words().join(" ")
}

proc test_wc_counts(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "words.txt", contents: b"one two\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lwc $input ?
  test.contains(normalized_counts(output), "2 3 14")?
  test.contains(output, "words.txt")?
}

proc test_wc_reads_stdin(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "stdin.txt", contents: b"one two\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lwc < ${input} ?
  test.eq(normalized_counts(output), "2 3 14")?
}

proc test_wc_counts_non_utf8_file_bytes_without_decoding(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "non-utf8.bin", contents: b"\xff\n\0x")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lc $input ?
  test.contains(normalized_counts(output), "2 4")?
  test.contains(output, "non-utf8.bin")?
}

proc test_wc_counts_non_utf8_stdin_bytes_without_decoding(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "stdin.bin", contents: b"\xff\n\0x")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lc < ${input} ?
  test.eq(normalized_counts(output), "2 4")?
}

proc test_wc_word_count_requires_utf8(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "non-utf8.bin", contents: b"\xff\n")?
  let output = run.capture --text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -w $input ?
  test.ok(output.status.exited_with(3))?
  test.contains(output.stderr, "invalid-utf8")?
}
