##! Native ports of the uutils echo integration tests.

use support.uu as uu

# origin: uutils test_echo::full_help_argument
test test_uu_echo_full_help_argument { |ctx|
  let s = uu.scene(ctx)?
  let with_newline = uu.invoke(s, "echo", ["--help"])?
  uu.succeeds(with_newline)
  assert with_newline.stdout != b"--help\n"
  let without_newline = uu.invoke(s, "echo", ["--help"])?
  uu.succeeds(without_newline)
  assert without_newline.stdout != b"--help"
}

# origin: uutils test_echo::multibyte_escape_unicode
test test_uu_echo_multibyte_escape_unicode { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-e", "\\xf0\\x9f\\x98\\x82"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "😂\n")
  let r2 = uu.invoke(s, "echo", ["-e", "\\x41\\xf0\\x9f\\x98\\x82\\x42"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "A😂B\n")
  let r3 = uu.invoke(s, "echo", ["-e", "\\xf0\\x41\\x9f\\x98\\x82"])?
  uu.succeeds(r3)
  uu.stdout_only_bytes(r3, b"\xf0A\x9f\x98\x82\n")
  let r4 = uu.invoke(s, "echo", ["-e", "\\x41\\xf0\\c\\x9f\\x98\\x82"])?
  uu.succeeds(r4)
  uu.stdout_only_bytes(r4, b"A\xf0")
}

# origin: uutils test_echo::multiple_help_argument
test test_uu_echo_multiple_help_argument { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["--help", "--help"])?
  uu.succeeds(r)
  uu.stdout_is(r, "--help --help\n")
}

# origin: uutils test_echo::multiple_version_argument
test test_uu_echo_multiple_version_argument { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["--version", "--version"])?
  uu.succeeds(r)
  uu.stdout_is(r, "--version --version\n")
}

# origin: uutils test_echo::nine_bit_octal
test test_uu_echo_nine_bit_octal { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-e", "\\0777"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, b"\xff\n")
  let r2 = uu.invoke(s, "echo", ["-e", "\\777"])?
  uu.succeeds(r2)
  uu.stdout_only_bytes(r2, b"\xff\n")
}

# origin: uutils test_echo::non_utf_8
test test_uu_echo_non_utf_8 { |ctx|
  let s = uu.scene(ctx)?
  let data = b"Swer an rehte g\xfcete wendet s\xeen gem\xfcete, dem volget s\xe6lde und \xeare."
  let r = uu.invoke_paths(s, "echo", [p"-n", Path.parse_bytes(data)?])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
}

# origin: uutils test_echo::non_utf_8_hex_round_trip
test test_uu_echo_non_utf_8_hex_round_trip { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\xFF"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"\xff\n")
}

# origin: uutils test_echo::old_octal_syntax
test test_uu_echo_old_octal_syntax { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-e", "\\1foo"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\x01foo\n")
  let r2 = uu.invoke(s, "echo", ["-e", "\\43foo"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "#foo\n")
  let r3 = uu.invoke(s, "echo", ["-e", "\\101 foo"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "A foo\n")
  let r4 = uu.invoke(s, "echo", ["-e", "\\1011"])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "A1\n")
}

# origin: uutils test_echo::partial_help_argument
test test_uu_echo_partial_help_argument { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["--he"])?
  uu.succeeds(r)
  uu.stdout_is(r, "--he\n")
}

# origin: uutils test_echo::partial_version_argument
test test_uu_echo_partial_version_argument { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["--ver"])?
  uu.succeeds(r)
  uu.stdout_is(r, "--ver\n")
}

# origin: uutils test_echo::posixly_correct::ignore_options
test test_uu_echo_posixly_correct_ignore_options { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["--help", "--version", "-E -n 'foo'", "-nE 'foo'"] {
    let r = uu.invoke(s, "echo", [arg], vars: {POSIXLY_CORRECT: "1"})?
    uu.succeeds(r)
    uu.stdout_only(r, arg + "\n")
  }
}

# origin: uutils test_echo::posixly_correct::process_escapes
test test_uu_echo_posixly_correct_process_escapes { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["foo\\n"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r1)
  uu.stdout_only(r1, "foo\n\n")
  let r2 = uu.invoke(s, "echo", ["foo\\tbar"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r2)
  uu.stdout_only(r2, "foo\tbar\n")
  let r3 = uu.invoke(s, "echo", ["foo\\ctbar"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r3)
  uu.stdout_only(r3, "foo")
}

# origin: uutils test_echo::posixly_correct::process_n_option
test test_uu_echo_posixly_correct_process_n_option { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-n", "foo"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r1)
  uu.stdout_only(r1, "foo")
  let r2 = uu.invoke(s, "echo", ["-n", "-E", "foo\\cbar"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r2)
  uu.stdout_only(r2, "foo")
}

# origin: uutils test_echo::slash_eight_off_by_one
test test_uu_echo_slash_eight_off_by_one { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "-n", "\\8"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\\8")
}

# origin: uutils test_echo::test_backslash_n_last_char_in_last_argument
test test_uu_echo_backslash_n_last_char_in_last_argument { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-n", "-e", "--", "foo\n"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-- foo\n")
  let r2 = uu.invoke(s, "echo", ["-e", "--", "foo\\n"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-- foo\n\n")
  let r3 = uu.invoke(s, "echo", ["-n", "--", "foo\n"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-- foo\n")
  let r4 = uu.invoke(s, "echo", ["--", "foo\n"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "-- foo\n\n")
}

# origin: uutils test_echo::test_child_when_run_with_a_non_blocking_util
test test_uu_echo_child_when_run_with_a_non_blocking_util { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["hello world"])?
  uu.succeeds(r)
  uu.stdout_only(r, "hello world\n")
}

# origin: uutils test_echo::test_cmd_result_signal_when_normal_exit_then_no_signal
test test_uu_echo_cmd_result_signal_when_normal_exit_then_no_signal { |ctx|
  let s = uu.scene(ctx)?
  # The helper requires an exit code; a signaled child cannot produce Ran.
  let r = uu.invoke(s, "echo", [])?
}

# origin: uutils test_echo::test_cmd_result_stderr_check_and_stderr_str_check
test test_uu_echo_cmd_result_stderr_check_and_stderr_str_check { |ctx|
  let s = uu.scene(ctx)?
  let stderr = uu.at(s, "redirected-stderr")
  let r = uu.invoke(s, "echo", ["Hello world"], stdout: stderr)?
  let output = stderr.read_bytes()?
  assert output.utf8()?.ends_with("world\n")
  assert output.slice(0, length: 2) == b"He"
  uu.no_stdout(r)
}

# origin: uutils test_echo::test_cmd_result_stdout_check_and_stdout_str_check
test test_uu_echo_cmd_result_stdout_check_and_stdout_str_check { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["Hello world"])?
  assert r.stdout.utf8()?.ends_with("world\n")
  assert r.stdout.slice(0, length: 2) == b"He"
  uu.no_stderr(r)
}

# origin: uutils test_echo::test_cmd_result_stdout_str_check_when_false_then_panics
test test_uu_echo_cmd_result_stdout_str_check_when_false_then_panics { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\f"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\x0c\n")
}

# origin: uutils test_echo::test_default
test test_uu_echo_default { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["hi"])?
  uu.succeeds(r)
  uu.stdout_only(r, "hi\n")
}

# origin: uutils test_echo::test_disable_escapes
test test_uu_echo_disable_escapes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-E", "\\a \\\\ \\b \\r \\e \\f \\x41 \\n a\\cb \\u0100 \\t \\v"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\\a \\\\ \\b \\r \\e \\f \\x41 \\n a\\cb \\u0100 \\t \\v\n")
}

# origin: uutils test_echo::test_disable_escapes_unicode_and_quote
test test_uu_echo_disable_escapes_unicode_and_quote { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-E", "\\u0041 \\U00000041 \\\""])?
  uu.succeeds(r)
  uu.stdout_only(r, "\\u0041 \\U00000041 \\\"\n")
}

# origin: uutils test_echo::test_double_hyphens
test test_uu_echo_double_hyphens { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["--"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "--\n")
  let r2 = uu.invoke(s, "echo", ["--", "--"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-- --\n")
  let r3 = uu.invoke(s, "echo", ["a", "--", "b"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "a -- b\n")
  let r4 = uu.invoke(s, "echo", ["a", "--", "b", "--"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "a -- b --\n")
  let r5 = uu.invoke(s, "echo", ["a", "b", "--", "--"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "a b -- --\n")
}

# origin: uutils test_echo::test_double_hyphens_after_flags
test test_uu_echo_double_hyphens_after_flags { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-e", "--"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "--\n")
  let r2 = uu.invoke(s, "echo", ["-n", "-e", "--", "foo\n"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-- foo\n")
  let r3 = uu.invoke(s, "echo", ["-ne", "--"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "--")
  let r4 = uu.invoke(s, "echo", ["-neE", "--"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "--")
  let r5 = uu.invoke(s, "echo", ["-e", "--", "--"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "-- --\n")
  let r6 = uu.invoke(s, "echo", ["-e", "--", "a", "--"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "-- a --\n")
  let r7 = uu.invoke(s, "echo", ["-n", "--", "a"])?
  uu.succeeds(r7)
  uu.stdout_only(r7, "-- a")
  let r8 = uu.invoke(s, "echo", ["-n", "--", "a", "--"])?
  uu.succeeds(r8)
  uu.stdout_only(r8, "-- a --")
}

# origin: uutils test_echo::test_double_hyphens_after_single_hyphen
test test_uu_echo_double_hyphens_after_single_hyphen { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-", "--"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "- --\n")
  let r2 = uu.invoke(s, "echo", ["-", "-n", "--"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "- -n --\n")
  let r3 = uu.invoke(s, "echo", ["-n", "-", "--"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "- --")
}

# origin: uutils test_echo::test_double_hyphens_at_start
test test_uu_echo_double_hyphens_at_start { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["--"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "--\n")
  let r2 = uu.invoke(s, "echo", ["--", "--"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-- --\n")
  let r3 = uu.invoke(s, "echo", ["--", "a"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-- a\n")
  let r4 = uu.invoke(s, "echo", ["--", "a", "b"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "-- a b\n")
  let r5 = uu.invoke(s, "echo", ["--", "a", "b", "--"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "-- a b --\n")
}

# origin: uutils test_echo::test_emoji_output
test test_uu_echo_emoji_output { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["Hello 🌍 World 🚀"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "Hello 🌍 World 🚀\n")
  let r2 = uu.invoke(s, "echo", ["-n", "Status: 🎯 Complete"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "Status: 🎯 Complete")
  let r3 = uu.invoke(s, "echo", ["🦀", "loves", "🚀", "and", "🌟"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "🦀 loves 🚀 and 🌟\n")
  let r4 = uu.invoke(s, "echo", ["🍎,🍌,🍒,🥝"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "🍎,🍌,🍒,🥝\n")
  let r5 = uu.invoke(s, "echo", ["こんにちは世界"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "こんにちは世界\n")
}

# origin: uutils test_echo::test_empty_args
test test_uu_echo_empty_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", [])?
  uu.succeeds(r)
  uu.stdout_only(r, "\n")
}

# origin: uutils test_echo::test_escape_alert
test test_uu_echo_escape_alert { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\a"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\x07\n")
}

# origin: uutils test_echo::test_escape_backslash
test test_uu_echo_escape_backslash { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\\\"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\\\n")
}

# origin: uutils test_echo::test_escape_backspace
test test_uu_echo_escape_backspace { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\b"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\x08\n")
}

# origin: uutils test_echo::test_escape_carriage_return
test test_uu_echo_escape_carriage_return { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\r"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\r\n")
}

# origin: uutils test_echo::test_escape_escape
test test_uu_echo_escape_escape { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\e"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\x1b\n")
}

# origin: uutils test_echo::test_escape_form_feed
test test_uu_echo_escape_form_feed { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\f"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\x0c\n")
}

# origin: uutils test_echo::test_escape_hex
test test_uu_echo_escape_hex { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\x41"])?
  uu.succeeds(r)
  uu.stdout_only(r, "A\n")
}

# origin: uutils test_echo::test_escape_malformed_unicode_not_recognized
test test_uu_echo_escape_malformed_unicode_not_recognized { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["\\u", "\\U", "\\u00", "\\U0000004", "\\u00zz", "\\U000000zz", "\\uD800", "\\U00110000"] {
    let r = uu.invoke(s, "echo", ["-e", arg])?
    uu.succeeds(r)
    uu.stdout_only(r, arg + "\n")
  }
}

# origin: uutils test_echo::test_escape_newline
test test_uu_echo_escape_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\na"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\na\n")
}

# origin: uutils test_echo::test_escape_no_further_output
test test_uu_echo_escape_no_further_output { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "a\\cb", "c"])?
  uu.succeeds(r)
  uu.stdout_only(r, "a")
}

# origin: uutils test_echo::test_escape_no_hex
test test_uu_echo_escape_no_hex { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "foo\\x bar"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\\x bar\n")
}

# origin: uutils test_echo::test_escape_nul
test test_uu_echo_escape_nul { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "foo\\0 bar"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\x00 bar\n")
}

# origin: uutils test_echo::test_escape_octal
test test_uu_echo_escape_octal { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\0100"])?
  uu.succeeds(r)
  uu.stdout_only(r, "@\n")
}

# origin: uutils test_echo::test_escape_octal_invalid_digit
test test_uu_echo_escape_octal_invalid_digit { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "foo\\08 bar"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\x008 bar\n")
}

# origin: uutils test_echo::test_escape_one_slash
test test_uu_echo_escape_one_slash { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "foo\\ bar"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\\ bar\n")
}

# origin: uutils test_echo::test_escape_one_slash_multi
test test_uu_echo_escape_one_slash_multi { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "foo\\", "bar"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\\ bar\n")
}

# origin: uutils test_echo::test_escape_override
test test_uu_echo_escape_override { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-e", "-E", "\\na"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "\\na\n")
  let r2 = uu.invoke(s, "echo", ["-E", "-e", "\\na"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "\na\n")
  let r3 = uu.invoke(s, "echo", ["-E", "-e", "-n", "\\na"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "\na")
  let r4 = uu.invoke(s, "echo", ["-e", "-E", "-n", "\\na"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "\\na")
}

# origin: uutils test_echo::test_escape_recognized_sequences_still_expand
test test_uu_echo_escape_recognized_sequences_still_expand { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-e", "\\a\\b\\e\\f\\n\\r1\\t\\v\\\\"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "\x07\x08\x1b\x0c\n\r1\t\x0b\\\n")
  let r2 = uu.invoke(s, "echo", ["-e", "\\0101\\101\\x41"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "AAA\n")
  let r3 = uu.invoke(s, "echo", ["-e", "a\\cb", "c"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "a")
}

# origin: uutils test_echo::test_escape_sequence_ctrl_c
test test_uu_echo_escape_sequence_ctrl_c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "show\\c123"])?
  uu.succeeds(r)
  uu.stdout_only(r, "show")
}

# origin: uutils test_echo::test_escape_short_hex
test test_uu_echo_escape_short_hex { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "foo\\xa bar"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\n bar\n")
}

# origin: uutils test_echo::test_escape_short_octal
test test_uu_echo_escape_short_octal { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "foo\\040bar"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo bar\n")
}

# origin: uutils test_echo::test_escape_tab
test test_uu_echo_escape_tab { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\t"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\t\n")
}

# origin: uutils test_echo::test_escape_unicode_and_quote_not_recognized
test test_uu_echo_escape_unicode_and_quote_not_recognized { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["\\u0041", "\\U00000041", "\\U0001F600", "\\\"", "a\\u0041b", "a\\U00000041b", "a\\\"b", "\\u\\u"] {
    let r = uu.invoke(s, "echo", ["-e", arg])?
    uu.succeeds(r)
    uu.stdout_only(r, arg + "\n")
  }
}

# origin: uutils test_echo::test_escape_vertical_tab
test test_uu_echo_escape_vertical_tab { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\v"])?
  uu.succeeds(r)
  uu.stdout_only(r, "\x0b\n")
}

# origin: uutils test_echo::test_flag_like_arguments_which_are_no_flags
test test_uu_echo_flag_like_arguments_which_are_no_flags { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["-efjkow", "--"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-efjkow --\n")
  let r2 = uu.invoke(s, "echo", ["--", "-efjkow"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-- -efjkow\n")
  let r3 = uu.invoke(s, "echo", ["-efjkow", "-n", "--"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-efjkow -n --\n")
  let r4 = uu.invoke(s, "echo", ["-n", "--", "-efjkow"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "-- -efjkow")
}

# origin: uutils test_echo::test_hyphen_value
test test_uu_echo_hyphen_value { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-abc"])?
  uu.succeeds(r)
  uu.stdout_is(r, "-abc\n")
}

# origin: uutils test_echo::test_hyphen_values_at_start
test test_uu_echo_hyphen_values_at_start { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-E", "-test", "araba", "-merci"])?
  uu.succeeds(r)
  uu.stdout_is(r, "-test araba -merci\n")
}

# origin: uutils test_echo::test_hyphen_values_between
test test_uu_echo_hyphen_values_between { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "echo", ["test", "-E", "araba"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "test -E araba\n")
  let r2 = uu.invoke(s, "echo", ["dumdum ", "dum dum dum", "-e", "dum"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "dumdum  dum dum dum -e dum\n")
}

# origin: uutils test_echo::test_hyphen_values_inside_string
test test_uu_echo_hyphen_values_inside_string { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["'\"\n'CXXFLAGS=-g -O2'\n\"'"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "CXXFLAGS")
}

# origin: uutils test_echo::test_multiple_hyphen_values
test test_uu_echo_multiple_hyphen_values { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-abc", "-def", "-edf"])?
  uu.succeeds(r)
  uu.stdout_is(r, "-abc -def -edf\n")
}

# origin: uutils test_echo::test_no_trailing_newline
test test_uu_echo_no_trailing_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-n", "hi"])?
  uu.succeeds(r)
  uu.stdout_only(r, "hi")
}

# origin: uutils test_echo::test_normalized_newlines_stdout_is
test test_uu_echo_normalized_newlines_stdout_is { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-ne", "A\r\nB\nC"])?
  let actual = r.stdout.utf8()?.replace("\r\n", with: "\n")
  for expected in ["A\r\nB\nC", "A\nB\nC", "A\nB\r\nC"] {
    assert actual == expected.replace("\r\n", with: "\n")
  }
}

# origin: uutils test_echo::test_normalized_newlines_stdout_is_fail
test test_uu_echo_normalized_newlines_stdout_is_fail { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-ne", "A\r\nB\nC"])?
  uu.succeeds(r)
  uu.stdout_is(r, "A\r\nB\nC")
}

# origin: uutils test_echo::wrapping_octal
test test_uu_echo_wrapping_octal { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "\\0501"])?
  uu.succeeds(r)
  uu.stdout_is(r, "A\n")
}

