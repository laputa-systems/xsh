##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_expr.rs.

use support.uu as uu

# origin: uutils test_expr::test_regex_range_quantifier
test test_uu_expr_regex_range_quantifier { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "expr", ["a", ":", "a\\{1\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")

  r = uu.invoke(s, "expr", ["aaaaaaaaaa", ":", "a\\{1,\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "10\n")

  r = uu.invoke(s, "expr", ["aaa", ":", "a\\{,3\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")

  r = uu.invoke(s, "expr", ["aa", ":", "a\\{1,3\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")

  r = uu.invoke(s, "expr", ["aaaa", ":", "a\\{,\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "4\n")

  r = uu.invoke(s, "expr", ["a", ":", "ab\\{,3\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")

  r = uu.invoke(s, "expr", ["abbb", ":", "ab\\{,3\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "4\n")

  r = uu.invoke(s, "expr", ["abcabc", ":", "\\(abc\\)\\{,\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "abc\n")

  r = uu.invoke(s, "expr", ["a", ":", "a\\{,6\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")

  r = uu.invoke(s, "expr", ["{abc}", ":", "\\{abc\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "5\n")

  r = uu.invoke(s, "expr", ["a{bc}", ":", "a\\(\\{bc\\}\\)"])?
  uu.succeeds(r)
  uu.stdout_only(r, "{bc}\n")

  r = uu.invoke(s, "expr", ["{b}", ":", "a\\|\\{b\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")

  r = uu.invoke(s, "expr", ["{", ":", "a\\|\\{"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")

  r = uu.invoke(s, "expr", ["{}}}", ":", "\\{\\}\\}\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "4\n")

  r = uu.invoke(s, "expr", ["a{}}}", ":", "a\\{\\}\\}\\}"])?
  uu.fails(r)
  uu.stderr_only(r, "expr: Invalid content of \\{\\}\n")

  r = uu.invoke(s, "expr", ["ab", ":", "ab\\{\\}"])?
  uu.fails(r)
  uu.stderr_only(r, "expr: Invalid content of \\{\\}\n")

  r = uu.invoke(s, "expr", ["_", ":", "a\\{12345678901234567890\\}"])?
  uu.fails(r)
  uu.stderr_only(r, "expr: Regular expression too big\n")

  r = uu.invoke(s, "expr", ["_", ":", "a\\{12345678901234567890,\\}"])?
  uu.fails(r)
  uu.stderr_only(r, "expr: Regular expression too big\n")

  r = uu.invoke(s, "expr", ["_", ":", "a\\{,12345678901234567890\\}"])?
  uu.fails(r)
  uu.stderr_only(r, "expr: Regular expression too big\n")

  r = uu.invoke(s, "expr", ["_", ":", "a\\{1,12345678901234567890\\}"])?
  uu.fails(r)
  uu.stderr_only(r, "expr: Regular expression too big\n")

  r = uu.invoke(s, "expr", ["_", ":", "a\\{1,1234567890abcdef\\}"])?
  uu.fails(r)
  uu.stderr_only(r, "expr: Invalid content of \\{\\}\n")
}

# origin: uutils test_expr::test_regex_trailing_backslash
test test_uu_expr_regex_trailing_backslash { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "expr", ["\\", ":", "\\\\"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")

  r = uu.invoke(s, "expr", ["\\", ":", "\\"])?
  uu.fails(r)
  uu.stderr_only(r, "expr: Trailing backslash\n")

  r = uu.invoke(s, "expr", ["abc\\", ":", "abc\\\\"])?
  uu.succeeds(r)
  uu.stdout_only(r, "4\n")

  r = uu.invoke(s, "expr", ["abc\\", ":", "abc\\"])?
  uu.fails(r)
  uu.stderr_only(r, "expr: Trailing backslash\n")
}

# origin: uutils test_expr::test_simple_arithmetic
test test_uu_expr_simple_arithmetic { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "expr", ["1", "+", "1"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")

  r = uu.invoke(s, "expr", ["1", "-", "1"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "0\n")

  r = uu.invoke(s, "expr", ["3", "*", "2"])?
  uu.succeeds(r)
  uu.stdout_only(r, "6\n")

  r = uu.invoke(s, "expr", ["4", "/", "2"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")

  r = uu.invoke(s, "expr", ["4", "=", "2"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "0\n")

  r = uu.invoke(s, "expr", ["4", "=", "4"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::test_simple_values
test test_uu_expr_simple_values { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "expr", [""])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")

  r = uu.invoke(s, "expr", ["0"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "0\n")

  r = uu.invoke(s, "expr", ["00"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "00\n")

  r = uu.invoke(s, "expr", ["-0"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "-0\n")

  r = uu.invoke(s, "expr", ["1"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::test_substr
test test_uu_expr_substr { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "expr", ["substr", "abc", "1", "1"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")

  r = uu.invoke(s, "expr", ["abc", "substr", "1", "1"])?
  uu.fails_with_code(r, 2)
  uu.stderr_only(r, "expr: syntax error: unexpected argument 'substr'\n")
}

# origin: uutils test_expr::test_substr_large_length_capacity_overflow
test test_uu_expr_substr_large_length_capacity_overflow { |ctx|
  let s = uu.scene(ctx)?
  var r = uu.invoke(s, "expr", ["substr", "abc", "1", "18446744073709551615"])?
  uu.succeeds(r)
  uu.stdout_only(r, "abc\n")
}
