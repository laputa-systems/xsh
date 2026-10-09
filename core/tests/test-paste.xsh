test test_paste_parallel_serial_and_delimiters { |ctx|
  let left = test.temp_file(ctx, name: "left.txt", contents: b"a\nb\n")?
  let right = test.temp_file(ctx, name: "right.txt", contents: b"1\n2\n3\n")?
  let parallel = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" $left $right ?
  let serial = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -s -d: $left $right ?

  assert parallel == f"""a	1
b	2
	3
"""

  assert serial == """a:b
1:2:3
"""
}

test test_paste_reads_stdin_and_zero_delimiters { |ctx|
  let script = fp"{ctx.core_dir}/paste.xsh"

  let command = f"""printf 'a
b
' | {ctx.xsh_bin} {script} -s"""

  let output = run.text sh -c $command ?

  assert output == f"""a	b
"""

  let left = test.temp_file(ctx, name: "zero-left", contents: b"a\0b")?
  let right = test.temp_file(ctx, name: "zero-right", contents: b"1\x002")?
  let zero = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -z $left $right ?
  assert zero == "a\t1\0b\t2\0"
}

test test_paste_delimiter_escape_and_serial_reset { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"a\nb\n")?
  let second = test.temp_file(ctx, name: "second", contents: b"c\nd\ne\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -d ":|" -s $first $second ?
  assert output == "a:b\nc:d|e\n"
}
