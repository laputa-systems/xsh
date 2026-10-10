test test_paste_parallel_serial_and_delimiters { |ctx|
  let left = test.temp_file(ctx, name: "left.txt", contents: b"a\nb\n")?
  let right = test.temp_file(ctx, name: "right.txt", contents: b"1\n2\n3\n")?
  let parallel = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- $left $right
  let serial = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- -s -d: $left $right

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

  let output = run.text sh -c $command

  assert output == f"""a	b
"""

  let err = test.temp_path(ctx, name: "paste.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- --definitely-invalid 2> $err
  assert ! status.exited_with(0)
  assert "unrecognized option" in err.read_text()?
}

test test_paste_cycles_delimiters_and_serial_does_not_add_final_delimiter { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"a\nb\nc")?
  let second = test.temp_file(ctx, name: "second", contents: b"1\n2\n")?
  let parallel = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- -d ,: $first $second $first
  assert parallel == "a,1:a\nb,2:b\nc,:c\n"
  let serial = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- -s -d ,: $first
  assert serial == "a,b:c\n"
}

test test_paste_unicode_and_control_delimiters { |ctx|
  let input = test.temp_file(ctx, name: "lines", contents: b"a\nb\nc\n")?
  let unicode = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- -s -d "💐" $input
  assert unicode == "a💐b💐c\n"
  let control = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- -s -d "\\r\\n" $input
  assert control == "a\rb\nc\n"
}

test test_paste_streams_a_record_before_its_terminator { |ctx|
  # Leave room for the debug interpreter's preparation stack while bounding a regressed read.
  let command = "ulimit -v 524288; timeout 10 \"$1\" \"$2\" -- /dev/zero | /usr/bin/head -c1"
  let output = test.temp_path(ctx, name: "output")
  let status = run.status sh -c $command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" > $output
  assert status.exited_with(0)
  assert output.read_bytes()? == b"\0"
}

test test_paste_accepts_non_utf8_paths_and_delimiters { |ctx|
  let root = test.temp_dir(ctx, name: "paste-raw-arguments")?
  let left = Path.parse_bytes(bytes.concat([root.bytes(), b"/left\xff"]))?
  let right = Path.parse_bytes(bytes.concat([root.bytes(), b"/right\xfe"]))?
  left.write("1\n")
  right.write("a\n")
  let output = test.temp_path(ctx, name: "output")
  let command = "delimiter=$(printf '\\255'); exec \"$1\" \"$2\" -- --delimiters=\"$delimiter\" \"$3\" \"$4\" > \"$5\""
  let status = run.status env LC_ALL=C sh -c $command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" $left $right $output
  assert status.exited_with(0)
  assert output.read_bytes()? == b"1\xada\n"
}

test test_paste_non_utf8_delimiter_in_a_gb18030_locale { |ctx|
  let root = test.temp_dir(ctx, name: "paste-non-utf8-delimiter")?
  let left = fp"{root}/f1"
  let right = fp"{root}/f2"
  left.write("1\n2\n")
  right.write("a\nb\n")
  let output = test.temp_path(ctx, name: "output")
  let command = "delimiter=$(printf '\\242\\343'); exec \"$1\" \"$2\" -- -d \"$delimiter\" \"$3\" \"$4\" > \"$5\""
  let status = run.status env LC_ALL=zh_CN.gb18030 sh -c $command sh ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" $left $right $output
  assert status.exited_with(0)
  assert output.read_bytes()? == b"1\xa2\xe3a\n2\xa2\xe3b\n"
}

test test_paste_trailing_backslash_diagnostic_matches_gnu { |ctx|
  let err = test.temp_path(ctx, name: "paste.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/paste.xsh" -- -d "\\" 2> $err
  assert ! status.exited_with(0)
  let expected = bytes.concat([bytes.from_text("paste: delimiter list ends with an unescaped backslash: "), b"\\\n"])
  assert err.read_bytes()? == expected
}
