test test_shuf_head_count { |ctx|
  let input = test.temp_file(ctx, name: "shuf.txt", contents: b"a\nb\nc\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/shuf.xsh" -- -n 2 $input
  assert output.count_lines() == 2
}
