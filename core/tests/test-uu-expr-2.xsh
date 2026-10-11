##! Transcribed from the uutils coreutils integration tests for expr.

use support.uu as uu

# origin: uutils test_expr::expr_arithmetic::test_double_dash_positive
test test_uu_expr_expr_arithmetic_double_dash_positive { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["--", "3", "+", "6"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "9\n")
}

# origin: uutils test_expr::expr_arithmetic::test_emptysub
test test_uu_expr_expr_arithmetic_emptysub { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["a", ":", "\\(b\\)*"])?
  uu.fails(r1)
  uu.fails_with_code(r1, 1)
  uu.stdout_only(r1, "\n")
}

# origin: uutils test_expr::expr_arithmetic::test_fail_a
test test_uu_expr_expr_arithmetic_fail_a { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["3", "+", "-"])?
  uu.fails(r1)
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "non-integer argument")
}

# origin: uutils test_expr::expr_arithmetic::test_fail_c
test test_uu_expr_expr_arithmetic_fail_c { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", [])?
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "missing operand")
  uu.stderr_contains(r1, "Try")
  uu.stderr_contains(r1, "for more information")
}

# origin: uutils test_expr::expr_arithmetic::test_integer_division
test test_uu_expr_expr_arithmetic_integer_division { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["120", "/", "8"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "15\n")
}

# origin: uutils test_expr::expr_arithmetic::test_minus0
test test_uu_expr_expr_arithmetic_minus0 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["-0"])?
  uu.fails(r1)
  uu.fails_with_code(r1, 1)
  uu.stdout_only(r1, "-0\n")
}

# origin: uutils test_expr::expr_arithmetic::test_modulo_remainder
test test_uu_expr_expr_arithmetic_modulo_remainder { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["120", "%", "7"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_multiplication
test test_uu_expr_expr_arithmetic_multiplication { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["7", "*", "6"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "42\n")
}

# origin: uutils test_expr::expr_arithmetic::test_neg_large_no_dash
test test_uu_expr_expr_arithmetic_neg_large_no_dash { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["-7", "+", "15"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "8\n")
}

# origin: uutils test_expr::expr_arithmetic::test_neg_small_no_dash
test test_uu_expr_expr_arithmetic_neg_small_no_dash { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["-2", "+", "9"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "7\n")
}

# origin: uutils test_expr::expr_arithmetic::test_orempty
test test_uu_expr_expr_arithmetic_orempty { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["", "|", ""])?
  uu.fails(r1)
  uu.fails_with_code(r1, 1)
  uu.stdout_only(r1, "0\n")
}

# origin: uutils test_expr::expr_arithmetic::test_oror
test test_uu_expr_expr_arithmetic_oror { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["1", "|", "1", "/", "0"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_parens_add_mod
test test_uu_expr_expr_arithmetic_parens_add_mod { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["9", "+", "(", "120", "%", "7", ")"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "10\n")
}

# origin: uutils test_expr::expr_arithmetic::test_parens_div_mod_minus
test test_uu_expr_expr_arithmetic_parens_div_mod_minus { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["18", "/", "(", "120", "%", "7", ")", "-", "6"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "12\n")
}

# origin: uutils test_expr::expr_arithmetic::test_parens_div_nested_mod
test test_uu_expr_expr_arithmetic_parens_div_nested_mod { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["24", "/", "(", "(", "120", "%", "7", ")", "+", "5", ")"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "4\n")
}

# origin: uutils test_expr::expr_arithmetic::test_parens_mod_minus
test test_uu_expr_expr_arithmetic_parens_mod_minus { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["(", "120", "%", "7", ")", "-", "8"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-7\n")
}

# origin: uutils test_expr::expr_arithmetic::test_parens_simple_mod
test test_uu_expr_expr_arithmetic_parens_simple_mod { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["(", "120", "%", "7", ")"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n")
}

# origin: uutils test_expr::expr_arithmetic::test_se0
test test_uu_expr_expr_arithmetic_se0 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["9", "9"])?
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "syntax error: unexpected argument '9'")
}

# origin: uutils test_expr::expr_arithmetic::test_se1
test test_uu_expr_expr_arithmetic_se1 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["2", "a"])?
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "syntax error: unexpected argument 'a'")
}

# origin: uutils test_expr::expr_arithmetic::test_se2
test test_uu_expr_expr_arithmetic_se2 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["2", "+"])?
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "syntax error: missing argument after '+'")
}

# origin: uutils test_expr::expr_arithmetic::test_se3
test test_uu_expr_expr_arithmetic_se3 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["2", ":"])?
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "syntax error: missing argument after ':'")
}

# origin: uutils test_expr::expr_arithmetic::test_se4
test test_uu_expr_expr_arithmetic_se4 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["length"])?
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "syntax error: missing argument after 'length'")
}

# origin: uutils test_expr::expr_arithmetic::test_se5
test test_uu_expr_expr_arithmetic_se5 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["(", "2"])?
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "syntax error: expecting ')' after '2'")
}

# origin: uutils test_expr::expr_arithmetic::test_se6
test test_uu_expr_expr_arithmetic_se6 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["(", "2", "a"])?
  uu.fails_with_code(r1, 2)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "syntax error: expecting ')' instead of 'a'")
}

# origin: uutils test_expr::expr_arithmetic::test_subtraction
test test_uu_expr_expr_arithmetic_subtraction { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["9", "-", "4"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "5\n")
}

# origin: uutils test_expr::expr_arithmetic::test_two_negatives_added
test test_uu_expr_expr_arithmetic_two_negatives_added { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["-3", "+", "-5"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-8\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_i1
test test_uu_expr_expr_multibyte_arithmetic_i1 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"index")?, Path.parse_bytes(b"abcdef")?, Path.parse_bytes(b"fb")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"2\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"2\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_i2
test test_uu_expr_expr_multibyte_arithmetic_i2 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"index")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"b")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"2\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"3\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_i3
test test_uu_expr_expr_multibyte_arithmetic_i3 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"index")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"f")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"6\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"8\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_i4
test test_uu_expr_expr_multibyte_arithmetic_i4 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"index")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"\xCE\xB4")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"4\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"1\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_i5
test test_uu_expr_expr_multibyte_arithmetic_i5 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"index")?, Path.parse_bytes(b"\xCEbc\xCE\xB4ef")?, Path.parse_bytes(b"\xCE\xB4")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"4\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"1\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_i6
test test_uu_expr_expr_multibyte_arithmetic_i6 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"index")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"\xB4")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 1)
  uu.stdout_is_bytes(r1, b"0\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"6\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_i7
test test_uu_expr_expr_multibyte_arithmetic_i7 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"index")?, Path.parse_bytes(b"\xCE\xB1bc\xB4ef")?, Path.parse_bytes(b"\xB4")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"4\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_m1
test test_uu_expr_expr_multibyte_arithmetic_m1 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"match")?, Path.parse_bytes(b"abcdef")?, Path.parse_bytes(b"ab")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"2\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"2\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_m2
test test_uu_expr_expr_multibyte_arithmetic_m2 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"match")?, Path.parse_bytes(b"abcdef")?, Path.parse_bytes(b"\\(ab\\)")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"ab\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"ab\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_m3
test test_uu_expr_expr_multibyte_arithmetic_m3 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"match")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b".bc")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"3\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 1)
  uu.stdout_is_bytes(r2, b"0\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_m4
test test_uu_expr_expr_multibyte_arithmetic_m4 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"match")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"..bc")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 1)
  uu.stdout_is_bytes(r1, b"0\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"4\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_m5
test test_uu_expr_expr_multibyte_arithmetic_m5 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"match")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"\\(.b\\)c")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"\xCE\xB1b\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 1)
  uu.stdout_is_bytes(r2, b"\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_m6
test test_uu_expr_expr_multibyte_arithmetic_m6 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"match")?, Path.parse_bytes(b"\xCEbc\xCE\xB4ef")?, Path.parse_bytes(b"\\(.\\)")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 1)
  uu.stdout_is_bytes(r1, b"\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"\xCE\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_m7
test test_uu_expr_expr_multibyte_arithmetic_m7 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"match")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"\\(.\\)")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"\xCE\xB1\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"\xCE\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_mb_length_ascii_middle
test test_uu_expr_expr_multibyte_arithmetic_mb_length_ascii_middle { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"length")?, Path.parse_bytes(b"abc\xCE\xB4ef")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"6\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"7\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_mb_length_ascii_prefix
test test_uu_expr_expr_multibyte_arithmetic_mb_length_ascii_prefix { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"length")?, Path.parse_bytes(b"\xCE\xB1bcdef")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"6\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"7\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_mb_length_ascii_suffix
test test_uu_expr_expr_multibyte_arithmetic_mb_length_ascii_suffix { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"length")?, Path.parse_bytes(b"fedcb\xCE\xB1")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"6\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"7\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_mb_length_expression
test test_uu_expr_expr_multibyte_arithmetic_mb_length_expression { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"length")?, Path.parse_bytes(b"汉字测试")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"4\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"12\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_mb_length_full
test test_uu_expr_expr_multibyte_arithmetic_mb_length_full { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"length")?, Path.parse_bytes(b"abcdef")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"6\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"6\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_mb_length_incomplete_end
test test_uu_expr_expr_multibyte_arithmetic_mb_length_incomplete_end { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"length")?, Path.parse_bytes(b"aaa\xCE")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"4\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"4\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_mb_length_invalid_seq
test test_uu_expr_expr_multibyte_arithmetic_mb_length_invalid_seq { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"length")?, Path.parse_bytes(b"\xB1aaa")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"4\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"4\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_s1
test test_uu_expr_expr_multibyte_arithmetic_s1 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"substr")?, Path.parse_bytes(b"abcdef")?, Path.parse_bytes(b"2")?, Path.parse_bytes(b"3")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"bcd\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"bcd\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_s2
test test_uu_expr_expr_multibyte_arithmetic_s2 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"substr")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"1")?, Path.parse_bytes(b"1")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"\xCE\xB1\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"\xCE\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_s3
test test_uu_expr_expr_multibyte_arithmetic_s3 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"substr")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"3")?, Path.parse_bytes(b"2")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"c\xCE\xB4\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"bc\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_s4
test test_uu_expr_expr_multibyte_arithmetic_s4 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"substr")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"4")?, Path.parse_bytes(b"1")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"\xCE\xB4\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"c\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_s5
test test_uu_expr_expr_multibyte_arithmetic_s5 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"substr")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"4")?, Path.parse_bytes(b"2")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"\xCE\xB4e\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"c\xCE\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_s6
test test_uu_expr_expr_multibyte_arithmetic_s6 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"substr")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"6")?, Path.parse_bytes(b"1")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"f\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"\xB4\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_s7
test test_uu_expr_expr_multibyte_arithmetic_s7 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"substr")?, Path.parse_bytes(b"\xCE\xB1bc\xCE\xB4ef")?, Path.parse_bytes(b"7")?, Path.parse_bytes(b"1")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 1)
  uu.stdout_is_bytes(r1, b"\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"e\n")
}

# origin: uutils test_expr::expr_multibyte_arithmetic::test_s8
test test_uu_expr_expr_multibyte_arithmetic_s8 { |ctx|
  let s = uu.scene(ctx)?
  let args = [Path.parse_bytes(b"substr")?, Path.parse_bytes(b"\xCE\xB1bc\xB4ef")?, Path.parse_bytes(b"3")?, Path.parse_bytes(b"3")?]
  let r1 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails_with_code(r1, 0)
  uu.stdout_is_bytes(r1, b"c\xB4e\n")
  let r2 = uu.invoke_paths(s, "expr", args, vars: {LC_ALL: "C"})?
  uu.fails_with_code(r2, 0)
  uu.stdout_is_bytes(r2, b"bc\xB4\n")
}

# origin: uutils test_expr::locale_aware::test_expr_collating
test test_uu_expr_locale_aware_expr_collating { |ctx|
  let s = uu.scene(ctx)?
  for case in [
    {locale: "C", code: 0, output: "1\n"},
    {locale: "fr_FR.UTF-8", code: 1, output: "0\n"},
    {locale: "fr_FR.utf-8", code: 1, output: "0\n"},
    {locale: "en_US", code: 1, output: "0\n"},
  ] {
    let r = uu.invoke(s, "expr", ["50n", ">", "-51"], vars: {LC_ALL: case.locale})?
    uu.fails_with_code(r, case.code)
    uu.stdout_only(r, case.output)
  }
}

# origin: uutils test_expr::test_and
test test_uu_expr_and { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["foo", "&", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "foo\n")
  let r2 = uu.invoke(s, "expr", ["14", "&", "1"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "14\n")
  let r3 = uu.invoke(s, "expr", ["-14", "&", "1"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-14\n")
  let r4 = uu.invoke(s, "expr", ["-1", "&", "10", "/", "5"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "-1\n")
  let r5 = uu.invoke(s, "expr", ["0", "&", "a", "/", "5"])?
  uu.fails(r5)
  uu.stdout_only(r5, "0\n")
  let r6 = uu.invoke(s, "expr", ["", "&", "a", "/", "5"])?
  uu.fails(r6)
  uu.stdout_only(r6, "0\n")
  let r7 = uu.invoke(s, "expr", ["", "&", "1"])?
  uu.fails(r7)
  uu.stdout_only(r7, "0\n")
  let r8 = uu.invoke(s, "expr", ["", "&", ""])?
  uu.fails(r8)
  uu.stdout_only(r8, "0\n")
}

# origin: uutils test_expr::test_builtin_functions_precedence
test test_uu_expr_builtin_functions_precedence { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["substr", "ab cd", "3", "1", "!=", " "])?
  uu.fails_with_code(r1, 1)
  uu.stdout_only(r1, "0\n")
  let r2 = uu.invoke(s, "expr", ["substr", "ab cd", "3", "1", "=", " "])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "1\n")
  let r3 = uu.invoke(s, "expr", ["length", "abcd", "!=", "4"])?
  uu.fails_with_code(r3, 1)
  uu.stdout_only(r3, "0\n")
  let r4 = uu.invoke(s, "expr", ["length", "abcd", "=", "4"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "1\n")
  let r5 = uu.invoke(s, "expr", ["index", "abcd", "c", "!=", "3"])?
  uu.fails_with_code(r5, 1)
  uu.stdout_only(r5, "0\n")
  let r6 = uu.invoke(s, "expr", ["index", "abcd", "c", "=", "3"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "1\n")
  let r7 = uu.invoke(s, "expr", ["match", "abcd", "ab\\(.*\\)", "!=", "cd"])?
  uu.fails_with_code(r7, 1)
  uu.stdout_only(r7, "0\n")
  let r8 = uu.invoke(s, "expr", ["match", "abcd", "ab\\(.*\\)", "=", "cd"])?
  uu.succeeds(r8)
  uu.stdout_only(r8, "1\n")
}

# origin: uutils test_expr::test_complex_arithmetic
test test_uu_expr_complex_arithmetic { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["9223372036854775807", "+", "9223372036854775807"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18446744073709551614\n")
  let r2 = uu.invoke(s, "expr", ["92233720368547758076549841651981984981498415651", "%", "922337203685",])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "533691697086\n")
  let r3 = uu.invoke(s, "expr", ["92233720368547758076549841651981984981498415651", "*", "922337203685",])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "85070591730190566808700855121818604965830915152801178873935\n")
  let r4 = uu.invoke(s, "expr", ["92233720368547758076549841651981984981498415651", "-", "922337203685",])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "92233720368547758076549841651981984059161211966\n")
  let r5 = uu.invoke(s, "expr", ["9", "/", "0"])?
  uu.fails(r5)
  uu.stderr_only(r5, "expr: division by zero\n")
}

# origin: uutils test_expr::test_deeply_nested_expression
test test_uu_expr_deeply_nested_expression { |ctx|
  let s = uu.scene(ctx)?
  let args = [@["(" for _ in range(10000)], "1", @[")" for _ in range(10000)]]
  let r = uu.invoke(s, "expr", args)?
  uu.succeeds(r)
  uu.stdout_is(r, "1\n")
}

# origin: uutils test_expr::test_deeply_nested_length
test test_uu_expr_deeply_nested_length { |ctx|
  let s = uu.scene(ctx)?
  let args = ["length" for _ in range(10000)].extend(["1"])
  let r = uu.invoke(s, "expr", args)?
  uu.succeeds(r)
  uu.stdout_is(r, "1\n")
}

# origin: uutils test_expr::test_emoji_operations
test test_uu_expr_emoji_operations { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["🚀", "=", "🚀"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n")
  let r2 = uu.invoke(s, "expr", ["🚀", "!=", "🚀"])?
  uu.fails(r2)
  uu.stdout_only(r2, "0\n")
  let r3 = uu.invoke(s, "expr", ["🚀", "=", "🧨"])?
  uu.fails(r3)
  uu.stdout_only(r3, "0\n")
  let r4 = uu.invoke(s, "expr", ["length", "🦀🚀🎯"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(r4)
  uu.stdout_only(r4, "3\n")
  let r5 = uu.invoke(s, "expr", ["🌍", "!=", "🌎"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "1\n")
}

# origin: uutils test_expr::test_escape
test test_uu_expr_escape { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["+", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n")
  let r2 = uu.invoke(s, "expr", ["1", "+", "+", "1"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "2\n")
  let r3 = uu.invoke(s, "expr", ["2", "*", "+", "3"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "6\n")
  let r4 = uu.invoke(s, "expr", ["(", "1", ")", "+", "1"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "2\n")
}

# origin: uutils test_expr::test_invalid_substr
test test_uu_expr_invalid_substr { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["substr", "abc", "0", "1"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_only(r1, "\n")
  let r2 = uu.invoke(s, "expr", ["substr", "abc", "184467440737095516150", "1"])?
  uu.fails_with_code(r2, 1)
  uu.stdout_only(r2, "\n")
  let r3 = uu.invoke(s, "expr", ["substr", "abc", "0", "184467440737095516150"])?
  uu.fails_with_code(r3, 1)
  uu.stdout_only(r3, "\n")
}

# origin: uutils test_expr::test_invalid_syntax
test test_uu_expr_invalid_syntax { |ctx|
  let s = uu.scene(ctx)?
  for args in [["12", "12"], ["12", "|"], ["|", "12"]] {
    let r = uu.invoke(s, "expr", args)?
    uu.fails_with_code(r, 2)
    uu.stderr_contains(r, "syntax error")
  }
}

# origin: uutils test_expr::test_length
test test_uu_expr_length { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["length", "abcdef"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "6\n")
  let r2 = uu.invoke(s, "expr", ["abcdef", "length"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_only(r2, "expr: syntax error: unexpected argument 'length'\n")
}

# origin: uutils test_expr::test_length_fail
test test_uu_expr_length_fail { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["length", "αbcdef", "1"])?
  uu.fails(r1)
}

# origin: uutils test_expr::test_long_input
test test_uu_expr_long_input { |ctx|
  let s = uu.scene(ctx)?
  let args: List[Str] = collect {
    yield "1"
    for n in range(2, 40001) {
      yield "+"
      yield f"{n}"
    }
  }
  let r = uu.invoke(s, "expr", args)?
  uu.succeeds(r)
  uu.stdout_is(r, "800020000\n")
}

# origin: uutils test_expr::test_missing_closing_parenthesis_reports_syntax_error
test test_uu_expr_missing_closing_parenthesis_reports_syntax_error { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["(", "1", "/", "0"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "division by zero")
}

# origin: uutils test_expr::test_no_arguments
test test_uu_expr_no_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", [])?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "expr: missing operand\nTry 'expr --help' for more information.\n")
}

# origin: uutils test_expr::test_num_str_comparison
test test_uu_expr_num_str_comparison { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["1a", "<", "1", "+", "1"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\n")
}

# origin: uutils test_expr::test_or
test test_uu_expr_or { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["0", "|", "foo"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "foo\n")
  let r2 = uu.invoke(s, "expr", ["foo", "|", "bar"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "foo\n")
  let r3 = uu.invoke(s, "expr", ["14", "|", "1"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "14\n")
  let r4 = uu.invoke(s, "expr", ["-14", "|", "1"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "-14\n")
  let r5 = uu.invoke(s, "expr", ["1", "|", "a", "/", "5"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "1\n")
  let r6 = uu.invoke(s, "expr", ["foo", "|", "a", "/", "5"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "foo\n")
  let r7 = uu.invoke(s, "expr", ["0", "|", "10", "/", "5"])?
  uu.succeeds(r7)
  uu.stdout_only(r7, "2\n")
  let r8 = uu.invoke(s, "expr", ["12", "|", "9a", "+", "1"])?
  uu.succeeds(r8)
  uu.stdout_only(r8, "12\n")
  let r9 = uu.invoke(s, "expr", ["", "|", ""])?
  uu.fails(r9)
  uu.stdout_only(r9, "0\n")
  let r10 = uu.invoke(s, "expr", ["", "|", "0"])?
  uu.fails(r10)
  uu.stdout_only(r10, "0\n")
  let r11 = uu.invoke(s, "expr", ["", "|", "00"])?
  uu.fails(r11)
  uu.stdout_only(r11, "0\n")
  let r12 = uu.invoke(s, "expr", ["", "|", "-0"])?
  uu.fails(r12)
  uu.stdout_only(r12, "0\n")
}

# origin: uutils test_expr::test_parenthesis
test test_uu_expr_parenthesis { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["(", "1", "+", "1", ")", "*", "2"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "4\n")
  let r2 = uu.invoke(s, "expr", ["1", "(", ")"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_only(r2, "expr: syntax error: unexpected argument '('\n")
}

# origin: uutils test_expr::test_parenthesized_short_circuit_dead_branches
test test_uu_expr_parenthesized_short_circuit_dead_branches { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["1", "|", "(", "1", "/", "0", ")"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n")
  let r2 = uu.invoke(s, "expr", ["0", "&", "(", "1", "/", "0", ")"])?
  uu.fails_with_code(r2, 1)
  uu.stdout_only(r2, "0\n")
  let r3 = uu.invoke(s, "expr", ["1", "|", "(", "0", "&", "(", "1", "/", "0", ")", ")"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "1\n")
  let r4 = uu.invoke(s, "expr", ["0", "&", "(", "1", "|", "(", "1", "/", "0", ")", ")"])?
  uu.fails_with_code(r4, 1)
  uu.stdout_only(r4, "0\n")
}

# origin: uutils test_expr::test_regex_caret
test test_uu_expr_regex_caret { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["a^b", ":", "a^b"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "3\n")
  let r2 = uu.invoke(s, "expr", ["a^b", ":", "a\\^b"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "3\n")
  let r3 = uu.invoke(s, "expr", ["abc", ":", "^abc"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "3\n")
  let r4 = uu.invoke(s, "expr", ["^abc", ":", "^^abc"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "4\n")
  let r5 = uu.invoke(s, "expr", ["b", ":", "a\\|^b"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "1\n")
  let r6 = uu.invoke(s, "expr", ["ab", ":", "\\(^a\\)b"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "a\n")
  let r7 = uu.invoke(s, "expr", ["^abc", ":", "^abc"])?
  uu.fails(r7)
  uu.stdout_only(r7, "0\n")
  let r8 = uu.invoke(s, "expr", ["^^^^^^^^^", ":", "^^^"])?
  uu.succeeds(r8)
  uu.stdout_only(r8, "2\n")
  let r9 = uu.invoke(s, "expr", ["ab[^c]", ":", "ab[^c]"])?
  uu.succeeds(r9)
  uu.stdout_only(r9, "3\n")
  let r10 = uu.invoke(s, "expr", ["ab[^c]", ":", "ab\\[^c]"])?
  uu.succeeds(r10)
  uu.stdout_only(r10, "6\n")
  let r11 = uu.invoke(s, "expr", ["[^x]", ":", "\\[^x]"])?
  uu.succeeds(r11)
  uu.stdout_only(r11, "4\n")
  let r12 = uu.invoke(s, "expr", ["\\a", ":", "\\\\[^^]"])?
  uu.succeeds(r12)
  uu.stdout_only(r12, "2\n")
  let r13 = uu.invoke(s, "expr", ["abc", ":", "bc"])?
  uu.fails(r13)
  uu.stdout_only(r13, "0\n")
  let r14 = uu.invoke(s, "expr", ["^a", ":", "^^[^^]"])?
  uu.succeeds(r14)
  uu.stdout_only(r14, "2\n")
  let r15 = uu.invoke(s, "expr", ["abc", ":", "ab[^c]"])?
  uu.fails(r15)
  uu.stdout_only(r15, "0\n")
}

# origin: uutils test_expr::test_regex_catastrophic_backtracking
test test_uu_expr_regex_catastrophic_backtracking { |ctx|
  let s = uu.scene(ctx)?
  let input = ["a" for _ in range(30)].join("") + "c"
  let r1 = uu.invoke(s, "expr", [input, ":", "\\(a\\+a\\+\\)\\+b"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_only(r1, "\n")
}

# origin: uutils test_expr::test_regex_dollar
test test_uu_expr_regex_dollar { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["a$b", ":", "a\\$b"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "3\n")
  let r2 = uu.invoke(s, "expr", ["a", ":", "a$\\|b"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "1\n")
  let r3 = uu.invoke(s, "expr", ["ab", ":", "a\\(b$\\)"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "b\n")
  let r4 = uu.invoke(s, "expr", ["a$c", ":", "a$\\c"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "3\n")
  let r5 = uu.invoke(s, "expr", ["$a", ":", "$a"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "2\n")
  let r6 = uu.invoke(s, "expr", ["a", ":", "a$\\|b"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "1\n")
  let r7 = uu.invoke(s, "expr", ["-5", ":", "-\\{0,1\\}[0-9]*$"])?
  uu.succeeds(r7)
  uu.stdout_only(r7, "2\n")
  let r8 = uu.invoke(s, "expr", ["$", ":", "$"])?
  uu.fails(r8)
  uu.stdout_only(r8, "0\n")
  let r9 = uu.invoke(s, "expr", ["a$", ":", "a$\\|b"])?
  uu.fails(r9)
  uu.stdout_only(r9, "0\n")
}

# origin: uutils test_expr::test_regex_empty
test test_uu_expr_regex_empty { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["", ":", ""])?
  uu.fails(r1)
  uu.stdout_only(r1, "0\n")
  let r2 = uu.invoke(s, "expr", ["abc", ":", ""])?
  uu.fails(r2)
  uu.stdout_only(r2, "0\n")
}

# origin: uutils test_expr::test_regex_leftmost_longest_match_semantics
test test_uu_expr_regex_leftmost_longest_match_semantics { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["ab", ":", "a\\|ab"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "2\n")
}

# origin: uutils test_expr::test_regex_newline
test test_uu_expr_regex_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "expr", ["line1\nline2\nline3 ", ":", ".*line2.*"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n")
}
