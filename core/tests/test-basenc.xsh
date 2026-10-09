type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", sink: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "basenc")?
  let out = sink ?? fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/basenc.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  let stdout = if sink == null { fp"{root}/stdout".read_bytes()? } else { b"" }
  Ok({status: status.exit_code()?, stdout: stdout, stderr: err.read_text()?})
}

test test_basenc_base16_base58_and_base32hex { |ctx|
  assert invoke(ctx, ["--base16"], b"Hello, World!")?.stdout == b"48656C6C6F2C20576F726C6421\n"
  assert invoke(ctx, ["--base58"], b"Hello, World!")?.stdout == b"72k1xXWG59fYdzSNoA\n"
  assert invoke(ctx, ["--base32hex"], b"nice>base?")?.stdout == b"DPKM6P9UC9GN6P9V\n"
  assert invoke(ctx, ["--base32hex", "-d"], b"DPKM6P9UC9GN6P9V")?.stdout == b"nice>base?"
  let truncated = invoke(ctx, ["--base32hex", "-d"], b"CPNMUO")?
  assert truncated.status == 1
  assert truncated.stdout == b"foo"
  assert truncated.stderr == "basenc: error: invalid input\n"
  let bad_tail = invoke(ctx, ["--base32", "-d"], b"MFRGGZDF=")?
  assert bad_tail.status == 1
  assert bad_tail.stdout == b"abcde"
}

test test_basenc_base58_large_zero_prefix { |ctx|
  let zeros = bytes.from_ints([0 for _ in range(4096)])?
  let result = invoke(ctx, ["--base58", "--wrap=0"], zeros)?

  assert result.stdout == bytes.concat([b"1" for _ in range(4096)])
}

test test_basenc_base2_and_decode { |ctx|
  let bits = invoke(ctx, ["--base2lsbf"], b"lsbf")?
  assert bits.stdout == b"00110110110011100100011001100110\n"
  let decoded = invoke(ctx, ["--base2lsbf", "-d"], bits.stdout)?
  assert decoded.stdout == b"lsbf"
}

test test_basenc_decoder_alphabets_and_whitespace { |ctx|
  let url_garbage = invoke(ctx, ["--base64url", "-d", "-i"], b"+/SGVsbG8=/")?
  assert url_garbage.status == 0
  assert url_garbage.stdout == b"Hello"
  let url_standard_alphabet = invoke(ctx, ["--base64url", "-d"], b"SGVsbG8+/")?
  assert url_standard_alphabet.status == 1
  assert url_standard_alphabet.stderr == "basenc: error: invalid input\n"

  let hex_garbage = invoke(ctx, ["--base32hex", "-d", "-i"], b"WCPNMU===")?
  assert hex_garbage.status == 0
  assert hex_garbage.stdout == b"foo"
  let hex_outside_alphabet = invoke(ctx, ["--base32hex", "-d"], b"WCPNMU===")?
  assert hex_outside_alphabet.status == 1
  assert hex_outside_alphabet.stderr == "basenc: error: invalid input\n"

  let base58_whitespace = invoke(ctx, ["--base58", "-d"], b"72k1xXWG59fYdzSNoA ")?
  assert base58_whitespace.status == 1
  assert base58_whitespace.stderr == "basenc: error: invalid input\n"
  let base58_newline = invoke(ctx, ["--base58", "-d"], b"2NEpo7TZRRrLZSi2U\n")?
  assert base58_newline.status == 0
  assert base58_newline.stdout == b"Hello World!"
  let z85_newlines = invoke(ctx, ["--z85", "-d"], b"he\nl\nlo")?
  assert z85_newlines.status == 0
  assert z85_newlines.stdout == b"5jXu"

  let hex_bad_tail = invoke(ctx, ["--base32hex", "-d"], b"VNC0FKD5W")?
  assert hex_bad_tail.status == 1
  assert hex_bad_tail.stdout == b"\xfd\xd8\x07\xd1\xa5"

  let mixed_padding = invoke(ctx, ["--base64", "-d"], b"QWI=\nQQ")?
  assert mixed_padding.status == 0
  assert mixed_padding.stdout == b"AbA"
}

test test_basenc_last_encoding_wins { |ctx|
  let result = invoke(ctx, ["--base32", "--base64"], b"Hello, World!")?
  assert result.stdout == b"SGVsbG8sIFdvcmxkIQ==\n"
}

test test_basenc_reports_write_error_without_runtime_prefix { |ctx|
  if ! p"/dev/full".exists()? { test.skip("/dev/full is not available") }
  let result = invoke(ctx, ["--base16"], b"Hello, World!", sink: p"/dev/full")?
  assert result.status == 1
  assert result.stderr == "basenc: No space left on device\n"
}
