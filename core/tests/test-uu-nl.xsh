##! Transcribed line numbering tests from the uutils coreutils integration suite.

use support.uu as uu

# origin: uutils test_nl::test_default_body_numbering
test test_uu_nl_default_body_numbering { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", [], stdin: bytes.from_text("a\n\nb"))?
  uu.succeeds(r)
  uu.stdout_is(r, "     1\ta\n       \n     2\tb\n")

}

# origin: uutils test_nl::test_invalid_arg
test test_uu_nl_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)

}

# origin: uutils test_nl::test_number_width_max_i32
test test_uu_nl_number_width_max_i32 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-w", "2147483647"], stdin: bytes.from_text(""))?
  uu.succeeds(r)

}

# origin: uutils test_nl::test_padding_with_overflow
test test_uu_nl_padding_with_overflow { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "nl", "simple.txt", "simple.txt")?
  let r = uu.invoke(s, "nl", ["-i", "1000", "-s", "x", "-n", "rz", "-w", "4", "simple.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "0001xL1\n1001xL2\n2001xL3\n3001xL4\n4001xL5\n5001xL6\n6001xL7\n7001xL8\n8001xL9\n9001xL10\n10001xL11\n11001xL12\n12001xL13\n13001xL14\n14001xL15\n")

}

# origin: uutils test_nl::test_padding_without_overflow
test test_uu_nl_padding_without_overflow { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "nl", "simple.txt", "simple.txt")?
  let r = uu.invoke(s, "nl", ["-i", "1000", "-s", "x", "-n", "rz", "simple.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "000001xL1\n001001xL2\n002001xL3\n003001xL4\n004001xL5\n005001xL6\n006001xL7\n007001xL8\n008001xL9\n009001xL10\n010001xL11\n011001xL12\n012001xL13\n013001xL14\n014001xL15\n")

}

# origin: uutils test_nl::test_repeated_body_numbering_flag
test test_uu_nl_repeated_body_numbering_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-ba", "-bt"], stdin: bytes.from_text("a\n\nb\n\nc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "     1\ta\n       \n     2\tb\n       \n     3\tc\n")

}

# origin: uutils test_nl::test_repeated_footer_numbering_flag
test test_uu_nl_repeated_footer_numbering_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-fa", "-ft"], stdin: bytes.from_text("\\:\na\nb\n\nc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "\n     1\ta\n     2\tb\n       \n     3\tc\n")

}

# origin: uutils test_nl::test_repeated_header_numbering_flag
test test_uu_nl_repeated_header_numbering_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-ha", "-ht"], stdin: bytes.from_text("\\:\\:\\:\na\nb\n\nc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "\n     1\ta\n     2\tb\n       \n     3\tc\n")

}

# origin: uutils test_nl::test_repeated_join_blank_lines_flag
test test_uu_nl_repeated_join_blank_lines_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-l", "1", "-l", "2", "-ba"], stdin: bytes.from_text("a\n\n\nb"))?
  uu.succeeds(r)
  uu.stdout_is(r, "     1\ta\n       \n     2\t\n     3\tb\n")

}

# origin: uutils test_nl::test_repeated_line_increment_flag
test test_uu_nl_repeated_line_increment_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-i", "1", "-i", "5"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "     1\ta\n     6\tb\n    11\tc\n")

}

# origin: uutils test_nl::test_repeated_number_format_flag
test test_uu_nl_repeated_number_format_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-n", "ln", "-n", "rn"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "     1\ta\n     2\tb\n     3\tc\n")

}

# origin: uutils test_nl::test_repeated_number_separator_flag
test test_uu_nl_repeated_number_separator_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-s", ":", "-s", "|"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "     1|a\n     2|b\n     3|c\n")

}

# origin: uutils test_nl::test_repeated_number_width_flag
test test_uu_nl_repeated_number_width_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-w", "3", "-w", "8"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "       1\ta\n       2\tb\n       3\tc\n")

}

# origin: uutils test_nl::test_repeated_section_delimiter_flag
test test_uu_nl_repeated_section_delimiter_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-d", ":", "-d", "|"], stdin: bytes.from_text("|:|:|:\na\nb\nc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "\n       a\n       b\n       c\n")

}

# origin: uutils test_nl::test_repeated_starting_line_number_flag
test test_uu_nl_repeated_starting_line_number_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-v", "1", "-v", "10"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r)
  uu.stdout_is(r, "    10\ta\n    11\tb\n    12\tc\n")

}

# origin: uutils test_nl::test_stdin_newline
test test_uu_nl_stdin_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", ["-s", "-", "-w", "1"], stdin: bytes.from_text("Line One\nLine Two\n"))?
  uu.succeeds(r)
  uu.stdout_is(r, "1-Line One\n2-Line Two\n")

}

# origin: uutils test_nl::test_stdin_no_newline
test test_uu_nl_stdin_no_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", [], stdin: bytes.from_text("No Newline"))?
  uu.succeeds(r)
  uu.stdout_is(r, "     1\tNo Newline\n")

}

# origin: uutils test_nl::test_no_renumber
test test_uu_nl_no_renumber { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-p"], stdin: bytes.from_text("a\n\\:\\:\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n\n     2\tb\n")

  let r1 = uu.invoke(s, "nl", ["--no-renumber"], stdin: bytes.from_text("a\n\\:\\:\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n\n     2\tb\n")

}

# origin: uutils test_nl::test_number_format_ln
test test_uu_nl_number_format_ln { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-nln"], stdin: bytes.from_text("test"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1     \ttest\n")

  let r1 = uu.invoke(s, "nl", ["--number-format=ln"], stdin: bytes.from_text("test"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1     \ttest\n")

}

# origin: uutils test_nl::test_number_format_rn
test test_uu_nl_number_format_rn { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-nrn"], stdin: bytes.from_text("test"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ttest\n")

  let r1 = uu.invoke(s, "nl", ["--number-format=rn"], stdin: bytes.from_text("test"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ttest\n")

}

# origin: uutils test_nl::test_number_format_rz
test test_uu_nl_number_format_rz { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-nrz"], stdin: bytes.from_text("test"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "000001\ttest\n")

  let r1 = uu.invoke(s, "nl", ["--number-format=rz"], stdin: bytes.from_text("test"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "000001\ttest\n")

}

# origin: uutils test_nl::test_number_format_rz_with_negative_line_number
test test_uu_nl_number_format_rz_with_negative_line_number { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-nrz", "-v-12"], stdin: bytes.from_text("test"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "-00012\ttest\n")

  let r1 = uu.invoke(s, "nl", ["--number-format=rz", "-v-12"], stdin: bytes.from_text("test"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "-00012\ttest\n")

}

# origin: uutils test_nl::test_number_separator
test test_uu_nl_number_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-s:-:"], stdin: bytes.from_text("test"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1:-:test\n")

  let r1 = uu.invoke(s, "nl", ["--number-separator=:-:"], stdin: bytes.from_text("test"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1:-:test\n")

}

# origin: uutils test_nl::test_starting_line_number
test test_uu_nl_starting_line_number { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-v10"], stdin: bytes.from_text("test"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "    10\ttest\n")

  let r1 = uu.invoke(s, "nl", ["--starting-line-number=10"], stdin: bytes.from_text("test"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "    10\ttest\n")

}

# origin: uutils test_nl::test_negative_starting_line_number
test test_uu_nl_negative_starting_line_number { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-v-10"], stdin: bytes.from_text("test"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "   -10\ttest\n")

  let r1 = uu.invoke(s, "nl", ["--starting-line-number=-10"], stdin: bytes.from_text("test"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "   -10\ttest\n")

}

# origin: uutils test_nl::test_line_increment
test test_uu_nl_line_increment { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-i10"], stdin: bytes.from_text("a\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n    11\tb\n")

  let r1 = uu.invoke(s, "nl", ["--line-increment=10"], stdin: bytes.from_text("a\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n    11\tb\n")

}

# origin: uutils test_nl::test_line_increment_from_negative_starting_line
test test_uu_nl_line_increment_from_negative_starting_line { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-i10", "-v-19"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "   -19\ta\n    -9\tb\n     1\tc\n")

  let r1 = uu.invoke(s, "nl", ["--line-increment=10", "-v-19"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "   -19\ta\n    -9\tb\n     1\tc\n")

}

# origin: uutils test_nl::test_negative_line_increment
test test_uu_nl_negative_line_increment { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-i-10"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n    -9\tb\n   -19\tc\n")

  let r1 = uu.invoke(s, "nl", ["--line-increment=-10"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n    -9\tb\n   -19\tc\n")

}

# origin: uutils test_nl::test_body_numbering_all_lines_without_delimiter
test test_uu_nl_body_numbering_all_lines_without_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-ba"], stdin: bytes.from_text("a\n\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n     2\t\n     3\tb\n")

  let r1 = uu.invoke(s, "nl", ["--body-numbering=a"], stdin: bytes.from_text("a\n\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n     2\t\n     3\tb\n")

}

# origin: uutils test_nl::test_body_numbering_no_lines_without_delimiter
test test_uu_nl_body_numbering_no_lines_without_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-bn"], stdin: bytes.from_text("a\n\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "       a\n       \n       b\n")

  let r1 = uu.invoke(s, "nl", ["--body-numbering=n"], stdin: bytes.from_text("a\n\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "       a\n       \n       b\n")

}

# origin: uutils test_nl::test_body_numbering_non_empty_lines_without_delimiter
test test_uu_nl_body_numbering_non_empty_lines_without_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-bt"], stdin: bytes.from_text("a\n\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n       \n     2\tb\n")

  let r1 = uu.invoke(s, "nl", ["--body-numbering=t"], stdin: bytes.from_text("a\n\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n       \n     2\tb\n")

}

# origin: uutils test_nl::test_body_numbering_matched_lines_without_delimiter
test test_uu_nl_body_numbering_matched_lines_without_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-bp^[ac]"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n       b\n     2\tc\n")

  let r1 = uu.invoke(s, "nl", ["--body-numbering=p^[ac]"], stdin: bytes.from_text("a\nb\nc"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n       b\n     2\tc\n")

}

# origin: uutils test_nl::test_empty_section_delimiter
test test_uu_nl_empty_section_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-d ''"], stdin: bytes.from_text("a\n\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n       \n     2\tb\n")

  let r1 = uu.invoke(s, "nl", ["--section-delimiter=''"], stdin: bytes.from_text("a\n\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n       \n     2\tb\n")

}

# origin: uutils test_nl::test_number_width
test test_uu_nl_number_width { |ctx|
  let s = uu.scene(ctx)?
  for width in range(1, 10) {
    for arg in [f"-w{width}", f"--number-width={width}"] {
      let r = uu.invoke(s, "nl", [arg], stdin: b"test")?
      uu.succeeds(r)
      let spaces = [" " for unused in range(width - 1)].join("")
      uu.stdout_is(r, f"{spaces}1\ttest\n")
    }
  }
}

# origin: uutils test_nl::test_join_blank_lines
test test_uu_nl_join_blank_lines { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-l3", "--body-numbering=a"], stdin: bytes.from_text("\n\n\n\n\n\n"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "       \n       \n     1\t\n       \n       \n     2\t\n")

  let r1 = uu.invoke(s, "nl", ["--join-blank-lines=3", "--body-numbering=a"], stdin: bytes.from_text("\n\n\n\n\n\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "       \n       \n     1\t\n       \n       \n     2\t\n")

}

# origin: uutils test_nl::test_join_blank_lines_zero
test test_uu_nl_join_blank_lines_zero { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-l0", "--body-numbering=a"], stdin: bytes.from_text("\n\n\n\n\n\n"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\t\n     2\t\n     3\t\n     4\t\n     5\t\n     6\t\n")

  let r1 = uu.invoke(s, "nl", ["--join-blank-lines=0", "--body-numbering=a"], stdin: bytes.from_text("\n\n\n\n\n\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\t\n     2\t\n     3\t\n     4\t\n     5\t\n     6\t\n")

}

# origin: uutils test_nl::test_join_blank_lines_multiple_files
test test_uu_nl_join_blank_lines_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a.txt", "\n\n")?
  uu.write(s, "b.txt", "\n\n")?
  uu.write(s, "c.txt", "\n\n")?
  let r0 = uu.invoke(s, "nl", ["-l3", "--body-numbering=a", "a.txt", "b.txt", "c.txt"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "       \n       \n     1\t\n       \n       \n     2\t\n")

  let r1 = uu.invoke(s, "nl", ["--join-blank-lines=3", "--body-numbering=a", "a.txt", "b.txt", "c.txt"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "       \n       \n     1\t\n       \n       \n     2\t\n")

}

# origin: uutils test_nl::test_default_body_numbering_multiple_files
test test_uu_nl_default_body_numbering_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a.txt", "a")?
  uu.write(s, "b.txt", "b")?
  uu.write(s, "c.txt", "c")?
  let r = uu.invoke(s, "nl", ["a.txt", "b.txt", "c.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "     1\ta\n     2\tb\n     3\tc\n")

}

# origin: uutils test_nl::test_default_body_numbering_multiple_files_and_stdin
test test_uu_nl_default_body_numbering_multiple_files_and_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a.txt", "a")?
  uu.write(s, "c.txt", "c")?
  let r = uu.invoke(s, "nl", ["a.txt", "-", "c.txt"], stdin: bytes.from_text("b"))?
  uu.succeeds(r)
  uu.stdout_is(r, "     1\ta\n     2\tb\n     3\tc\n")

}

# origin: uutils test_nl::test_default_body_numbering_multiple_files_with_non_existing_file
test test_uu_nl_default_body_numbering_multiple_files_with_non_existing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a.txt", "a")?
  uu.write(s, "b.txt", "b")?
  let r = uu.invoke(s, "nl", ["a.txt", "non_existing", "b.txt"])?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "     1\ta\n     2\tb\n")
  uu.stderr_is(r, "nl: non_existing: No such file or directory\n")
}

# origin: uutils test_nl::test_numbering_all_lines
test test_uu_nl_numbering_all_lines { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-ha"], stdin: bytes.from_text("\\:\\:\\:\na\n\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "\n     1\ta\n     2\t\n     3\tb\n")

  let r1 = uu.invoke(s, "nl", ["--header-numbering=a"], stdin: bytes.from_text("\\:\\:\\:\na\n\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\n     1\ta\n     2\t\n     3\tb\n")

  let r2 = uu.invoke(s, "nl", ["-ba"], stdin: bytes.from_text("\\:\\:\na\n\nb"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "\n     1\ta\n     2\t\n     3\tb\n")

  let r3 = uu.invoke(s, "nl", ["--body-numbering=a"], stdin: bytes.from_text("\\:\\:\na\n\nb"))?
  uu.succeeds(r3)
  uu.stdout_is(r3, "\n     1\ta\n     2\t\n     3\tb\n")

  let r4 = uu.invoke(s, "nl", ["-fa"], stdin: bytes.from_text("\\:\na\n\nb"))?
  uu.succeeds(r4)
  uu.stdout_is(r4, "\n     1\ta\n     2\t\n     3\tb\n")

  let r5 = uu.invoke(s, "nl", ["--footer-numbering=a"], stdin: bytes.from_text("\\:\na\n\nb"))?
  uu.succeeds(r5)
  uu.stdout_is(r5, "\n     1\ta\n     2\t\n     3\tb\n")

}

# origin: uutils test_nl::test_numbering_no_lines
test test_uu_nl_numbering_no_lines { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-hn"], stdin: bytes.from_text("\\:\\:\\:\na\n\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "\n       a\n       \n       b\n")

  let r1 = uu.invoke(s, "nl", ["--header-numbering=n"], stdin: bytes.from_text("\\:\\:\\:\na\n\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\n       a\n       \n       b\n")

  let r2 = uu.invoke(s, "nl", ["-bn"], stdin: bytes.from_text("\\:\\:\na\n\nb"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "\n       a\n       \n       b\n")

  let r3 = uu.invoke(s, "nl", ["--body-numbering=n"], stdin: bytes.from_text("\\:\\:\na\n\nb"))?
  uu.succeeds(r3)
  uu.stdout_is(r3, "\n       a\n       \n       b\n")

  let r4 = uu.invoke(s, "nl", ["-fn"], stdin: bytes.from_text("\\:\na\n\nb"))?
  uu.succeeds(r4)
  uu.stdout_is(r4, "\n       a\n       \n       b\n")

  let r5 = uu.invoke(s, "nl", ["--footer-numbering=n"], stdin: bytes.from_text("\\:\na\n\nb"))?
  uu.succeeds(r5)
  uu.stdout_is(r5, "\n       a\n       \n       b\n")

}

# origin: uutils test_nl::test_numbering_non_empty_lines
test test_uu_nl_numbering_non_empty_lines { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-ht"], stdin: bytes.from_text("\\:\\:\\:\na\n\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "\n     1\ta\n       \n     2\tb\n")

  let r1 = uu.invoke(s, "nl", ["--header-numbering=t"], stdin: bytes.from_text("\\:\\:\\:\na\n\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\n     1\ta\n       \n     2\tb\n")

  let r2 = uu.invoke(s, "nl", ["-bt"], stdin: bytes.from_text("\\:\\:\na\n\nb"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "\n     1\ta\n       \n     2\tb\n")

  let r3 = uu.invoke(s, "nl", ["--body-numbering=t"], stdin: bytes.from_text("\\:\\:\na\n\nb"))?
  uu.succeeds(r3)
  uu.stdout_is(r3, "\n     1\ta\n       \n     2\tb\n")

  let r4 = uu.invoke(s, "nl", ["-ft"], stdin: bytes.from_text("\\:\na\n\nb"))?
  uu.succeeds(r4)
  uu.stdout_is(r4, "\n     1\ta\n       \n     2\tb\n")

  let r5 = uu.invoke(s, "nl", ["--footer-numbering=t"], stdin: bytes.from_text("\\:\na\n\nb"))?
  uu.succeeds(r5)
  uu.stdout_is(r5, "\n     1\ta\n       \n     2\tb\n")

}

# origin: uutils test_nl::test_numbering_matched_lines
test test_uu_nl_numbering_matched_lines { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-hp^[ac]"], stdin: bytes.from_text("\\:\\:\\:\na\nb\nc"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "\n     1\ta\n       b\n     2\tc\n")

  let r1 = uu.invoke(s, "nl", ["--header-numbering=p^[ac]"], stdin: bytes.from_text("\\:\\:\\:\na\nb\nc"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\n     1\ta\n       b\n     2\tc\n")

  let r2 = uu.invoke(s, "nl", ["-bp^[ac]"], stdin: bytes.from_text("\\:\\:\na\nb\nc"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "\n     1\ta\n       b\n     2\tc\n")

  let r3 = uu.invoke(s, "nl", ["--body-numbering=p^[ac]"], stdin: bytes.from_text("\\:\\:\na\nb\nc"))?
  uu.succeeds(r3)
  uu.stdout_is(r3, "\n     1\ta\n       b\n     2\tc\n")

  let r4 = uu.invoke(s, "nl", ["-fp^[ac]"], stdin: bytes.from_text("\\:\na\nb\nc"))?
  uu.succeeds(r4)
  uu.stdout_is(r4, "\n     1\ta\n       b\n     2\tc\n")

  let r5 = uu.invoke(s, "nl", ["--footer-numbering=p^[ac]"], stdin: bytes.from_text("\\:\na\nb\nc"))?
  uu.succeeds(r5)
  uu.stdout_is(r5, "\n     1\ta\n       b\n     2\tc\n")

}

# origin: uutils test_nl::test_invalid_numbering
test test_uu_nl_invalid_numbering { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-hinvalid"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "invalid header numbering style: 'invalid'")
  let r1 = uu.invoke(s, "nl", ["--header-numbering=invalid"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid header numbering style: 'invalid'")
  let r2 = uu.invoke(s, "nl", ["-binvalid"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "invalid body numbering style: 'invalid'")
  let r3 = uu.invoke(s, "nl", ["--body-numbering=invalid"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "invalid body numbering style: 'invalid'")
  let r4 = uu.invoke(s, "nl", ["-finvalid"])?
  uu.fails(r4)
  uu.stderr_contains(r4, "invalid footer numbering style: 'invalid'")
  let r5 = uu.invoke(s, "nl", ["--footer-numbering=invalid"])?
  uu.fails(r5)
  uu.stderr_contains(r5, "invalid footer numbering style: 'invalid'")
}

# origin: uutils test_nl::test_invalid_regex_numbering
test test_uu_nl_invalid_regex_numbering { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-hp["])?
  uu.fails(r0)
  uu.stderr_contains(r0, "Invalid regular expression")
  let r1 = uu.invoke(s, "nl", ["--header-numbering=p["])?
  uu.fails(r1)
  uu.stderr_contains(r1, "Invalid regular expression")
  let r2 = uu.invoke(s, "nl", ["-bp["])?
  uu.fails(r2)
  uu.stderr_contains(r2, "Invalid regular expression")
  let r3 = uu.invoke(s, "nl", ["--body-numbering=p["])?
  uu.fails(r3)
  uu.stderr_contains(r3, "Invalid regular expression")
  let r4 = uu.invoke(s, "nl", ["-fp["])?
  uu.fails(r4)
  uu.stderr_contains(r4, "Invalid regular expression")
  let r5 = uu.invoke(s, "nl", ["--footer-numbering=p["])?
  uu.fails(r5)
  uu.stderr_contains(r5, "Invalid regular expression")
}

# origin: uutils test_nl::test_line_number_overflow
test test_uu_nl_line_number_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["--starting-line-number=9223372036854775807"], stdin: bytes.from_text("a\nb"))?
  uu.fails(r0)
  uu.stdout_is(r0, "9223372036854775807\ta\n")
  uu.stderr_is(r0, "nl: line number overflow\n")
  let r1 = uu.invoke(s, "nl", ["--starting-line-number=-9223372036854775808", "--line-increment=-1"], stdin: bytes.from_text("a\nb"))?
  uu.fails(r1)
  uu.stdout_is(r1, "-9223372036854775808\ta\n")
  uu.stderr_is(r1, "nl: line number overflow\n")
}

# origin: uutils test_nl::test_line_number_no_overflow
test test_uu_nl_line_number_no_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["--starting-line-number=9223372036854775807"], stdin: bytes.from_text("a\n\\:\\:\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "9223372036854775807\ta\n\n9223372036854775807\tb\n")

  let r1 = uu.invoke(s, "nl", ["--starting-line-number=-9223372036854775808", "--line-increment=-1"], stdin: bytes.from_text("a\n\\:\\:\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "-9223372036854775808\ta\n\n-9223372036854775808\tb\n")

}

# origin: uutils test_nl::test_section_delimiter
test test_uu_nl_section_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-dabc"], stdin: bytes.from_text("a\nabcabcabc\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n\n       b\n")

  let r1 = uu.invoke(s, "nl", ["-dabc"], stdin: bytes.from_text("a\nabcabc\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n\n     1\tb\n")

  let r2 = uu.invoke(s, "nl", ["-dabc"], stdin: bytes.from_text("a\nabc\nb"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "     1\ta\n\n       b\n")

  let r3 = uu.invoke(s, "nl", ["--section-delimiter=abc"], stdin: bytes.from_text("a\nabcabcabc\nb"))?
  uu.succeeds(r3)
  uu.stdout_is(r3, "     1\ta\n\n       b\n")

  let r4 = uu.invoke(s, "nl", ["--section-delimiter=abc"], stdin: bytes.from_text("a\nabcabc\nb"))?
  uu.succeeds(r4)
  uu.stdout_is(r4, "     1\ta\n\n     1\tb\n")

  let r5 = uu.invoke(s, "nl", ["--section-delimiter=abc"], stdin: bytes.from_text("a\nabc\nb"))?
  uu.succeeds(r5)
  uu.stdout_is(r5, "     1\ta\n\n       b\n")

}

# origin: uutils test_nl::test_one_char_section_delimiter
test test_uu_nl_one_char_section_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-da"], stdin: bytes.from_text("a\na:a:a:\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n\n       b\n")

  let r1 = uu.invoke(s, "nl", ["-da"], stdin: bytes.from_text("a\na:a:\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n\n     1\tb\n")

  let r2 = uu.invoke(s, "nl", ["-da"], stdin: bytes.from_text("a\na:\nb"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "     1\ta\n\n       b\n")

  let r3 = uu.invoke(s, "nl", ["--section-delimiter=a"], stdin: bytes.from_text("a\na:a:a:\nb"))?
  uu.succeeds(r3)
  uu.stdout_is(r3, "     1\ta\n\n       b\n")

  let r4 = uu.invoke(s, "nl", ["--section-delimiter=a"], stdin: bytes.from_text("a\na:a:\nb"))?
  uu.succeeds(r4)
  uu.stdout_is(r4, "     1\ta\n\n     1\tb\n")

  let r5 = uu.invoke(s, "nl", ["--section-delimiter=a"], stdin: bytes.from_text("a\na:\nb"))?
  uu.succeeds(r5)
  uu.stdout_is(r5, "     1\ta\n\n       b\n")

}

# origin: uutils test_nl::test_multi_byte_one_char_section_delimiter
test test_uu_nl_multi_byte_one_char_section_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "nl", ["-dä"], stdin: bytes.from_text("a\nä:ä:ä:\nb"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n     2\tä:ä:ä:\n     3\tb\n")

  let r1 = uu.invoke(s, "nl", ["-dä"], stdin: bytes.from_text("a\nä:ä:\nb"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n     2\tä:ä:\n     3\tb\n")

  let r2 = uu.invoke(s, "nl", ["-dä"], stdin: bytes.from_text("a\nä:\nb"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "     1\ta\n     2\tä:\n     3\tb\n")

  let r3 = uu.invoke(s, "nl", ["--section-delimiter=ä"], stdin: bytes.from_text("a\nä:ä:ä:\nb"))?
  uu.succeeds(r3)
  uu.stdout_is(r3, "     1\ta\n     2\tä:ä:ä:\n     3\tb\n")

  let r4 = uu.invoke(s, "nl", ["--section-delimiter=ä"], stdin: bytes.from_text("a\nä:ä:\nb"))?
  uu.succeeds(r4)
  uu.stdout_is(r4, "     1\ta\n     2\tä:ä:\n     3\tb\n")

  let r5 = uu.invoke(s, "nl", ["--section-delimiter=ä"], stdin: bytes.from_text("a\nä:\nb"))?
  uu.succeeds(r5)
  uu.stdout_is(r5, "     1\ta\n     2\tä:\n     3\tb\n")

}

# origin: uutils test_nl::test_section_delimiter_non_utf8
test test_uu_nl_section_delimiter_non_utf8 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"-d\xff\xfe")?], stdin: b"a\n\xff\xfe\xff\xfe\xff\xfe\nb")?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n\n       b\n")
  let r1 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"-d\xff\xfe")?], stdin: b"a\n\xff\xfe\xff\xfe\nb")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n\n     1\tb\n")
  let r2 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"-d\xff\xfe")?], stdin: b"a\n\xff\xfe\nb")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "     1\ta\n\n       b\n")
  let r3 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"--section-delimiter=\xff\xfe")?], stdin: b"a\n\xff\xfe\xff\xfe\xff\xfe\nb")?
  uu.succeeds(r3)
  uu.stdout_is(r3, "     1\ta\n\n       b\n")
  let r4 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"--section-delimiter=\xff\xfe")?], stdin: b"a\n\xff\xfe\xff\xfe\nb")?
  uu.succeeds(r4)
  uu.stdout_is(r4, "     1\ta\n\n     1\tb\n")
  let r5 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"--section-delimiter=\xff\xfe")?], stdin: b"a\n\xff\xfe\nb")?
  uu.succeeds(r5)
  uu.stdout_is(r5, "     1\ta\n\n       b\n")
}

# origin: uutils test_nl::test_one_byte_section_delimiter
test test_uu_nl_one_byte_section_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"-d\xff")?], stdin: b"a\n\xff:\xff:\xff:\nb")?
  uu.succeeds(r0)
  uu.stdout_is(r0, "     1\ta\n\n       b\n")
  let r1 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"-d\xff")?], stdin: b"a\n\xff:\xff:\nb")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "     1\ta\n\n     1\tb\n")
  let r2 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"-d\xff")?], stdin: b"a\n\xff:\nb")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "     1\ta\n\n       b\n")
  let r3 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"--section-delimiter=\xff")?], stdin: b"a\n\xff:\xff:\xff:\nb")?
  uu.succeeds(r3)
  uu.stdout_is(r3, "     1\ta\n\n       b\n")
  let r4 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"--section-delimiter=\xff")?], stdin: b"a\n\xff:\xff:\nb")?
  uu.succeeds(r4)
  uu.stdout_is(r4, "     1\ta\n\n     1\tb\n")
  let r5 = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"--section-delimiter=\xff")?], stdin: b"a\n\xff:\nb")?
  uu.succeeds(r5)
  uu.stdout_is(r5, "     1\ta\n\n       b\n")
}

# origin: uutils test_nl::test_number_separator_non_utf8
test test_uu_nl_number_separator_non_utf8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"--number-separator=\xff\xfe")?], stdin: b"test")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"     1\xff\xfetest\n")
}

# origin: uutils test_nl::test_non_utf8_paths
test test_uu_nl_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let filename = uu.at_bytes(s, b"\xff\xfe")?
  filename.write(b"line 1\nline 2\nline 3\n")?
  let r = uu.invoke_paths(s, "nl", [Path.parse_bytes(b"\xff\xfe")?])?
  uu.succeeds(r)
  uu.stdout_contains(r, "1\t")
  uu.stdout_contains(r, "2\t")
  uu.stdout_contains(r, "3\t")
}

# origin: uutils test_nl::test_file_with_non_utf8_content
test test_uu_nl_file_with_non_utf8_content { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "file", b"a\n\xff\xfe\nb")?
  let r = uu.invoke(s, "nl", ["file"])?
  uu.succeeds(r)

  uu.stdout_is_bytes(r, b"     1\ta\n     2\t\xff\xfe\n     3\tb\n")
}

# origin: uutils test_nl::test_stdin_non_utf8_preserved
test test_uu_nl_stdin_non_utf8_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "nl", [], stdin: b"f\xe9vr.\n")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"     1\tf\xe9vr.\n")
}

# origin: uutils test_nl::test_directory_as_input
test test_uu_nl_directory_as_input { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.write(s, "file", "aaa")?
  let r = uu.invoke(s, "nl", ["dir", "file"])?
  uu.fails(r)
  uu.stderr_is(r, "nl: dir: Is a directory\n")
  uu.stdout_contains(r, "aaa")
}

# origin: uutils test_nl::test_no_skip_after_error
test test_uu_nl_no_skip_after_error { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "hello")?
  let r = uu.invoke(s, "nl", ["/proc/self/mem", "f"])?
  uu.fails(r)
  uu.stdout_is(r, "     1\thello\n")

}

# origin: uutils test_nl::test_sections_and_styles
test test_uu_nl_sections_and_styles { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "nl", "section.txt", "section.txt")?
  let r0 = uu.invoke(s, "nl", ["-s", "|", "-n", "ln", "-w", "3", "-b", "a", "-l", "5", "section.txt"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "\n    HEADER1\n    HEADER2\n\n1  |BODY1\n2  |BODY2\n\n    FOOTER1\n    FOOTER2\n\n    NEXTHEADER1\n    NEXTHEADER2\n\n1  |NEXTBODY1\n2  |NEXTBODY2\n\n    NEXTFOOTER1\n    NEXTFOOTER2\n")

  uu.fixture(s, "nl", "joinblanklines.txt", "joinblanklines.txt")?
  let r1 = uu.invoke(s, "nl", ["-s", "|", "-n", "ln", "-w", "3", "-b", "a", "-l", "5", "joinblanklines.txt"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1  |Nonempty\n2  |Nonempty\n3  |Followed by 10x empty\n    \n    \n    \n    \n4  |\n    \n    \n    \n    \n5  |\n6  |Followed by 5x empty\n    \n    \n    \n    \n7  |\n8  |Followed by 4x empty\n    \n    \n    \n    \n9  |Nonempty\n10 |Nonempty\n11 |Nonempty.\n")

}
