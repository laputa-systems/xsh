use support.uu as uu

# origin: busybox sed/sed 's///w FILE'
test test_bb_sed_sed_s_w_FILE_d6d1b542 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s/qwe/ZZZ/wz"], stdin: b"123\nqwe\nasd\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"123\nZZZ\nasd\n")
  assert uu.read(s, "z")? == b"ZZZ\n"
}

# origin: busybox sed/sed -i finishes ranges correctly
test test_bb_sed_sed_i_finishes_ranges_correctly_3c12d64c { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"1\n2\n3\n4\n")?
  let r = uu.invoke(s, "sed", ["1,2d", "-i", "input"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
  assert uu.read(s, "input")? == b"3\n4\n"
}

# origin: busybox sed/sed -i with address modifies all files, not only first
test test_bb_sed_sed_i_with_address_modifies_all_files_not_only_first_48dcecf6 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"foo\n")?
  uu.write_bytes(s, "input2", b"foo\n")?
  let r = uu.invoke(s, "sed", ["-i", "-e", "1s/foo/bar/", "input", "input2"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
  assert uu.read(s, "input")? == b"bar\n"
  assert uu.read(s, "input2")? == b"bar\n"
}

# origin: busybox sed/sed -i with no arg [GNUFAIL]
test test_bb_sed_sed_i_with_no_arg_GNUFAIL_1ae21f53 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "-", b"")?
  let r = uu.invoke(s, "sed", ["-e", "", "-i"], stdin: b"", timeout: 5s)?
  uu.fails(r)
  uu.stdout_is_bytes(r, b"")
}

# origin: busybox sed/sed -n
test test_bb_sed_sed_n_c172b8ce { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-n", "-e", "s/foo/bar/", "-e", "s/bar/baz/"], stdin: b"foo\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
}

# origin: busybox sed/sed -n s//p
test test_bb_sed_sed_n_s_p_5eaea8ef { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-ne", "s/abc/def/p"], stdin: b"abc\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"def\n")
}

# origin: busybox sed/sed /$_in_regex/ should not match newlines, only end-of-line
test test_bb_sed_sed__in_regex_should_not_match_newlines_only_end_of_line_07aa27d8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", [": testcont; /\\\\$/{ =; N; b testcont }"], stdin: b"this is a regular line\nline with \\\ncontinuation\nmore regular lines\nline with \\\ncontinuation\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"this is a regular line\n2\nline with \\\ncontinuation\nmore regular lines\n5\nline with \\\ncontinuation\n")
}

# origin: busybox sed/sed /regex/,+0<cmd> -i works
test test_bb_sed_sed_regex_0_cmd_i_works_efba3b87 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"1\n2\n3\n4\n5\n6\n7\n8\n")?
  uu.write_bytes(s, "input2", b"1\n2\n4\n5\n6\n7\n8\n")?
  let r = uu.invoke(s, "sed", ["/^4/,+0d", "-i", "input", "input2"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
  assert uu.read(s, "input")? == b"1\n2\n3\n5\n6\n7\n8\n"
  assert uu.read(s, "input2")? == b"1\n2\n5\n6\n7\n8\n"
}

# origin: busybox sed/sed /regex/,+0{...} -i works
test test_bb_sed_sed_regex_0_i_works_0ad5389b { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"1\n2\n3\n4\n5\n6\n7\n8\n")?
  uu.write_bytes(s, "input2", b"1\n2\n4\n5\n6\n7\n8\n")?
  let r = uu.invoke(s, "sed", ["/^4/,+0{d}", "-i", "input", "input2"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
  assert uu.read(s, "input")? == b"1\n2\n3\n5\n6\n7\n8\n"
  assert uu.read(s, "input2")? == b"1\n2\n5\n6\n7\n8\n"
}

# origin: busybox sed/sed /regex/,+N{...} -i works
test test_bb_sed_sed_regex_N_i_works_e23e1052 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"1\n2\n3\n4\n5\n6\n7\n8\n")?
  uu.write_bytes(s, "input2", b"1\n2\n4\n5\n6\n7\n8\n")?
  let r = uu.invoke(s, "sed", ["/^4/,+2{d}", "-i", "input", "input2"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
  assert uu.read(s, "input")? == b"1\n2\n3\n7\n8\n"
  assert uu.read(s, "input2")? == b"1\n2\n7\n8\n"
}

# origin: busybox sed/sed /regex/,+N{...} addresses work
test test_bb_sed_sed_regex_N_addresses_work_1cfa74cb { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["/^2/,+2{d}"], stdin: b"1\n2\n3\n4\n5\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"1\n5\n")
}

# origin: busybox sed/sed /regex/,+N{...} addresses work 2
test test_bb_sed_sed_regex_N_addresses_work_2_2eacc914 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-n", "/a/,+1 p"], stdin: b"a\n1\nc\nc\na\n2\na\n3\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a\n1\na\n2\na\n3\n")
}

# origin: busybox sed/sed /regex/,N{...} addresses work
test test_bb_sed_sed_regex_N_addresses_work_78c5f002 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["/^2/,2{d}"], stdin: b"1\n2\n3\n4\n5\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"1\n3\n4\n5\n")
}

# origin: busybox sed/sed 2d;2,1p (gnu compat)
test test_bb_sed_sed_2d_2_1p_gnu_compat_9ff933da { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-n", "2d;2,1p"], stdin: b"first\nsecond\nthird\nfourth\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"third\n")
}

# origin: busybox sed/sed G (append hold space to pattern space)
test test_bb_sed_sed_G_append_hold_space_to_pattern_space_0806db3c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["G"], stdin: b"a\nb\nc\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a\n\nb\n\nc\n\n")
}

# origin: busybox sed/sed N (flushes pattern space (GNU behavior))
test test_bb_sed_sed_N_flushes_pattern_space_GNU_behavior_831b3f7b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "N;p"], stdin: b"a\nb\nc\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a\nb\na\nb\nc\n")
}

# origin: busybox sed/sed N (stops at end of input) and P (prints to first newline only)
test test_bb_sed_sed_N_stops_at_end_of_input_and_P_prints_to_first_newline_only_51c39ff0 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-n", "N;P;p"], stdin: b"a\nb\nc\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a\na\nb\n")
}

# origin: busybox sed/sed N test2
test test_bb_sed_sed_N_test2_dd2e5125 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", [":a;N;s/\\n/ /;ta"], stdin: b"a\nb\nc\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a b c\n")
}

# origin: busybox sed/sed N test3
test test_bb_sed_sed_N_test3_a115fb18 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["N;s/\\n/ /"], stdin: b"a\nb\nc\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a b\nc\n")
}

# origin: busybox sed/sed T (!test/branch)
test test_bb_sed_sed_T_test_branch_7093ff38 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/a/1/;T notone;p;: notone;p"], stdin: b"a\nb\nc\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"1\n1\n1\nb\nb\nc\nc\n")
}

# origin: busybox sed/sed ^ OR not^
test test_bb_sed_sed_OR_not_cfd133ba { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/^a\\|b//g"], stdin: b"abca\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"ca\n")
}

# origin: busybox sed/sed a cmd ended by double backslash
test test_bb_sed_sed_a_cmd_ended_by_double_backslash_c45dba53 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "/| one /a \\\n\t| three \\\\", "-e", "/| one-/a \\\n\t| three-* \\\\"], stdin: b"\t| one \\\n\t| two \\\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"\t| one \\\n\t| three \\\n\t| two \\\n")
}

# origin: busybox sed/sed a cmd understands \n,\t,\r
test test_bb_sed_sed_a_cmd_understands_n_t_r_75b1f06f { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["/1/a\\\\t\\rzero\\none\\\\ntwo\\\\\\nthree"], stdin: b"line1\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"line1\n\t\rzero\none\\ntwo\\\nthree\n")
}

# origin: busybox sed/sed accepts blanks before command
test test_bb_sed_sed_accepts_blanks_before_command_dc187443 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "1 d"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
}

# origin: busybox sed/sed accepts multiple -e
test test_bb_sed_sed_accepts_multiple_e_6edb9da8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "i\\", "-e", "1", "-e", "a\\", "-e", "3"], stdin: b"2\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"1\n2\n3\n")
}

# origin: busybox sed/sed accepts newlines in -e
test test_bb_sed_sed_accepts_newlines_in_e_e836b0cc { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "i\\\n1\na\\\n3"], stdin: b"2\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"1\n2\n3\n")
}

# origin: busybox sed/sed address match newline
test test_bb_sed_sed_address_match_newline_2bd19987 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["/b/N;/b\\nc/i woo"], stdin: b"a\nb\nc\nd\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a\nwoo\nb\nc\nd\n")
}

# origin: busybox sed/sed append autoinserts newline
test test_bb_sed_sed_append_autoinserts_newline_c3afb84e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "/woot/a woo", "-"], stdin: b"woot", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"woot\nwoo\n")
}

# origin: busybox sed/sed append autoinserts newline 2
test test_bb_sed_sed_append_autoinserts_newline_2_b927dfb8 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"boot")?
  let r = uu.invoke(s, "sed", ["-e", "/oot/a woo", "-", "input"], stdin: b"woot", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"woot\nwoo\nboot\nwoo\n")
}

# origin: busybox sed/sed append autoinserts newline 3
test test_bb_sed_sed_append_autoinserts_newline_3_246a2f55 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"boot")?
  let r = uu.invoke(s, "sed", ["-e", "/oot/a woo", "-i", "input"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
  assert uu.read(s, "input")? == b"boot\nwoo\n"
}

# origin: busybox sed/sed autoinsert newline
test test_bb_sed_sed_autoinsert_newline_7cfe5a74 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"woo")?
  let r = uu.invoke(s, "sed", ["-e", "s/woo/bang/", "input", "-"], stdin: b"woo", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"bang\nbang")
}

# origin: busybox sed/sed b (branch with no label jumps to end)
test test_bb_sed_sed_b_branch_with_no_label_jumps_to_end_3b0f9b6d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "b;p"], stdin: b"foo\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"foo\n")
}

# origin: busybox sed/sed b (branch)
test test_bb_sed_sed_b_branch_cafdd132 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "b one;p;: one"], stdin: b"foo\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"foo\n")
}

# origin: busybox sed/sed backref from empty s uses range regex
test test_bb_sed_sed_backref_from_empty_s_uses_range_regex_2f67c448 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "/woot/s//eep \\0 eep/"], stdin: b"woot", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"eep woot eep")
}

# origin: busybox sed/sed backref from empty s uses range regex with newline
test test_bb_sed_sed_backref_from_empty_s_uses_range_regex_with_newline_edde14d6 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "/woot/s//eep \\0 eep/"], stdin: b"woot\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"eep woot eep\n")
}

# origin: busybox sed/sed beginning (^) matches only once
test test_bb_sed_sed_beginning_matches_only_once_8885e36a { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s,\\(^/\\|\\)[^/][^/]*,>\\0<,g"], stdin: b"/usr/lib\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b">/usr</>lib<\n")
}

# origin: busybox sed/sed c
test test_bb_sed_sed_c_31e9d6a6 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["crepl"], stdin: b"first\nsecond\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"repl\nrepl\n")
}

# origin: busybox sed/sed cat plus empty file
test test_bb_sed_sed_cat_plus_empty_file_3086a501 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"one\ntwo")?
  let r = uu.invoke(s, "sed", ["-e", "s/nohit//", "input", "-"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"one\ntwo")
}

# origin: busybox sed/sed clusternewline
test test_bb_sed_sed_clusternewline_34f28451 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"one")?
  let r = uu.invoke(s, "sed", ["-e", "/one/a 111", "-e", "/two/i 222", "-e", "p", "input", "-"], stdin: b"two", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"one\none\n111\n222\ntwo\ntwo")
}

# origin: busybox sed/sed d does not break n,m matching
test test_bb_sed_sed_d_does_not_break_n_m_matching_3c306d87 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-n", "1d;1,3p"], stdin: b"first\nsecond\nthird\nfourth\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"second\nthird\n")
}

# origin: busybox sed/sed d does not break n,regex matching
test test_bb_sed_sed_d_does_not_break_n_regex_matching_5366f126 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-n", "1d;1,/hir/p"], stdin: b"first\nsecond\nthird\nfourth\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"second\nthird\n")
}

# origin: busybox sed/sed d does not break n,regex matching #2
test test_bb_sed_sed_d_does_not_break_n_regex_matching_2_a2a03bac { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-n", "1,5d;1,/hir/p"], stdin: b"first\nsecond\nthird\nfourth\nfirst2\nsecond2\nthird2\nfourth2\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"second2\nthird2\n")
}

# origin: busybox sed/sed d ends script iteration
test test_bb_sed_sed_d_ends_script_iteration_dc1b4c8b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "/ook/d;s/ook/ping/p;i woot"], stdin: b"ook\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
}

# origin: busybox sed/sed d ends script iteration (2)
test test_bb_sed_sed_d_ends_script_iteration_2_74e32f6d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "/ook/d;a\\", "-e", "bang"], stdin: b"ook\nwoot\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"woot\nbang\n")
}

# origin: busybox sed/sed embedded NUL g
test test_bb_sed_sed_embedded_NUL_g_d82a9a6c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/woo/bang/g"], stdin: b"woo\0woo\0", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"bang\0bang\0")
}

# origin: busybox sed/sed empty file plus cat
test test_bb_sed_sed_empty_file_plus_cat_4fd3d535 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"")?
  let r = uu.invoke(s, "sed", ["-e", "s/nohit//", "input", "-"], stdin: b"one\ntwo", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"one\ntwo")
}

# origin: busybox sed/sed escaped newline in command
test test_bb_sed_sed_escaped_newline_in_command_2eaf03d6 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"a")?
  let r = uu.invoke(s, "sed", ["s/a/z\\\nz/", "input"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"z\nz")
}

# origin: busybox sed/sed explicit stdin
test test_bb_sed_sed_explicit_stdin_a77887f9 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["", "-"], stdin: b"hello\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"hello\n")
}

# origin: busybox sed/sed handles empty lines
test test_bb_sed_sed_handles_empty_lines_01877bfc { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/$/@/"], stdin: b"\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"@\n")
}

# origin: busybox sed/sed i cmd understands \n,\t,\r
test test_bb_sed_sed_i_cmd_understands_n_t_r_6b284088 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["/1/i\\\\t\\rzero\\none\\\\ntwo\\\\\\nthree"], stdin: b"line1\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"\t\rzero\none\\ntwo\\\nthree\nline1\n")
}

# origin: busybox sed/sed insert doesn't autoinsert newline
test test_bb_sed_sed_insert_doesn_t_autoinsert_newline_af591a21 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "/woot/i woo", "-"], stdin: b"woot", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"woo\nwoot")
}

# origin: busybox sed/sed leave off trailing newline
test test_bb_sed_sed_leave_off_trailing_newline_fe196d74 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"woo\n")?
  let r = uu.invoke(s, "sed", ["-e", "s/woo/bang/", "input", "-"], stdin: b"woo", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"bang\nbang")
}

# origin: busybox sed/sed lie-to-autoconf
test test_bb_sed_sed_lie_to_autoconf_861911e0 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["--version"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  assert r.stdout.utf8()?.split("GNU sed version ").len() == 2
  uu.no_stderr(r)
}

# origin: busybox sed/sed match EOF
test test_bb_sed_sed_match_EOF_9bac1a29 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "$p"], stdin: b"hello\nthere", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"hello\nthere\nthere")
}

# origin: busybox sed/sed match EOF inline
test test_bb_sed_sed_match_EOF_inline_053b8c16 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"one\ntwo")?
  uu.write_bytes(s, "input2", b"three\nfour")?
  let r = uu.invoke(s, "sed", ["-e", "$i ook", "-i", "input", "input2"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
  assert uu.read(s, "input")? == b"one\nook\ntwo"
  assert uu.read(s, "input2")? == b"three\nook\nfour"
}

# origin: busybox sed/sed match EOF two files
test test_bb_sed_sed_match_EOF_two_files_7c3c25c7 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"one\ntwo")?
  let r = uu.invoke(s, "sed", ["-e", "$p", "input", "-"], stdin: b"three\nfour", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"one\ntwo\nthree\nfour\nfour")
}

# origin: busybox sed/sed n (flushes pattern space, terminates early)
test test_bb_sed_sed_n_flushes_pattern_space_terminates_early_464a9ce6 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "n;p"], stdin: b"a\nb\nc\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a\nb\nb\nc\n")
}

# origin: busybox sed/sed n command must reset 'substituted' bit
test test_bb_sed_sed_n_command_must_reset_substituted_bit_9f197d47 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s/1/x/;T;n;: next;s/3/y/;t quit;n;b next;: quit;q"], stdin: b"0\n1\n2\n3\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"0\nx\n2\ny\n")
}

# origin: busybox sed/sed nested {}s
test test_bb_sed_sed_nested_s_f5f4c711 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["/asd/ { p; /s/ { s/s/c/ }; p; q }"], stdin: b"qwe\nasd\nzxc\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"qwe\nasd\nacd\nacd\n")
}

# origin: busybox sed/sed no files (stdin)
test test_bb_sed_sed_no_files_stdin_c44f9ab9 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", [""], stdin: b"hello\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"hello\n")
}

# origin: busybox sed/sed noprint, no match, no newline
test test_bb_sed_sed_noprint_no_match_no_newline_fa062fa7 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"no\n")?
  let r = uu.invoke(s, "sed", ["-ne", "s/woo/bang/", "input"], stdin: b"", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"")
}

# origin: busybox sed/sed normal newlines
test test_bb_sed_sed_normal_newlines_a3cedd0b { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"woo\n")?
  let r = uu.invoke(s, "sed", ["-e", "s/woo/bang/", "input", "-"], stdin: b"woo\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"bang\nbang\n")
}

# origin: busybox sed/sed print autoinsert newlines
test test_bb_sed_sed_print_autoinsert_newlines_2b788599 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "p", "-"], stdin: b"one", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"one\none")
}

# origin: busybox sed/sed print autoinsert newlines two files
test test_bb_sed_sed_print_autoinsert_newlines_two_files_9dc06c22 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"one")?
  let r = uu.invoke(s, "sed", ["-e", "p", "input", "-"], stdin: b"two", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"one\none\ntwo\ntwo")
}

# origin: busybox sed/sed s [delimiter]
test test_bb_sed_sed_s_delimiter_aeec8f45 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s@[@]@@"], stdin: b"one@two", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"onetwo")
}

# origin: busybox sed/sed s arbitrary delimiter
test test_bb_sed_sed_s_arbitrary_delimiter_17ebc540 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s woo boing "], stdin: b"woo\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"boing\n")
}

# origin: busybox sed/sed s chains
test test_bb_sed_sed_s_chains_141bd878 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/foo/bar/", "-e", "s/bar/baz/"], stdin: b"foo\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"baz\n")
}

# origin: busybox sed/sed s chains2
test test_bb_sed_sed_s_chains2_e4f41db3 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/foo/bar/", "-e", "s/baz/nee/"], stdin: b"foo\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"bar\n")
}

# origin: busybox sed/sed s with \t (GNU ext)
test test_bb_sed_sed_s_with_t_GNU_ext_dd30b820 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s/\t/ /"], stdin: b"one\ttwo", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"one two")
}

# origin: busybox sed/sed s///NUM test
test test_bb_sed_sed_s_NUM_test_6956e96f { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/a/b/2; s/a/c/g"], stdin: b"aa\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"cb\n")
}

# origin: busybox sed/sed s//g (exhaustive)
test test_bb_sed_sed_s_g_exhaustive_400a7450 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/[[:space:]]*/,/g"], stdin: b"12345\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b",1,2,3,4,5,\n")
}

# origin: busybox sed/sed s//p
test test_bb_sed_sed_s_p_6ddcad68 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/foo/bar/p", "-e", "s/bar/baz/p"], stdin: b"foo\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"bar\nbaz\nbaz\n")
}

# origin: busybox sed/sed s/xxx/[/
test test_bb_sed_sed_s_xxx_bba90a5e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["-e", "s/xxx/[/"], stdin: b"xxx\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"[\n")
}

# origin: busybox sed/sed selective matches insert newline
test test_bb_sed_sed_selective_matches_insert_newline_e73e4218 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"a woo\nb woo")?
  let r = uu.invoke(s, "sed", ["-ne", "s/woo/bang/p", "input", "-"], stdin: b"c no\nd woo", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a bang\nb bang\nd bang")
}

# origin: busybox sed/sed selective matches noinsert newline
test test_bb_sed_sed_selective_matches_noinsert_newline_2c94ec2d { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"a woo\nb woo")?
  let r = uu.invoke(s, "sed", ["-ne", "s/woo/bang/p", "input", "-"], stdin: b"c no\nd no", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a bang\nb bang")
}

# origin: busybox sed/sed selective matches with one nl
test test_bb_sed_sed_selective_matches_with_one_nl_ff00b69e { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "input", b"a woo\nb no")?
  let r = uu.invoke(s, "sed", ["-ne", "s/woo/bang/p", "input", "-"], stdin: b"c woo\nd no", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"a bang\nc bang\n")
}

# origin: busybox sed/sed special char as s/// delimiter, in pattern
test test_bb_sed_sed_special_char_as_s_delimiter_in_pattern_e8e6c0e7 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sed", ["s+9\\++X+"], stdin: b"9+8=17\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"X8=17\n")
}
