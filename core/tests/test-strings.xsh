test test_strings_min_len [fs, process, env, error] { |ctx|
  let input = test.temp_file(ctx, name: "strings.bin", contents: b"\0hello\0xy\0there\0")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/strings.xsh" -- -n 5 $input ?
  "hello" in output
  "there" in output
  ! ("xy" in output)
}
