test test_tail_lines { |ctx|
  let input = test.temp_file(ctx, name: "lines.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tail.xsh" -- -n 2 $input
  assert ! ("one" in output)
  assert "two" in output
  assert "three" in output
}
