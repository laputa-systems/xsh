type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "base32")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/base32.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_base32_encodes_and_decodes { |ctx|
  let encoded = invoke(ctx, [], b"Hello, World!")?
  assert encoded.status == 0
  assert encoded.stdout == b"JBSWY3DPFQQFO33SNRSCC===\n"
  let decoded = invoke(ctx, ["--decode"], encoded.stdout)?
  assert decoded.status == 0
  assert decoded.stdout == b"Hello, World!"
}

test test_base32_wrap_and_ignore { |ctx|
  let wrapped = invoke(ctx, ["-w10"], b"Hello, World!\n")?
  assert wrapped.stdout == b"JBSWY3DPFQ\nQFO33SNRSC\nCCQ=\n"
  let decoded = invoke(ctx, ["-di"], b"JBSWY\x013DPFQQFO33SNRSCC===\n")?
  assert decoded.status == 0
  assert decoded.stdout == b"Hello, World!"
}
