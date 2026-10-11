type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", sink: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "basenc")?
  let out = sink ?? fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/basenc.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: if sink == null { out.read_bytes()? } else { b"" }, stderr: err.read_text()?})
}

test test_basenc_all_binary_encodings { |ctx|
  for kind in ["base64", "base64url", "base32", "base32hex", "base16", "base2msbf", "base2lsbf", "z85", "base58"] {
    let encoded = invoke(ctx, [f"--{kind}"], b"\0\xff\xfe\0")?
    assert encoded.status == 0, encoded.stderr
    let decoded = invoke(ctx, [f"--{kind}", "-d"], encoded.stdout)?
    assert decoded.status == 0, decoded.stderr
    assert decoded.stdout == b"\0\xff\xfe\0", kind
  }
}

test test_basenc_last_selector_and_garbage { |ctx|
  assert invoke(ctx, ["--base32", "--base16"], b"foo")?.stdout == b"666F6F\n"
  assert invoke(ctx, ["--base16", "-di"], b"66!6f6F")?.stdout == b"foo"
  let bad = invoke(ctx, ["--z85"], b"123")?
  assert bad.status == 1
  assert bad.stderr == "basenc: invalid input (length must be multiple of 4 characters)\n"
}

test test_basenc_decode_error_preserves_prefix { |ctx|
  let bad = invoke(ctx, ["--base32", "-d"], b"MZXW6===!")?
  assert bad.status == 1
  assert bad.stdout == b"foo"
  assert bad.stderr == "basenc: invalid input\n"
}

test test_basenc_base16_reports_write_error { |ctx|
  if ! p"/dev/full".exists() { test.skip("/dev/full is not available"); return }
  let result = invoke(ctx, ["--base16"], b"Hello, World!", sink: /dev/full)?
  assert result.status == 1
  assert result.stderr == "basenc: write error: No space left on device\n"
}

test test_basenc_reads_non_utf8_file_operand { |ctx|
  let root = test.temp_dir(ctx, name: "basenc-raw-operand")?
  let file = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  file.write(b"foo")
  let output = fp"{root}/out"
  let error = fp"{root}/err"
  let argv: List[Union[Str, Path]] = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/basenc.xsh".display(), file, "--base64"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", output, error)
  let status = process.run(plan)?
  assert status.exit_code()? == 0, error.read_text()?
  assert output.read_bytes()? == b"Zm9v\n"
  assert error.read_text()? == ""
}

test test_basenc_wrap_width_is_decimal { |ctx|
  let hex = invoke(ctx, ["--base64", "-w", "0x0"], b"foo")?
  assert hex.status == 1
  assert hex.stdout == b""
  assert hex.stderr == "basenc: invalid wrap size: '0x0'\n", hex.stderr
  assert invoke(ctx, ["--base64", "-w", "0"], b"foo")?.stdout == b"Zm9v"
}

test test_basenc_base64url_rejects_standard_alphabet_before_output { |ctx|
  let slash = invoke(ctx, ["--base64url", "-d"], b"VA/c8A+vSg==")?
  assert slash.status == 1
  assert slash.stdout == b"", "GNU writes nothing for a base64url block with '/'"
  assert slash.stderr == "basenc: invalid input\n", slash.stderr
}

test test_basenc_base58_invalid_byte_outputs_nothing { |ctx|
  let trailing = invoke(ctx, ["--base58", "-d"], b"2NEpo7TZRRrLZSi2U ")?
  assert trailing.status == 1
  assert trailing.stdout == b"", "base58 converts only after the whole input is valid"
}

test test_basenc_nonzero_padding_bits_are_invalid { |ctx|
  let base64 = invoke(ctx, ["--base64", "-d"], b"SB==")?
  assert base64.status == 1
  assert base64.stdout == b"H", "GNU writes the bytes decoded before the invalid final symbol"
  assert base64.stderr == "basenc: invalid input\n", base64.stderr

  let base32 = invoke(ctx, ["--base32", "-d"], b"MZXW5===")?
  assert base32.status == 1
  assert base32.stdout == b"fon"
  assert base32.stderr == "basenc: invalid input\n", base32.stderr
  assert invoke(ctx, ["--base32", "-d"], b"MZXW4===")?.stdout == b"fon"
}

test test_basenc_base58_large_zero_input_preserves_default_wrapping { |ctx|
  let root = test.temp_dir(ctx, name: "basenc-large-zeros")?
  let input = fp"{root}/zeros"
  input.write(b"")
  let size = 20 * 1024 * 1024
  input.truncate(size)
  let output = fp"{root}/encoded"
  let result = invoke(ctx, ["--base58", input.display()], sink: output)?
  assert result.status == 0, result.stderr
  assert result.stderr == ""
  let encoded = output.read_bytes()?
  assert encoded.len() == size + (size + 75) / 76
  let line = bytes.from_text(["1" for _ in range(76)].join("") + "\n")
  let lines = size / 76
  assert bytes.repeat_prefix_count(encoded, line)? == lines
  let tail = bytes.from_text(["1" for _ in range(size % 76)].join("") + "\n")
  assert encoded[lines * 77..] == tail
}

test test_basenc_base58_leading_zeros_keep_nonzero_suffix { |ctx|
  for example in [
    {input: b"", encoded: b""},
    {input: b"\0", encoded: b"1"},
    {input: b"\0\0\x01\0", encoded: b"115R"},
    {input: b"\x01\0", encoded: b"5R"},
  ] {
    let result = invoke(ctx, ["--base58", "-w0"], example.input)?
    assert result.status == 0, result.stderr
    assert result.stdout == example.encoded
  }
}
