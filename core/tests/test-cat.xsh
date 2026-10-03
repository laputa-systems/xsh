test test_cat_file_and_stdin { |ctx|
  let input = test.temp_file(ctx, name: "input.txt", contents: b"file\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/cat.xsh" -- $input ?

  output == """file
"""

  let stdin = test.temp_file(ctx, name: "stdin.txt", contents: b"stdin\n")?
  let stdin_output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/cat.xsh" < ${stdin} ?

  stdin_output == """stdin
"""
}

test test_cat_preserves_non_utf8_file_and_stdin_bytes { |ctx|
  let input = test.temp_file(ctx, name: "binary.dat", contents: b"\0\xff\n")?
  (run.bytes ${ctx.xsh_bin} fp"${ctx.core_dir}/cat.xsh" -- $input?) == b"\0\xff\n"
  (run.bytes ${ctx.xsh_bin} fp"${ctx.core_dir}/cat.xsh" < ${input}?) == b"\0\xff\n"
  (run.bytes ${ctx.xsh_bin} fp"${ctx.core_dir}/cat.xsh" -- $input - < ${input}?) == b"\0\xff\n\0\xff\n"
}
