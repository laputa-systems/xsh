test test_fold_width [fs, process, env, error] { |ctx|
  let input = test.temp_file(ctx, name: "wide.txt", contents: b"abcdef\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/fold.xsh" -- -w 3 $input ?
  "abc" in output
  "def" in output
}
