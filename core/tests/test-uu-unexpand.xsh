##! Transcribed from the uutils coreutils integration tests for unexpand.

use support.uu as uu

proc unexpand_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  uu.fixture(s, "unexpand", "with_spaces.txt", "with_spaces.txt")?
  Ok(s)
}

# origin: uutils test_unexpand::test_blank_separated_tabs
test test_uu_unexpand_blank_separated_tabs { |ctx|
  let s = unexpand_scene(ctx)?
  for sep in [" ", "\t"] {
    let r = uu.invoke(s, "unexpand", ["-a", "-t", f"3{sep}9"], stdin: bytes.from_text("a  b     c"))?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\tc")
  }
}

# origin: uutils test_unexpand::test_blanks_ext1
test test_uu_unexpand_blanks_ext1 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t", "3,+6"], stdin: bytes.from_text("\t      "))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\t")
}

# origin: uutils test_unexpand::test_blanks_ext2
test test_uu_unexpand_blanks_ext2 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t", "3,/9"], stdin: bytes.from_text("\t      "))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\t")
}

# origin: uutils test_unexpand::test_buffered_read_edgecase_behaviour
test test_uu_unexpand_buffered_read_edgecase_behaviour { |ctx|
  let s = unexpand_scene(ctx)?
  let second_line = ["0" for _ in range(3897)].join("")
  for row in [{count: 180, input_tail: "\n        " + second_line + "\n", expected_tail: "\n\t" + second_line + "\n"}, {count: 4096, input_tail: "\n        0000\n", expected_tail: "\n\t0000\n"}, {count: 4095, input_tail: "\n        0000\n", expected_tail: "\n\t0000\n"}, {count: 4088, input_tail: "        \n        0000\n", expected_tail: "        \n\t0000\n"}, {count: 4087, input_tail: "\n00000000        \n", expected_tail: "\n00000000        \n"}, {count: 4087, input_tail: "\n                \n", expected_tail: "\n\t\t\n"}, {count: 4091, input_tail: "\n        \n", expected_tail: "\n\t\n"}] {
    let prefix = ["0" for _ in range(row.count)].join("")
    let r = uu.invoke(s, "unexpand", [], stdin: bytes.from_text(prefix + row.input_tail))?
    uu.succeeds(r)
    uu.stdout_only(r, prefix + row.expected_tail)
  }
}

# origin: uutils test_unexpand::test_comma_separated_tabs_shortcut
test test_uu_unexpand_comma_separated_tabs_shortcut { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-a", "-3,9"], stdin: bytes.from_text("a  b     c"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\tb\tc")
}

# origin: uutils test_unexpand::test_extended_tabstop_increment_overflow
test test_uu_unexpand_extended_tabstop_increment_overflow { |ctx|
  let s = unexpand_scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["--tabs=18446744073709551615,+1"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_contains(r, "tab stop is too large")
}

# origin: uutils test_unexpand::test_extended_tabstop_syntax
test test_uu_unexpand_extended_tabstop_syntax { |ctx|
  let s = unexpand_scene(ctx)?
  for row in [{tabs: "/9", input: "         ", output: "\t"}, {tabs: "/9", input: "                  ", output: "\t\t"}, {tabs: "+6", input: "      ", output: "\t"}, {tabs: "+6", input: "            ", output: "\t\t"}, {tabs: "3,/0", input: "          ", output: "\t\t\t "}, {tabs: "3,+0", input: "          ", output: "\t\t\t "}, {tabs: "3", input: "          ", output: "\t\t\t "}, {tabs: "3,/0", input: "   test", output: "\ttest"}, {tabs: "3,+6", input: "                    ", output: "\t\t\t     "}] {
    let r = uu.invoke(s, "unexpand", ["-t", row.tabs], stdin: bytes.from_text(row.input))?
    uu.succeeds(r)
    uu.stdout_is(r, row.output)
  }
}

# origin: uutils test_unexpand::test_invalid_arg
test test_uu_unexpand_invalid_arg { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["--definitely-invalid"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_unexpand::test_is_directory
test test_uu_unexpand_is_directory { |ctx|
  let s = unexpand_scene(ctx)?
  uu.mkdir(s, "dir")?
  let r = uu.invoke(s, "unexpand", ["dir"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_contains(r, "unexpand: dir: Is a directory")
}

# origin: uutils test_unexpand::test_multiple_files
test test_uu_unexpand_multiple_files { |ctx|
  let s = unexpand_scene(ctx)?
  uu.write(s, "file", "content")?
  uu.write(s, "file1", "a        b")?
  let r = uu.invoke(s, "unexpand", ["file", "file1"], stdin: bytes.from_text(""))?
  uu.succeeds(r)
  uu.stdout_is(r, "contenta        b")
}

# origin: uutils test_unexpand::test_non_utf8_filename
test test_uu_unexpand_non_utf8_filename { |ctx|
  let s = unexpand_scene(ctx)?
  let name = Path.parse_bytes(b"\xff\xfe")?
  uu.at_bytes(s, b"\xff\xfe")?.write(b"        a\n")?
  let r = uu.invoke_paths(s, "unexpand", [name])?
  uu.succeeds(r)
  uu.stdout_is(r, "\ta\n")
}

# origin: uutils test_unexpand::test_one_nonexisting_file
test test_uu_unexpand_one_nonexisting_file { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["asdf.txt"], stdin: bytes.from_text(""))?
  uu.fails(r1)
  uu.stderr_contains(r1, "asdf.txt: No such file or directory")
}

# origin: uutils test_unexpand::test_tabs_cannot_be_zero
test test_uu_unexpand_tabs_cannot_be_zero { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["--tabs=0"], stdin: bytes.from_text(""))?
  uu.fails(r1)
  uu.stderr_contains(r1, "tab size cannot be 0")
}

# origin: uutils test_unexpand::test_tabs_must_be_ascending
test test_uu_unexpand_tabs_must_be_ascending { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["--tabs=1,1"], stdin: bytes.from_text(""))?
  uu.fails(r1)
  uu.stderr_contains(r1, "tab sizes must be ascending")
}

# origin: uutils test_unexpand::test_tabs_shortcut
test test_uu_unexpand_tabs_shortcut { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-3"], stdin: bytes.from_text("   a   b"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\ta   b")
}

# origin: uutils test_unexpand::test_tabs_shortcut_combined_with_all_arg
test test_uu_unexpand_tabs_shortcut_combined_with_all_arg { |ctx|
  let s = unexpand_scene(ctx)?
  for all_arg in ["-a", "--all"] {
    let r = uu.invoke(s, "unexpand", [all_arg, "-3"], stdin: bytes.from_text("a  b  c"))?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\tc")
  }
}

# origin: uutils test_unexpand::test_tabs_shortcut_with_too_large_size
test test_uu_unexpand_tabs_shortcut_with_too_large_size { |ctx|
  let s = unexpand_scene(ctx)?
  let r = uu.invoke(s, "unexpand", ["-340282366920938463463374607431768211455"], stdin: bytes.from_text(""))?
  uu.fails(r)
  uu.stderr_contains(r, "tab stop is too large")
}

# origin: uutils test_unexpand::test_tabs_with_invalid_chars
test test_uu_unexpand_tabs_with_invalid_chars { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["--tabs=x"], stdin: bytes.from_text(""))?
  uu.fails(r1)
  uu.stderr_contains(r1, "tab size contains invalid character(s): 'x'")
  let r2 = uu.invoke(s, "unexpand", ["--tabs=1x2"], stdin: bytes.from_text(""))?
  uu.fails(r2)
  uu.stderr_contains(r2, "tab size contains invalid character(s): 'x2'")
}

# origin: uutils test_unexpand::unexpand_deep_indent_stops_at_last_tabstop
test test_uu_unexpand_unexpand_deep_indent_stops_at_last_tabstop { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t", "2,4"], stdin: bytes.from_text("            x\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\t        x\n")
}

# origin: uutils test_unexpand::unexpand_first_only_0
test test_uu_unexpand_unexpand_first_only_0 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t3"], stdin: bytes.from_text("        A     B"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\t  A\t  B")
}

# origin: uutils test_unexpand::unexpand_first_only_1
test test_uu_unexpand_unexpand_first_only_1 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t3", "--first-only"], stdin: bytes.from_text("        A     B"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\t  A     B")
}

# origin: uutils test_unexpand::unexpand_first_only_2
test test_uu_unexpand_unexpand_first_only_2 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t3", "-f"], stdin: bytes.from_text("        A     B"))?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "unexpand: invalid option -- 'f'\nTry 'unexpand --help' for more information.\n")
}

# origin: uutils test_unexpand::unexpand_first_only_3
test test_uu_unexpand_unexpand_first_only_3 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-f", "-t8"], stdin: bytes.from_text("        A     B"))?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "unexpand: invalid option -- 'f'\nTry 'unexpand --help' for more information.\n")
}

# origin: uutils test_unexpand::unexpand_flag_a_0
test test_uu_unexpand_unexpand_flag_a_0 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["--"], stdin: bytes.from_text("e     E\nf      F\ng       G\nh        H\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "e     E\nf      F\ng       G\nh        H\n")
}

# origin: uutils test_unexpand::unexpand_flag_a_1
test test_uu_unexpand_unexpand_flag_a_1 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-a"], stdin: bytes.from_text("e     E\nf      F\ng       G\nh        H\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "e     E\nf      F\ng\tG\nh\t H\n")
}

# origin: uutils test_unexpand::unexpand_flag_a_2
test test_uu_unexpand_unexpand_flag_a_2 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t8"], stdin: bytes.from_text("e     E\nf      F\ng       G\nh        H\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "e     E\nf      F\ng\tG\nh\t H\n")
}

# origin: uutils test_unexpand::unexpand_ideographic_space_absorbed_into_tab
test test_uu_unexpand_unexpand_ideographic_space_absorbed_into_tab { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-a"], stdin: bytes.from_text("　　　　Z\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "　　　　Z\n")
}

# origin: uutils test_unexpand::unexpand_ideographic_space_preserved_when_not_converted
test test_uu_unexpand_unexpand_ideographic_space_preserved_when_not_converted { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-a"], stdin: bytes.from_text("a　b\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a　b\n")
}

# origin: uutils test_unexpand::unexpand_init_0
test test_uu_unexpand_unexpand_init_0 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t4"], stdin: bytes.from_text(" 1\n  2\n   3\n    4\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, " 1\n  2\n   3\n\t4\n")
}

# origin: uutils test_unexpand::unexpand_init_1
test test_uu_unexpand_unexpand_init_1 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t4"], stdin: bytes.from_text("     5\n      6\n       7\n        8\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t 5\n\t  6\n\t   7\n\t\t8\n")
}

# origin: uutils test_unexpand::unexpand_init_list_0
test test_uu_unexpand_unexpand_init_list_0 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t2,4"], stdin: bytes.from_text(" 1\n  2\n   3\n    4\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, " 1\n\t2\n\t 3\n\t\t4\n")
}

# origin: uutils test_unexpand::unexpand_init_list_1
test test_uu_unexpand_unexpand_init_list_1 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t2,4"], stdin: bytes.from_text("     5\n      6\n       7\n        8\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\t 5\n\t\t  6\n\t\t   7\n\t\t    8\n")
}

# origin: uutils test_unexpand::unexpand_leading_spaces_before_multibyte
test test_uu_unexpand_unexpand_leading_spaces_before_multibyte { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", [], stdin: bytes.from_text("        café\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\tcafé\n")
}

# origin: uutils test_unexpand::unexpand_multibyte_utf8_gnu_compat
test test_uu_unexpand_unexpand_multibyte_utf8_gnu_compat { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-a"], stdin: bytes.from_text("1ΔΔΔ5   99999\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1ΔΔΔ5   99999\n")
}

# origin: uutils test_unexpand::unexpand_read_from_file
test test_uu_unexpand_unexpand_read_from_file { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["with_spaces.txt", "-t4"], stdin: bytes.from_text(""))?
  uu.succeeds(r1)
}

# origin: uutils test_unexpand::unexpand_read_from_two_file
test test_uu_unexpand_unexpand_read_from_two_file { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["with_spaces.txt", "with_spaces.txt", "-t4"], stdin: bytes.from_text(""))?
  uu.succeeds(r1)
}

# origin: uutils test_unexpand::unexpand_spaces_after_fields
test test_uu_unexpand_unexpand_spaces_after_fields { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-a"], stdin: bytes.from_text("   \t        A B C D             A\t\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\tA B C D\t\t    A\t\n")
}

# origin: uutils test_unexpand::unexpand_spaces_follow_tabs_0
test test_uu_unexpand_unexpand_spaces_follow_tabs_0 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", [], stdin: bytes.from_text("  \t\t   A"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\t   A")
}

# origin: uutils test_unexpand::unexpand_spaces_follow_tabs_1
test test_uu_unexpand_unexpand_spaces_follow_tabs_1 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t1,4,5"], stdin: bytes.from_text("a \t   B \t"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\t\t  B \t")
}

# origin: uutils test_unexpand::unexpand_trailing_space_0
test test_uu_unexpand_unexpand_trailing_space_0 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t4"], stdin: bytes.from_text("123 \t1\n123 1\n123 \n123 "))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "123\t\t1\n123 1\n123 \n123 ")
}

# origin: uutils test_unexpand::unexpand_trailing_space_1
test test_uu_unexpand_unexpand_trailing_space_1 { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-t1"], stdin: bytes.from_text(" abc d e  f  g "))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\tabc d e\t\tf\t\tg ")
}

# origin: uutils test_unexpand::unexpand_wide_char_width_before_tab
test test_uu_unexpand_unexpand_wide_char_width_before_tab { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-a", "-t", "4"], stdin: bytes.from_text("   　X\ty\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "   　X\ty\n")
}

# origin: uutils test_unexpand::unexpand_wide_multibyte_char_width
test test_uu_unexpand_unexpand_wide_multibyte_char_width { |ctx|
  let s = unexpand_scene(ctx)?
  let r1 = uu.invoke(s, "unexpand", ["-a"], stdin: bytes.from_text("－      X\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "－\t X\n")
}

