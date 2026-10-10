type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "od")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/od.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_od_formats_share_one_block_width { |ctx|
  # Each -t column pads its fields to the widest block of all formats, as GNU od does.
  let input = b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n14\n15\n16\n17\n18\n19\n"
  let output = invoke(ctx, ["-An", "-tdS", "-txC"], input)?.stdout
  assert output == b"   2609   2610   2611   2612   2613   2614   2615   2616\n  31 0a  32 0a  33 0a  34 0a  35 0a  36 0a  37 0a  38 0a\n   2617  12337  12554   2609  12849  12554   2611  13361\n  39 0a  31 30  0a 31  31 0a  31 32  0a 31  33 0a  31 34\n  12554   2613  13873  12554   2615  14385  12554   2617\n  0a 31  35 0a  31 36  0a 31  37 0a  31 38  0a 31  39 0a\n"
}

test test_od_default_and_byte_formats { |ctx|
  assert invoke(ctx, [], b"abcdefghijklmnopqrstuvwxyz\n")?.stdout == b"0000000 061141 062143 063145 064147 065151 066153 067155 070157\n0000020 071161 072163 073165 074167 075171 000012\n0000033\n"
  assert invoke(ctx, ["-An", "-tx1"], b"\0\xff\xfe")?.stdout == b" 00 ff fe\n"
  assert invoke(ctx, ["-An", "-c"], b"a\n\xff")?.stdout == b"   a  \\n 377\n"
}

test test_od_machine_word_signed_unsigned_endian { |ctx|
  assert invoke(ctx, ["-An", "-tu8"], b"\xff\xff\xff\xff\xff\xff\xff\xff")?.stdout == b" 18446744073709551615\n"
  assert invoke(ctx, ["-An", "-td8"], b"\0\0\0\0\0\0\0\x80")?.stdout == b" -9223372036854775808\n"
  assert invoke(ctx, ["-An", "--endian=big", "-tx2"], b"\x12\x34")?.stdout == b" 1234\n"
}

test test_od_skip_count_and_duplicate_suppression { |ctx|
  assert invoke(ctx, ["-An", "-tx1", "-j2", "-N2"], b"abcdef")?.stdout == b" 63 64\n"
  let input = bytes.zero(48)?
  assert invoke(ctx, ["-An", "-tx1"], input)?.stdout == b" 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00\n*\n"
}

test test_od_legacy_offsets_and_labels { |ctx|
  assert invoke(ctx, ["-An", "-c", "+2"], b"abcd")?.stdout == b"   c   d\n"
  assert invoke(ctx, ["-An", "--traditional", "-c", "-", "2", "0x10"], b"abcd")?.stdout == b"(0000020)   c   d\n(0000022)\n"
  assert invoke(ctx, ["-An", "-tx1", "-j0x2"], b"abcd")?.stdout == b" 63 64\n"
}

test test_od_multiple_formats_share_alignment { |ctx|
  assert invoke(ctx, ["-Xxa"], b"\x0a\x0d\x65\x66\x67\0\x9e\x9f")?.stdout == b"0000000        66650d0a        9f9e0067\n           0d0a    6665    0067    9f9e\n         nl  cr   e   f   g nul  rs  us\n0000010\n"
}

test test_od_upstream_width_and_high_bit_regressions { |ctx|
  let binary = invoke(ctx, ["-An", "-a"], b"\xc0\xff")?
  assert binary.status == 0, binary.stderr
  assert binary.stdout == b"   @ del\n"
  assert invoke(ctx, ["-An", "-tx1", "-j0xB"], b"abcdefghijkl")?.stdout == b" 6c\n"
  assert invoke(ctx, ["-An", "-w", "-v"], bytes.zero(32)?)?.stdout.lines().len() == 1
  assert invoke(ctx, ["--strings", "-N7"], b"foo\0bar\0")?.stdout == b"0000000 foo\n0000004 bar\n"
  assert invoke(ctx, ["-An", "-O"], b"\xff\xff\xff\xff")?.status == 0
}

test test_od_proc_input_is_read_without_metadata_size { |ctx|
  if ! p"/proc/version".exists() { test.skip("procfs is not available") }
  let proc_text = p"/proc/version".read_text()?
  let tail = test.temp_file(ctx, name: "od-tail", contents: b"e")?
  let result = invoke(ctx, ["-An", "-c", "-j", f"{proc_text.byte_len()}", "/proc/version", tail.display()])?
  assert result.status == 0, result.stderr
  assert result.stdout == b"   e\n"
}

test test_od_format_flags_can_precede_a_type_value_in_one_cluster { |ctx|
  let result = invoke(ctx, ["-An", "-Oobtu8"], b"\xff\xff\xff\xff\xff\xff\xff\xff")?
  assert result.status == 0, result.stderr
  assert "18446744073709551615" in result.stdout as Str
}

test test_od_decodes_float32_and_binary16 { |ctx|
  let float32 = invoke(ctx, ["-An", "--endian=little", "-f", "-w8"], b"\0\0\x80?\0\0\0@")?
  assert float32.status == 0, float32.stderr
  assert float32.stdout == b"               1               2\n"

  let binary16 = invoke(ctx, ["-An", "--endian=big", "-tfH", "-w4"], b"\x3c\0\x40\0")?
  assert binary16.status == 0, binary16.stderr
  assert binary16.stdout == b"               1               2\n"
}

test test_od_formats_binary16_with_a_roundtripping_decimal { |ctx|
  let result = invoke(ctx, ["-An", "--endian=big", "-tfH", "-w2"], b"\x3c\x01")?
  assert result.status == 0, result.stderr
  assert result.stdout == b"       1.0009766\n"

  let bfloat = invoke(ctx, ["-An", "--endian=big", "-tfB", "-w2"], b"\x3f\x81")?
  assert bfloat.status == 0, bfloat.stderr
  assert bfloat.stdout == b"       1.0078125\n"
}

test test_od_formats_float32_subnormals_and_special_values { |ctx|
  let subnormal = invoke(ctx, ["-An", "--endian=little", "-f", "-w4"], b"\xc2\x16\x01\0")?
  assert subnormal.status == 0, subnormal.stderr
  assert subnormal.stdout == b"           1e-40\n"

  let special = invoke(ctx, ["-An", "--endian=little", "-f", "-w16"], b"\0\0\x80\x7f\0\0\x80\xff\xff\xff\xff\x7f\0\0\0\x80")?
  assert special.status == 0, special.stderr
  assert "inf" in special.stdout as Str
  assert "-inf" in special.stdout as Str
  assert "NaN" in special.stdout as Str
  assert "-0" in special.stdout as Str
  assert ! ("Infinity" in special.stdout as Str)
}

test test_od_binary64_uses_its_full_column_width { |ctx|
  let result = invoke(ctx, ["--endian=little", "-F", "-w8"], b"\0\0\0\0\0\0\0\xc0")?
  assert result.status == 0, result.stderr
  assert result.stdout == b"0000000                       -2\n0000010\n"
}

test test_od_float_width_aligns_smaller_hex_formats { |ctx|
  let result = invoke(ctx, ["-An", "--endian=little", "-F", "-x"], b"\0\0\0\0\0\0\0\xc0")?
  assert result.status == 0, result.stderr
  assert "0000  0000  0000  c000\n" in result.stdout as Str
}

test test_od_rejects_empty_address_radix_with_its_domain_error { |ctx|
  let bad = invoke(ctx, ["-A", ""], b"")?
  assert bad.status == 1
  assert bad.stderr == "od: Radix cannot be empty, and must be one of [o, d, x, n]\n"
}

test test_od_invalid_read_and_skip_counts_name_the_option { |ctx|
  let read = invoke(ctx, ["--read-bytes=x"], b"")?
  assert read.status == 1
  assert read.stderr == "od: invalid --read-bytes argument 'x'\n"

  let short_read = invoke(ctx, ["-N", "-1"], b"")?
  assert short_read.status == 1
  assert short_read.stderr == "od: invalid -N argument '-1'\n"

  let short_skip = invoke(ctx, ["-j", "-1"], b"")?
  assert short_skip.status == 1
  assert short_skip.stderr == "od: invalid -j argument '-1'\n"
}

test test_od_rejects_overflowing_legacy_offset { |ctx|
  let value = "7777777777777777777777"
  let bad = invoke(ctx, ["-", value], b"")?
  assert bad.status == 1
  assert bad.stderr == f"od: {value}: Result not representable\n"
}

test test_od_byte_counts_reject_values_too_large_to_represent { |ctx|
  let bad = invoke(ctx, ["--read-bytes=1Y"], b"")?
  assert bad.status == 1
  assert bad.stderr == "od: --read-bytes argument '1Y' too large\n"
}

test test_od_width_errors_report_the_spelling_used { |ctx|
  let short = invoke(ctx, ["-w", "-1"], b"")?
  assert short.status == 1
  assert short.stderr == "od: invalid -w argument '-1'\n"

  let long = invoke(ctx, ["--width=x"], b"")?
  assert long.status == 1
  assert long.stderr == "od: invalid --width argument 'x'\n"
}

test test_od_rejects_width_padding_overflow_before_allocating_a_line { |ctx|
  let root = test.temp_dir(ctx, name: "od-wide")?
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/od.xsh".display(), "-w3037000501", "-tcz"]
  let plan = process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {LC_ALL: "C"},
    b"x",
    p"/dev/null",
    stderr,
    timeout: 2s,
  )
  let status = process.run(plan)?

  assert status.exit_code()? == 1
  assert stderr.read_text()? == "od: 3037000501 is too large\n"
}

test test_od_streams_padding_when_a_large_width_is_valid { |ctx|
  let root = test.temp_dir(ctx, name: "od-wide-pipe")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let od = fp"{ctx.core_dir}/od.xsh"
  let command = f"printf x | {ctx.xsh_bin} {od} -w3037000500 -tcz | head -c 1"
  let plan = process.command_argv(
    "sh",
    ["sh", "-c", command],
    root,
    {LC_ALL: "C"},
    b"",
    stdout,
    stderr,
    timeout: 5s,
  )
  let status = process.run(plan)?

  assert status.exit_code()? == 0
  assert stdout.read_bytes()? == b"0"
  assert stderr.read_text()? == ""
}

test test_od_formats_binary64_normal_and_subnormal_values { |ctx|
  let data = bytes.concat([b"\x27\x6b\x0a\x2f\x2a\xee\x45\x43", bytes.zero(8)?, b"\0\0\0\0\0\0\x10\x80", b"\x01\0\0\0\0\0\0\0", b"\0\0\0\0\0\0\0\xc0"])
  let result = invoke(ctx, ["--endian=little", "-F"], data)?
  assert result.status == 0, result.stderr
  assert result.stdout == b"0000000        12345678912345678                        0\n0000020 -2.2250738585072014e-308                   5e-324\n0000040                       -2\n0000050\n"
}
