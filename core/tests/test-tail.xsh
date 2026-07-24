proc test_tail_lines(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "lines.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tail.xsh" -- -n 2 $input ?
  test.ok(! ("one" in output))?
  test.contains(output, "two")?
  test.contains(output, "three")?
}
