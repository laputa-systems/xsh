type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", timeout = 3s) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "dd")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err, timeout:))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_dd_binary_counts_and_conversions { |ctx|
  assert invoke(ctx, ["status=none", "ibs=2", "skip=1", "count=2"], b"ab\0\xffcdEF")?.stdout == b"\0\xffcd"
  assert invoke(ctx, ["status=none", "ibs=3", "conv=swab"], b"abcdefg")?.stdout == b"bacedfg"
  assert invoke(ctx, ["status=none", "ibs=4", "conv=sync"], b"abcde")?.stdout == b"abcde\0\0\0"
  assert invoke(ctx, ["status=none", "cbs=4", "conv=block,ucase"], b"ab\nabcdef\n")?.stdout == b"AB  ABCD"
  assert invoke(ctx, ["status=none", "cbs=4", "conv=unblock"], b"ab  cd  ")?.stdout == b"ab\ncd\n"
}

test test_dd_count_bytes_limits_records_by_byte_count { |ctx|
  let result = invoke(ctx, ["status=noxfer", "bs=2", "count=3", "oflag=count_bytes"], b"abcdef")?
  assert result.status == 0, result.stderr
  assert result.stdout == b"abc"
  assert result.stderr == "1+1 records in\n1+1 records out\n"
}

test test_dd_warns_about_zero_multipliers { |ctx|
  let result = invoke(ctx, ["status=none", "count=0x1"])?
  assert result.status == 0, result.stderr
  assert result.stdout == b""
  assert result.stderr == "dd: warning: '0x' is a zero multiplier; use '00x' if that is intended\n"
}

test test_dd_seek_and_notrunc { |ctx|
  let file = test.temp_file(ctx, name: "dd-file", contents: b"abcdefgh")?
  let result = invoke(ctx, ["status=none", f"of={file}", "bs=2", "seek=1", "conv=notrunc"], b"XY")?
  assert result.status == 0, result.stderr
  assert file.read_bytes()? == b"abXYefgh"
  assert invoke(ctx, ["status=none", f"of={file}", "bs=2", "seek=1"], b"ZZ")?.status == 0
  assert file.read_bytes()? == b"abZZ"
}

test test_dd_seek_bytes_on_stdout { |ctx|
  let result = invoke(ctx, ["status=none", "seek=3", "oflag=seek_bytes"], b"abc")?
  assert result.status == 0, result.stderr
  assert result.stdout == b"\0\0\0abc"
  let empty = invoke(ctx, ["status=none", "seek=8", "oflag=seek_bytes", "count=0"])?
  assert empty.status == 0, empty.stderr
  assert empty.stdout == b"\0\0\0\0\0\0\0\0"
}

test test_dd_seek_bytes_on_regular_output { |ctx|
  let file = test.temp_file(ctx, name: "dd-byte-seek", contents: b"old contents")?
  let result = invoke(ctx, ["status=none", f"of={file}", "seek=3", "oflag=seek_bytes"], b"abc")?
  assert result.status == 0, result.stderr
  assert file.read_bytes()? == b"oldabc"

  let empty = invoke(ctx, ["status=none", f"of={file}", "seek=4", "oflag=seek_bytes", "count=0"])?
  assert empty.status == 0, empty.stderr
  assert file.read_bytes()? == b"olda"
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

test test_dd_rejects_overflowing_block_offsets { |ctx|
  let seek = invoke(ctx, ["seek=17592186044416", "obs=1048576"])?
  assert seek.status == 1
  assert seek.stderr == "dd: Value too large for defined data type\n"

  let skip = invoke(ctx, ["skip=17592186044416", "ibs=1048576"])?
  assert skip.status == 1
  assert skip.stderr == "dd: Value too large for defined data type\n"
}

test test_dd_skip_past_input_warns_without_failing { |ctx|
  let result = invoke(ctx, ["bs=1", "skip=5", "count=0", "status=noxfer"], b"abcd")?
  assert result.status == 0, result.stderr
  assert result.stdout == b""
  assert result.stderr == "dd: 'standard input': cannot skip to specified offset\n0+0 records in\n0+0 records out\n"
}

test test_dd_zero_count_skips_an_input_fifo { |ctx|
  let root = test.temp_dir(ctx, name: "dd-skip-fifo")?
  let fifo = fp"{root}/fifo"
  fs.mkfifo(fifo, 0o600)
  let payload = fp"{root}/payload"
  payload.write(bytes.zero(512)?)
  let writer_script = fp"{root}/writer.xsh"
  writer_script.write(f"fp\"{fifo}\".write(fp\"{payload}\".read_bytes()?)\n")
  let writer = spawn run timeout -s KILL 2 ${ctx.xsh_bin} $writer_script ?
  defer writer.cancel(kill_after: 100ms)

  let result = invoke(ctx, ["status=noxfer", f"if={fifo}", "skip=1", "count=0"])?
  assert result.status == 0, result.stderr
  assert result.stdout == b""
  assert result.stderr == "0+0 records in\n0+0 records out\n"
  assert (wait writer?).exited_with(0)
}

test test_dd_zero_count_opens_an_output_fifo { |ctx|
  let root = test.temp_dir(ctx, name: "dd-seek-fifo")?
  let fifo = fp"{root}/fifo"
  let received = fp"{root}/received"
  fs.mkfifo(fifo, 0o600)
  let reader_script = fp"{root}/reader.xsh"
  reader_script.write(f"let data = fp\"{fifo}\".read_bytes()?\nfp\"{received}\".write(data)\n")
  let reader = spawn run timeout -s KILL 2 ${ctx.xsh_bin} $reader_script ?
  defer reader.cancel(kill_after: 100ms)

  let result = invoke(ctx, ["status=noxfer", f"of={fifo}", "seek=1", "count=0"])?
  assert result.status == 0, result.stderr
  assert result.stdout == b""
  assert result.stderr == "0+0 records in\n0+0 records out\n"
  assert (wait reader?).exited_with(0)
  assert received.read_bytes()? == b""
}

test test_dd_streams_output_records_to_a_fifo { |ctx|
  let root = test.temp_dir(ctx, name: "dd-write-fifo")?
  let fifo = fp"{root}/fifo"
  let received = fp"{root}/received"
  fs.mkfifo(fifo, 0o600)
  let reader_script = fp"{root}/reader.xsh"
  reader_script.write(f"let data = fp\"{fifo}\".read_bytes()?\nfp\"{received}\".write(data)\n")
  let reader = spawn run timeout -s KILL 2 ${ctx.xsh_bin} $reader_script ?
  defer reader.cancel(kill_after: 100ms)

  let result = invoke(ctx, ["status=none", f"of={fifo}", "bs=3"], b"payload")?
  assert result.status == 0, result.stderr
  assert (wait reader?).exited_with(0)
  assert received.read_bytes()? == b"payload"
}

test test_dd_sync_pads_a_partial_fifo_record { |ctx|
  let root = test.temp_dir(ctx, name: "dd-sync-fifo")?
  let fifo = fp"{root}/fifo"
  fs.mkfifo(fifo, 0o600)
  let writer_script = fp"{root}/writer.xsh"
  writer_script.write(f"fp\"{fifo}\".write(b\"abcdefgh\")\n")
  let writer = spawn run timeout -s KILL 2 ${ctx.xsh_bin} $writer_script ?
  defer writer.cancel(kill_after: 100ms)

  let result = invoke(ctx, ["status=none", f"if={fifo}", "ibs=16", "conv=sync"])?
  let padded = bytes.concat([b"abcdefgh", bytes.zero(8)?])
  assert result.status == 0, result.stderr
  assert result.stdout == padded
  assert result.stderr == ""
  assert (wait writer?).exited_with(0)
}
