use support.uu as uu

# origin: busybox tr/tr does not stop after [:digit:]
test test_bb_tr_tr_does_not_stop_after_digit_1df15799 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:digit:]y-z", "111111111123"], stdin: b"789abcxyz\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "111abcx23\n")
}

# origin: busybox tr/tr does not treat [] in [a-z] as special
test test_bb_tr_tr_does_not_treat_in_a_z_as_special_a1f0364f { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[q-z]", "_Q-Z+"], stdin: b"[qwe]")?
  uu.succeeds(r)
  uu.stdout_only(r, "_QWe+")
}

# origin: busybox tr/tr has correct xdigit sequence
test test_bb_tr_tr_has_correct_xdigit_sequence_6b8256e6 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:xdigit:]Gg", "1111111151242222333330xX"], stdin: b"#0123456789ABCDEFGabcdefg\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "#1111111151242222x333330X\n")
}

# origin: busybox tr/tr understands 0-9A-F
test test_bb_tr_tr_understands_0_9A_F_56303524 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-cd", "[0-9A-F]"], stdin: b"19AFH\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "19AF")
}

# origin: busybox tr/tr understands [:xdigit:]
test test_bb_tr_tr_understands_xdigit_c44ae2ca { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-cd", "[:xdigit:]"], stdin: b"19AFH\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "19AF")
}

# origin: busybox tr/tr-d-alnum-works
test test_bb_tr_tr_d_alnum_works_0fe90838 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[[:alnum:]]"], stdin: b"testing\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "\n")
}

# origin: busybox tr/tr-d-works
test test_bb_tr_tr_d_works_4c9811a6 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "aeiou"], stdin: b"testing\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "tstng\n")
}

# origin: busybox tr/tr-non-gnu
test test_bb_tr_tr_non_gnu_1dda373a { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[a-z]", "[n-z][a-m]"], stdin: b"fdhrnzvfu bffvsentr\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "squc]kgsf ossgdr]ec\n")
}

