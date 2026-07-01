pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc normalized_counts(output: Str) [error] -> Str {
  return output.words().join(" ")
}

proc test_wc_counts(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "words.txt", contents: b"one two\nthree\n")?
  let output = run.text xsh_bin() wc.xsh -- -lwc $input ?
  test.contains(normalized_counts(output), "2 3 14")?
  test.contains(output, "words.txt")?
}

proc test_wc_reads_stdin() [fs, process, error] {
  let xsh = xsh_bin().resolve()?
  let script = p"wc.xsh".resolve()?
  let command = f"printf 'one two\\nthree\\n' | ${xsh.display()} ${script.display()} -- -lwc"
  let output = run.text sh -c $command ?
  test.eq(normalized_counts(output), "2 3 14")?
}
