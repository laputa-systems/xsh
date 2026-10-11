##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_tac.rs.

use support.uu as uu

# origin: uutils test_tac::test_before_empty_file
test test_uu_tac_before_empty_file { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-b"], stdin: b"")?
  uu.succeeds(r0)
  uu.no_output(r0)
}

# origin: uutils test_tac::test_before_leading_separator_no_trailing_separator
test test_uu_tac_before_leading_separator_no_trailing_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-b"], stdin: b"\na\nb")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"\nb\na")
}

# origin: uutils test_tac::test_before_no_separator
test test_uu_tac_before_no_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-b"], stdin: b"ab")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"ab")
}

# origin: uutils test_tac::test_before_trailing_separator_and_leading_separator
test test_uu_tac_before_trailing_separator_and_leading_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-b"], stdin: b"\na\nb\n")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"\n\nb\na")
}

# origin: uutils test_tac::test_before_trailing_separator_no_leading_separator
test test_uu_tac_before_trailing_separator_no_leading_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-b"], stdin: b"a\nb\n")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"\n\nba")
}

# origin: uutils test_tac::test_escaped_middle_anchor
test test_uu_tac_escaped_middle_anchor { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-r", "-s", "c\\^b"], stdin: b"aaabc^bcdddd")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"cddddaaabc^b")
  let r1 = uu.invoke(s, "tac", ["-r", "-s", "c\\$b"], stdin: b"aaabc$bcdddd")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"cddddaaabc$b")
}

# origin: uutils test_tac::test_invalid_arg
test test_uu_tac_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["--definitely-invalid"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_tac::test_invalid_input
test test_uu_tac_invalid_input { |ctx|
  let s = uu.scene(ctx)?
  let missing = uu.invoke(s, "tac", ["b"])?
  uu.fails(missing)
  uu.stderr_contains(missing, "failed to open 'b' for reading: No such file or directory")
  uu.mkdir(s, "a")?
  let directory = uu.invoke(s, "tac", ["a"])?
  uu.fails(directory)
  uu.stderr_contains(directory, "a: read error: Is a directory")
}

# origin: uutils test_tac::test_multi_char_separator
test test_uu_tac_multi_char_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-s", "xx"], stdin: b"axxbxx")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"bxxaxx")
}

# origin: uutils test_tac::test_multi_char_separator_overlap
test test_uu_tac_multi_char_separator_overlap { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-s", "xx"], stdin: b"axxx")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"axxx")
  let r1 = uu.invoke(s, "tac", ["-s", "xx"], stdin: b"axxxx")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"xxaxx")
}

# origin: uutils test_tac::test_multi_char_separator_overlap_before
test test_uu_tac_multi_char_separator_overlap_before { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-b", "-s", "xx"], stdin: b"axxx")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"xxax")
  let r1 = uu.invoke(s, "tac", ["-b", "-s", "xx"], stdin: b"axxxx")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"xxxxa")
}

# origin: uutils test_tac::test_no_line_separators
test test_uu_tac_no_line_separators { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", [], stdin: b"a")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"a")
}

# origin: uutils test_tac::test_non_utf8_separator
test test_uu_tac_non_utf8_separator { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "tac", [p"-s", Path.parse_bytes(b"\xe9")?], stdin: b"1\xe92")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, b"21\xe9")
}

# origin: uutils test_tac::test_null_separator
test test_uu_tac_null_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-s", ""], stdin: b"a\0b\0")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"b\0a\0")
}

# origin: uutils test_tac::test_regex
test test_uu_tac_regex { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-r", "-s", "[xyz]+"], stdin: b"axyz")?
  uu.succeeds(r0)
  uu.no_stderr(r0)
  uu.stdout_is_bytes(r0, b"zyax")
  let r1 = uu.invoke(s, "tac", ["-r", "-s", ":+"], stdin: b"a:b::c:::d::::")?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, b":::d:::c::b:a:")
  let r2 = uu.invoke(s, "tac", ["-r", "-s", "[\\+]+[-]+[\\+]+"], stdin: b"a+-+b++--++c+d-e+---+")?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  uu.stdout_is_bytes(r2, b"c+d-e+---+b++--++a+-+")
}

# origin: uutils test_tac::test_regex_bare_anchors
test test_uu_tac_regex_bare_anchors { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-r", "-s", "^"], stdin: b"a\nb\nc\n")?
  uu.succeeds(r0)
  uu.no_stderr(r0)
  uu.stdout_is_bytes(r0, b"c\nb\na\n")
  let r1 = uu.invoke(s, "tac", ["-r", "-s", "$"], stdin: b"a\nb\nc\n")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"\n\nc\nba")
  let r2 = uu.invoke(s, "tac", ["-r", "-s", "^$"], stdin: b"a\nb\nc\n")?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, b"a\nb\nc\n")
}

# origin: uutils test_tac::test_regex_before
test test_uu_tac_regex_before { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-b", "-r", "-s", "[xyz]+"], stdin: b"axyz")?
  uu.succeeds(r0)
  uu.no_stderr(r0)
  uu.stdout_is_bytes(r0, b"zyxa")
  let r1 = uu.invoke(s, "tac", ["-b", "-r", "-s", ":+"], stdin: b":a::b:::c::::d")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b":d::::c:::b::a")
  let r2 = uu.invoke(s, "tac", ["-b", "-r", "-s", "[\\+]+[-]+[\\+]+"], stdin: b"+-+a++--++b+---+c+d-e")?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  uu.stdout_is_bytes(r2, b"+---+c+d-e+--++b+-+a+")
}

# origin: uutils test_tac::test_regex_or_operator
test test_uu_tac_regex_or_operator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-r", "-s", "[^x]\\|x"], stdin: b"abc")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"cba")
}

# origin: uutils test_tac::test_regular_end_anchor
test test_uu_tac_regular_end_anchor { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-r", "-s", "abc$"], stdin: b"123abcxyzabc")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"123abcxyzabc")
  let r1 = uu.invoke(s, "tac", ["-r", "-s", "b$"], stdin: b"aaa\nbbb\nccc\n")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"\nccc\nbbaaa\nb")
}

# origin: uutils test_tac::test_regular_start_anchor
test test_uu_tac_regular_start_anchor { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-r", "-s", "^abc"], stdin: b"xyzabc123abc")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"xyzabc123abc")
  let r1 = uu.invoke(s, "tac", ["-r", "-s", "^b"], stdin: b"aaa\nbbb\nccc\n")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"bb\nccc\naaa\nb")
}

# origin: uutils test_tac::test_single_default
test test_uu_tac_single_default { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "tac", "prime_per_line.txt", "prime_per_line.txt")?
  let r0 = uu.invoke(s, "tac", ["prime_per_line.txt"])?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, fp"{ctx.core_dir}/tests/data/uutils/tac/prime_per_line.expected".read_bytes()?)
}

# origin: uutils test_tac::test_single_non_newline_separator
test test_uu_tac_single_non_newline_separator { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "tac", "delimited_primes.txt", "delimited_primes.txt")?
  let r0 = uu.invoke(s, "tac", ["-s", ":", "delimited_primes.txt"])?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, fp"{ctx.core_dir}/tests/data/uutils/tac/delimited_primes.expected".read_bytes()?)
}

# origin: uutils test_tac::test_single_non_newline_separator_before
test test_uu_tac_single_non_newline_separator_before { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "tac", "delimited_primes.txt", "delimited_primes.txt")?
  let r0 = uu.invoke(s, "tac", ["-b", "-s", ":", "delimited_primes.txt"])?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, fp"{ctx.core_dir}/tests/data/uutils/tac/delimited_primes_before.expected".read_bytes()?)
}

# origin: uutils test_tac::test_stdin_bad_tmpdir_fallback
test test_uu_tac_stdin_bad_tmpdir_fallback { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-"], stdin: b"a\nb\nc\n", vars: {TMPDIR: "/nonexistent/dir"})?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"c\nb\na\n")
}

# origin: uutils test_tac::test_stdin_default
test test_uu_tac_stdin_default { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", [], stdin: b"100\n200\n300\n400\n500")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"500400\n300\n200\n100\n")
}

# origin: uutils test_tac::test_stdin_non_newline_separator
test test_uu_tac_stdin_non_newline_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-s", ":"], stdin: b"100:200:300:400:500")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"500400:300:200:100:")
}

# origin: uutils test_tac::test_stdin_non_newline_separator_before
test test_uu_tac_stdin_non_newline_separator_before { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-b", "-s", ":"], stdin: b"100:200:300:400:500")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b":500:400:300:200100")
}

# origin: uutils test_tac::test_tac_file_truncated_during_read_does_not_crash
test test_uu_tac_tac_file_truncated_during_read_does_not_crash { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "input")?
  uu.truncate(s, "input", 64 * 1024 * 1024)?
  let plan = uu.command(s, "tac", ["input"], stdout: uu.at(s, "out"), stderr: uu.at(s, "err"), timeout: 10s)?
  let child = spawn plan?
  time.sleep(2ms)?
  uu.truncate(s, "input", 0)?
  let status = wait child?
  assert ! status.signaled(), "tac was killed by a signal"
}

# origin: uutils test_tac::test_tac_non_utf8_paths
test test_uu_tac_tac_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  uu.at_bytes(s, b"\xff\xfe")?.write(b"line1\nline2\nline3\n")?
  let r = uu.invoke_paths(s, "tac", [Path.parse_bytes(b"\xff\xfe")?])?
  uu.succeeds(r)
  uu.stdout_is(r, "line3\nline2\nline1\n")
}

# origin: uutils test_tac::test_tac_stdin_redirected_file_truncated_during_read_does_not_crash
test test_uu_tac_tac_stdin_redirected_file_truncated_during_read_does_not_crash { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "input")?
  uu.truncate(s, "input", 64 * 1024 * 1024)?
  let argv = uu.argv(s, "tac", [])?
  let plan = process.command_argv(ctx.xsh_bin, argv, s.root, {}, uu.at(s, "input"), uu.at(s, "out"), uu.at(s, "err"), timeout: 10s)
  let child = spawn plan?
  time.sleep(2ms)?
  uu.truncate(s, "input", 0)?
  let status = wait child?
  assert ! status.signaled(), "tac was killed by a signal"
}

# origin: uutils test_tac::test_unescaped_middle_anchor
test test_uu_tac_unescaped_middle_anchor { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "tac", ["-r", "-s", "1^2"], stdin: b"111^222")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, b"22111^2")
  let r1 = uu.invoke(s, "tac", ["-r", "-s", "a$b"], stdin: b"aaa$bbb")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"bbaaa$b")
}
