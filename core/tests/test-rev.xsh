test test_rev_lines_files_and_stdin { |ctx|
  let input = test.temp_file(ctx, name: "rev.txt", contents: b"abc\ncaf\xc3\xa9\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rev.xsh" -- $input ?

  assert output == """cba
éfac
"""

  let script = fp"${ctx.core_dir}/rev.xsh"

  let command = f"""printf 'one
two
' | ${ctx.xsh_bin.display()} ${script.display()}"""

  let stdin_output = run.text sh -c $command ?

  assert stdin_output == """eno
owt
"""
}

test test_rev_rejects_options { |ctx|
  let err = test.temp_path(ctx, name: "rev.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/rev.xsh" -- -z 2> $err
  assert ! status.exited_with(0)
  assert "unsupported option" in err.read_text()?
}
