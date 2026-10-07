type Ran = {status: Int, stdout: Bytes, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "od")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/od.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
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

test test_od_formats_binary64_normal_and_subnormal_values { |ctx|
  let data = bytes.concat([b"\x27\x6b\x0a\x2f\x2a\xee\x45\x43", bytes.zero(8)?, b"\0\0\0\0\0\0\x10\x80", b"\x01\0\0\0\0\0\0\0", b"\0\0\0\0\0\0\0\xc0"])
  let result = invoke(ctx, ["--endian=little", "-F"], data)?
  assert result.status == 0, result.stderr
  assert result.stdout == b"0000000        12345678912345678                        0\n0000020 -2.2250738585072014e-308                   5e-324\n0000040                       -2\n0000050\n"
}
