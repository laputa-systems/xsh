type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "basenc")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/basenc.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
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
  assert invoke(ctx, ["--z85"], b"123")?.status == 1
}
