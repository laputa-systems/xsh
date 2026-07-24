proc test_strings_min_len(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "strings.bin", contents: b"\0hello\0xy\0there\0")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/strings.xsh" -- -n 5 $input ?
  test.contains(output, "hello")?
  test.contains(output, "there")?
  test.ok(! ("xy" in output))?
}
