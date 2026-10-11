use support.uu

# origin: busybox printf/printf handles multiple flags
test test_bb_printf_printf_handles_multiple_flags_5c1e72db{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%0 d\\n", "2"])?
  uu.succeeds(r)
  uu.stdout_only(r, " 2\n")
}

# origin: busybox printf/printf handles positive numbers for %d
test test_bb_printf_printf_handles_positive_numbers_for_d_763a6a23{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%d\\n", "3", "+3", "   3", "   +3"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n3\n3\n3\n")
}

# origin: busybox printf/printf handles positive numbers for %i
test test_bb_printf_printf_handles_positive_numbers_for_i_16d58db4{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%i\\n", "3", "+3", "   3", "   +3"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n3\n3\n3\n")
}

# origin: busybox printf/printf handles positive numbers for %x
test test_bb_printf_printf_handles_positive_numbers_for_x_566d5880{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%x\\n", "42", "+42", "   42", "   +42"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2a\n2a\n2a\n2a\n")
}

# origin: busybox printf/printf handles positive numbers for %f
test test_bb_printf_printf_handles_positive_numbers_for_f_d72e281f{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%0.3f\\n", ".42", "+.42", "   .42", "   +.42"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0.420\n0.420\n0.420\n0.420\n")
}

# origin: busybox printf/printf produces no further output 1
test test_bb_printf_printf_produces_no_further_output_1_f7319920{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["\\c", "foo"])?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox printf/printf produces no further output 2
test test_bb_printf_printf_produces_no_further_output_2_1f148528{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%s\\c", "foo", "bar"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo")
}

# origin: busybox printf/printf repeatedly uses pattern for each argv
test test_bb_printf_printf_repeatedly_uses_pattern_for_each_argv_f71aabda{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%s\\n", "foo", "$HOME"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\n$HOME\n")
}

# origin: busybox printf/printf treats leading 0 as flag
test test_bb_printf_printf_treats_leading_0_as_flag_72b81d0b{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%0*d\\n", "2", "1"])?
  uu.succeeds(r)
  uu.stdout_only(r, "01\n")
}

# origin: busybox printf/printf understands %%
test test_bb_printf_printf_understands_8ab03c1f{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%%\\n"])?
  uu.succeeds(r)
  uu.stdout_only(r, "%\n")
}

# origin: busybox printf/printf understands %*.*f
test test_bb_printf_printf_understands_f_33b5a793{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["|%*.*f|\\n", "23", "12", "5.25"])?
  uu.succeeds(r)
  uu.stdout_only(r, "|         5.250000000000|\n")
}

# origin: busybox printf/printf understands %*.*f with negative width/precision
test test_bb_printf_printf_understands_f_with_negative_width_precision_8421dcfa{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["|%*.*f|\\n", "-23", "-12", "5.25"])?
  uu.succeeds(r)
  uu.stdout_only(r, "|5.250000               |\n")
}

# origin: busybox printf/printf understands %*f with negative width
test test_bb_printf_printf_understands_f_with_negative_width_9a7a3883{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["|%*f|\\n", "-23", "5.25"])?
  uu.succeeds(r)
  uu.stdout_only(r, "|5.250000               |\n")
}

# origin: busybox printf/printf understands %.*f with negative precision
test test_bb_printf_printf_understands_f_with_negative_precision_9e8285b3{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["|%.*f|\\n", "-12", "5.25"])?
  uu.succeeds(r)
  uu.stdout_only(r, "|5.250000|\n")
}

# origin: busybox printf/printf understands %23.12f
test test_bb_printf_printf_understands_23_12f_a51d6e3e{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["|%23.12f|\\n", "5.25"])?
  uu.succeeds(r)
  uu.stdout_only(r, "|         5.250000000000|\n")
}

# origin: busybox printf/printf understands %Ld
test test_bb_printf_printf_understands_Ld_cab0d3fb{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%Ld\\n", "-5"])?
  uu.succeeds(r)
  uu.stdout_only(r, "-5\n")
}

# origin: busybox printf/printf understands %ld
test test_bb_printf_printf_understands_ld_3326e304{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%ld\\n", "-5"])?
  uu.succeeds(r)
  uu.stdout_only(r, "-5\n")
}

# origin: busybox printf/printf understands %zd
test test_bb_printf_printf_understands_zd_ff655be6{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%zd\\n", "-5"])?
  uu.succeeds(r)
  uu.stdout_only(r, "-5\n")
}

# origin: busybox printf/printf understands %b escaped_string
test test_bb_printf_printf_understands_b_escaped_string_4b135bfb{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%b", "a\\tb", "c\\d\\n"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\tbc\\d\n")
}

# origin: busybox printf/printf understands %s '"x' "'y" "'zTAIL"
test test_bb_printf_printf_understands_s_x_y_zTAIL_b72998fb{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%s\\n", "\"x", "'y", "'zTAIL"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\"x\n'y\n'zTAIL\n")
}

