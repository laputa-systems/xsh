type Ran = {root: Path, status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "dd")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/dd.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({root: root, status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_dd_copy_count_skip_and_block_size { |ctx|
  assert invoke(ctx, ["status=none"], b"abcdef")?.stdout == b"abcdef"
  assert invoke(ctx, ["status=none", "bs=2", "count=2"], b"abcdef")?.stdout == b"abcd"
  assert invoke(ctx, ["status=none", "ibs=2", "skip=1", "count=2"], b"abcdef")?.stdout == b"cdef"
}

test test_dd_output_seek_and_short_input_skip { |ctx|
  assert invoke(ctx, ["status=none", "bs=2", "seek=1"], b"xy")?.stdout == b"\0\0xy"
  let skipped = invoke(ctx, ["status=noxfer", "bs=1", "skip=5", "count=0"], b"abcd")?
  assert skipped.status == 0
  assert skipped.stdout == b""
  assert skipped.stderr == "dd: 'standard input': cannot skip to specified offset\n0+0 records in\n0+0 records out\n"
}

test test_dd_swab_case_and_invalid_operand { |ctx|
  assert invoke(ctx, ["status=none", "conv=swab,ucase"], b"abcd!")?.stdout == b"BADC!"
  let bad = invoke(ctx, ["status=none", "conv=unknown"], b"abc")?
  assert bad.status == 1
}

test test_dd_block_and_unblock_records { |ctx|
  assert invoke(ctx, ["status=none", "ibs=5", "cbs=5", "conv=block"], b"012\nabcde\n")?.stdout == b"012  abcde"
  assert invoke(ctx, ["status=none", "cbs=5", "conv=unblock"], b"012  abcde")?.stdout == b"012\nabcde\n"
}

test test_dd_byte_sizes_and_numeric_diagnostics { |ctx|
  assert invoke(ctx, ["status=none", "count=3B"], b"abcdef")?.stdout == b"abc"
  assert invoke(ctx, ["status=none", "count=2Bx2"], b"abcdef")?.stdout == b"abcd"
  assert invoke(ctx, ["status=none", "skip=3B"], b"abcdef")?.stdout == b"def"
  assert invoke(ctx, ["status=none", "seek=3B"], b"abcdef")?.stdout == b"\0\0\0abcdef"

  let empty = invoke(ctx, ["count=B"], b"")?
  assert empty.status == 1
  assert empty.stderr == "dd: invalid number: 'B'\n"

  let too_large = invoke(ctx, ["skip=9223372036854775808"], b"")?
  assert too_large.status == 1
  assert too_large.stderr == "dd: invalid number: '9223372036854775808': Value too large for defined data type\n"
}

test test_dd_zero_multiplier_warning { |ctx|
  let warned = invoke(ctx, ["status=none", "count=0x0x1"], b"abc")?
  assert warned.status == 0
  assert warned.stdout == b""
  assert warned.stderr == "dd: warning: '0x' is a zero multiplier; use '00x' if that is intended\ndd: warning: '0x' is a zero multiplier; use '00x' if that is intended\n"

  let explicit_zero = invoke(ctx, ["status=none", "count=00x1"], b"abc")?
  assert explicit_zero.status == 0
  assert explicit_zero.stdout == b""
  assert explicit_zero.stderr == ""
}

test test_dd_sparse_output_and_truncated_records { |ctx|
  let sparse = invoke(ctx, ["status=none", "ibs=2", "obs=2", "conv=sparse", "of=out"], b"\0\0A\0\0\0")?
  assert sparse.status == 0
  assert fp"{sparse.root}/out".read_bytes()? == b"\0\0A\0\0\0"

  let truncated = invoke(ctx, ["status=noxfer", "cbs=1", "conv=block"], b"ab\ncd\n")?
  assert truncated.status == 0
  assert truncated.stdout == b"ac"
  assert truncated.stderr == "0+1 records in\n0+1 records out\n2 truncated records\n"
}

test test_dd_rejects_opposing_case_conversions { |ctx|
  let result = invoke(ctx, ["conv=ucase,lcase"], b"abc")?
  assert result.status == 1
  assert "ucase" in result.stderr
  assert "lcase" in result.stderr
}
