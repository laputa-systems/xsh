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

test test_paste_repeated_stdin_operands_share_the_stream { |ctx|
  let script = fp"{ctx.core_dir}/paste.xsh"
  let parallel = run.text sh -c f"printf 'a\\nb\\nc\\nd\\n' | {ctx.xsh_bin} {script} - -" ?
  assert parallel == f"a\tb\nc\td\n"

  let serial = run.text sh -c f"printf 'a\\nb\\n' | {ctx.xsh_bin} {script} -s - - -" ?
  assert serial == f"a\tb\n\n\n"
}

type PasteStreamRun = {status: Int, stderr: Str}

proc paste_dev_zero_run(ctx: TestContext, sink: Path) [fs, process, error] -> Result[PasteStreamRun] {
  let root = test.temp_dir(ctx, name: "paste-dev-zero")?
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/paste.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "/dev/zero"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", sink, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stderr: err.read_text()?})
}

test test_paste_streams_dev_zero_to_full_and_reports_write_error { |ctx|
  if ! p"/dev/zero".exists()? or ! p"/dev/full".exists()? {
    test.skip("/dev/zero and /dev/full are required")
  }

  let result = paste_dev_zero_run(ctx, p"/dev/full")?
  assert result.status == 1
  assert result.stderr == "paste: write error: No space left on device\n", result.stderr
}

test test_paste_streams_dev_zero_to_closed_pipe_silently { |ctx|
  if ! p"/dev/zero".exists()? { test.skip("/dev/zero is required") }

  let root = test.temp_dir(ctx, name: "paste-broken-pipe")?
  let script = fp"{ctx.core_dir}/paste.xsh"
  let status_file = fp"{root}/status"
  let err = fp"{root}/stderr"
  const pipeline = """{
  "$1" "$2" /dev/zero
  printf '%s\\n' "$?" > "$3"
} | head -c 0 >/dev/null"""
  let argv = ["sh", "-c", pipeline, "sh", ctx.xsh_bin.display(), script.display(), status_file.display()]
  let plan = process.command_argv("sh", argv, root, {LC_ALL: "C"}, b"", fp"{root}/stdout", err)
  let _ = process.run(plan)?
  let status = status_file.read_text()?.trim().parse_int() ?? -1

  assert status == 141, f"expected SIGPIPE status 141, got {status}"
  assert err.read_text()? == ""
}
