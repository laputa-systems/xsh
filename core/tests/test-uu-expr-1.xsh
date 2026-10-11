##! Transcribed from the uutils coreutils expr integration tests.

use support.uu as uu

# origin: uutils test_expr::diagnostics::test_plain_message_is_the_default
test test_uu_expr_diagnostics_plain_message_is_the_default { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["6", "+", "7", "spare"])?
  uu.fails_with_code(r, 2)
  uu.stderr_is(r, "expr: syntax error: unexpected argument 'spare'\n")
}

# origin: uutils test_expr::expr_arithmetic::test_00
test test_uu_expr_expr_arithmetic_00 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["00"])?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "00\n")
}

# origin: uutils test_expr::expr_arithmetic::test_0bang
test test_uu_expr_expr_arithmetic_0bang { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["00", "<", "0!"])?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "0\n")
}

# origin: uutils test_expr::expr_arithmetic::test_add_with_negative
test test_uu_expr_expr_arithmetic_add_with_negative { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["6", "+", "-4"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_addition
test test_uu_expr_expr_arithmetic_addition { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["8", "+", "5"])?
  uu.succeeds(r)
  uu.stdout_only(r, "13\n")
}

# origin: uutils test_expr::expr_arithmetic::test_anchor
test test_uu_expr_expr_arithmetic_anchor { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a\nb", ":", "a$"])?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "0\n")
}

# origin: uutils test_expr::expr_arithmetic::test_andand
test test_uu_expr_expr_arithmetic_andand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["0", "&", "1", "/", "0"])?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "0\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bigcmp
test test_uu_expr_expr_arithmetic_bigcmp { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["--", "-2417851639229258349412352", "<", "2417851639229258349412352"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bignum_add
test test_uu_expr_expr_arithmetic_bignum_add { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["98782897298723498732987928734", "+", "1"])?
  uu.succeeds(r)
  uu.stdout_only(r, "98782897298723498732987928735\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bignum_add1
test test_uu_expr_expr_arithmetic_bignum_add1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["98782897298723498732987928734", "+", "98782897298723498732987928735"])?
  uu.succeeds(r)
  uu.stdout_only(r, "197565794597446997465975857469\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bignum_div
test test_uu_expr_expr_arithmetic_bignum_div { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["9758060798730154302876482828124348356960410232492450771490", "/", "98782897298723498732987928734"])?
  uu.succeeds(r)
  uu.stdout_only(r, "98782897298723498732987928735\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bignum_mul
test test_uu_expr_expr_arithmetic_bignum_mul { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["98782897298723498732987928735", "*", "98782897298723498732987928734"])?
  uu.succeeds(r)
  uu.stdout_only(r, "9758060798730154302876482828124348356960410232492450771490\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bignum_sub
test test_uu_expr_expr_arithmetic_bignum_sub { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["98782897298723498732987928735", "-", "1"])?
  uu.succeeds(r)
  uu.stdout_only(r, "98782897298723498732987928734\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bignum_sub1
test test_uu_expr_expr_arithmetic_bignum_sub1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["197565794597446997465975857469", "-", "98782897298723498732987928734"])?
  uu.succeeds(r)
  uu.stdout_only(r, "98782897298723498732987928735\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre1
test test_uu_expr_expr_arithmetic_bre1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abc", ":", "a\\(b\\)c"])?
  uu.succeeds(r)
  uu.stdout_only(r, "b\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre10
test test_uu_expr_expr_arithmetic_bre10 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a^b", ":", "a^b"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre11
test test_uu_expr_expr_arithmetic_bre11 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a$b", ":", "a$b"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre12
test test_uu_expr_expr_arithmetic_bre12 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["", ":", "\\($\\)\\(^\\)"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre13
test test_uu_expr_expr_arithmetic_bre13 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["b", ":", "a*\\(b$\\)c*"])?
  uu.succeeds(r)
  uu.stdout_only(r, "b\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre14
test test_uu_expr_expr_arithmetic_bre14 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["X|", ":", "X\\(|\\)", ":", "(", "X|", ":", "X\\(|\\)", ")"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre15
test test_uu_expr_expr_arithmetic_bre15 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["X*", ":", "X\\(*\\)", ":", "(", "X*", ":", "X\\(*\\)", ")"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre16
test test_uu_expr_expr_arithmetic_bre16 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abc", ":", "\\(\\)"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre17
test test_uu_expr_expr_arithmetic_bre17 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["{1}a", ":", "\\(\\{1\\}a\\)"])?
  uu.succeeds(r)
  uu.stdout_only(r, "{1}a\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre18
test test_uu_expr_expr_arithmetic_bre18 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["X*", ":", "X\\(*\\)", ":", "^*"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre19
test test_uu_expr_expr_arithmetic_bre19 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["{1}", ":", "\\{1\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre2
test test_uu_expr_expr_arithmetic_bre2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a(", ":", "a("])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre20
test test_uu_expr_expr_arithmetic_bre20 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["{", ":", "{"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre21
test test_uu_expr_expr_arithmetic_bre21 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abbcbd", ":", "a\\(b*\\)c\\1d"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre22
test test_uu_expr_expr_arithmetic_bre22 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abbcbbbd", ":", "a\\(b*\\)c\\1d"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre23
test test_uu_expr_expr_arithmetic_bre23 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abc", ":", "\\(.\\)\\1"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre24
test test_uu_expr_expr_arithmetic_bre24 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abbccd", ":", "a\\(\\([bc]\\)\\2\\)*d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "cc\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre25
test test_uu_expr_expr_arithmetic_bre25 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abbcbd", ":", "a\\(\\([bc]\\)\\2\\)*d"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre26
test test_uu_expr_expr_arithmetic_bre26 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abbbd", ":", "a\\(\\(b\\)*\\2\\)*d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "bbb\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre27
test test_uu_expr_expr_arithmetic_bre27 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aabcd", ":", "\\(a\\)\\1bcd"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre28
test test_uu_expr_expr_arithmetic_bre28 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aabcd", ":", "\\(a\\)\\1bc*d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre29
test test_uu_expr_expr_arithmetic_bre29 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aabd", ":", "\\(a\\)\\1bc*d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre3
test test_uu_expr_expr_arithmetic_bre3 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\("])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Unmatched ( or \\(")
}

# origin: uutils test_expr::expr_arithmetic::test_bre30
test test_uu_expr_expr_arithmetic_bre30 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aabcccd", ":", "\\(a\\)\\1bc*d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre31
test test_uu_expr_expr_arithmetic_bre31 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aabcccd", ":", "\\(a\\)\\1bc*[ce]d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre32
test test_uu_expr_expr_arithmetic_bre32 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aabcccd", ":", "\\(a\\)\\1b\\(c\\)*cd"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre33
test test_uu_expr_expr_arithmetic_bre33 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a*b", ":", "a\\(*\\)b"])?
  uu.succeeds(r)
  uu.stdout_only(r, "*\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre34
test test_uu_expr_expr_arithmetic_bre34 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["ab", ":", "a\\(**\\)b"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre35
test test_uu_expr_expr_arithmetic_bre35 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["ab", ":", "a\\(***\\)b"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre36
test test_uu_expr_expr_arithmetic_bre36 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["*a", ":", "*a"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre37
test test_uu_expr_expr_arithmetic_bre37 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a", ":", "**a"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre38
test test_uu_expr_expr_arithmetic_bre38 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a", ":", "***a"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre39
test test_uu_expr_expr_arithmetic_bre39 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["ab", ":", "a\\{1\\}b"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre4
test test_uu_expr_expr_arithmetic_bre4 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\(b"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Unmatched ( or \\(")
}

# origin: uutils test_expr::expr_arithmetic::test_bre40
test test_uu_expr_expr_arithmetic_bre40 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["ab", ":", "a\\{1,\\}b"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre41
test test_uu_expr_expr_arithmetic_bre41 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aab", ":", "a\\{1,2\\}b"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre42
test test_uu_expr_expr_arithmetic_bre42 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\{1"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Unmatched \\{")
}

# origin: uutils test_expr::expr_arithmetic::test_bre43
test test_uu_expr_expr_arithmetic_bre43 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\{1a"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Unmatched \\{")
}

# origin: uutils test_expr::expr_arithmetic::test_bre44
test test_uu_expr_expr_arithmetic_bre44 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\{1a\\}"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Invalid content of \\{\\}")
}

# origin: uutils test_expr::expr_arithmetic::test_bre45
test test_uu_expr_expr_arithmetic_bre45 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a", ":", "a\\{,2\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre46
test test_uu_expr_expr_arithmetic_bre46 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a", ":", "a\\{,\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre47
test test_uu_expr_expr_arithmetic_bre47 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\{1,x\\}"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Invalid content of \\{\\}")
}

# origin: uutils test_expr::expr_arithmetic::test_bre48
test test_uu_expr_expr_arithmetic_bre48 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\{1,x"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Unmatched \\{")
}

# origin: uutils test_expr::expr_arithmetic::test_bre49
test test_uu_expr_expr_arithmetic_bre49 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\{32768\\}"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Regular expression too big\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre5
test test_uu_expr_expr_arithmetic_bre5 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a(b", ":", "a(b"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre50
test test_uu_expr_expr_arithmetic_bre50 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\{1,0\\}"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_contains(r, "Invalid content of \\{\\}")
}

# origin: uutils test_expr::expr_arithmetic::test_bre51
test test_uu_expr_expr_arithmetic_bre51 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["acabc", ":", ".*ab\\{0,0\\}c"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre52
test test_uu_expr_expr_arithmetic_bre52 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abcac", ":", "ab\\{0,1\\}c"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre53
test test_uu_expr_expr_arithmetic_bre53 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abbcac", ":", "ab\\{0,3\\}c"])?
  uu.succeeds(r)
  uu.stdout_only(r, "4\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre54
test test_uu_expr_expr_arithmetic_bre54 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abcac", ":", ".*ab\\{1,1\\}c"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre55
test test_uu_expr_expr_arithmetic_bre55 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abcac", ":", ".*ab\\{1,3\\}c"])?
  uu.succeeds(r)
  uu.stdout_only(r, "3\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre56
test test_uu_expr_expr_arithmetic_bre56 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abbcabc", ":", ".*ab\\{2,2\\}c"])?
  uu.succeeds(r)
  uu.stdout_only(r, "4\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre57
test test_uu_expr_expr_arithmetic_bre57 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["abbcabc", ":", ".*ab\\{2,4\\}c"])?
  uu.succeeds(r)
  uu.stdout_only(r, "4\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre58
test test_uu_expr_expr_arithmetic_bre58 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aa", ":", "a\\{1\\}\\{1\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre59
test test_uu_expr_expr_arithmetic_bre59 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aa", ":", "a*\\{1\\}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre6
test test_uu_expr_expr_arithmetic_bre6 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["a)", ":", "a)"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre60
test test_uu_expr_expr_arithmetic_bre60 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["aa", ":", "a\\{1\\}*"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre61
test test_uu_expr_expr_arithmetic_bre61 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["acd", ":", "a\\(b\\)?c\\1d"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre62
test test_uu_expr_expr_arithmetic_bre62 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["--", "-5", ":", "-\\{0,1\\}[0-9]*$"])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_expr::expr_arithmetic::test_bre7
test test_uu_expr_expr_arithmetic_bre7 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "a\\)"])?
  uu.fails_with_code(r, 2)
  uu.stderr_contains(r, "Unmatched ) or \\)")
}

# origin: uutils test_expr::expr_arithmetic::test_bre8
test test_uu_expr_expr_arithmetic_bre8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["_", ":", "\\)"])?
  uu.fails_with_code(r, 2)
  uu.stderr_contains(r, "Unmatched ) or \\)")
}

# origin: uutils test_expr::expr_arithmetic::test_bre9
test test_uu_expr_expr_arithmetic_bre9 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["ab", ":", "a\\(\\)b"])?
  uu.fails_with_code(r, 1)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_double_dash_neg_large
test test_uu_expr_expr_arithmetic_double_dash_neg_large { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["--", "-7", "+", "15"])?
  uu.succeeds(r)
  uu.stdout_only(r, "8\n")
}

# origin: uutils test_expr::expr_arithmetic::test_double_dash_neg_small
test test_uu_expr_expr_arithmetic_double_dash_neg_small { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "expr", ["--", "-2", "+", "9"])?
  uu.succeeds(r)
  uu.stdout_only(r, "7\n")
}
