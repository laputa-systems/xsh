use support.uu

# origin: busybox seq/seq (exit with error)
test test_bb_seq_seq_exit_with_error_1c5cdc8f { |ctx|
  let s = uu.scene(ctx)?
  for args in [[], ["1", "2", "3", "4"]] {
    let r = uu.invoke(s, "seq", args)?
    uu.fails(r)
    uu.no_stdout(r)
  }
}

# origin: busybox seq/seq one argument
test test_bb_seq_seq_one_argument_486b43f1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["3"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n2\n3\n")
}

# origin: busybox seq/seq two arguments
test test_bb_seq_seq_two_arguments_f5e2a4a1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["5", "7"])?
  uu.succeeds(r)
  uu.stdout_only(r, "5\n6\n7\n")
}

# origin: busybox seq/seq two arguments reversed
test test_bb_seq_seq_two_arguments_reversed_ad51eb8d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["7", "5"])?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox seq/seq two arguments equal
test test_bb_seq_seq_two_arguments_equal_d568aa54 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["3", "3"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")
}

# origin: busybox seq/seq two arguments equal, arbitrary negative step
test test_bb_seq_seq_two_arguments_equal_arbitrary_negative_step_84ffbdcd { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["1", "-15", "1"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: busybox seq/seq two arguments equal, arbitrary positive step
test test_bb_seq_seq_two_arguments_equal_arbitrary_positive_step_47756806 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["1", "+15", "1"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: busybox seq/seq count up by 2
test test_bb_seq_seq_count_up_by_2_3f3ca2ba { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["4", "2", "8"])?
  uu.succeeds(r)
  uu.stdout_only(r, "4\n6\n8\n")
}

# origin: busybox seq/seq count down by 2
test test_bb_seq_seq_count_down_by_2_cc9c7e56 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["8", "-2", "4"])?
  uu.succeeds(r)
  uu.stdout_only(r, "8\n6\n4\n")
}

# origin: busybox seq/seq count wrong way #1
test test_bb_seq_seq_count_wrong_way_1_34c28876 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["4", "-2", "8"])?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox seq/seq count wrong way #2
test test_bb_seq_seq_count_wrong_way_2_6a3b4c95 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["8", "2", "4"])?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: busybox seq/seq count by .3
test test_bb_seq_seq_count_by_3_f20544ed { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["3", ".3", "4"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3.0\n3.3\n3.6\n3.9\n")
}

# origin: busybox seq/seq count by .30
test test_bb_seq_seq_count_by_30_adceb0f6 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["3", ".30", "4"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3.00\n3.30\n3.60\n3.90\n")
}

# origin: busybox seq/seq count by .30 to 4.000
test test_bb_seq_seq_count_by_30_to_4_000_a989d298 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["3", ".30", "4.000"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3.00\n3.30\n3.60\n3.90\n")
}

# origin: busybox seq/seq count by -.9
test test_bb_seq_seq_count_by_9_6ca71d66 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", [".7", "-.9", "-2.2"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0.7\n-0.2\n-1.1\n-2.0\n")
}

# origin: busybox seq/seq one argument with padding
test test_bb_seq_seq_one_argument_with_padding_e802837d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["-w", "003"])?
  uu.succeeds(r)
  uu.stdout_only(r, "001\n002\n003\n")
}

# origin: busybox seq/seq two arguments with padding
test test_bb_seq_seq_two_arguments_with_padding_303fce70 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["-w", "005", "7"])?
  uu.succeeds(r)
  uu.stdout_only(r, "005\n006\n007\n")
}

# origin: busybox seq/seq count down by 3 with padding
test test_bb_seq_seq_count_down_by_3_with_padding_7762371a { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["-w", "8", "-3", "04"])?
  uu.succeeds(r)
  uu.stdout_only(r, "08\n05\n")
}

# origin: busybox seq/seq count by .3 with padding 1
test test_bb_seq_seq_count_by_3_with_padding_1_544a86ed { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["-w", "09", ".3", "11"])?
  uu.succeeds(r)
  uu.stdout_only(r, "09.0\n09.3\n09.6\n09.9\n10.2\n10.5\n10.8\n")
}

# origin: busybox seq/seq count by .3 with padding 2
test test_bb_seq_seq_count_by_3_with_padding_2_9a2b2cbe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["-w", "03", ".3", "0004"])?
  uu.succeeds(r)
  uu.stdout_only(r, "0003.0\n0003.3\n0003.6\n0003.9\n")
}

