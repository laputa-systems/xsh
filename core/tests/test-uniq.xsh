test test_uniq_counts { |ctx|
  let input = test.temp_file(ctx, name: "uniq.txt", contents: b"a\na\nb\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/uniq.xsh" -- -c $input ?
  assert "2 a" in output
  assert "1 b" in output
}
