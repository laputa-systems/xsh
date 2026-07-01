pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_head_lines(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "lines.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text xsh_bin() head.xsh -- -n2 $input ?
  test.contains(output, "one")?
  test.contains(output, "two")?
  test.ok(! ("three" in output))?
}

proc test_head_reads_stdin() [fs, process, error] {
  let xsh = xsh_bin().resolve()?
  let script = p"head.xsh".resolve()?

  let command = f"""printf 'one
two
three
' | ${xsh.display()} ${script.display()} -- -n2"""

  let output = run.text sh -c $command ?
  test.contains(output, "one")?
  test.contains(output, "two")?
  test.ok(! ("three" in output))?
}
