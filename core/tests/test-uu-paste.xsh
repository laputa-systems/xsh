##! Native ports of the uutils paste integration tests.
use support.uu as uu

# origin: uutils test_paste::test_invalid_arg
test test_uu_paste_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "paste", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_paste::test_delimiter_hyphen_leading_as_separate_arg
test test_uu_paste_delimiter_hyphen_leading_as_separate_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f1", "a\nb\n")?
  uu.write(s, "f2", "1\n2\n")?
  let r = uu.invoke(s, "paste", ["-d", "-x", "f1", "f2"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a-1\nb-2\n")
}

# origin: uutils test_paste::test_combine_pairs_of_lines
test test_uu_paste_combine_pairs_of_lines { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "paste", "html_colors.txt", "html_colors.txt")?
  let expected = fp"{ctx.core_dir}/tests/data/uutils/paste/html_colors.expected".read_bytes()?
  for serial in ["-s", "--serial"] {
    for d in ["-d", "--delimiters"] {
      let r = uu.invoke(s, "paste", [serial, d, "\t\n", "html_colors.txt"])?
      uu.succeeds(r)
      uu.stdout_is_bytes(r, expected)
    }
  }
}

# origin: uutils test_paste::test_multi_stdin
test test_uu_paste_multi_stdin { |ctx|
  let s = uu.scene(ctx)?
  let input = fp"{ctx.core_dir}/tests/data/uutils/paste/html_colors.txt".read_bytes()?
  let expected = fp"{ctx.core_dir}/tests/data/uutils/paste/html_colors.expected".read_bytes()?
  for d in ["-d", "--delimiters"] {
    let r = uu.invoke(s, "paste", [d, "\t\n", "-", "-"], stdin: input)?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, expected)
  }
}

# origin: uutils test_paste::test_delimiter_list_ending_with_escaped_backslash
test test_uu_paste_delimiter_list_ending_with_escaped_backslash { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in0", "a\n")?
  uu.write(s, "in1", "b\n")?
  for d in ["-d", "--delimiters"] {
    let r = uu.invoke(s, "paste", [d, "\\\\", "in0", "in1"])?
    uu.succeeds(r)
    uu.stdout_is(r, "a\\b\n")
  }
}

# origin: uutils test_paste::test_delimiter_list_ending_with_unescaped_backslash
test test_uu_paste_delimiter_list_ending_with_unescaped_backslash { |ctx|
  let s = uu.scene(ctx)?
  for d in ["-d", "--delimiters"] {
    for delimiter in ["\\", "\\\\\\", "_\\"] {
      let r = uu.invoke(s, "paste", [d, delimiter])?
      uu.fails(r)
      uu.stderr_contains(r, f"delimiter list ends with an unescaped backslash: {delimiter}")
    }
  }
}

# origin: uutils test_paste::test_three_trailing_backslashes_delimiter
test test_uu_paste_three_trailing_backslashes_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let delimiter = "\\\\\\"
  for d in ["-d", "--delimiters"] {
    let r = uu.invoke(s, "paste", [d, delimiter])?
    uu.fails(r)
    uu.no_stdout(r)
    assert r.stderr.utf8()?.ends_with(f": delimiter list ends with an unescaped backslash: {delimiter}\n")
  }
}

# origin: uutils test_paste::test_delimiter_list_empty
test test_uu_paste_delimiter_list_empty { |ctx|
  let s = uu.scene(ctx)?
  for d in ["-d", "--delimiters"] {
    let r = uu.invoke(s, "paste", [d, "", "-s"], stdin: bytes.from_text("A ALPHA 1 _\nB BRAVO 2 _\nC CHARLIE 3 _\n"))?
    uu.succeeds(r)
    uu.stdout_only(r, "A ALPHA 1 _B BRAVO 2 _C CHARLIE 3 _\n")
  }
}

# origin: uutils test_paste::test_delimiter_truncation
test test_uu_paste_delimiter_truncation { |ctx|
  let s = uu.scene(ctx)?
  for d in ["-d", "--delimiters"] {
    let r = uu.invoke(s, "paste", [d, "!@#", "-s", "-", "-", "-"], stdin: bytes.from_text("FIRST\nSECOND\nTHIRD\nFOURTH\nABCDEFG\n"))?
    uu.succeeds(r)
    uu.stdout_only(r, "FIRST!SECOND@THIRD#FOURTH!ABCDEFG\n\n\n")
  }
}

# origin: uutils test_paste::test_serial_delimiter_list_resets_per_file
test test_uu_paste_serial_delimiter_list_resets_per_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f1", "a\nb\n")?
  uu.write(s, "f2", "c\nd\ne\n")?
  uu.write(s, "f3", "f\ng\n")?
  for d in ["-d", "--delimiters"] {
    for serial in ["-s", "--serial"] {
      let r = uu.invoke(s, "paste", [d, ":|", serial, "f1", "f2", "f3"])?
      uu.succeeds(r)
      uu.stdout_only(r, "a:b\nc:d|e\nf:g\n")
    }
  }
}

# origin: uutils test_paste::test_non_utf8_input
test test_uu_paste_non_utf8_input { |ctx|
  let s = uu.scene(ctx)?
  let input = bytes.concat([b"Non-UTF-8 test: ", bytes.from_ints([192, 0, 192])?, b".\n"])
  let r = uu.invoke(s, "paste", [], stdin: input)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, input)
}

# origin: uutils test_paste::test_posix_unspecified_delimiter
test test_uu_paste_posix_unspecified_delimiter { |ctx|
  let s = uu.scene(ctx)?
  for d in ["-d", "--delimiters"] {
    let r = uu.invoke(s, "paste", [d, "\\z", "-s"], stdin: bytes.from_text("1\n2\n3\n4\n"))?
    uu.succeeds(r)
    uu.stdout_only(r, "1z2z3z4\n")
  }
}

# origin: uutils test_paste::test_backslash_zero_delimiter
test test_uu_paste_backslash_zero_delimiter { |ctx|
  let s = uu.scene(ctx)?
  for d in ["-d", "--delimiters"] {
    let r = uu.invoke(s, "paste", [d, "\\0z\\0", "-s"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n"))?
    uu.succeeds(r)
    uu.stdout_only(r, "12z345z6\n")
  }
}

# origin: uutils test_paste::test_paste_delimiter_escape_sequences
test test_uu_paste_paste_delimiter_escape_sequences { |ctx|
  let s = uu.scene(ctx)?
  for case in [{escape: "\\b", value: 8}, {escape: "\\f", value: 12}, {escape: "\\r", value: 13}, {escape: "\\v", value: 11}] {
    let r = uu.invoke(s, "paste", ["-s", "-d", case.escape], stdin: b"a\nb\nc\n")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, bytes.from_ints([97, case.value, 98, case.value, 99, 10])?)
  }
}

# origin: uutils test_paste::test_multi_byte_delimiter
test test_uu_paste_multi_byte_delimiter { |ctx|
  let s = uu.scene(ctx)?
  for d in ["-d", "--delimiters"] {
    let r = uu.invoke(s, "paste", [d, "!ß@", "-s"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n"), vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_only(r, "1!2ß3@4!5ß6\n")
  }
}

# origin: uutils test_paste::test_data
test test_uu_paste_data { |ctx|
  let s = uu.scene(ctx)?
  # no-nl-1
  uu.write(s, "in0", "a")?
  uu.write(s, "in1", "b")?
  {
    let r = uu.invoke(s, "paste", ["in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\n")
  }
  # no-nl-2
  uu.write(s, "in0", "a\n")?
  uu.write(s, "in1", "b")?
  {
    let r = uu.invoke(s, "paste", ["in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\n")
  }
  # no-nl-3
  uu.write(s, "in0", "a")?
  uu.write(s, "in1", "b\n")?
  {
    let r = uu.invoke(s, "paste", ["in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\n")
  }
  # no-nl-4
  uu.write(s, "in0", "a\n")?
  uu.write(s, "in1", "b\n")?
  {
    let r = uu.invoke(s, "paste", ["in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\n")
  }
  # zno-nl-1
  uu.write(s, "in0", "a")?
  uu.write(s, "in1", "b")?
  {
    let r = uu.invoke(s, "paste", ["-z", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\0")
  }
  # zno-nl-2
  uu.write(s, "in0", "a\0")?
  uu.write(s, "in1", "b")?
  {
    let r = uu.invoke(s, "paste", ["-z", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\0")
  }
  # zno-nl-3
  uu.write(s, "in0", "a")?
  uu.write(s, "in1", "b\0")?
  {
    let r = uu.invoke(s, "paste", ["-z", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\0")
  }
  # zno-nl-4
  uu.write(s, "in0", "a\0")?
  uu.write(s, "in1", "b\0")?
  {
    let r = uu.invoke(s, "paste", ["-z", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "a\tb\0")
  }
  # no-nla-1
  uu.write(s, "in0", "1\na")?
  uu.write(s, "in1", "2\nb")?
  {
    let r = uu.invoke(s, "paste", ["-d", " ", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1 2\na b\n")
  }
  # no-nla-2
  uu.write(s, "in0", "1\na\n")?
  uu.write(s, "in1", "2\nb")?
  {
    let r = uu.invoke(s, "paste", ["-d", " ", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1 2\na b\n")
  }
  # no-nla-3
  uu.write(s, "in0", "1\na")?
  uu.write(s, "in1", "2\nb\n")?
  {
    let r = uu.invoke(s, "paste", ["-d", " ", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1 2\na b\n")
  }
  # no-nla-4
  uu.write(s, "in0", "1\na\n")?
  uu.write(s, "in1", "2\nb\n")?
  {
    let r = uu.invoke(s, "paste", ["-d", " ", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1 2\na b\n")
  }
  # zno-nla1
  uu.write(s, "in0", "1\0a")?
  uu.write(s, "in1", "2\0b")?
  {
    let r = uu.invoke(s, "paste", ["-zd", " ", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1 2\0a b\0")
  }
  # zno-nla2
  uu.write(s, "in0", "1\0a\0")?
  uu.write(s, "in1", "2\0b")?
  {
    let r = uu.invoke(s, "paste", ["-zd", " ", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1 2\0a b\0")
  }
  # zno-nla3
  uu.write(s, "in0", "1\0a")?
  uu.write(s, "in1", "2\0b\0")?
  {
    let r = uu.invoke(s, "paste", ["-zd", " ", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1 2\0a b\0")
  }
  # zno-nla4
  uu.write(s, "in0", "1\0a\0")?
  uu.write(s, "in1", "2\0b\0")?
  {
    let r = uu.invoke(s, "paste", ["-zd", " ", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1 2\0a b\0")
  }
  # multibyte-delim
  uu.write(s, "in0", "1\na\n")?
  uu.write(s, "in1", "2\nb\n")?
  {
    let r = uu.invoke(s, "paste", ["-d", "💣", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1💣2\na💣b\n")
  }
  # multibyte-delim-serial
  uu.write(s, "in0", "1\na\n")?
  uu.write(s, "in1", "2\nb\n")?
  {
    let r = uu.invoke(s, "paste", ["-d", "💣", "-s", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1💣a\n2💣b\n")
  }
  # trailing whitespace
  uu.write(s, "in0", "1 \na \n")?
  uu.write(s, "in1", "2\t\nb\t\n")?
  {
    let r = uu.invoke(s, "paste", ["-d", "|", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1 |2\t\na |b\t\n")
  }
  # utf8-2byte-delim
  uu.write(s, "in0", "1\n2\n")?
  uu.write(s, "in1", "a\nb\n")?
  {
    let r = uu.invoke(s, "paste", ["-d", "¢", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1¢a\n2¢b\n")
  }
  # utf8-3byte-delim
  uu.write(s, "in0", "1\n2\n")?
  uu.write(s, "in1", "a\nb\n")?
  {
    let r = uu.invoke(s, "paste", ["-d", "€", "in0", "in1"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1€a\n2€b\n")
  }
  # utf8-4byte-delim
  uu.write(s, "in0", "1\n2\n3\n")?
  {
    let r = uu.invoke(s, "paste", ["-d", "😀", "-s", "in0"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "1😀2😀3\n")
  }
  # utf8-multi-delim-cycle
  uu.write(s, "in0", "a\nb\nc\n")?
  uu.write(s, "in1", "1\n2\n3\n")?
  uu.write(s, "in2", "x\ny\nz\n")?
  {
    let r = uu.invoke(s, "paste", ["-d", "¢€", "in0", "in1", "in2"], vars: {"LC_ALL": "C.UTF-8"})?
    uu.succeeds(r)
    uu.stdout_is(r, "a¢1€x\nb¢2€y\nc¢3€z\n")
  }
}

# origin: uutils test_paste::test_non_utf8_delimiter
test test_uu_paste_non_utf8_delimiter { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f1", "1\n2\n")?
  uu.write(s, "f2", "a\nb\n")?
  let delimiter = Path.parse_bytes(bytes.from_ints([162, 227])?)?
  let r = uu.invoke_paths(s, "paste", [p"-d", delimiter, p"f1", p"f2"], vars: {"LC_ALL": "zh_CN.gb18030"})?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.concat([b"1", bytes.from_ints([162, 227])?, b"a\n2", bytes.from_ints([162, 227])?, b"b\n"]))
}

# origin: uutils test_paste::test_paste_non_utf8_paths
test test_uu_paste_paste_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let first = uu.at_bytes(s, bytes.from_ints([255, 254])?)?
  let second = uu.at_bytes(s, bytes.from_ints([240, 144])?)?
  first.write(b"line1\nline2\n")?
  second.write(b"col1\ncol2\n")?
  let r = uu.invoke_paths(s, "paste", [first, second])?
  uu.succeeds(r)
  uu.stdout_is(r, "line1\tcol1\nline2\tcol2\n")
}

# origin: uutils test_paste::test_dev_zero_write_error_dev_full
test test_uu_paste_dev_zero_write_error_dev_full { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "paste", ["/dev/zero"], stdout: p"/dev/full", timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "No space left on device")
}

# origin: uutils test_paste::test_repeated_delimiter_takes_the_last
test test_uu_paste_repeated_delimiter_takes_the_last { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "paste", ["-d", ",", "-d", ":", "-s", "-"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r)
  uu.stdout_is(r, "a:b\n")
}

# origin: uutils test_paste::test_crlf_input_in_serial_mode
test test_uu_paste_crlf_input_in_serial_mode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "paste", ["-s", "-d", ","], stdin: bytes.from_text("1\r\n2\r\n3\r\n"))?
  uu.succeeds(r)
  uu.stdout_is(r, "1\r,2\r,3\r\n")
}

# origin: uutils test_paste::test_crlf_input_in_parallel_mode
test test_uu_paste_crlf_input_in_parallel_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a", b"1\r\n2\r\n")?
  uu.write_bytes(s, "b", b"3\r\n4\r\n")?
  let r = uu.invoke(s, "paste", ["a", "b"])?
  uu.succeeds(r)
  uu.stdout_is(r, "1\r\t3\r\n2\r\t4\r\n")
}

# origin: uutils test_paste::test_crlf_kept_when_input_is_zero_terminated
test test_uu_paste_crlf_kept_when_input_is_zero_terminated { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "paste", ["-s", "-z", "-d", ","], stdin: bytes.from_text("1\r\n2\r\0"))?
  uu.succeeds(r)
  uu.stdout_is(r, "1\r\n2\r\0")
}

# origin: uutils test_paste::test_crlf_input_of_a_single_file_is_copied
test test_uu_paste_crlf_input_of_a_single_file_is_copied { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a", b"1\r\n2\r\n")?
  let r = uu.invoke(s, "paste", ["a"])?
  uu.succeeds(r)
  uu.stdout_is(r, "1\r\n2\r\n")
}

# origin: uutils test_paste::test_crlf_kept_when_last_line_ends_with_cr_and_no_newline
test test_uu_paste_crlf_kept_when_last_line_ends_with_cr_and_no_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "paste", ["-s", "-d", ","], stdin: bytes.from_text("1\r\n2\r"))?
  uu.succeeds(r)
  uu.stdout_is(r, "1\r,2\r\n")
}

# origin: uutils test_paste::test_crlf_delimiter_not_eaten_by_an_empty_line
test test_uu_paste_crlf_delimiter_not_eaten_by_an_empty_line { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "paste", ["-s", "-d", "\\r"], stdin: bytes.from_text("1\r\n\n"))?
  uu.succeeds(r)
  uu.stdout_is(r, "1\r\r\n")
}
