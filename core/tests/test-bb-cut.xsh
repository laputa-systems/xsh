use support.uu as uu

# origin: busybox cut/-b encapsulated
test test_bb_cut_b_encapsulated_83c0e332 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-b", "3-8,4-6", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "e:two:\npha:be\ne quic\n")
}

# origin: busybox cut/cut '-' (stdin) and multi file handling
test test_bb_cut_cut_stdin_and_multi_file_handling_80e0e8b3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "the quick brown fox\n")?
  let r = uu.invoke(s, "cut", ["-d ", "-f2", "-", "input"], stdin: b"jumps over the lazy dog\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "over\nquick\n")
}

# origin: busybox cut/cut -b a,a,a
test test_bb_cut_cut_b_a_a_a_3e3cb692 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-b", "3,3,3", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "e\np\ne\n")
}

# origin: busybox cut/cut -b overlaps
test test_bb_cut_cut_b_overlaps_1c77040b { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-b", "1-3,2-5,7-9,9-10", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "one:to:th\nalphabeta\nthe qick \n")
}

# origin: busybox cut/cut -c -b
test test_bb_cut_cut_c_b_db0acf7a { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-c", "-39", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta\nthe quick brown fox jumps over the lazy\n")
}

# origin: busybox cut/cut -c a
test test_bb_cut_cut_c_a_a6dab94b { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-c", "40", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "\n:\n \n")
}

# origin: busybox cut/cut -c a,b-c,d
test test_bb_cut_cut_c_a_b_c_d_1bced7d6 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-c", "3,5-7,10", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "etwoh\npa:ba\nequi \n")
}

# origin: busybox cut/cut -c a-
test test_bb_cut_cut_c_a_0c74fe52 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-c", "41-", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "\ntheta:iota:kappa:lambda:mu\ndog\n")
}

# origin: busybox cut/cut -c a-b
test test_bb_cut_cut_c_a_b_3043ac06 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-c", "4-10", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, ":two:th\nha:beta\n quick \n")
}

# origin: busybox cut/cut -f a-
test test_bb_cut_cut_f_a_902eca23 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-d", ":", "-f", "5-", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "five:six:seven\nepsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")
}

# origin: busybox cut/cut empty field
test test_bb_cut_cut_empty_field_a31f49a0 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cut", ["-d", ":", "-f", "1-3"], stdin: b"a::b\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "a::b\n")
}

# origin: busybox cut/cut empty field 2
test test_bb_cut_cut_empty_field_2_2e016172 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cut", ["-d", ":", "-f", "3-5"], stdin: b"a::b::c:d\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "b::c\n")
}

# origin: busybox cut/cut high-low error
test test_bb_cut_cut_high_low_error_0502ae9a { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-b", "8-3", "abc.txt"], stdin: b"")?
  uu.fails(r)
  uu.no_stdout(r)
}

# origin: busybox cut/cut show whole line with no delim
test test_bb_cut_cut_show_whole_line_with_no_delim_b503b0f8 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nthe quick brown fox jumps over the lazy dog\n")?
  let r = uu.invoke(s, "cut", ["-d", " ", "-f", "3", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "one:two:three:four:five:six:seven\nalpha:beta:gamma:delta:epsilon:zeta:eta:theta:iota:kappa:lambda:mu\nbrown\n")
}

# origin: busybox cut/cut with -b (a,b,c)
test test_bb_cut_cut_with_b_a_b_c_9c001f9c { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "abcdefghijklmnopqrstuvwxyz")?
  let r = uu.invoke(s, "cut", ["-b", "4,5,20", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "det\n")
}

# origin: busybox cut/cut with -c (a,b,c)
test test_bb_cut_cut_with_c_a_b_c_ce698d17 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "abcdefghijklmnopqrstuvwxyz")?
  let r = uu.invoke(s, "cut", ["-c", "4,5,20", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "det\n")
}

# origin: busybox cut/cut with -d -f( ) -s
test test_bb_cut_cut_with_d_f_s_5eaead44 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "406378:Sales:Itorre:Jan\n031762:Marketing:Nasium:Jim\n636496:Research:Ancholie:Mel\n396082:Sales:Jucacion:Ed\n")?
  let r = uu.invoke(s, "cut", ["-d ", "-f3", "-s", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox cut/cut with -d -f(:) -s
test test_bb_cut_cut_with_d_f_s_69b14cae { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "406378:Sales:Itorre:Jan\n031762:Marketing:Nasium:Jim\n636496:Research:Ancholie:Mel\n396082:Sales:Jucacion:Ed\n")?
  let r = uu.invoke(s, "cut", ["-d:", "-f3", "-s", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "Itorre\nNasium\nAncholie\nJucacion\n")
}

# origin: busybox cut/cut with -d -f(a) -s
test test_bb_cut_cut_with_d_f_a_s_d61fc86f { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "406378:Sales:Itorre:Jan\n031762:Marketing:Nasium:Jim\n636496:Research:Ancholie:Mel\n396082:Sales:Jucacion:Ed\n")?
  let r = uu.invoke(s, "cut", ["-da", "-f3", "-s", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "n\nsium:Jim\n\ncion:Ed\n")
}

# origin: busybox cut/cut with -d -f(a) -s -n
test test_bb_cut_cut_with_d_f_a_s_n_24c311d3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "406378:Sales:Itorre:Jan\n031762:Marketing:Nasium:Jim\n636496:Research:Ancholie:Mel\n396082:Sales:Jucacion:Ed\n")?
  let r = uu.invoke(s, "cut", ["-da", "-f3", "-s", "-n", "input"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "n\nsium:Jim\n\ncion:Ed\n")
}

# origin: busybox cut/cut with echo, -c (a)
test test_bb_cut_cut_with_echo_c_a_820aea45 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cut", ["-c", "14"], stdin: b"ref_categorie=test\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "=\n")
}

# origin: busybox cut/cut with echo, -c (a-b)
test test_bb_cut_cut_with_echo_c_a_b_12a84efa { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cut", ["-c", "1-15"], stdin: b"ref_categorie=test\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "ref_categorie=t\n")
}

# origin: busybox cut/cut-cuts-a-character
test test_bb_cut_cut_cuts_a_character_3d323a0b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cut", ["-c", "3"], stdin: b"abcd\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "c\n")
}

# origin: busybox cut/cut-cuts-a-closed-range
test test_bb_cut_cut_cuts_a_closed_range_96035db4 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cut", ["-c", "1-2"], stdin: b"abcd\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "ab\n")
}

# origin: busybox cut/cut-cuts-a-field
test test_bb_cut_cut_cuts_a_field_5a69bf20 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cut", ["-f", "2"], stdin: b"f1\tf2\tf3\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "f2\n")
}

# origin: busybox cut/cut-cuts-an-open-range
test test_bb_cut_cut_cuts_an_open_range_6a7f1921 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cut", ["-c", "-3"], stdin: b"abcd\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "abc\n")
}

# origin: busybox cut/cut-cuts-an-unclosed-range
test test_bb_cut_cut_cuts_an_unclosed_range_c967f3f4 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cut", ["-c", "3-"], stdin: b"abcd\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "cd\n")
}

