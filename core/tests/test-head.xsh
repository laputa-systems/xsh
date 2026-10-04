test test_head_lines { |ctx|
  let input = test.temp_file(ctx, name: "lines.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/head.xsh" -- -n2 $input ?
  assert "one" in output
  assert "two" in output
  assert ! ("three" in output)
}

test test_head_reads_stdin { |ctx|
  let input = test.temp_file(ctx, name: "stdin.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/head.xsh" -- -n2 < ${input} ?
  assert "one" in output
  assert "two" in output
  assert ! ("three" in output)
}
