type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "dd")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_dd_binary_counts_and_conversions { |ctx|
  assert invoke(ctx, ["status=none", "ibs=2", "skip=1", "count=2"], b"ab\0\xffcdEF")?.stdout == b"\0\xffcd"
  assert invoke(ctx, ["status=none", "ibs=3", "conv=swab"], b"abcdefg")?.stdout == b"bacedfg"
  assert invoke(ctx, ["status=none", "ibs=4", "conv=sync"], b"abcde")?.stdout == b"abcde\0\0\0"
  assert invoke(ctx, ["status=none", "cbs=4", "conv=block,ucase"], b"ab\nabcdef\n")?.stdout == b"AB  ABCD"
  assert invoke(ctx, ["status=none", "cbs=4", "conv=unblock"], b"ab  cd  ")?.stdout == b"ab\ncd\n"
}

test test_dd_seek_and_notrunc { |ctx|
  let file = test.temp_file(ctx, name: "dd-file", contents: b"abcdefgh")?
  let result = invoke(ctx, ["status=none", f"of={file}", "bs=2", "seek=1", "conv=notrunc"], b"XY")?
  assert result.status == 0, result.stderr
  assert file.read_bytes()? == b"abXYefgh"
  assert invoke(ctx, ["status=none", f"of={file}", "bs=2", "seek=1"], b"ZZ")?.status == 0
  assert file.read_bytes()? == b"abZZ"
}

test test_dd_unsupported_flags_fail_explicitly { |ctx|
  let result = invoke(ctx, ["iflag=direct"])?
  assert result.status == 1
  assert "unsupported" in result.stderr
}

test test_dd_character_set_roundtrip { |ctx|
  let original = b"Hello, World!\0\xff"
  for direction in ["ebcdic", "ibm"] {
    let encoded = invoke(ctx, ["status=none", f"conv={direction}"], original)?
    assert encoded.status == 0, encoded.stderr
    let decoded = invoke(ctx, ["status=none", "conv=ascii"], encoded.stdout)?
    assert decoded.status == 0, decoded.stderr
    assert decoded.stdout == original
  }
  assert invoke(ctx, ["status=none", "conv=ebcdic,ucase"], b"a")?.stdout == b"\xc1"
}

test test_dd_regular_file_copy_and_records { |ctx|
  let file = test.temp_file(ctx, name: "dd-source", contents: b"abcdef")?
  let out = test.temp_file(ctx, name: "dd-dest", contents: b"old")?
  let copied = invoke(ctx, [f"if={file}", f"of={out}", "ibs=2", "obs=3", "status=noxfer"])?
  assert copied.status == 0, copied.stderr
  assert out.read_bytes()? == b"abcdef"
  assert copied.stderr == "3+0 records in\n2+0 records out\n"
  assert invoke(ctx, ["bs=2", "ibs=3", "count=1", "status=none"], b"abcd")?.stdout == b"ab"
}

test test_dd_invalid_numbers_and_flags_use_gnu_messages { |ctx|
  assert invoke(ctx, ["bs=0"])?.stderr == "dd: invalid number: '0'\n"
  let bad = invoke(ctx, ["iflag="])?
  assert bad.stderr.starts_with("dd: invalid input flag: ''\n")
  assert invoke(ctx, ["status=none", "count=2Bx2"], b"abcdef")?.stdout == b"abcd"
}

test test_dd_skip_past_input_warns_without_failing { |ctx|
  let result = invoke(ctx, ["bs=1", "skip=5", "count=0", "status=noxfer"], b"abcd")?
  assert result.status == 0, result.stderr
  assert result.stdout == b""
  assert result.stderr == "dd: 'standard input': cannot skip to specified offset\n0+0 records in\n0+0 records out\n"
}
