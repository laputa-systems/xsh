use support.uu

# origin: busybox test/test ! -f: should be false (1)
test test_bb_test_test_f_should_be_false_1_bc2bfcaf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["!", "-f"])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox test/test ! a = b -a ! c = c: should be false (1)
test test_bb_test_test_a_b_a_c_c_should_be_false_1_fb9d83fe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["!", "a", "=", "b", "-a", "!", "c", "=", "c"])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox test/test ! a = b -a ! c = d: should be true (0)
test test_bb_test_test_a_b_a_c_d_should_be_true_0_f80e517e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["!", "a", "=", "b", "-a", "!", "c", "=", "d"])?
  uu.fails_with_code(r, 0)
  uu.no_output(r)
}

# origin: busybox test/test !: should be true (0)
test test_bb_test_test_should_be_true_0_a443163e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["!"])?
  uu.fails_with_code(r, 0)
  uu.no_output(r)
}

# origin: busybox test/test '!' '!' = '!': should be false (1)
test test_bb_test_test_should_be_false_1_d5f5cc2b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["!", "!", "=", "!"])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox test/test '!' '(' = '(': should be false (1)
test test_bb_test_test_should_be_false_1_1528d81e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["!", "(", "=", "("])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox test/test '!' = '!': should be true (0)
test test_bb_test_test_should_be_true_0_172eb998 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["!", "=", "!"])?
  uu.fails_with_code(r, 0)
  uu.no_output(r)
}

# origin: busybox test/test '': should be false (1)
test test_bb_test_test_should_be_false_1_771fa131 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", [""])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox test/test '(' = '(': should be true (0)
test test_bb_test_test_should_be_true_0_525e2187 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["(", "=", "("])?
  uu.fails_with_code(r, 0)
  uu.no_output(r)
}

# origin: busybox test/test --help: should be true (0)
test test_bb_test_test_help_should_be_true_0_be6af107 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["--help"])?
  uu.fails_with_code(r, 0)
  uu.no_output(r)
}

# origin: busybox test/test -f = a -o b: should be true (0)
test test_bb_test_test_f_a_o_b_should_be_true_0_6c9087ad { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["-f", "=", "a", "-o", "b"])?
  uu.fails_with_code(r, 0)
  uu.no_output(r)
}

# origin: busybox test/test -f: should be true (0)
test test_bb_test_test_f_should_be_true_0_d0eb734c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["-f"])?
  uu.fails_with_code(r, 0)
  uu.no_output(r)
}

# origin: busybox test/test -lt = -gt: should be false (1)
test test_bb_test_test_lt_gt_should_be_false_1_e5a600e0 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["-lt", "=", "-gt"])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox test/test a = a: should be true (0)
test test_bb_test_test_a_a_should_be_true_0_50f1971f { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["a", "=", "a"])?
  uu.fails_with_code(r, 0)
  uu.no_output(r)
}

# origin: busybox test/test a: should be true (0)
test test_bb_test_test_a_should_be_true_0_487251e8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", ["a"])?
  uu.fails_with_code(r, 0)
  uu.no_output(r)
}

# origin: busybox test/test: should be false (1)
test test_bb_test_test_should_be_false_1_514462a7 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "test", [])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

