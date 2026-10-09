type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "basenc")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/basenc.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
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

test test_basenc_base2_and_decode { |ctx|
  let bits = invoke(ctx, ["--base2lsbf"], b"lsbf")?
  assert bits.stdout == b"00110110110011100100011001100110\n"
  let decoded = invoke(ctx, ["--base2lsbf", "-d"], bits.stdout)?
  assert decoded.stdout == b"lsbf"
}

test test_basenc_last_encoding_wins { |ctx|
  let result = invoke(ctx, ["--base32", "--base64"], b"Hello, World!")?
  assert result.stdout == b"SGVsbG8sIFdvcmxkIQ==\n"
}
