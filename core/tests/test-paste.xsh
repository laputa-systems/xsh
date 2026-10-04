test test_paste_parallel_serial_and_delimiters { |ctx|
  let left = test.temp_file(ctx, name: "left.txt", contents: b"a\nb\n")?
  let right = test.temp_file(ctx, name: "right.txt", contents: b"1\n2\n3\n")?
  let parallel = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- $left $right ?
  let serial = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- -s -d: $left $right ?

  assert parallel == f"""a	1
b	2
	3
"""

  assert serial == """a:b
1:2:3
"""
}

test test_paste_reads_stdin_and_rejects_flags { |ctx|
  let script = fp"{ctx.core_dir}/paste.xsh"

  let command = f"""printf 'a
b
' | {ctx.xsh_bin} {script} -- -s"""

  let output = run.text sh -c $command ?

  assert output == f"""a	b
"""

  let err = test.temp_path(ctx, name: "paste.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- -z 2> $err
  assert ! status.exited_with(0)
  assert "unknown argument" in err.read_text()?
}
