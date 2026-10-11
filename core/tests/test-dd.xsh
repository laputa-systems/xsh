type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", timeout = 3s) [fs, process, error] -> Result[Ran] {
  invoke_with_env(ctx, args, {LC_ALL: "C"}, input, timeout:)
}

proc invoke_with_env(ctx: TestContext, args: List[Str], variables: Record, input = b"", timeout = 3s) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "dd")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, variables, input, out, err, timeout:))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

proc invoke_with_path_args(ctx: TestContext, args: List[Union[Path, Str]], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "dd-path-args")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/dd.xsh"
  let argv: List[Union[Path, Str]] = [ctx.xsh_bin, script, @args]
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_dd_forced_rich_diagnostics_point_at_operands { |ctx|
  let unknown = invoke_with_env(ctx, ["bsx=1"], {LC_ALL: "C", UUTILS_DIAG: "always"})?
  assert unknown.status == 1, unknown.stderr
  assert unknown.stderr == "dd: unrecognized operand 'bsx=1'\n   ╭─[ dd:1:4 ]\n   │\n 1 │ dd bsx=1\n   │    ───\n   │\n   │ Help: an operand is KEY=VALUE, as in if=file bs=4k count=10\n───╯\nTry 'dd --help' for more information.\n", unknown.stderr

  let conversion = invoke_with_env(ctx, ["conv=ucase,zap"], {LC_ALL: "C", UUTILS_DIAG: "always"})?
  assert conversion.status == 1
  assert "dd:1:15" in conversion.stderr, conversion.stderr
  assert "not a known conversion" in conversion.stderr, conversion.stderr
  assert "conv= is one of ascii" in conversion.stderr, conversion.stderr

  let input_flag = invoke_with_env(ctx, ["iflag=fullblock,nope"], {LC_ALL: "C", UUTILS_DIAG: "always"})?
  assert input_flag.status == 1
  assert "dd: invalid input flag: 'nope'" in input_flag.stderr, input_flag.stderr
  assert "dd:1:20" in input_flag.stderr, input_flag.stderr

  let count = invoke_with_env(ctx, ["count=8x"], {LC_ALL: "C", UUTILS_DIAG: "always"})?
  assert count.status == 1
  assert "dd:1:10" in count.stderr, count.stderr
  assert "a number may be followed by a multiplier" in count.stderr, count.stderr
}

test test_dd_reads_and_writes_non_utf8_operand_paths { |ctx|
  let root = test.temp_dir(ctx, name: "dd-non-utf8")?
  let input = Path.parse_bytes(bytes.concat([root.bytes(), b"/in-\xff\xfe"]))?
  let output = Path.parse_bytes(bytes.concat([root.bytes(), b"/out-\xff\xfe"]))?
  let contents = b"dd accepts path operands whose bytes are not UTF-8\n"
  input.write(contents)
  let args: List[Union[Path, Str]] = ["status=none", Path.parse_bytes(bytes.concat([b"if=", input.bytes()]))?, Path.parse_bytes(bytes.concat([b"of=", output.bytes()]))?]

  let result = invoke_with_path_args(ctx, args)?
  assert result.status == 0, result.stderr
  assert result.stdout == b""
  assert output.read_bytes()? == contents
}

test test_dd_empty_file_operands_keep_the_open_error { |ctx|
  let input = invoke(ctx, ["if="])?
  assert input.status == 1
  assert input.stderr == "dd: failed to open '': No such file or directory\n", input.stderr

  let output = invoke(ctx, ["status=none", "of="])?
  assert output.status == 1
  assert output.stderr == "dd: failed to open '': No such file or directory\n", output.stderr
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
  assert empty.stdout == b""
}

test test_dd_zero_count_seek_preserves_existing_stdout_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "dd-seek-stdout")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display(), "status=none", "bs=2", "oseek=8", "oflag=seek_bytes", "count=0"]
  let plan = process.command_argv(p"/bin/sh", ["sh", "-c", "printf abcdef; exec \"$@\"", "dd-seek-stdout"].extend(argv), root, {LC_ALL: "C"}, b"", out, err, timeout: 3s)
  assert process.run(plan)?.exited_with(0), err.read_text()?
  assert out.read_bytes()? == b"abcdef"
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
  let result = invoke(ctx, ["iflag=append"])?
  assert result.status == 1
  assert "unsupported" in result.stderr
}

test test_dd_direct_flags_transfer_whole_and_partial_blocks { |ctx|
  let data = bytes.from_ints([97 + index % 26 for index in range(8192 * 3 + 511)])?
  let upper = bytes.from_ints([65 + index % 26 for index in range(8192 * 3 + 511)])?
  let source = test.temp_file(ctx, name: "dd-direct-in", contents: data)?
  let out = test.temp_file(ctx, name: "dd-direct-out", contents: b"")?
  let plain = invoke(ctx, ["status=none", f"if={source}", f"of={out}", "iflag=direct", "oflag=direct", "bs=8192"])?
  if plain.status != 0 and "Invalid argument" in plain.stderr {
    test.skip("the filesystem does not support O_DIRECT")
  }
  assert plain.status == 0, plain.stderr
  assert out.read_bytes()? == data

  # A conversion takes the other output path, which has its own partial tail.
  let converted = invoke(ctx, ["status=none", f"if={source}", f"of={out}", "iflag=direct", "oflag=direct", "bs=8192", "conv=ucase"])?
  assert converted.status == 0, converted.stderr
  assert out.read_bytes()? == upper

  let skipped = invoke(ctx, ["status=none", f"if={source}", f"of={out}", "iflag=direct", "bs=4096", "skip=2", "count=1"])?
  assert skipped.status == 0, skipped.stderr
  assert out.read_bytes()? == data[8192..12288]

  let blocks = invoke(ctx, ["status=none", "oflag=direct", "cbs=4", "conv=block", f"of={out}"], b"a\n")?
  assert blocks.status == 1
  assert "cannot be combined" in blocks.stderr
}

test test_dd_nocache_drops_the_cache_and_reports_what_it_cannot { |ctx|
  let source = test.temp_file(ctx, name: "dd-nocache-in", contents: b"abcdef")?
  let out = test.temp_file(ctx, name: "dd-nocache-out", contents: b"")?
  let copied = invoke(ctx, ["status=none", f"if={source}", f"of={out}", "iflag=nocache", "oflag=nocache,sync", "bs=4"])?
  assert copied.status == 0, copied.stderr
  assert out.read_bytes()? == b"abcdef"

  # A character device holds no cache, so advising it is not a failure.
  let device = invoke(ctx, ["status=none", "if=/dev/zero", "of=/dev/null", "count=1", "iflag=nocache", "oflag=nocache"])?
  assert device.status == 0, device.stderr
  assert device.stderr == ""

  # Standard input is a pipe here, which has no cache to drop.
  let piped = invoke(ctx, ["iflag=nocache", "count=0", "status=noxfer"])?
  assert piped.status == 1
  assert piped.stderr.starts_with("dd: failed to discard cache for: 'standard input': "), piped.stderr
  assert piped.stderr.ends_with("0+0 records in\n0+0 records out\n"), piped.stderr
}

test test_dd_noatime_flags_read_and_write { |ctx|
  let source = test.temp_file(ctx, name: "dd-noatime-in", contents: b"abcdef")?
  let out = test.temp_file(ctx, name: "dd-noatime-out", contents: b"")?
  let result = invoke(ctx, ["status=none", f"if={source}", f"of={out}", "iflag=noatime", "oflag=noatime", "bs=4"])?
  assert result.status == 0, result.stderr
  assert out.read_bytes()? == b"abcdef"
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
  assert invoke(ctx, ["status=none", "cbs=4", "conv=block,ebcdic"], b"a\n")?.stdout == b"\x81\x40\x40\x40"
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

test test_dd_sparse_copy_preserves_zero_file_as_a_hole { |ctx|
  let root = test.temp_dir(ctx, name: "dd-sparse")?
  let input = fp"{root}/input"
  let output = fp"{root}/output"
  input.write(bytes.zero(1048576)?)

  let result = invoke(ctx, ["status=none", "bs=32K", f"if={input}", f"of={output}", "conv=sparse"])?
  assert result.status == 0, result.stderr
  let meta = output.metadata()?
  assert meta.size == 1048576
  assert meta.blocks_512 * 512 < meta.size, "zero blocks should remain unallocated"
}

test test_dd_sparse_copy_writes_data_after_a_hole_at_the_right_offset { |ctx|
  let root = test.temp_dir(ctx, name: "dd-sparse-offset")?
  let input = fp"{root}/input"
  let output = fp"{root}/output"
  let data = bytes.concat([b"head", bytes.zero(1048576)?, b"tail"])
  input.write(data)

  let result = invoke(ctx, ["status=none", "bs=32K", f"if={input}", f"of={output}", "conv=sparse"])?
  assert result.status == 0, result.stderr
  assert output.read_bytes()? == data
  let meta = output.metadata()?
  assert meta.blocks_512 * 512 < meta.size, "zero blocks should remain unallocated"
}

test test_dd_accepts_very_large_blocks_without_allocating_a_block_buffer { |ctx|
  let root = test.temp_dir(ctx, name: "dd-large-bs")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display(), "status=none", "bs=4G", "if=/dev/null", "of=/dev/null", "skip=1", "count=0"]
  # Keep the cap below the requested block size while leaving room for the debug interpreter stack.
  let status = process.run(process.command_argv(p"/bin/sh",
    ["sh", "-c", "ulimit -v 524288; exec \"$@\"", "dd-large-bs"].extend(argv),
    root, {LC_ALL: "C"}, b"", stdout, stderr, timeout: 3s))?
  assert status.exited_with(0), stderr.read_text()?
  assert stderr.read_text()? == ""
}

test test_dd_rejects_an_unbounded_output_record_size_cleanly { |ctx|
  let result = invoke(ctx, ["obs=1PB"])?
  assert result.status == 1
  assert "memory" in result.stderr
}

test test_dd_large_cbs_pads_until_output_fails { |ctx|
  let root = test.temp_dir(ctx, name: "dd-large-cbs")?
  let input = fp"{root}/input"
  input.write("x\n")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display(), "status=none", "conv=block", "cbs=1PB", f"if={input}", "of=/dev/full"]
  # Keep the cap far below the requested record size while leaving room for the debug interpreter stack.
  let status = process.run(process.command_argv(p"/bin/sh",
    ["sh", "-c", "ulimit -v 524288; exec \"$@\"", "dd-large-cbs"].extend(argv),
    root, {LC_ALL: "C"}, b"", stdout, stderr, timeout: 3s))?
  assert status.exited_with(1)
  assert "No space left on device" in stderr.read_text()?
}

test test_dd_reports_output_records_when_a_write_is_limited { |ctx|
  let root = test.temp_dir(ctx, name: "dd-write-limit")?
  let output = fp"{root}/output"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display(), f"if=/dev/zero", f"of={output}", "bs=512K", "count=3"]
  let status = process.run(process.command_argv(p"/bin/sh",
    ["sh", "-c", "trap '' XFSZ; ulimit -f 1536; exec \"$@\"", "dd-write-limit"].extend(argv),
    root, {LC_ALL: "C"}, b"", stdout, stderr, timeout: 3s))?
  assert status.exited_with(1)
  let message = stderr.read_text()?
  assert "1+1 records out" in message, message
  assert "786432 bytes" in message, message
  assert output.metadata()?.size == 786432
}

test test_dd_block_conversion_reports_partial_output_when_limited { |ctx|
  let root = test.temp_dir(ctx, name: "dd-block-write-limit")?
  let input = fp"{root}/input"
  let output = fp"{root}/output"
  input.write("x\n")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display(), f"if={input}", f"of={output}", "conv=block", "cbs=1M", "obs=64K"]
  let status = process.run(process.command_argv(p"/bin/sh",
    ["sh", "-c", "trap '' XFSZ; ulimit -f 400; exec \"$@\"", "dd-block-write-limit"].extend(argv),
    root, {LC_ALL: "C"}, b"", stdout, stderr, timeout: 3s))?
  assert status.exited_with(1)
  let message = stderr.read_text()?
  assert "3+1 records out" in message, message
  assert "204800 bytes" in message, message
  assert output.metadata()?.size == 204800
}

test test_dd_iflag_directory_requires_a_directory_input { |ctx|
  let piped = invoke(ctx, ["iflag=directory", "count=0"], b"")?
  assert piped.status == 1
  assert piped.stderr == "dd: setting flags for 'standard input': Not a directory\n", piped.stderr

  let root = test.temp_dir(ctx, name: "dd-iflag-directory")?
  let file = fp"{root}/plain"
  file.write(b"abc")
  let regular = invoke(ctx, ["iflag=directory", "count=0", f"if={file}"])?
  assert regular.status == 1
  assert regular.stderr == f"dd: failed to open '{file}': Not a directory\n", regular.stderr

  let directory = invoke(ctx, ["iflag=directory", "count=0", f"if={root}"])?
  assert directory.status == 0, directory.stderr
  assert directory.stdout == b""
}

test test_dd_skip_past_input_warns_without_failing { |ctx|
  let result = invoke(ctx, ["bs=1", "skip=5", "count=0", "status=noxfer"], b"abcd")?
  assert result.status == 0, result.stderr
  assert result.stdout == b""
  assert result.stderr == "dd: 'standard input': cannot skip to specified offset\n0+0 records in\n0+0 records out\n"
}

test test_dd_status_none_silences_skip_past_input { |ctx|
  let quiet = invoke(ctx, ["bs=1", "skip=5", "count=0", "status=none"], b"abcd")?
  assert quiet.status == 0, quiet.stderr
  assert quiet.stderr == ""
  let largest = invoke(ctx, ["bs=1", "skip=9223372036854775807", "count=0", "status=none"], b"abcd")?
  assert "invalid number" not in largest.stderr, largest.stderr
  assert largest.status == 0, largest.stderr
}

test test_dd_oflag_append_extends_a_regular_output { |ctx|
  let root = test.temp_dir(ctx, name: "dd-append")?
  let file = fp"{root}/file"
  file.write("x")
  let result = invoke(ctx, ["status=none", f"of={file}", "conv=notrunc", "oflag=append"], b"ab")?
  assert result.status == 0, result.stderr
  assert file.read_text()? == "xab"
}

test test_dd_oflag_append_on_standard_output { |ctx|
  let root = test.temp_dir(ctx, name: "dd-append-stdout")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display(), "status=none", "oflag=append"]
  # The shell writes first, so standard output already has data when dd starts.
  let plan = process.command_argv(p"/bin/sh", ["sh", "-c", "printf x; exec \"$@\"", "dd-append-stdout"].extend(argv), root, {LC_ALL: "C"}, b"ab", out, err, timeout: 3s)
  let status = process.run(plan)?
  assert status.exited_with(0), err.read_text()?
  assert out.read_text()? == "xab"
  let converted_out = fp"{root}/converted"
  let converted = process.command_argv(p"/bin/sh", ["sh", "-c", "printf x; exec \"$@\"", "dd-append-stdout"].extend(argv).extend(["conv=ucase"]), root, {LC_ALL: "C"}, b"ab", converted_out, err, timeout: 3s)
  assert process.run(converted)?.exited_with(0), err.read_text()?
  assert converted_out.read_text()? == "xAB"
}

test test_dd_sparse_notrunc_skips_nul_blocks_in_place { |ctx|
  let root = test.temp_dir(ctx, name: "dd-sparse-notrunc")?
  let input = fp"{root}/input"
  let output = fp"{root}/output"
  input.write(b"a\0\0b")
  output.write(b"____")
  let result = invoke(ctx, ["status=none", "bs=1", f"if={input}", f"of={output}", "conv=sparse,notrunc"])?
  assert result.status == 0, result.stderr
  assert output.read_bytes()? == b"a__b"
}

test test_dd_sparse_zero_run_inside_a_larger_output_block_is_written { |ctx|
  let root = test.temp_dir(ctx, name: "dd-sparse-block")?
  let input = fp"{root}/input"
  let output = fp"{root}/output"
  let data = bytes.concat([bytes.zero(1048576)?, b"tail"])
  input.write(data)

  let result = invoke(ctx, ["status=none", "ibs=1M", "obs=2M", f"if={input}", f"of={output}", "conv=sparse"])?
  assert result.status == 0, result.stderr
  assert output.read_bytes()? == data
  let meta = output.metadata()?
  assert meta.blocks_512 * 512 >= 1048576, "a zero run that is part of a larger output block is written, not made a hole"
}

test test_dd_failed_status_write_ends_the_run { |ctx|
  let root = test.temp_dir(ctx, name: "dd-status-full")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/dd.xsh".display()]
  let plan = process.command_argv(p"/bin/sh", ["sh", "-c", "exec \"$@\" 2>/dev/full", "dd-status-full"].extend(argv), root, {LC_ALL: "C"}, b"ab", stdout, stderr, timeout: 3s)
  let status = process.run(plan)?
  assert status.exited_with(1)
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

test test_dd_zero_count_seeks_an_output_fifo_by_reading { |ctx|
  let root = test.temp_dir(ctx, name: "dd-seek-fifo")?
  let fifo = fp"{root}/fifo"
  let payload = fp"{root}/payload"
  payload.write(bytes.zero(512)?)
  fs.mkfifo(fifo, 0o600)
  let writer_script = fp"{root}/writer.xsh"
  writer_script.write(f"fp\"{fifo}\".write(fp\"{payload}\".read_bytes()?)\n")
  let writer = spawn run timeout -s KILL 2 ${ctx.xsh_bin} $writer_script ?
  defer writer.cancel(kill_after: 100ms)

  let result = invoke(ctx, ["status=noxfer", f"of={fifo}", "seek=1", "count=0"])?
  assert result.status == 0, result.stderr
  assert result.stdout == b""
  assert result.stderr == "0+0 records in\n0+0 records out\n"
  assert (wait writer?).exited_with(0)
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

test test_dd_block_size_beyond_the_representable_range_is_an_invalid_number { |ctx|
  for arg in ["bs=9223372036854775807", "ibs=9223372036854775807", "obs=99999999999999999999"] {
    let result = invoke(ctx, [arg])?
    assert result.status == 1, result.stderr
    assert result.stdout == b"", arg
    assert result.stderr.starts_with("dd: invalid number: '"), result.stderr
    assert result.stderr.ends_with("': Value too large for defined data type\n"), result.stderr
  }
}

test test_dd_unallocatable_block_sizes_report_gnu_buffer_errors { |ctx|
  for case in [
    ["bs=9223372036854775806", "dd: memory exhausted by input buffer of size 9223372036854775806 bytes (8.0 EiB)\n"],
    ["bs=4E", "dd: memory exhausted by input buffer of size 4611686018427387904 bytes (4.0 EiB)\n"],
    ["ibs=1EB", "dd: memory exhausted by input buffer of size 1000000000000000000 bytes (888 PiB)\n"],
    ["obs=1PB", "dd: memory exhausted by output buffer of size 1000000000000000 bytes (909 TiB)\n"],
    ["obs=4E", "dd: memory exhausted by output buffer of size 4611686018427387904 bytes (4.0 EiB)\n"],
    ["bs=1023G", "dd: memory exhausted by input buffer of size 1098437885952 bytes (1023 GiB)\n"],
    ["bs=1099468678103", "dd: memory exhausted by input buffer of size 1099468678103 bytes (1.0 TiB)\n"],
    ["bs=10929145580093", "dd: memory exhausted by input buffer of size 10929145580093 bytes (9.9 TiB)\n"],
  ] {
    let result = invoke(ctx, [case[0]])?
    assert result.status == 1, result.stderr
    assert result.stderr == case[1], result.stderr
  }
}

test test_dd_zero_count_allocates_no_block_buffer { |ctx|
  for arg in ["bs=1PB", "ibs=1PB", "obs=1PB"] {
    let result = invoke(ctx, [arg, "count=0", "status=none"])?
    assert result.status == 0, result.stderr
    assert result.stdout == b""
    assert result.stderr == "", arg
  }
}

test test_dd_zero_transfer_reports_kilobytes_per_second { |ctx|
  let result = invoke(ctx, ["count=0"])?
  assert result.status == 0, result.stderr
  assert result.stderr.ends_with(" s, 0.0 kB/s\n"), result.stderr
}

test test_dd_warns_once_for_repeated_zero_multipliers { |ctx|
  let result = invoke(ctx, ["count=0x0x1", "status=none"])?
  assert result.status == 0, result.stderr
  assert result.stderr == "dd: warning: '0x' is a zero multiplier; use '00x' if that is intended\n", result.stderr
}

test test_dd_nocache_pipe_reports_illegal_seek { |ctx|
  let result = invoke(ctx, ["iflag=nocache", "count=0", "status=none"])?
  assert result.status == 1, result.stderr
  assert result.stderr == "dd: failed to discard cache for: 'standard input': Illegal seek\n", result.stderr
}

test test_dd_help_labels_block_size_and_conversions { |ctx|
  let result = invoke(ctx, ["--help"])?
  assert result.status == 0, result.stderr
  let text = result.stdout.utf8()?
  assert "\n  bs=BYTES" in text, text
  assert "\nEach CONV symbol may be:\n" in text, text
}
