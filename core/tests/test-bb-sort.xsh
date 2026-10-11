use support.uu

# origin: busybox sort/sort
test test_bb_sort_sort_400543f8 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"c\na\nb\n")?
  let r = uu.invoke(s, "sort", ["input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a\nb\nc\n")
}

# origin: busybox sort/sort #2
test test_bb_sort_sort_2_00ec2929 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"3\n1\n010\n")?
  let r = uu.invoke(s, "sort", ["input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"010\n1\n3\n")
}

# origin: busybox sort/sort stdin
test test_bb_sort_sort_stdin_eee0dda5 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", [], stdin: b"b\na\nc\n")?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a\nb\nc\n")
}

# origin: busybox sort/sort numeric
test test_bb_sort_sort_numeric_146ae6c6 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"3\n1\n010\n")?
  let r = uu.invoke(s, "sort", ["-n", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"1\n3\n010\n")
}

# origin: busybox sort/sort reverse
test test_bb_sort_sort_reverse_2bccebfa { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"point\nwook\npabst\naargh\nwalrus\n")?
  let r = uu.invoke(s, "sort", ["-r", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"wook\nwalrus\npoint\npabst\naargh\n")
}

# origin: busybox sort/sort one key
test test_bb_sort_sort_one_key_858f534d { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"42\t1\t3\twoot\n42\t1\t010\tzoology\negg\t1\t2\tpapyrus\n7\t3\t42\tsoup\n999\t3\t0\talgebra\n")?
  let r = uu.invoke(s, "sort", ["-k4,4", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"999\t3\t0\talgebra\negg\t1\t2\tpapyrus\n7\t3\t42\tsoup\n42\t1\t3\twoot\n42\t1\t010\tzoology\n")
}

# origin: busybox sort/sort key range with numeric option
test test_bb_sort_sort_key_range_with_numeric_option_7d8e5bcb { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"42\t1\t3\twoot\n42\t1\t010\tzoology\negg\t1\t2\tpapyrus\n7\t3\t42\tsoup\n999\t3\t0\talgebra\n")?
  let r = uu.invoke(s, "sort", ["-k2,3n", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"42\t1\t010\tzoology\n42\t1\t3\twoot\negg\t1\t2\tpapyrus\n7\t3\t42\tsoup\n999\t3\t0\talgebra\n")
}

# origin: busybox sort/sort key range with multiple options
test test_bb_sort_sort_key_range_with_multiple_options_1a1c466e { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"42\t1\t3\twoot\n42\t1\t010\tzoology\negg\t1\t2\tpapyrus\n7\t3\t42\tsoup\n999\t3\t0\talgebra\n")?
  let r = uu.invoke(s, "sort", ["-k2,3rn", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"7\t3\t42\tsoup\n999\t3\t0\talgebra\n42\t1\t010\tzoology\n42\t1\t3\twoot\negg\t1\t2\tpapyrus\n")
}

# origin: busybox sort/sort key range with two -k options
test test_bb_sort_sort_key_range_with_two_k_options_5de270ae { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"c 3\nb 2\nd 2\n")?
  let r = uu.invoke(s, "sort", ["-k", "2,2n", "-k", "1,1r", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"d 2\nb 2\nc 3\n")
}

# origin: busybox sort/sort with non-default leading delim 1
test test_bb_sort_sort_with_non_default_leading_delim_1_a580b1bc { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"/a/2\n/b/1\n")?
  let r = uu.invoke(s, "sort", ["-n", "-k2", "-t/", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"/a/2\n/b/1\n")
}

# origin: busybox sort/sort with non-default leading delim 2
test test_bb_sort_sort_with_non_default_leading_delim_2_e2f2bc21 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"/b/1\n/a/2\n")?
  let r = uu.invoke(s, "sort", ["-n", "-k3", "-t/", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"/b/1\n/a/2\n")
}

# origin: busybox sort/sort with non-default leading delim 3
test test_bb_sort_sort_with_non_default_leading_delim_3_0659534e { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"//a/2\n//b/1\n")?
  let r = uu.invoke(s, "sort", ["-n", "-k3", "-t/", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"//a/2\n//b/1\n")
}

# origin: busybox sort/sort with non-default leading delim 4
test test_bb_sort_sort_with_non_default_leading_delim_4_6b932e45 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"a/a:a\na:b\n")?
  let r = uu.invoke(s, "sort", ["-t:", "-k1,1", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a:b\na/a:a\n")
}

# origin: busybox sort/sort with ENDCHAR
test test_bb_sort_sort_with_ENDCHAR_2f9d09a3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"aa.2\nab.1\n")?
  let r = uu.invoke(s, "sort", ["-t.", "-k1,1.1", "-k2", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"ab.1\naa.2\n")
}

# origin: busybox sort/glibc build sort
test test_bb_sort_glibc_build_sort_c091e69c { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"GLIBC_2.21\nGLIBC_2.1.1\nGLIBC_2.2.1\nGLIBC_2.2\nGLIBC_2.20\nGLIBC_2.10\nGLIBC_2.1\n")?
  let r = uu.invoke(s, "sort", ["-t.", "-k", "1,1", "-k", "2n,2n", "-k", "3", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"GLIBC_2.1\nGLIBC_2.1.1\nGLIBC_2.2\nGLIBC_2.2.1\nGLIBC_2.10\nGLIBC_2.20\nGLIBC_2.21\n")
}

# origin: busybox sort/glibc build sort unique
test test_bb_sort_glibc_build_sort_unique_3657cbb1 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"GLIBC_2.10\nGLIBC_2.2.1\nGLIBC_2.1.1\nGLIBC_2.20\nGLIBC_2.2\nGLIBC_2.1\nGLIBC_2.21\n")?
  let r = uu.invoke(s, "sort", ["-u", "-t.", "-k", "1,1", "-k", "2n,2n", "-k", "3", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"GLIBC_2.1\nGLIBC_2.1.1\nGLIBC_2.2\nGLIBC_2.2.1\nGLIBC_2.10\nGLIBC_2.20\nGLIBC_2.21\n")
}

# origin: busybox sort/sort -u should consider field only when discarding
test test_bb_sort_sort_u_should_consider_field_only_when_discarding_3bef37ea { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"a c\nb c\n")?
  let r = uu.invoke(s, "sort", ["-u", "-k2", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a c\n")
}

# origin: busybox sort/sort -z outputs NUL terminated lines
test test_bb_sort_sort_z_outputs_NUL_terminated_lines_b3c756b5 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"one\0two\0three\0")?
  let r = uu.invoke(s, "sort", ["-z", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"one\0three\0two\0")
}

# origin: busybox sort/sort key doesn't strip leading blanks, disables fallback global sort
test test_bb_sort_sort_key_doesn_t_strip_leading_blanks_disables_fallback_global_sort_d009ecf1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-n", "-k2", "-t", " "], stdin: b" 2 \n 1 \n a \n")?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b" a \n 1 \n 2 \n")
}

# origin: busybox sort/sort file in place
test test_bb_sort_sort_file_in_place_b713dfec { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"222\n111\n")?
  let r = uu.invoke(s, "sort", ["-o", "input", "input"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.read(s, "input")? == b"111\n222\n"
}

# origin: busybox sort/sort -sr (stable and reverse) does NOT reverse 'stable' ordering
test test_bb_sort_sort_sr_stable_and_reverse_does_NOT_reverse_stable_ordering_a6b492a2 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"a 1\nb 2\nc 1\nd 2\n")?
  let r = uu.invoke(s, "sort", ["-k2", "-r", "-s", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"b 2\nd 2\na 1\nc 1\n")
}

# origin: busybox sort/sort -h
test test_bb_sort_sort_h_843d36dc { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"1Y\n5y\n1M\n2E\n3k\n3e\n2K\n4m\n1023\n1025\n3000\n1024\n")?
  let r = uu.invoke(s, "sort", ["-h", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"3e\n4m\n5y\n1023\n1024\n1025\n3000\n2K\n3k\n1M\n2E\n1Y\n")
}

# origin: busybox sort/sort -k2,2M
test test_bb_sort_sort_k2_2M_bfd5e7c8 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"2 April\n1  May\n3 March\n")?
  let r = uu.invoke(s, "sort", ["-k2,2M", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"3 March\n2 April\n1  May\n")
}

# origin: busybox sort/sort -s -u
test test_bb_sort_sort_s_u_b87ac301 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"z b\na b\nz a\na a")?
  let r = uu.invoke(s, "sort", ["-s", "-u", "-k", "2", "input"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"z a\nz b\n")
}

