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
