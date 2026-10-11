##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_test.rs.

use support.uu as uu

# origin: uutils test_test::diagnostics::test_dash_t_plain_message_is_unchanged
test test_uu_test_diagnostics_dash_t_plain_message_is_unchanged { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-t", "stdout"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_is(r0, "test: invalid integer 'stdout'\n")
}

# origin: uutils test_test::diagnostics::test_missing_operand_plain_message_is_unchanged
test test_uu_test_diagnostics_missing_operand_plain_message_is_unchanged { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["31", "-gt"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_is(r0, "test: missing argument after '-gt'\n")
}

# origin: uutils test_test::diagnostics::test_plain_message_is_the_default
test test_uu_test_diagnostics_plain_message_is_the_default { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["7", "-eq", "zap"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_is(r0, "test: invalid integer 'zap'\n")
}

# origin: uutils test_test::test_a_bunch_of_not
test test_uu_test_a_bunch_of_not { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "", "!=", "", "-a", "!", "", "!=", ""])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_and_not_is_false
test test_uu_test_and_not_is_false { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-a", "!"])?
  uu.fails_with_code(r0, 2)
}

# origin: uutils test_test::test_and_or_do_not_short_circuit
test test_uu_test_and_or_do_not_short_circuit { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["", "-a", "1", "-eq", "bad"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_contains(r0, "invalid integer 'bad'")
  let r1 = uu.invoke(s, "test", ["x", "-o", "1", "-eq", "bad"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_contains(r1, "invalid integer 'bad'")
}

# origin: uutils test_test::test_bang_bool_op_precedence
test test_uu_test_bang_bool_op_precedence { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "", "-a", ""])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "test", ["!", "", "-o", ""])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "test", ["!", "a value", "-o", "another value"])?
  uu.fails_with_code(r2, 1)
  let r3 = uu.invoke(s, "test", ["!", "-n", "", "-a", ""])?
  uu.fails_with_code(r3, 1)
  let r4 = uu.invoke(s, "test", ["!", "", "-a", "-n", ""])?
  uu.fails_with_code(r4, 1)
  let r5 = uu.invoke(s, "test", ["!", "", "-a", "", "-a", ""])?
  uu.fails_with_code(r5, 1)
  let r6 = uu.invoke(s, "test", ["!", "(", "", "-a", "", "-a", "", ")"])?
  uu.succeeds(r6)
}

# origin: uutils test_test::test_boolop_without_right_operand
test test_uu_test_boolop_without_right_operand { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  for op in ["-a", "-o"] {
  let r0 = uu.invoke(s, "test", ["x", op])?
  uu.fails_with_code(r0, 2)
  uu.stderr_is(r0, f"test: missing argument after '{op}'\n")
  let r1 = uu.invoke(s, "test", ["", op])?
  uu.fails_with_code(r1, 2)
  let r2 = uu.invoke(s, "test", ["x", "-a", "y", op])?
  uu.fails_with_code(r2, 2)
  let r3 = uu.invoke(s, "test", ["(", "x", ")", op])?
  uu.fails_with_code(r3, 2)
  let r4 = uu.invoke(s, "test", ["!", "x", op])?
  uu.fails_with_code(r4, 2)
  }
}

# origin: uutils test_test::test_bracket_syntax_failure
test test_uu_test_bracket_syntax_failure { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r1 = uu.invoke(s, "[", ["1", "-eq", "2", "]"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_test::test_bracket_syntax_help
test test_uu_test_bracket_syntax_help { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r1 = uu.invoke(s, "[", ["--help"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "Usage:")
}

# origin: uutils test_test::test_bracket_syntax_missing_right_bracket
test test_uu_test_bracket_syntax_missing_right_bracket { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r1 = uu.invoke(s, "[", ["1", "-eq"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_is(r1, "[: missing ']'\n")
}

# origin: uutils test_test::test_bracket_syntax_success
test test_uu_test_bracket_syntax_success { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r1 = uu.invoke(s, "[", ["1", "-eq", "1", "]"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_complicated_parenthesized_expression
test test_uu_test_complicated_parenthesized_expression { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["(", "(", "!", "(", "a", "=", "b", ")", "-o", "c", "=", "d", ")", "-a", "(", "q", "!=",
            "r", ")", ")"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_dangling_parenthesis
test test_uu_test_dangling_parenthesis { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["(", "(", "a", "!=", "b", ")", "-o", "-n", "c"])?
  uu.fails_with_code(r0, 2)
  let r1 = uu.invoke(s, "test", ["(", "(", "a", "!=", "b", ")", "-o", "-n", "c", ")"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_dangling_string_comparison_is_error
test test_uu_test_dangling_string_comparison_is_error { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["missing_something", "="])?
  uu.fails_with_code(r0, 2)
  uu.stderr_is(r0, "test: missing argument after '='\n")
}

# origin: uutils test_test::test_directory_is_executable
test test_uu_test_directory_is_executable { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  uu.mkdir(s, "dir")?
  let r1 = uu.invoke(s, "test", ["-x", "dir"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_double_equal_is_string_comparison_op
test test_uu_test_double_equal_is_string_comparison_op { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["t", "==", "t"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "test", ["t", "==", "f"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_test::test_double_not_is_false
test test_uu_test_double_not_is_false { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "!"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_empty_string_is_false
test test_uu_test_empty_string_is_false { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", [""])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_empty_test_equivalent_to_false
test test_uu_test_empty_test_equivalent_to_false { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", [])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_erroneous_parenthesized_expression
test test_uu_test_erroneous_parenthesized_expression { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["a", "!=", "(", "b", "-a", "b", ")", "!=", "c"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_is(r0, "test: extra argument 'b'\n")
}

# origin: uutils test_test::test_errors_miss_and_or
test test_uu_test_errors_miss_and_or { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-o", "arg"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_contains(r0, "'-o': unary operator expected")
  let r1 = uu.invoke(s, "test", ["-a", "arg"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "'-a': unary operator expected")
}

# origin: uutils test_test::test_file_N
test test_uu_test_file_N { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  uu.touch(s, "file")?
  fs.set_times(uu.at(s, "file"), atime_sec: 123, mtime_sec: 0)?
  let stat0 = process.command_argv("stat", ["stat", "file"], s.root, {}, b"", uu.at(s, "stat-out"), uu.at(s, "stat-err"))
  assert process.run(stat0)?.exited_with(0), "stat file must succeed"
  let first = uu.invoke(s, "test", ["-N", "file"])?
  uu.fails(first)
  fs.set_times(uu.at(s, "file"), atime_sec: 0, mtime_sec: 123)?
  let stat1 = process.command_argv("stat", ["stat", "file"], s.root, {}, b"", uu.at(s, "stat-out"), uu.at(s, "stat-err"))
  assert process.run(stat1)?.exited_with(0), "stat file must succeed"
  let r = uu.invoke(s, "test", ["-N", "file"])?
  uu.succeeds(r)
}

# origin: uutils test_test::test_file_exists
test test_uu_test_file_exists { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-e", "regular_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_file_exists_and_is_regular
test test_uu_test_file_exists_and_is_regular { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-f", "regular_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_file_is_executable
test test_uu_test_file_is_executable { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let mode = uu.at(s, "regular_file").metadata()?.mode
  uu.set_mode(s, "regular_file", mode.bit_or(0o100).bit_and(0o7777))?
  let r0 = uu.invoke(s, "test", ["-x", "regular_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_file_is_itself
test test_uu_test_file_is_itself { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["regular_file", "-ef", "regular_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_file_is_newer_than_and_older_than_itself
test test_uu_test_file_is_newer_than_and_older_than_itself { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["regular_file", "-nt", "regular_file"])?
  uu.fails_with_code(r0, 1)
  let r1 = uu.invoke(s, "test", ["regular_file", "-ot", "regular_file"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_test::test_file_is_newer_than_non_existing_file
test test_uu_test_file_is_newer_than_non_existing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["non_existing_file", "-nt", "regular_file"])?
  uu.fails_with_code(r0, 1)
  uu.no_output(r0)
  let r1 = uu.invoke(s, "test", ["regular_file", "-nt", "non_existing_file"])?
  uu.succeeds(r1)
  uu.no_output(r1)
  let r2 = uu.invoke(s, "test", ["non_existing_file", "-ot", "regular_file"])?
  uu.succeeds(r2)
  uu.no_output(r2)
  let r3 = uu.invoke(s, "test", ["regular_file", "-ot", "non_existing_file"])?
  uu.fails_with_code(r3, 1)
  uu.no_output(r3)
}

# origin: uutils test_test::test_file_is_not_executable
test test_uu_test_file_is_not_executable { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let mode = uu.at(s, "regular_file").metadata()?.mode
  uu.set_mode(s, "regular_file", mode.bit_and(0o7677))?
  let r2 = uu.invoke(s, "test", ["!", "-x", "regular_file"])?
  uu.succeeds(r2)
}

# origin: uutils test_test::test_file_is_not_readable
test test_uu_test_file_is_not_readable { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  uu.touch(s, "crypto_file")?
  let mode = uu.at(s, "crypto_file").metadata()?.mode
  uu.set_mode(s, "crypto_file", mode.bit_and(0o7377))?
  let r1 = uu.invoke(s, "test", ["!", "-r", "crypto_file"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_file_is_not_sticky
test test_uu_test_file_is_not_sticky { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-k", "regular_file"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_file_is_not_symlink
test test_uu_test_file_is_not_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "-h", "regular_file"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "test", ["!", "-L", "regular_file"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_file_is_not_writable
test test_uu_test_file_is_not_writable { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  uu.touch(s, "immutable_file")?
  let mode = uu.at(s, "immutable_file").metadata()?.mode
  uu.set_mode(s, "immutable_file", mode.bit_and(0o7577))?
  let r1 = uu.invoke(s, "test", ["!", "-w", "immutable_file"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_file_is_readable
test test_uu_test_file_is_readable { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-r", "regular_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_file_is_sticky
test test_uu_test_file_is_sticky { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  uu.touch(s, "sticky_file")?
  let mode = uu.at(s, "sticky_file").metadata()?.mode
  uu.set_mode(s, "sticky_file", mode.bit_or(0o1000).bit_and(0o7777))?
  let r1 = uu.invoke(s, "test", ["-k", "sticky_file"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_file_is_writable
test test_uu_test_file_is_writable { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-w", "regular_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_file_not_owned_by_egid
test test_uu_test_file_not_owned_by_egid { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-f", "/bin/sh", "-a", "!", "-G", "/bin/sh"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_file_not_owned_by_euid
test test_uu_test_file_not_owned_by_euid { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-f", "/bin/sh", "-a", "!", "-O", "/bin/sh"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_file_operator_rejects_length_operand
test test_uu_test_file_operator_rejects_length_operand { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-l", "a", "-nt", "b"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_contains(r0, "-nt does not accept -l")
}

# origin: uutils test_test::test_file_owned_by_egid
test test_uu_test_file_owned_by_egid { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let entry = uu.at(s, "regular_file").metadata()?
  let who = unix.id()?
  if entry.gid != who.egid { fs.chgrp(uu.at(s, "regular_file"), group.by_gid(who.egid)?)? }
  let r0 = uu.invoke(s, "test", ["-G", "regular_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_file_owned_by_euid
test test_uu_test_file_owned_by_euid { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-O", "regular_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_filename_or_with_equal
test test_uu_test_filename_or_with_equal { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-f", "=", "a", "-o", "b"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_float_inequality_is_error
test test_uu_test_float_inequality_is_error { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["123.45", "-ge", "6"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_is(r0, "test: invalid integer '123.45'\n")
}

# origin: uutils test_test::test_hard_link_is_same_file
test test_uu_test_hard_link_is_same_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  uu.hard_link(s, "regular_file", "hard_link")?
  let r1 = uu.invoke(s, "test", ["regular_file", "-ef", "hard_link"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_int_compares_ignore_redundant_sign_and_leading_zeros
test test_uu_test_int_compares_ignore_redundant_sign_and_leading_zeros { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  for args in [["-0", "-eq", "0"], ["+0", "-eq", "-0"], ["0", "-eq", "0000000000"], ["007", "-eq", "7"], ["-007", "-eq", "-7"], ["+0016267277278126277227728782172782882627278282882172762677623672762783782", "-eq", "16267277278126277227728782172782882627278282882172762677623672762783782"]] {
  let r = uu.invoke(s, "test", args)?
  uu.succeeds(r)
  }
}

# origin: uutils test_test::test_integer_length_operands
test test_uu_test_integer_length_operands { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-l", "abc", "-eq", "3"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "test", ["3", "-eq", "-l", "abc"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "test", ["-l", "abc", "-ne", "4"])?
  uu.succeeds(r2)
}

# origin: uutils test_test::test_integer_whitespace_stripping
test test_uu_test_integer_whitespace_stripping { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["42", "-eq", " 42 "])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "test", ["42", "-eq", " 42"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "test", ["42", "-eq", "42 "])?
  uu.succeeds(r2)
  let r3 = uu.invoke(s, "test", [" 42 ", "-eq", "42"])?
  uu.succeeds(r3)
  let r4 = uu.invoke(s, "test", ["42", "-eq", "\t42"])?
  uu.succeeds(r4)
  let r5 = uu.invoke(s, "test", ["42", "-eq", "\n42"])?
  uu.succeeds(r5)
  let r6 = uu.invoke(s, "test", ["42", "-eq", "\x0b42"])?
  uu.succeeds(r6)
  let r7 = uu.invoke(s, "test", ["42", "-eq", "\x0c42"])?
  uu.succeeds(r7)
  let r8 = uu.invoke(s, "test", ["42", "-eq", "\r42"])?
  uu.succeeds(r8)
}

# origin: uutils test_test::test_inverted_parenthetical_bool_op_precedence
test test_uu_test_inverted_parenthetical_bool_op_precedence { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "a value", "-o", "another value"])?
  uu.fails_with_code(r0, 1)
  let r1 = uu.invoke(s, "test", ["!", "(", "a value", ")", "-o", "another value"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_is_not_empty
test test_uu_test_is_not_empty { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-s", "non_empty_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_isatty_invalid_fd_is_false
test test_uu_test_isatty_invalid_fd_is_false { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-t", "99"])?
  uu.fails_with_code(r0, 1)
  let r1 = uu.invoke(s, "test", ["-t", "-1"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_test::test_isatty_whitespace_stripping
test test_uu_test_isatty_whitespace_stripping { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-t", " 0 "])?
  uu.fails_with_code(r0, 1)
  let r1 = uu.invoke(s, "test", ["-t", "\n0\t"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_test::test_large_int_compares
test test_uu_test_large_int_compares { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  for args in [["16267277278126277227728782172782882627278282882172762677623672762783782", "-eq", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-ge", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-le", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-ne", "1"], ["1", "-lt", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-gt", "1"], ["16267277278126277227728782172782882627278282882172762677623672762783783", "-gt", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-lt", "16267277278126277227728782172782882627278282882172762677623672762783783"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-gt", "1626727727812627722772878217278288262727828288217276267762367276278378"], ["1626727727812627722772878217278288262727828288217276267762367276278378", "-lt", "16267277278126277227728782172782882627278282882172762677623672762783782"]] {
  let r = uu.invoke(s, "test", args)?
  uu.succeeds(r)
  }
  for args in [["16267277278126277227728782172782882627278282882172762677623672762783782", "-eq", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-ge", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-le", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-ne", "1"], ["1", "-lt", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-gt", "1"], ["16267277278126277227728782172782882627278282882172762677623672762783783", "-gt", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-lt", "16267277278126277227728782172782882627278282882172762677623672762783783"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-gt", "1626727727812627722772878217278288262727828288217276267762367276278378"], ["1626727727812627722772878217278288262727828288217276267762367276278378", "-lt", "16267277278126277227728782172782882627278282882172762677623672762783782"]] {
  let r = uu.invoke(s, "test", ["!"].extend(args))?
  uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_test::test_large_negative_int_compares
test test_uu_test_large_negative_int_compares { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  for args in [["-16267277278126277227728782172782882627278282882172762677623672762783782", "-eq", "-16267277278126277227728782172782882627278282882172762677623672762783782"], ["-16267277278126277227728782172782882627278282882172762677623672762783782", "-lt", "0"], ["-16267277278126277227728782172782882627278282882172762677623672762783782", "-lt", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-gt", "-16267277278126277227728782172782882627278282882172762677623672762783782"], ["-16267277278126277227728782172782882627278282882172762677623672762783782", "-lt", "-1626727727812627722772878217278288262727828288217276267762367276278378"], ["-1626727727812627722772878217278288262727828288217276267762367276278378", "-gt", "-16267277278126277227728782172782882627278282882172762677623672762783782"]] {
  let r = uu.invoke(s, "test", args)?
  uu.succeeds(r)
  }
  for args in [["-16267277278126277227728782172782882627278282882172762677623672762783782", "-eq", "-16267277278126277227728782172782882627278282882172762677623672762783782"], ["-16267277278126277227728782172782882627278282882172762677623672762783782", "-lt", "0"], ["-16267277278126277227728782172782882627278282882172762677623672762783782", "-lt", "16267277278126277227728782172782882627278282882172762677623672762783782"], ["16267277278126277227728782172782882627278282882172762677623672762783782", "-gt", "-16267277278126277227728782172782882627278282882172762677623672762783782"], ["-16267277278126277227728782172782882627278282882172762677623672762783782", "-lt", "-1626727727812627722772878217278288262727828288217276267762367276278378"], ["-1626727727812627722772878217278288262727828288217276267762367276278378", "-gt", "-16267277278126277227728782172782882627278282882172762677623672762783782"]] {
  let r = uu.invoke(s, "test", ["!"].extend(args))?
  uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_test::test_lone_boolop_is_a_string
test test_uu_test_lone_boolop_is_a_string { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  for op in ["-a", "-o"] {
  let r = uu.invoke(s, "test", [op])?
  uu.succeeds(r)
  let neg = uu.invoke(s, "test", ["!", op])?
  uu.fails_with_code(neg, 1)
  }
  let unary = uu.invoke(s, "test", ["-n", "-a"])?
  uu.succeeds(unary)
}

# origin: uutils test_test::test_long_integer
test test_uu_test_long_integer { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["18446744073709551616", "-eq", "0"])?
  uu.fails(r0)
  let r1 = uu.invoke(s, "test", ["-9223372036854775809", "-ge", "18446744073709551616"])?
  uu.fails(r1)
  let r2 = uu.invoke(s, "test", ["'('",
            "-9223372036854775809",
            "-ge",
            "18446744073709551616",
            "')'"])?
  uu.fails(r2)
}

# origin: uutils test_test::test_malformed_integers_are_still_errors
test test_uu_test_malformed_integers_are_still_errors { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  for operand in ["1_0", "0x10", "1e3", "++5", "5-", "-", "+", "", "4 2"] {
  let r = uu.invoke(s, "test", [operand, "-eq", "0"])?
  uu.fails_with_code(r, 2)
  uu.stderr_is(r, f"test: invalid integer '{operand}'\n")
  }
}

# origin: uutils test_test::test_missing_argument_after
test test_uu_test_missing_argument_after { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r = uu.invoke(s, "test", ["(", "foo"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  assert r.stderr.utf8()?.trim() == "test: missing argument after 'foo'"
}

# origin: uutils test_test::test_negated_boolean_precedence
test test_uu_test_negated_boolean_precedence { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  for args in [["!", "(", "foo", ")", "-o", "bar"], ["!", "", "-o", "", "-a", ""], ["!", "(", "", "-a", "", ")", "-o", ""]] {
  let r = uu.invoke(s, "test", args)?
  uu.succeeds(r)
  }
  for args in [["!", "-n", "", "-a", ""], ["", "-a", "", "-o", ""], ["!", "", "-a", "", "-o", ""], ["!", "(", "", "-a", "", ")", "-a", ""]] {
  let r = uu.invoke(s, "test", args)?
  uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_test::test_negated_or
test test_uu_test_negated_or { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "foo", "-o", "bar"])?
  uu.fails_with_code(r0, 1)
  let r1 = uu.invoke(s, "test", ["foo", "-o", "!", "bar"])?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "test", ["!", "foo", "-o", "!", "bar"])?
  uu.fails_with_code(r2, 1)
}

# origin: uutils test_test::test_negative_arg_is_a_string
test test_uu_test_negative_arg_is_a_string { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-12345"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "test", ["--qwert"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_negative_int_compare
test test_uu_test_negative_int_compare { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  for args in [["-1", "-eq", "-1"], ["-1", "-ne", "-2"], ["-3720", "-lt", "-421"], ["-10", "-le", "-10"], ["-21", "-gt", "-22"], ["-128", "-ge", "-256"], ["-9223372036854775808", "-le", "-9223372036854775807"]] {
  let r = uu.invoke(s, "test", args)?
  uu.succeeds(r)
  }
  for args in [["-1", "-eq", "-1"], ["-1", "-ne", "-2"], ["-3720", "-lt", "-421"], ["-10", "-le", "-10"], ["-21", "-gt", "-22"], ["-128", "-ge", "-256"], ["-9223372036854775808", "-le", "-9223372036854775807"]] {
  let r = uu.invoke(s, "test", ["!"].extend(args))?
  uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_test::test_newer_file
test test_uu_test_newer_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  uu.touch(s, "older_file")?
  fs.set_times(uu.at(s, "older_file"), mtime_sec: 0)?
  uu.touch(s, "newer_file")?
  let r0 = uu.invoke(s, "test", ["newer_file", "-nt", "older_file"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "test", ["older_file", "-nt", "newer_file"])?
  uu.fails(r1)
  let r2 = uu.invoke(s, "test", ["older_file", "-ot", "newer_file"])?
  uu.succeeds(r2)
  let r3 = uu.invoke(s, "test", ["newer_file", "-ot", "older_file"])?
  uu.fails(r3)
}

# origin: uutils test_test::test_nonexistent_file_does_not_exist
test test_uu_test_nonexistent_file_does_not_exist { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-e", "nonexistent_file"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_nonexistent_file_is_not_regular
test test_uu_test_nonexistent_file_is_not_regular { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-f", "nonexistent_file"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_nonexistent_file_is_not_symlink
test test_uu_test_nonexistent_file_is_not_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "-h", "nonexistent_file"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "test", ["!", "-L", "nonexistent_file"])?
  uu.succeeds(r1)
}

# origin: uutils test_test::test_nonexistent_file_not_owned_by_egid
test test_uu_test_nonexistent_file_not_owned_by_egid { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-G", "nonexistent_file"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_nonexistent_file_not_owned_by_euid
test test_uu_test_nonexistent_file_not_owned_by_euid { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-O", "nonexistent_file"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_nonexistent_file_size_test_is_false
test test_uu_test_nonexistent_file_size_test_is_false { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-s", "nonexistent_file"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_not_and_is_false
test test_uu_test_not_and_is_false { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "-a"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_not_and_not_is_a_syntax_error
test test_uu_test_not_and_not_is_a_syntax_error { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "-a", "!"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_contains(r0, "'-a': unary operator expected")
}

# origin: uutils test_test::test_not_is_not_empty
test test_uu_test_not_is_not_empty { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["!", "-s", "regular_file"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_nothing_is_empty
test test_uu_test_nothing_is_empty { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["-z"])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_op_precedence_and_or_1
test test_uu_test_op_precedence_and_or_1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", [" ", "-o", "", "-a", ""])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_op_precedence_and_or_1_overridden_by_parentheses
test test_uu_test_op_precedence_and_or_1_overridden_by_parentheses { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["(", " ", "-o", "", ")", "-a", ""])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_op_precedence_and_or_2
test test_uu_test_op_precedence_and_or_2 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["", "-a", "", "-o", " ", "-a", " "])?
  uu.succeeds(r0)
}

# origin: uutils test_test::test_op_precedence_and_or_2_overridden_by_parentheses
test test_uu_test_op_precedence_and_or_2_overridden_by_parentheses { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["", "-a", "(", "", "-o", " ", ")", "-a", " "])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_or_as_filename
test test_uu_test_or_as_filename { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["x", "-a", "-z", "-o"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_test::test_parenthesized_literal
test test_uu_test_parenthesized_literal { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  for args in [["(", "a string", ")"], ["(", "(", ")"], ["(", ")", ")"], ["(", "-", ")"], ["(", "--", ")"], ["(", "-0", ")"], ["(", "-f", ")"], ["(", "--help", ")"], ["(", "--version", ")"], ["(", "-e", ")"], ["(", "-t", ")"], ["(", "!", ")"], ["(", "-n", ")"], ["(", "-z", ")"], ["(", "[", ")"], ["(", "-a", ")"], ["(", "-o", ")"]] {
  let r = uu.invoke(s, "test", args)?
  uu.succeeds(r)
  }
  for args in [["(", "a string", ")"], ["(", "(", ")"], ["(", ")", ")"], ["(", "-", ")"], ["(", "--", ")"], ["(", "-0", ")"], ["(", "-f", ")"], ["(", "--help", ")"], ["(", "--version", ")"], ["(", "-e", ")"], ["(", "-t", ")"], ["(", "!", ")"], ["(", "-n", ")"], ["(", "-z", ")"], ["(", "[", ")"], ["(", "-a", ")"], ["(", "-o", ")"]] {
  let r = uu.invoke(s, "test", ["!"].extend(args))?
  uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_test::test_parenthesized_op_compares_literal_parenthesis
test test_uu_test_parenthesized_op_compares_literal_parenthesis { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "test", "regular_file", "regular_file")?
  uu.fixture(s, "test", "non_empty_file", "non_empty_file")?
  let r0 = uu.invoke(s, "test", ["(", "=", ")"])?
  uu.fails_with_code(r0, 1)
}
