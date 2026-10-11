use support.uu as uu

# origin: busybox unexpand/unexpand case 1
test test_bb_unexpand_unexpand_case_1_02827c99 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", [], stdin: b"        12345678\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t12345678\n")
}

# origin: busybox unexpand/unexpand case 2
test test_bb_unexpand_unexpand_case_2_73e11d9f { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", [], stdin: b"         12345678\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t 12345678\n")
}

# origin: busybox unexpand/unexpand case 3
test test_bb_unexpand_unexpand_case_3_af6f9afe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", [], stdin: b"          12345678\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t  12345678\n")
}

# origin: busybox unexpand/unexpand case 4
test test_bb_unexpand_unexpand_case_4_4afbd65b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", [], stdin: b"       \t12345678\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t12345678\n")
}

# origin: busybox unexpand/unexpand case 5
test test_bb_unexpand_unexpand_case_5_9f07fd51 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", [], stdin: b"      \t12345678\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t12345678\n")
}

# origin: busybox unexpand/unexpand case 6
test test_bb_unexpand_unexpand_case_6_3147494b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", [], stdin: b"     \t12345678\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t12345678\n")
}

# origin: busybox unexpand/unexpand case 8
test test_bb_unexpand_unexpand_case_8_a4f94bd5 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", [], stdin: b"a b\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "a b\n")
}

# origin: busybox unexpand/unexpand flags
test test_bb_unexpand_unexpand_flags_7c5d208d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", [], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\ta       b    c")
}

# origin: busybox unexpand/unexpand flags --first-only -t4
test test_bb_unexpand_unexpand_flags_first_only_t4_09518562 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["--first-only", "-t4"], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t\ta       b    c")
}

# origin: busybox unexpand/unexpand flags -a
test test_bb_unexpand_unexpand_flags_a_27453cd0 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-a"], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\ta\tb    c")
}

# origin: busybox unexpand/unexpand flags -a -t4
test test_bb_unexpand_unexpand_flags_a_t4_849b857d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-a", "-t4"], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t\ta\t\tb\t c")
}

# origin: busybox unexpand/unexpand flags -a -t8
test test_bb_unexpand_unexpand_flags_a_t8_1fbd4252 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-a", "-t8"], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\ta\tb    c")
}

# origin: busybox unexpand/unexpand flags -f
test test_bb_unexpand_unexpand_flags_f_72b2b742 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-f"], stdin: b"        a       b    c")?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "unexpand: invalid option -- 'f'\nTry 'unexpand --help' for more information.\n")
}

# origin: busybox unexpand/unexpand flags -f -t4
test test_bb_unexpand_unexpand_flags_f_t4_aeb78bbd { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-f", "-t4"], stdin: b"        a       b    c")?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "unexpand: invalid option -- 'f'\nTry 'unexpand --help' for more information.\n")
}

# origin: busybox unexpand/unexpand flags -f -t8
test test_bb_unexpand_unexpand_flags_f_t8_196f0d4f { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-f", "-t8"], stdin: b"        a       b    c")?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "unexpand: invalid option -- 'f'\nTry 'unexpand --help' for more information.\n")
}

# origin: busybox unexpand/unexpand flags -t4
test test_bb_unexpand_unexpand_flags_t4_f3b2f3dc { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-t4"], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t\ta\t\tb\t c")
}

# origin: busybox unexpand/unexpand flags -t4 --first-only
test test_bb_unexpand_unexpand_flags_t4_first_only_e7d7b1ce { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-t4", "--first-only"], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t\ta       b    c")
}

# origin: busybox unexpand/unexpand flags -t4 -a
test test_bb_unexpand_unexpand_flags_t4_a_842ecc45 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-t4", "-a"], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\t\ta\t\tb\t c")
}

# origin: busybox unexpand/unexpand flags -t4 -f
test test_bb_unexpand_unexpand_flags_t4_f_297f7e9b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-t4", "-f"], stdin: b"        a       b    c")?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "unexpand: invalid option -- 'f'\nTry 'unexpand --help' for more information.\n")
}

# origin: busybox unexpand/unexpand flags -t8
test test_bb_unexpand_unexpand_flags_t8_d24e3e7b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-t8"], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\ta\tb    c")
}

# origin: busybox unexpand/unexpand flags -t8 --first-only
test test_bb_unexpand_unexpand_flags_t8_first_only_90f29c83 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-t8", "--first-only"], stdin: b"        a       b    c")?
  uu.succeeds(r)
  uu.stdout_only(r, "\ta       b    c")
}

# origin: busybox unexpand/unexpand flags -t8 -f
test test_bb_unexpand_unexpand_flags_t8_f_0b32d2bc { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-t8", "-f"], stdin: b"        a       b    c")?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "unexpand: invalid option -- 'f'\nTry 'unexpand --help' for more information.\n")
}
