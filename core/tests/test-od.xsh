type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "od")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/od.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_od_default_and_hex_byte_output { |ctx|
  let default = invoke(ctx, ["--endian=little"], b"abc")?
  assert default.status == 0
  assert "0000000 061141" in (default.stdout.utf8() ?? "")
  let hex = invoke(ctx, ["-An", "-tx1"], b"abc")?
  assert hex.stdout == b"61 62 63\n"
}

test test_od_skip_limit_and_invalid_type { |ctx|
  let result = invoke(ctx, ["-An", "-j1", "-N2", "-tx1"], b"abcd")?
  assert result.stdout == b"62 63\n"
  let bad = invoke(ctx, ["-tq"], b"abc")?
  assert bad.status == 1
}

test test_od_endian_abbreviation_and_invalid_width_fallback { |ctx|
  let result = invoke(ctx, ["--endian=l", "-t", "o2", "-w5", "-v"], b"abcd")?
  assert result.status == 0
  assert result.stdout == b"0000000 061141\n0000002 062143\n0000004\n"
  assert result.stderr == "od: warning: invalid width 5; using 2 instead\n"
}

test test_od_string_and_duplicate_output { |ctx|
  let strings = invoke(ctx, ["-S0"], b"a\0b\0")?
  assert strings.stdout == b"0000000 a\n0000002 b\n"
  let duplicates = invoke(ctx, ["-tx1", "-w2"], b"\0\0\0\0")?
  assert duplicates.stdout == b"0000000 00 00\n*\n0000004\n"
}
