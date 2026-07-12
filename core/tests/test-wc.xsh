proc xsh_bin() [env] -> Path {
  let bin = env.get("CARGO_BIN_EXE_xsh") ?? ""

  if bin != "" {
    return fp"${bin}"
  }

  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = env.get("XSH_CORE_DIR") ?? ""

  if dir != "" {
    return fp"${dir}/${name}"
  }

  return ../name
}

proc normalized_counts(output: Str) [error] -> Str {
  return output.words().join(" ")
}

proc test_wc_counts(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "words.txt", contents: b"one two\nthree\n")?
  let output = run.text xsh_bin() core_script("wc.xsh") -- -lwc $input ?
  test.contains(normalized_counts(output), "2 3 14")?
  test.contains(output, "words.txt")?
}

proc test_wc_reads_stdin() [fs, process, env, error] {
  let xsh = xsh_bin().resolve()?
  let script = core_script("wc.xsh").resolve()?
  let command = f"printf 'one two\\nthree\\n' | ${xsh.display()} ${script.display()} -- -lwc"
  let output = run.text sh -c $command ?
  test.eq(normalized_counts(output), "2 3 14")?
}
