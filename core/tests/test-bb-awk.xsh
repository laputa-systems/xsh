use support.uu as uu

# origin: busybox awk/awk -F case 0
test test_bb_awk_awk_F_case_0_8eab4259{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "[#]", "{print NF}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
}

# origin: busybox awk/awk -F case 1
test test_bb_awk_awk_F_case_1_3bfa390a{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "[#]", "{print NF}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "0\n")
}

# origin: busybox awk/awk -F case 2
test test_bb_awk_awk_F_case_2_1798b73f{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "[#]", "{print NF}"], stdin: bytes.from_text("#\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "2\n")
}

# origin: busybox awk/awk -F case 3
test test_bb_awk_awk_F_case_3_b5db778e{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "[#]", "{print NF}"], stdin: bytes.from_text("#abc#\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "3\n")
}

# origin: busybox awk/awk -F case 4
test test_bb_awk_awk_F_case_4_c9536056{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "[#]", "{print NF}"], stdin: bytes.from_text("#abc#zz\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "3\n")
}

# origin: busybox awk/awk -F case 5
test test_bb_awk_awk_F_case_5_32c75df1{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "[#]", "{print NF}"], stdin: bytes.from_text("#abc##zz\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "4\n")
}

# origin: busybox awk/awk -F case 6
test test_bb_awk_awk_F_case_6_036dd6e3{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "[#]", "{print NF}"], stdin: bytes.from_text("z#abc##zz\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "4\n")
}

# origin: busybox awk/awk -F case 7
test test_bb_awk_awk_F_case_7_f0745f82{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "[#]", "{print NF}"], stdin: bytes.from_text("z##abc##zz\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "5\n")
}

# origin: busybox awk/awk if operator ==
test test_bb_awk_awk_if_operator_3df55f1c{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN { if (23 == 23) { print \"foo\" } }"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foo\n")
}

# origin: busybox awk/awk if operator !=
test test_bb_awk_awk_if_operator_99dd1fcb{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN { if (23 != 23) { print \"bar\" } }"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
}

# origin: busybox awk/awk if operator >=
test test_bb_awk_awk_if_operator_de706398{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN { if (23 >= 23) { print \"foo\" } }"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foo\n")
}

# origin: busybox awk/awk if operator <
test test_bb_awk_awk_if_operator_eb575c43{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN { if (2 < 13) { print \"foo\" } }"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foo\n")
}

# origin: busybox awk/awk if string ==
test test_bb_awk_awk_if_string_2a35cfdb{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN { if (\"a\" == \"ab\") {print \"bar\"} }"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
}

# origin: busybox awk/awk bitwise op
test test_bb_awk_awk_bitwise_op_d98a64b4{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{print or(4294967295, 1)}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "4294967295\n")
}

# origin: busybox awk/awk handles empty function f(arg){}
test test_bb_awk_awk_handles_empty_function_f_arg_9bb5221b{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["function idle(value) {} END { n=1; print \"L\" n \"\\n\"; idle(n+n+ ++n); print \"L\" n \"\\n\" }"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "L1\n\nL2\n\n")
}

# origin: busybox awk/awk handles empty function f(){}
test test_bb_awk_awk_handles_empty_function_f_360001dc{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["function idle() {} END { idle(); print \"Ok\" }"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "Ok\n")
}

# origin: busybox awk/awk properly handles function from other scope
test test_bb_awk_awk_properly_handles_function_from_other_scope_469c2676{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["function one() {return 1} END { n=1; print \"L\" n \"\\n\"; n+=one(); print \"L\" n \"\\n\" }"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "L1\n\nL2\n\n")
}

# origin: busybox awk/awk 'v (a)' is not a function call, it is a concatenation
test test_bb_awk_awk_v_a_is_not_a_function_call_it_is_a_concatenation_312ca079{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN { v=1; a=2; print v (a) }"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "12\n")
  uu.no_stderr(r)
}

# origin: busybox awk/awk unused function args are evaluated
test test_bb_awk_awk_unused_function_args_are_evaluated_d54593bb{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["func emit_f() {print \"F\"} func emit_g() {print \"G\"} BEGIN {emit_f(emit_g(),emit_g())}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "G\nG\nF\n")
  uu.no_stderr(r)
}

# origin: busybox awk/awk hex const 1
test test_bb_awk_awk_hex_const_1_5430ac3b{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{print or(0xffffffff, 1)}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "4294967295\n")
}

# origin: busybox awk/awk hex const 2
test test_bb_awk_awk_hex_const_2_ee9196a0{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{print or(0x80000000, 1)}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "2147483649\n")
}

# origin: busybox awk/awk oct const
test test_bb_awk_awk_oct_const_03704cec{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{print or(01234, 1)}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "669\n")
}

# origin: busybox awk/awk input is never oct
test test_bb_awk_awk_input_is_never_oct_308726e9{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{print $1, ($1 + 1)}"], stdin: bytes.from_text("011\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "011 12\n")
}

# origin: busybox awk/awk floating const with leading zeroes
test test_bb_awk_awk_floating_const_with_leading_zeroes_74725453{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{printf \"%f %f\\n\", \"000.123\", \"009.123\"}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "0.123000 9.123000\n")
}

# origin: busybox awk/awk long field sep
test test_bb_awk_awk_long_field_sep_1dc60722{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F--", "{print NF, length($NF), $NF}"], stdin: bytes.from_text("a--\na--b--\na--b--c--\na--b--c--d--"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "2 0 \n3 0 \n4 0 \n5 0 \n")
}

# origin: busybox awk/awk -F handles escapes
test test_bb_awk_awk_F_handles_escapes_547f57da{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F\\x21", "{print $1}"], stdin: bytes.from_text("a!b\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "a\n")
}

# origin: busybox awk/awk gsub falls back to non-extended-regex
test test_bb_awk_awk_gsub_falls_back_to_non_extended_regex_563659b0{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{gsub(\"@(samp|code|file)\\{\", \"\")}"], stdin: bytes.from_text("Hi\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")

}

# origin: busybox awk/awk NF in BEGIN
test test_bb_awk_awk_NF_in_BEGIN_77c25514{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {print \":\" NF \":\" $0 \":\" $1 \":\" $2 \":\"}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, ":0::::\n")
}

# origin: busybox awk/awk string cast (bug 725)
test test_bb_awk_awk_string_cast_bug_725_9bd9eda1{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["function number(value) {value=0; print \"\" value; return value} function relay(value) {value=number(); return value} BEGIN {print (relay() ? \"string\" : \"number\")}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "0\nnumber\n")
}

# origin: busybox awk/awk handles whitespace before array subscript
test test_bb_awk_awk_handles_whitespace_before_array_subscript_eb5ef338{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {arr [3]=1; print arr [3]}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "1\n")
}

# origin: busybox awk/awk nested loops with the same variable
test test_bb_awk_awk_nested_loops_with_the_same_variable_a3461406{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {u[\"a\"]=u[\"b\"]=u[\"c\"]=1; v[\"d\"]=v[\"e\"]=v[\"f\"]=1; for (l in u) {print \"outer1\",l; for (l in v) print \" inner\",l; print \"outer2\",l} print \"end\",l; l=\"a\"; exit}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "outer1 a\n inner d\n inner e\n inner f\nouter2 f\nouter1 b\n inner d\n inner e\n inner f\nouter2 f\nouter1 c\n inner d\n inner e\n inner f\nouter2 f\nend f\n")
}

# origin: busybox awk/awk 'delete a[v--]' evaluates v-- once
test test_bb_awk_awk_delete_a_v_evaluates_v_once_5f6b7a5d{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {v=0; a[v]=\"zeroth\"; a[++v]=\"first\"; delete a[v--]; print v; print \"[0]:\" a[0]; print \"[1]:\" a[1]}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "0\n[0]:zeroth\n[1]:\n")
}

# origin: busybox awk/awk FS assignment
test test_bb_awk_awk_FS_assignment_29d11835{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{FS=\":\"; print $1}"], stdin: bytes.from_text("a:b c:d\ne:f g:h"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "a:b\ne\n")
}

# origin: busybox awk/awk length(array)
test test_bb_awk_awk_length_array_3f6ea1ee{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {A[1]=2; A[\"qwe\"]=\"asd\"; print length(A)}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "2\n")
}

# origin: busybox awk/awk length()
test test_bb_awk_awk_length_b014757d{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{print length; print length(); print length(\"qwe\"); print length(99 + 9)}"], stdin: bytes.from_text("qwe"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "3\n3\n3\n3\n")
}

# origin: busybox awk/awk print length, 1
test test_bb_awk_awk_print_length_1_5e073ea8{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{print length, 1}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "0 1\n")
}

# origin: busybox awk/awk print length 1
test test_bb_awk_awk_print_length_1_49e90333{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{print length 1}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "01\n")
}

# origin: busybox awk/awk length == 0
test test_bb_awk_awk_length_0_d6ab4c1c{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["length == 0 {print \"foo\"}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foo\n")
}

# origin: busybox awk/awk if (length == 0)
test test_bb_awk_awk_if_length_0_dd284655{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["{if (length == 0) print \"bar\"}"], stdin: bytes.from_text("\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "bar\n")
}

# origin: busybox awk/awk -f and ARGC
test test_bb_awk_awk_f_and_ARGC_005000c7{ |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "do re mi\n")?
  let r = uu.invoke(s, "awk", ["-f", "-", "input"], stdin: bytes.from_text("{print $2; print ARGC;}"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "re\n2\n")
}

# origin: busybox awk/awk do not allow "str"++
test test_bb_awk_awk_do_not_allow_str_724d0fde{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-v", "i=1", "BEGIN {print \"str\" ++i}"], stdin: bytes.from_text("anything"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "str2\n")
}

# origin: busybox awk/awk FS regex which can match empty string
test test_bb_awk_awk_FS_regex_which_can_match_empty_string_98f36cab{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "-*", "{print $1 \"-\" $2 \"=\" $3 \"*\" $4}"], stdin: bytes.from_text("foo--bar"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foo-bar=*\n")
}

# origin: busybox awk/awk $NF is empty
test test_bb_awk_awk_NF_is_empty_6d0fe97e{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["-F", "=+", "{print $NF}"], stdin: bytes.from_text("a=====123="), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "\n")
}

# origin: busybox awk/awk exit N propagates through END's exit
test test_bb_awk_awk_exit_N_propagates_through_END_s_exit_30365011{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {exit 42} END {exit}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 42)
  uu.stdout_is(r, "")
}

# origin: busybox awk/awk print + redirect
test test_bb_awk_awk_print_redirect_4323c05b{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {print \"STDERR %s\" > \"/dev/stderr\"}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
  uu.stderr_is(r, "STDERR %s\n")
}

# origin: busybox awk/awk "cmd" | getline
test test_bb_awk_awk_cmd_getline_6bdf67c7{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {\"echo HELLO\" | getline; print $0}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "HELLO\n")
}

# origin: busybox awk/awk printf %% prints one %
test test_bb_awk_awk_printf_prints_one_e1cb147b{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {printf \"%%\\n\"}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "%\n")
}

# origin: busybox awk/awk backslash+newline eaten with no trace
test test_bb_awk_awk_backslash_newline_eaten_with_no_trace_68a9ed7c{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["BEGIN {printf \"Hello\\\n world\\n\"}"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "Hello world\n")
}

# origin: busybox awk/awk assign while test
test test_bb_awk_awk_assign_while_test_e7f687cd{ |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "awk", ["$1==$1=\"foo\" {print $1}"], stdin: bytes.from_text("foo"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "foo\n")
}

