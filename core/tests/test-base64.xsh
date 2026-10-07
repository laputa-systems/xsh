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
  assert bad.stderr == "base64: error: invalid input\n"
}

test test_base64_bad_padded_input_writes_nothing { |ctx|
  let bad = invoke(ctx, ["-d"], b"aGVsbG8sIHdvcmxkIQ==\0")?
  assert bad.status == 1
  assert bad.stdout == b""
  assert bad.stderr == "base64: error: invalid input\n"
}

test test_base64_wrap_without_value_uses_cli_error { |ctx|
  for option in ["-w", "--wrap"] {
    let bad = invoke(ctx, [option])?
    assert bad.status == 1
    assert bad.stderr == "base64: error: a value is required for '--wrap <COLS>' but none was supplied\nFor more information, try '--help'.\n"
  }
}

test test_base64_padded_blocks_continue { |ctx|
  assert invoke(ctx, ["-d"], b"MTIzNA==MTIzNA")?.stdout == b"12341234"
  assert invoke(ctx, ["-d"], b"MTIzNA==QUJD")?.stdout == b"1234ABC"
}
