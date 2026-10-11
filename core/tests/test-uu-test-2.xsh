##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_test.rs.

use support.uu as uu

# Each upstream command runs with the complete utility fixture snapshot.
proc scene(ctx: TestContext) -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  Ok(s)
}

# origin: uutils test_test::test_parenthesized_right_parenthesis_as_literal
test test_uu_test_parenthesized_right_parenthesis_as_literal { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["(", "-f", ")", ")"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_test::test_parenthesized_string_comparison
test test_uu_test_parenthesized_string_comparison { |ctx|
  let s = scene(ctx)?
  let cases = [["(", "foo", "!=", "bar", ")"], ["(", "contained\nnewline", "=", "contained\nnewline", ")"], ["(", "(", "=", "(", ")"], ["(", "(", "!=", ")", ")"], ["(", "!", "=", "!", ")"], ["(", "=", "=", "=", ")"]]
  for args in cases {
    let r = uu.invoke(s, "test", args)?
    # A ')' closes the group before it can become a binary comparison operand.
    if args == ["(", "(", "!=", ")", ")"] { uu.fails_with_code(r, 2) } else { uu.succeeds(r) }
  }
  for args in cases {
    let r = uu.invoke(s, "test", ["!"].extend(args))?
    uu.fails_with_code(r, if args == ["(", "(", "!=", ")", ")"] { 2 } else { 1 })
  }
}

# origin: uutils test_test::test_pseudofloat_equal
test test_uu_test_pseudofloat_equal { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["123.45", "=", "123.45"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_pseudofloat_not_equal
test test_uu_test_pseudofloat_not_equal { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["123.45", "!=", "123.450"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_simple_or
test test_uu_test_simple_or { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["foo", "-o", ""])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_solo_and_or_or_is_a_literal
test test_uu_test_solo_and_or_or_is_a_literal { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["-a"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "test", ["-o"])?
  uu.succeeds(r2)
}

# origin: uutils test_test::test_solo_empty_parenthetical_is_error
test test_uu_test_solo_empty_parenthetical_is_error { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["(", ")"])?
  uu.fails_with_code(r1, 2)
}

# origin: uutils test_test::test_solo_not
test test_uu_test_solo_not { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["!"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_some_int_compares
test test_uu_test_some_int_compares { |ctx|
  let s = scene(ctx)?
  let cases = [["0", "-eq", "0"], ["0", "-ne", "1"], ["421", "-lt", "3720"], ["0", "-le", "0"], ["11", "-gt", "10"], ["1024", "-ge", "512"], ["9223372036854775806", "-le", "9223372036854775807"]]
  for args in cases {
    uu.succeeds(uu.invoke(s, "test", args)?)
  }
  for args in cases {
    uu.fails_with_code(uu.invoke(s, "test", ["!"].extend(args))?, 1)
  }
}

# origin: uutils test_test::test_some_literals
test test_uu_test_some_literals { |ctx|
  let s = scene(ctx)?
  let cases = ["a string", "(", ")", "-", "--", "-0", "-f", "--help", "--version", "-eq", "-lt", "-ef", "["]
  for args in cases {
    uu.succeeds(uu.invoke(s, "test", [args])?)
  }
  for args in cases {
    uu.fails_with_code(uu.invoke(s, "test", ["!", args])?, 1)
  }
}

# origin: uutils test_test::test_string_comparison
test test_uu_test_string_comparison { |ctx|
  let s = scene(ctx)?
  let cases = [["foo", "!=", "bar"], ["contained\nnewline", "=", "contained\nnewline"], ["(", "=", "("], ["(", "!=", ")"], ["(", "!=", "="], ["!", "=", "!"], ["=", "=", "="]]
  for args in cases {
    uu.succeeds(uu.invoke(s, "test", args)?)
  }
  for args in cases {
    uu.fails_with_code(uu.invoke(s, "test", ["!"].extend(args))?, 1)
  }
}

# origin: uutils test_test::test_string_length_and_nothing
test test_uu_test_string_length_and_nothing { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["-n", "a", "-a"])?
  uu.fails_with_code(r1, 2)
}

# origin: uutils test_test::test_string_length_of_empty
test test_uu_test_string_length_of_empty { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["-n", ""])?
  uu.fails_with_code(r1, 1)
  let r2 = uu.invoke(s, "test", [""])?
  uu.fails_with_code(r2, 1)
}

# origin: uutils test_test::test_string_length_of_nothing
test test_uu_test_string_length_of_nothing { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["-n"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_string_operator_is_literal_after_bang
test test_uu_test_string_operator_is_literal_after_bang { |ctx|
  let s = scene(ctx)?
  let cases = [["!", "="], ["!", "!="], ["!", "-eq"], ["!", "-ne"], ["!", "-lt"], ["!", "-le"], ["!", "-gt"], ["!", "-ge"], ["!", "-ef"], ["!", "-nt"], ["!", "-ot"]]
  for args in cases {
    uu.fails_with_code(uu.invoke(s, "test", args)?, 1)
  }
}

# origin: uutils test_test::test_tty_out_of_range_is_false
test test_uu_test_tty_out_of_range_is_false { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["-t", "999999999999999999999999999999999999"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_test::test_unary_op_as_literal_in_three_arg_form
test test_uu_test_unary_op_as_literal_in_three_arg_form { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["-f", "=", "a"])?
  uu.fails_with_code(r1, 1)
  let r2 = uu.invoke(s, "test", ["-f", "=", "a", "-o", "b"])?
  uu.succeeds(r2)
}

# origin: uutils test_test::test_unknown_two_byte_operator_errors
test test_uu_test_unknown_two_byte_operator_errors { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["-Q", "x"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_contains(r1, "'-Q': unary operator expected")
  let r2 = uu.invoke(s, "test", ["x", "-Q", "y"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_contains(r2, "'-Q': binary operator expected")
}

# origin: uutils test_test::test_values_greater_than_i128_allowed
test test_uu_test_values_greater_than_i128_allowed { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["170141183460469231731687303715884105728", "-gt", "0"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "test", ["-170141183460469231731687303715884105729", "-lt", "0"])?
  uu.succeeds(r2)
}

# origin: uutils test_test::test_values_greater_than_i64_allowed
test test_uu_test_values_greater_than_i64_allowed { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["9223372036854775808", "-gt", "0"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_zero_len_equals_zero_len
test test_uu_test_zero_len_equals_zero_len { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["", "=", ""])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_zero_len_not_equals_zero_len_is_false
test test_uu_test_zero_len_not_equals_zero_len_is_false { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["", "!=", ""])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_test::test_zero_len_of_empty
test test_uu_test_zero_len_of_empty { |ctx|
  let s = scene(ctx)?
  let r1 = uu.invoke(s, "test", ["-z", ""])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_same_device_inode
test test_uu_test_same_device_inode { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "regular_file_second")?
  uu.at(s, "symlink").symlink(to: uu.at(s, "regular_file"))?
  uu.fails(uu.invoke(s, "test", ["regular_file", "-ef", "regular_file_second"])?)
  uu.succeeds(uu.invoke(s, "test", ["regular_file", "-ef", "symlink"])?)
}

# origin: uutils test_test::test_symlink_is_symlink
test test_uu_test_symlink_is_symlink { |ctx|
  let s = scene(ctx)?
  uu.at(s, "symlink").symlink(to: uu.at(s, "regular_file"))?
  uu.succeeds(uu.invoke(s, "test", ["-h", "symlink"])?)
  uu.succeeds(uu.invoke(s, "test", ["-L", "symlink"])?)
}

# origin: uutils test_test::test_string_lt_gt_operator
test test_uu_test_string_lt_gt_operator { |ctx|
  let s = scene(ctx)?
  let cases = [
    {left: "a", right: "b"},
    {left: "a", right: "aa"},
    {left: "a", right: "a "},
    {left: "a", right: "a b"},
    {left: "", right: "b"},
    {left: "a", right: "ä"},
  ]
  for item in cases {
    let less = uu.invoke(s, "test", [item.left, "<", item.right])?
    uu.succeeds(less)
    uu.no_output(less)
    let less_reversed = uu.invoke(s, "test", [item.right, "<", item.left])?
    uu.fails_with_code(less_reversed, 1)
    uu.no_output(less_reversed)
    let greater = uu.invoke(s, "test", [item.right, ">", item.left])?
    uu.succeeds(greater)
    uu.no_output(greater)
    let greater_reversed = uu.invoke(s, "test", [item.left, ">", item.right])?
    uu.fails_with_code(greater_reversed, 1)
    uu.no_output(greater_reversed)
  }
  let less_empty = uu.invoke(s, "test", ["", "<", ""])?
  uu.fails_with_code(less_empty, 1)
  uu.no_output(less_empty)
  let greater_empty = uu.invoke(s, "test", ["", ">", ""])?
  uu.fails_with_code(greater_empty, 1)
  uu.no_output(greater_empty)
}
