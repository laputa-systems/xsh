type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "base64")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/base64.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_base64_encodes_and_decodes { |ctx|
  let encoded = invoke(ctx, [], b"hello, world!")?
  assert encoded.status == 0
  assert encoded.stdout == b"aGVsbG8sIHdvcmxkIQ==\n"
  let decoded = invoke(ctx, ["-d"], b"aQ")?
  assert decoded.status == 0
  assert decoded.stdout == b"i"
}

test test_base64_wrapping_and_invalid_data { |ctx|
  let wrapped = invoke(ctx, ["-w10"], b"The quick brown fox jumps over the lazy dog.")?
  assert wrapped.stdout == b"VGhlIHF1aW\nNrIGJyb3du\nIGZveCBqdW\n1wcyBvdmVy\nIHRoZSBsYX\np5IGRvZy4=\n"
  let bad = invoke(ctx, ["-d"], b"%%%")?
  assert bad.status == 1
}

test test_base64_wrap_requires_a_width { |ctx|
  let short = invoke(ctx, ["-w"], b"input")?
  assert short.status == 1
  assert short.stdout == b""
  assert short.stderr == "base64: option requires an argument -- 'w'\nTry 'base64 --help' for more information.\n"

  let long = invoke(ctx, ["--wrap"], b"input")?
  assert long.status == 1
  assert long.stdout == b""
  assert long.stderr == "base64: option '--wrap' requires an argument\nTry 'base64 --help' for more information.\n"
}

test test_base64_rejects_non_decimal_wrap_widths { |ctx|
  let leading_zero = invoke(ctx, ["--wrap", "08"], b"a")?
  assert leading_zero.status == 0
  assert leading_zero.stdout == b"YQ==\n"

  let hex = invoke(ctx, ["-w0x0"], b"")?
  assert hex.status == 1
  assert hex.stderr == "base64: invalid wrap size: '0x0'\n"

  let suffix = invoke(ctx, ["--wrap=1k"], b"")?
  assert suffix.status == 1
  assert suffix.stderr == "base64: invalid wrap size: '1k'\n"

  let negative = invoke(ctx, ["-w-1"], b"")?
  assert negative.status == 1
  assert negative.stderr == "base64: invalid wrap size: '-1'\n"
}
