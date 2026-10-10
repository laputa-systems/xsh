type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "base32")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/base32.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_base32_binary_roundtrip { |ctx|
  assert invoke(ctx, [], b"\0\xff\xfe\0")?.stdout == b"AD774AA=\n"
  let decoded = invoke(ctx, ["-d"], b"AD774AA=\n")?
  assert decoded.status == 0, decoded.stderr
  assert decoded.stdout == b"\0\xff\xfe\0"
}

test test_base32_uppercase_decode_alias { |ctx|
  let decoded = invoke(ctx, ["-D"], b"MZXW6===\n")?
  assert decoded.status == 0, decoded.stderr
  assert decoded.stdout == b"foo"
}

test test_base32_wrap_and_invalid_prefix { |ctx|
  assert invoke(ctx, ["-w", "0"], b"foobar")?.stdout.len() > 0
  assert ! invoke(ctx, ["-w", "0"], b"foobar")?.stdout.ends_with(b"\n")
  let bad = invoke(ctx, ["-d"], b"MZXW6===!")?
  assert bad.status == 1
  assert bad.stdout == b"foo"
  assert bad.stderr == "base32: error: invalid input\n"
}

test test_base32_wrap_without_value_uses_cli_error { |ctx|
  for option in ["-w", "--wrap"] {
    let bad = invoke(ctx, [option])?
    assert bad.status == 1
    assert bad.stderr == "base32: error: a value is required for '--wrap <COLS>' but none was supplied\nFor more information, try '--help'.\n"
  }
}

test test_base32_reads_non_utf8_file_operand { |ctx|
  let root = test.temp_dir(ctx, name: "base32-raw-operand")?
  let file = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  file.write(b"foo")
  let output = fp"{root}/out"
  let error = fp"{root}/err"
  let argv: List[Union[Str, Path]] = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/base32.xsh".display(), file]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", output, error)
  let status = process.run(plan)?
  assert status.exit_code()? == 0, error.read_text()?
  assert output.read_bytes()? == b"MZXW6===\n"
  assert error.read_text()? == ""
}
