type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "dd")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/dd.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
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
