use support.uu

# origin: busybox factor/factor '  0'
test test_bb_factor_factor_0_c12492c8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["  0"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "0:\n")
}

# origin: busybox factor/factor +1
test test_bb_factor_factor_1_dd7fb906 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["+1"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "1:\n")
}

# origin: busybox factor/factor ' +2'
test test_bb_factor_factor_2_5a520e3d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", [" +2"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "2: 2\n")
}

# origin: busybox factor/factor 1024
test test_bb_factor_factor_1024_5069a16a { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["1024"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "1024: 2 2 2 2 2 2 2 2 2 2\n")
}

# origin: busybox factor/factor 2^61-1
test test_bb_factor_factor_2_61_1_48d66b52 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["2305843009213693951"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "2305843009213693951: 2305843009213693951\n")
}

# origin: busybox factor/factor 2^62-1
test test_bb_factor_factor_2_62_1_85e9393d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["4611686018427387903"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "4611686018427387903: 3 715827883 2147483647\n")
}

# origin: busybox factor/factor 2^64-1
test test_bb_factor_factor_2_64_1_7d59cf96 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["18446744073709551615"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "18446744073709551615: 3 5 17 257 641 65537 6700417\n")
}

# origin: busybox factor/factor $((2*3*5*7*11*13*17*19*23*29*31*37*41*43*47))
test test_bb_factor_factor_2_3_5_7_11_13_17_19_23_29_31_37_41_43_47_222afa47 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["614889782588491410"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "614889782588491410: 2 3 5 7 11 13 17 19 23 29 31 37 41 43 47\n")
}

# origin: busybox factor/factor 2 * 3037000493 * 3037000493
test test_bb_factor_factor_2_3037000493_3037000493_8de89809 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["18446743988964486098"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "18446743988964486098: 2 3037000493 3037000493\n")
}

# origin: busybox factor/factor 3 * 2479700513 * 2479700513
test test_bb_factor_factor_3_2479700513_2479700513_c4ff9aa2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["18446743902517389507"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "18446743902517389507: 3 2479700513 2479700513\n")
}

# origin: busybox factor/factor 3 * 37831 * 37831 * 37831 * 37831
test test_bb_factor_factor_3_37831_37831_37831_37831_d67f664c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["6144867742934288163"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "6144867742934288163: 3 37831 37831 37831 37831\n")
}

# origin: busybox factor/factor 3 * 13^16
test test_bb_factor_factor_3_13_16_b2f0b7b1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["1996249827549539523"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "1996249827549539523: 3 13 13 13 13 13 13 13 13 13 13 13 13 13 13 13 13\n")
}

# origin: busybox factor/factor 13^16
test test_bb_factor_factor_13_16_a6e0ddaf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "factor", ["665416609183179841"], timeout: 20s)?
  uu.succeeds(r)
  uu.stdout_only(r, "665416609183179841: 13 13 13 13 13 13 13 13 13 13 13 13 13 13 13 13\n")
}

