type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "base64")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/base64.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_base64_binary_roundtrip { |ctx|
  assert invoke(ctx, [], b"\0\xff\xfe\0")?.stdout == b"AP/+AA==\n"
  let decoded = invoke(ctx, ["-d"], b"AP/+AA==\n")?
  assert decoded.status == 0, decoded.stderr
  assert decoded.stdout == b"\0\xff\xfe\0"
}

test test_base64_uppercase_decode_alias { |ctx|
  let decoded = invoke(ctx, ["-D"], b"aGVsbG8=\n")?
  assert decoded.status == 0, decoded.stderr
  assert decoded.stdout == b"hello"
}

test test_base64_wrap_and_invalid_prefix { |ctx|
  assert invoke(ctx, ["-w", "0"], b"foobar")?.stdout.len() > 0
  assert ! invoke(ctx, ["-w", "0"], b"foobar")?.stdout.ends_with(b"\n")
  let bad = invoke(ctx, ["-d"], b"Zm9v!")?
  assert bad.status == 1
  assert bad.stdout == b"foo"
  assert bad.stderr == "base64: invalid input\n"
}

test test_base64_bad_padded_input_preserves_decoded_prefix { |ctx|
  let bad = invoke(ctx, ["-d"], b"aGVsbG8sIHdvcmxkIQ==\0")?
  assert bad.status == 1
  assert bad.stdout == b"hello, world!"
  assert bad.stderr == "base64: invalid input\n"
}

test test_base64_wrap_without_value_uses_gnu_error { |ctx|
  for option in ["-w", "--wrap"] {
    let bad = invoke(ctx, [option])?
    assert bad.status == 1
    let diagnostic = if option == "-w" { "option requires an argument -- 'w'" } else { "option '--wrap' requires an argument" }
    assert bad.stderr == f"base64: {diagnostic}\nTry 'base64 --help' for more information.\n"
  }
}

test test_base64_padded_blocks_continue { |ctx|
  assert invoke(ctx, ["-d"], b"MTIzNA==MTIzNA")?.stdout == b"12341234"
  assert invoke(ctx, ["-d"], b"MTIzNA==QUJD")?.stdout == b"1234ABC"
}

test test_base64_reads_non_utf8_file_operand { |ctx|
  let root = test.temp_dir(ctx, name: "base64-raw-operand")?
  let file = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  file.write(b"hello world")
  let output = fp"{root}/out"
  let error = fp"{root}/err"
  let argv: List[Union[Str, Path]] = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/base64.xsh".display(), file]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", output, error)
  let status = process.run(plan)?
  assert status.exit_code()? == 0, error.read_text()?
  assert output.read_bytes()? == b"aGVsbG8gd29ybGQ=\n"
  assert error.read_text()? == ""
}

test test_base64_non_utf8_extra_operand_is_quoted_as_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "base64-raw-extra")?
  let output = fp"{root}/out"
  let error = fp"{root}/err"
  let extra = Path.parse_bytes(b"\xff")?
  let argv: List[Union[Str, Path]] = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/base64.xsh".display(), "a", extra]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", output, error)
  let status = process.run(plan)?
  assert status.exit_code()? == 1
  assert output.read_bytes()? == b""
  assert error.read_text()? == "base64: extra operand ''$'\\377'\nTry 'base64 --help' for more information.\n", error.read_text()?
}
