##! Native ports of the uutils dd integration tests.

use support.uu as uu

# origin: uutils test_dd::test_atoe_and_lcase_conv_spec_test
test test_uu_dd_atoe_and_lcase_conv_spec_test { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "seq-byte-values-b632a992d3aed5d8d1a59cc5a5a455ba.test", "input")?
  uu.fixture(s, "dd", "lcase-ebcdic.test", "expected")?
  let r = uu.invoke(s, "dd", ["conv=ebcdic,lcase"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_atoe_and_ucase_conv_spec_test
test test_uu_dd_atoe_and_ucase_conv_spec_test { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "seq-byte-values-b632a992d3aed5d8d1a59cc5a5a455ba.test", "input")?
  uu.fixture(s, "dd", "ucase-ebcdic.test", "expected")?
  let r = uu.invoke(s, "dd", ["conv=ebcdic,ucase"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_atoe_conv_spec_test
test test_uu_dd_atoe_conv_spec_test { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "seq-byte-values-b632a992d3aed5d8d1a59cc5a5a455ba.test", "input")?
  uu.fixture(s, "dd", "gnudd-conv-atoe-seq-byte-values.spec", "expected")?
  let r = uu.invoke(s, "dd", ["conv=ebcdic"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_atoibm_and_lcase_conv_spec_test
test test_uu_dd_atoibm_and_lcase_conv_spec_test { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "seq-byte-values-b632a992d3aed5d8d1a59cc5a5a455ba.test", "input")?
  uu.fixture(s, "dd", "lcase-ibm.test", "expected")?
  let r = uu.invoke(s, "dd", ["conv=ibm,lcase"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_atoibm_and_ucase_conv_spec_test
test test_uu_dd_atoibm_and_ucase_conv_spec_test { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "seq-byte-values-b632a992d3aed5d8d1a59cc5a5a455ba.test", "input")?
  uu.fixture(s, "dd", "ucase-ibm.test", "expected")?
  let r = uu.invoke(s, "dd", ["conv=ibm,ucase"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_atoibm_conv_spec_test
test test_uu_dd_atoibm_conv_spec_test { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "seq-byte-values-b632a992d3aed5d8d1a59cc5a5a455ba.test", "input")?
  uu.fixture(s, "dd", "gnudd-conv-atoibm-seq-byte-values.spec", "expected")?
  let r = uu.invoke(s, "dd", ["conv=ibm"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_block_cbs16
test test_uu_dd_block_cbs16 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "dd-block-cbs16.test", "input")?
  uu.fixture(s, "dd", "dd-block-cbs16.spec", "expected")?
  let r = uu.invoke(s, "dd", ["conv=block", "cbs=16"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_block_cbs16_as_cbs8
test test_uu_dd_block_cbs16_as_cbs8 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "dd-block-cbs16.test", "input")?
  uu.fixture(s, "dd", "dd-block-cbs8.spec", "expected")?
  let r = uu.invoke(s, "dd", ["conv=block", "cbs=8"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_block_consecutive_nl
test test_uu_dd_block_consecutive_nl { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "dd-block-consecutive-nl.test", "input")?
  uu.fixture(s, "dd", "dd-block-consecutive-nl-cbs16.spec", "expected")?
  let r = uu.invoke(s, "dd", ["conv=block", "cbs=16"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_block_lower
test test_uu_dd_block_lower { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "dd-block8-lowercase.test", "input")?
  uu.fixture(s, "dd", "dd-block8-lowercase.spec", "expected")?
  let r = uu.invoke(s, "dd", ["conv=block,lcase", "cbs=8"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_bytes_oseek_bytes_oflag
test test_uu_dd_bytes_oseek_bytes_oflag { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "dd-bytes-alphabet-null.spec", "expected")?
  let r = uu.invoke(s, "dd", ["oseek=8", "oflag=seek_bytes", "bs=2"], stdin: b"abcdefghijklm")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_bytes_oseek_seek_not_additive
test test_uu_dd_bytes_oseek_seek_not_additive { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "dd-bytes-alphabet-null.spec", "expected")?
  let r = uu.invoke(s, "dd", ["oseek=8", "seek=8", "oflag=seek_bytes", "bs=2"], stdin: b"abcdefghijklm")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_deadbeef_32k_conv_sync_ibs_gt_obs
test test_uu_dd_deadbeef_32k_conv_sync_ibs_gt_obs { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test")?
  uu.fixture(s, "dd", "gnudd-conv-sync-ibs-1031-obs-521-deadbeef.spec", "expected")?
  let r = uu.invoke(s, "dd", ["conv=sync", "ibs=1031", "obs=521", "if=deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_deadbeef_32k_conv_sync_obs_gt_ibs
test test_uu_dd_deadbeef_32k_conv_sync_obs_gt_ibs { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test")?
  uu.fixture(s, "dd", "gnudd-conv-sync-ibs-521-obs-1031-deadbeef.spec", "expected")?
  let r = uu.invoke(s, "dd", ["conv=sync", "ibs=521", "obs=1031", "if=deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_deadbeef_32k_to_12345_test_count_bytes
test test_uu_dd_deadbeef_32k_to_12345_test_count_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test")?
  uu.fixture(s, "dd", "gnudd-deadbeef-first-12345.spec", "expected")?
  let r = uu.invoke(s, "dd", ["ibs=531", "obs=1031", "count=12345", "iflag=count_bytes", "if=deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_deadbeef_32k_to_16k_test_count_reads
test test_uu_dd_deadbeef_32k_to_16k_test_count_reads { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test")?
  uu.fixture(s, "dd", "gnudd-deadbeef-first-16k.spec", "expected")?
  let r = uu.invoke(s, "dd", ["ibs=1024", "obs=1031", "count=16", "if=deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_deadbeef_all_32k_test_count_bytes
test test_uu_dd_deadbeef_all_32k_test_count_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test")?
  uu.fixture(s, "dd", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test", "expected")?
  let r = uu.invoke(s, "dd", ["ibs=531", "obs=1031", "count=32x1024", "oflag=count_bytes", "if=deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_deadbeef_all_32k_test_count_reads
test test_uu_dd_deadbeef_all_32k_test_count_reads { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test")?
  uu.fixture(s, "dd", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test", "expected")?
  let r = uu.invoke(s, "dd", ["bs=1024", "count=32", "if=deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_etoa_conv_spec_test
test test_uu_dd_etoa_conv_spec_test { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "seq-byte-values-b632a992d3aed5d8d1a59cc5a5a455ba.test", "input")?
  uu.fixture(s, "dd", "gnudd-conv-etoa-seq-byte-values.spec", "expected")?
  let r = uu.invoke(s, "dd", ["conv=ascii"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_lcase_ascii_to_ucase_ascii
test test_uu_dd_lcase_ascii_to_ucase_ascii { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "lcase-ascii.test", "input")?
  uu.fixture(s, "dd", "ucase-ascii.test", "expected")?
  let r = uu.invoke(s, "dd", ["conv=ucase"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "expected")?)
}

# origin: uutils test_dd::test_identity
test test_uu_dd_identity { |ctx|
  let s = uu.scene(ctx)?
  for name in ["zeros-620f0b67a91f7f74151bc5be745b7110.test", "ones-6ae59e64850377ee5470c854761551ea.test", "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test", "random-5828891cb1230748e146f34223bbd3b5.test"] {
    uu.fixture(s, "dd", name, name)?
    let r = uu.invoke(s, "dd", [f"if={name}"])?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, uu.read(s, name)?)
  }
}

# origin: uutils test_dd::test_all_valid_ascii_ebcdic_ascii_roundtrip_conv_test
test test_uu_dd_all_valid_ascii_ebcdic_ascii_roundtrip_conv_test { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "all-valid-ascii-chars-37eff01866ba3f538421b30b7cbefcac.test", "input")?
  let first = uu.invoke(s, "dd", ["ibs=128", "obs=1024", "conv=ebcdic"], stdin: uu.read(s, "input")?)?
  uu.succeeds(first)
  let r = uu.invoke(s, "dd", ["ibs=256", "obs=1024", "conv=ascii"], stdin: first.stdout)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "input")?)
}

# origin: uutils test_dd::test_invalid_arg
test test_uu_dd_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["--definitely-invalid"], stdin: b"")?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_dd::test_huge_obs_reports_memory_error_instead_of_aborting
test test_uu_dd_huge_obs_reports_memory_error_instead_of_aborting { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["obs=1PB"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "memory")
}

# origin: uutils test_dd::test_huge_cbs_pads_without_allocating
test test_uu_dd_huge_cbs_pads_without_allocating { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=block", "cbs=1PB", "of=/dev/full"], stdin: b"x\x0a")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "No space left on device")
}

# origin: uutils test_dd::test_b_multiplier
test test_uu_dd_b_multiplier { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["bs=2b", "count=1"], stdin: b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text(["a" for _ in range(1024)].join("")))
}

# origin: uutils test_dd::test_final_stats_noxfer
test test_uu_dd_final_stats_noxfer { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["status=noxfer"], stdin: b"")?
  uu.succeeds(r)
  uu.stderr_only(r, "0+0 records in\n0+0 records out\n")
}

# origin: uutils test_dd::test_conv_ascii_implies_unblock
test test_uu_dd_conv_ascii_implies_unblock { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=ascii", "cbs=4"], stdin: b"@\xc1@\xc1@\xc1@@")?
  uu.succeeds(r)
  uu.stdout_is(r, " A A\n A\n")
}

# origin: uutils test_dd::test_conv_ebcdic_implies_block
test test_uu_dd_conv_ebcdic_implies_block { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=ebcdic", "cbs=4"], stdin: b" A A\x0a A\x0a")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"\x40\xc1\x40\xc1\x40\xc1\x40\x40")
}

# origin: uutils test_dd::test_block_keeps_trailing_record_of_spaces
test test_uu_dd_block_keeps_trailing_record_of_spaces { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=block", "cbs=4"], stdin: b"ab\x0a  ")?
  uu.succeeds(r)
  uu.stdout_is(r, "ab      ")
}

# origin: uutils test_dd::test_bytes_iseek_bytes_iflag
test test_uu_dd_bytes_iseek_bytes_iflag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["iseek=10", "iflag=skip_bytes", "bs=2"], stdin: b"0123456789abcdefghijklm")?
  uu.succeeds(r)
  uu.stdout_is(r, "abcdefghijklm")
}

# origin: uutils test_dd::test_bytes_iseek_skip_not_additive
test test_uu_dd_bytes_iseek_skip_not_additive { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["iseek=4", "skip=4", "iflag=skip_bytes", "bs=2"], stdin: b"0123456789abcdefghijklm")?
  uu.succeeds(r)
  uu.stdout_is(r, "456789abcdefghijklm")
}

# origin: uutils test_dd::test_empty_count_number
test test_uu_dd_empty_count_number { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["count=B"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "dd: invalid number: 'B'\n")
}

# origin: uutils test_dd::test_big_multiplication
test test_uu_dd_big_multiplication { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["ibs=10x10x10x10x10x10x10x10x10x10x10x10x10x10x10x10x10x10x10x10x10x10x10"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "invalid number")
}

# origin: uutils test_dd::test_etoa_and_lcase
test test_uu_dd_etoa_and_lcase { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=ascii,lcase", "status=none"], stdin: b"\xc8\x85\x93\x93\x96k@\xe6\x96\x99\x93\x84Z")?
  uu.succeeds(r)
  uu.stdout_only(r, "hello, world!")
}

# origin: uutils test_dd::test_etoa_and_ucase
test test_uu_dd_etoa_and_ucase { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=ascii,ucase", "status=none"], stdin: b"\xc8\x85\x93\x93\x96k@\xe6\x96\x99\x93\x84Z")?
  uu.succeeds(r)
  uu.stdout_only(r, "HELLO, WORLD!")
}

# origin: uutils test_dd::test_huge_block_size_is_rejected_without_panicking
test test_uu_dd_huge_block_size_is_rejected_without_panicking { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["bs=9223372036854775807", "ibs=99999999999999999999", "obs=18446744073709551616", "cbs=9223372036854775808", "ibs=1778172772721772786161B"] {
    let r = uu.invoke(s, "dd", [arg])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    uu.stderr_contains(r, "Value too large for defined data type")
  }
}

# origin: uutils test_dd::test_count_past_u64_is_rejected
test test_uu_dd_count_past_u64_is_rejected { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["count=99999999999999999999", "skip=18446744073709551616"] {
    let r = uu.invoke(s, "dd", ["if=/dev/null", arg])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "Value too large for defined data type")
  }
}

# origin: uutils test_dd::test_bs_not_positive
test test_uu_dd_bs_not_positive { |ctx|
  let s = uu.scene(ctx)?
  for value in [-5, 0, 0] {
    for key in ["bs", "ibs", "obs", "cbs"] {
      let r = uu.invoke(s, "dd", [f"{key}={value}"])?
      uu.fails_with_code(r, 1)
      uu.no_stdout(r)
      uu.stderr_is(r, f"dd: invalid number: '{value}'\n")
    }
  }
}

# origin: uutils test_dd::test_invalid_number_arg_gnu_compatibility
test test_uu_dd_invalid_number_arg_gnu_compatibility { |ctx|
  let s = uu.scene(ctx)?
  for key in ["bs", "cbs", "count", "ibs", "obs", "seek", "skip"] {
    for value in ["", "29d"] {
      let r = uu.invoke(s, "dd", [f"{key}={value}"])?
      uu.fails(r)
      uu.stderr_is(r, f"dd: invalid number: '{value}'\n")
    }
  }
}

# origin: uutils test_dd::test_invalid_flag_arg_gnu_compatibility
test test_uu_dd_invalid_flag_arg_gnu_compatibility { |ctx|
  let s = uu.scene(ctx)?
  for pair in [["iflag", "input"], ["oflag", "output"]] {
    for value in ["", "29d"] {
      let r = uu.invoke(s, "dd", [f"{pair[0]}={value}"])?
      uu.fails(r)
      uu.stderr_is(r, f"dd: invalid {pair[1]} flag: '{value}'\nTry 'dd --help' for more information.\n")
    }
  }
}

# origin: uutils test_dd::test_invalid_file_arg_gnu_compatibility
test test_uu_dd_invalid_file_arg_gnu_compatibility { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["if=", "if=81as9bn8as9g302az8ns9.pdf.zip.pl.com", "of="] {
    let r = uu.invoke(s, "dd", [arg])?
    uu.fails(r)
    let filename = arg.split("=")[1]
    uu.stderr_is(r, f"dd: failed to open '{filename}': No such file or directory\n")
  }
  let r = uu.invoke(s, "dd", ["of=81as9bn8as9g302az8ns9.pdf.zip.pl.com"], stdin: b"")?
  uu.succeeds(r)
}

# origin: uutils test_dd::test_excl_causes_failure_when_present
test test_uu_dd_excl_causes_failure_when_present { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "dd", "this-file-exists-excl.txt", "this-file-exists-excl.txt")?
  let r = uu.invoke(s, "dd", ["of=this-file-exists-excl.txt", "conv=excl"], stdin: b"")?
  uu.fails(r)
}

# origin: uutils test_dd::test_existing_file_truncated
test test_uu_dd_existing_file_truncated { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "this-file-exists-truncated.txt", bytes.zero(256)?)?
  uu.fixture(s, "dd", "null.txt", "null.txt")?
  let r = uu.invoke(s, "dd", ["status=none", "if=null.txt", "of=this-file-exists-truncated.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.size(s, "this-file-exists-truncated.txt")? == 0
}

# origin: uutils test_dd::test_ascii_10k_to_stdout
test test_uu_dd_ascii_10k_to_stdout { |ctx|
  let s = uu.scene(ctx)?
  let input = bytes.from_ints([i % 128 for i in range(1048576)])?
  uu.fixture(s, "dd", "ascii-10k.txt", "ascii-10k.txt")?
  let r = uu.invoke(s, "dd", ["status=none", "if=ascii-10k.txt"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, input)
}

# origin: uutils test_dd::test_ascii_521k_to_file
test test_uu_dd_ascii_521k_to_file { |ctx|
  let s = uu.scene(ctx)?
  let input = bytes.from_ints([i % 128 for i in range(524288)])?
  let r = uu.invoke(s, "dd", ["status=none", "of=TESTFILE-ascii-521k.tmp"], stdin: input)?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.size(s, "TESTFILE-ascii-521k.tmp")? == 524288
  assert uu.read(s, "TESTFILE-ascii-521k.tmp")? == input
}

# origin: uutils test_dd::test_iflag_direct_read_uses_aligned_buffer
test test_uu_dd_iflag_direct_read_uses_aligned_buffer { |ctx|
  let s = uu.scene(ctx)?
  let input = bytes.from_ints([i % 128 for i in range(8192)])?
  uu.write_bytes(s, "direct-in.bin", input)?
  let r = uu.invoke(s, "dd", ["if=direct-in.bin", "of=direct-out.bin", "iflag=direct", "bs=4096"], stdin: b"")?
  uu.succeeds(r)
  assert uu.read(s, "direct-out.bin")? == input
}

# origin: uutils test_dd::test_block_record_across_reads
test test_uu_dd_block_record_across_reads { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "1234\n56\n123456789\n78")?
  let r = uu.invoke(s, "dd", ["if=in", "conv=block", "cbs=6", "ibs=2", "obs=2"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "1234  56    12345678    ")
  uu.stderr_contains(r, "1 truncated record")
}

# origin: uutils test_dd::test_block_sync_small_ibs
test test_uu_dd_block_sync_small_ibs { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "ab\ncdefg\nh")?
  let r = uu.invoke(s, "dd", ["if=in", "ibs=3", "cbs=4", "conv=block,sync", "status=noxfer"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_is(r, "ab  cdefh   ")
  uu.stderr_is(r, "3+1 records in\n0+1 records out\n1 truncated record\n")
}

# origin: uutils test_dd::test_block_sync
test test_uu_dd_block_sync { |ctx|
  let s = uu.scene(ctx)?
  for pair in [["012\nabcde\n", "012  abcde", "2+0 records in\n0+1 records out\n"], ["012\nabcdefg\n", "012  abcde     ", "2+1 records in\n0+1 records out\n1 truncated record\n"]] {
    let r = uu.invoke(s, "dd", ["ibs=5", "cbs=5", "conv=block,sync", "status=noxfer"], stdin: bytes.from_text(pair[0]))?
    uu.succeeds(r)
    uu.stdout_is(r, pair[1])
    uu.stderr_is(r, pair[2])
  }
}

# origin: uutils test_dd::test_bytes_suffix
test test_uu_dd_bytes_suffix { |ctx|
  let s = uu.scene(ctx)?
  for key in ["count", "skip", "iseek", "seek", "oseek"] {
    let r = uu.invoke(s, "dd", [f"{key}=3B", "status=none"], stdin: b"abcdef")?
    uu.succeeds(r)
    let expected = if key == "count" { b"abc" } else if key in ["skip", "iseek"] { b"def" } else { bytes.concat([bytes.zero(3)?, b"abcdef"]) }
    uu.stdout_only_bytes(r, expected)
  }
}

# origin: uutils test_dd::test_bytes_suffix_recursive
test test_uu_dd_bytes_suffix_recursive { |ctx|
  let s = uu.scene(ctx)?
  for key in ["count", "skip", "iseek", "seek", "oseek"] {
    let r = uu.invoke(s, "dd", [f"{key}=2Bx2", "status=none"], stdin: b"abcdef")?
    uu.succeeds(r)
    let expected = if key == "count" { b"abcd" } else if key in ["skip", "iseek"] { b"ef" } else { bytes.concat([bytes.zero(4)?, b"abcdef"]) }
    uu.stdout_only_bytes(r, expected)
  }
}

# origin: uutils test_dd::test_final_stats_less_than_one_kb_si
test test_uu_dd_final_stats_less_than_one_kb_si { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "dd", [], stdin: bytes.from_text(["0" for _ in range(999)].join("")))?
  uu.succeeds(r0)
  assert r0.stderr.utf8()?.starts_with("1+1 records in\n1+1 records out\n999 bytes copied,")
}

# origin: uutils test_dd::test_final_stats_less_than_one_kb_iec
test test_uu_dd_final_stats_less_than_one_kb_iec { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "dd", [], stdin: bytes.from_text(["0" for _ in range(1000)].join("")))?
  uu.succeeds(r0)
  assert r0.stderr.utf8()?.starts_with("1+1 records in\n1+1 records out\n1000 bytes (1.0 kB) copied,")
  let r1 = uu.invoke(s, "dd", [], stdin: bytes.from_text(["0" for _ in range(1023)].join("")))?
  uu.succeeds(r1)
  assert r1.stderr.utf8()?.starts_with("1+1 records in\n1+1 records out\n1023 bytes (1.0 kB) copied,")
}

# origin: uutils test_dd::test_final_stats_more_than_one_kb
test test_uu_dd_final_stats_more_than_one_kb { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "dd", [], stdin: bytes.from_text(["0" for _ in range(1024)].join("")))?
  uu.succeeds(r0)
  assert r0.stderr.utf8()?.starts_with("2+0 records in\n2+0 records out\n1024 bytes (1.0 kB, 1.0 KiB) copied,")
}

# origin: uutils test_dd::test_final_stats_three_char_limit
test test_uu_dd_final_stats_three_char_limit { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "dd", [], stdin: bytes.from_text(["0" for _ in range(10000)].join("")))?
  uu.succeeds(r0)
  assert r0.stderr.utf8()?.starts_with("19+1 records in\n19+1 records out\n10000 bytes (10 kB, 9.8 KiB) copied,")
  let r1 = uu.invoke(s, "dd", [], stdin: bytes.from_text(["0" for _ in range(100000)].join("")))?
  uu.succeeds(r1)
  assert r1.stderr.utf8()?.starts_with("195+1 records in\n195+1 records out\n100000 bytes (100 kB, 98 KiB) copied,")
}

# origin: uutils test_dd::test_final_stats_unspec
test test_uu_dd_final_stats_unspec { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", [], stdin: b"")?
  uu.succeeds(r)
  uu.stderr_contains(r, "0+0 records in\n0+0 records out\n0 bytes copied, ")
  assert rx"[0-9](\.[0-9]+)?(e-[0-9][0-9])? s, ".matches(r.stderr.utf8()?)
  uu.stderr_contains(r, "0.0 kB/s")
}

# origin: uutils test_dd::test_count_bytes_with_expanding_block_conv
test test_uu_dd_count_bytes_with_expanding_block_conv { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input.txt", ["a" for _ in range(1000)].join("") + ["Z" for _ in range(24)].join(""))?
  let r = uu.invoke(s, "dd", ["if=input.txt", "of=output.bin", "conv=block", "cbs=1024", "count=1000", "iflag=count_bytes"], stdin: b"")?
  uu.succeeds(r)
  let output = uu.read(s, "output.bin")?
  assert [v for v in range(output.len()) if output.byte_at(v) == 97].len() == 1000
  assert ! (b"Z" in output)
}

# origin: uutils test_dd::test_ascii_case_conversion_fallback
test test_uu_dd_ascii_case_conversion_fallback { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=lcase", "status=none"], stdin: b"ABC\x0a" , vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"abc\x0a")
  let reverse = uu.invoke(s, "dd", ["conv=ucase", "status=none"], stdin: b"abc\x0a" , vars: {LC_ALL: "C"})?
  uu.succeeds(reverse)
  uu.stdout_is_bytes(reverse, b"ABC\x0a")
}

# origin: uutils test_dd::test_locale_aware_case_conversion
test test_uu_dd_locale_aware_case_conversion { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["conv=lcase", "status=none"], stdin: b"\xd0\x0a" , vars: {LC_ALL: "tr_TR.iso8859-9"})?
  uu.succeeds(r)
  assert r.stdout.len() > 0
}

# origin: uutils test_dd::test_french_locale_case_conversion
test test_uu_dd_french_locale_case_conversion { |ctx|
  let s = uu.scene(ctx)?
  for pair in [{input: 192, conv: "lcase"}, {input: 224, conv: "ucase"}, {input: 199, conv: "lcase"}] {
    let r = uu.invoke(s, "dd", [f"conv={pair.conv}", "status=none"], stdin: bytes.from_ints([pair.input, 10])?, vars: {LC_ALL: "fr_FR.iso8859-1"})?
    uu.succeeds(r)
    assert r.stdout.len() > 0
  }
}

# origin: uutils test_dd::test_iso8859_1_case_conversion
test test_uu_dd_iso8859_1_case_conversion { |ctx|
  let s = uu.scene(ctx)?
  match process.which("locale") {
    Err(_) => {},
    Ok(executable) => {
      let out = uu.at(s, "locales")
      let plan = process.command_argv(executable, ["locale", "-a"], s.root, {}, b"", out, uu.at(s, "locale-errors"))
      let _status = process.run(plan)?
      if "fr_FR" in out.read_text()? {
        for pair in [{input: 201, expected: 233, conv: "lcase"}, {input: 233, expected: 201, conv: "ucase"}] {
          let r = uu.invoke(s, "dd", [f"conv={pair.conv}", "status=none"], stdin: bytes.from_ints([pair.input, 10])?, vars: {LC_ALL: "fr_FR"})?
          uu.succeeds(r)
          uu.stdout_is_bytes(r, bytes.from_ints([pair.expected, 10])?)
        }
      }
    },
  }
}

# origin: uutils test_dd::test_iflag_directory_fails_when_file_is_passed_via_std_in
test test_uu_dd_iflag_directory_fails_when_file_is_passed_via_std_in { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "input")?
  let r = uu.invoke_from_path(s, "dd", ["iflag=directory", "count=0"], stdin: uu.at(s, "input"))?
  uu.fails(r)
  uu.stderr_only(r, "dd: setting flags for 'standard input': Not a directory\n")
}

# origin: uutils test_dd::test_iflag_directory_passes_when_dir_is_redirected
test test_uu_dd_iflag_directory_passes_when_dir_is_redirected { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_from_path(s, "dd", ["iflag=directory", "count=0"], stdin: s.root)?
  uu.succeeds(r)
}

# origin: uutils test_dd::test_iflag_directory_fails_when_file_is_piped_via_std_in
test test_uu_dd_iflag_directory_fails_when_file_is_piped_via_std_in { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["iflag=directory"], stdin: b"")?
  uu.fails(r)
  uu.stderr_only(r, "dd: setting flags for 'standard input': Not a directory\n")
}

# origin: uutils test_dd::diagnostics::test_plain_message_keeps_the_try_help_hint_of_a_flag_message
test test_uu_dd_diagnostics_plain_message_keeps_the_try_help_hint_of_a_flag_message { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["iflag=nope"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "dd: invalid input flag: 'nope'")
  uu.stderr_contains(r, "--help' for more information.")
}

# origin: uutils test_dd::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_dd_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["bsx=1"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "dd: unrecognized operand 'bsx=1'\n")
  uu.stderr_contains(r, "--help' for more information.")
  assert ! ("╭─" in r.stderr.utf8()?)
}

# origin: uutils test_dd::diagnostics::test_unknown_mode_leaves_the_terminal_in_charge
test test_uu_dd_diagnostics_unknown_mode_leaves_the_terminal_in_charge { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["bsx=1"], stdin: b"" , vars: {UUTILS_DIAG: "sometimes"})?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "dd: unrecognized operand 'bsx=1'\n")
  assert ! ("╭─" in r.stderr.utf8()?)
}

# origin: uutils test_dd::help
test test_uu_dd_help { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dd", ["--help"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_contains(r, "\n  bs=BYTES")
  uu.stdout_contains(r, "\nEach CONV symbol may be:\n")
  assert ! ("###" in r.stdout.utf8()?)
  assert ! ("```" in r.stdout.utf8()?)
}

