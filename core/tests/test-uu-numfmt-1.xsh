##! Transcribed numeric formatting tests from the uutils coreutils suite.

use support.uu as uu

# Capture a real stderr pipe, including its EOF boundary, through an owned reader.
proc pipe_stderr(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  uu.mkfifo(s, "stderr-pipe")?
  let captured = uu.at(s, "pipe-captured")
  let reader_plan = uu.command(s, "cat", ["stderr-pipe"], stdout: captured, stderr: uu.at(s, "reader-error"))?
  let reader = spawn reader_plan?
  let r = uu.invoke(s, "numfmt", args, stderr: uu.at(s, "stderr-pipe"))?
  let status = wait reader?
  assert status.exited_with(0)
  assert uu.at(s, "reader-error").read_bytes()? == b""
  Ok({util: r.util, args: r.args, status: r.status, stdout: r.stdout, stderr: captured.read_bytes()?})
}

# The plain diagnostic fits the terminal queue; close the last replica after
# the child exits so Linux's master EIO marks the end of captured output.
proc tty_stderr(s: uu.Scene, args: List[Str], stdin: Bytes = b"") [fs, process, env, error] -> Result[uu.Ran, Error] {
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  var replica_open = true
  defer { if replica_open { unix.close_fd(pty.replica)? } }
  let r = uu.invoke(s, "numfmt", args, stdin, stderr: Path(pty.name))?
  unix.close_fd(pty.replica)?
  replica_open = false
  var captured = b""
  loop {
    match unix.read_fd(pty.master, 1024) {
      Ok(data) => { if data.is_empty() { break }; captured = bytes.concat([captured, data]) },
      Err(failure) => { if failure.errno == 5 { break }; return Err(failure) },
    }
  }
  Ok({util: r.util, args: r.args, status: r.status, stdout: r.stdout, stderr: captured})
}

pure trim_end(text: Str) -> Str {
  var end = text.byte_len()
  while end > 0 and text.byte_slice(end - 1, length: 1) in [" ", "\t", "\r", "\n", "\u{b}", "\u{c}"] { end -= 1 }
  text.byte_slice(0, length: end)
}

# origin: uutils test_numfmt::test_abort_with_fields_preserves_partial_output
test test_uu_numfmt_abort_with_fields_preserves_partial_output { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--field=3", "--from=auto", "Hello 40M World 90G"])?
  uu.fails_with_code(r0, 2)
  uu.stdout_is(r0, "Hello 40M ")
  uu.stderr_is(r0, "numfmt: invalid number: 'World'\n")
}

# origin: uutils test_numfmt::test_debug_reports_failed_conversions_summary
test test_uu_numfmt_debug_reports_failed_conversions_summary { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--invalid=fail", "--debug", "--to=si", "1000", "Foo", "3000"])?
  uu.fails_with_code(r0, 2)
  uu.stdout_is(r0, "1.0k\nFoo\n3.0k\n")
  uu.stderr_is(r0, "numfmt: invalid number: 'Foo'\nnumfmt: failed to convert some of the input numbers\n")
}

# origin: uutils test_numfmt::test_debug_warnings
test test_uu_numfmt_debug_warnings { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--debug", "4096"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "4096\n")
  uu.stderr_is(r0, "numfmt: no conversion option specified\n")
  let r1 = uu.invoke(s, "numfmt", ["--debug", "--padding=10", "4096"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "      4096\n")
  let r2 = uu.invoke(s, "numfmt", ["--debug", "--header", "--to=iec", "4096"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "4.0K\n")
  uu.stderr_is(r2, "numfmt: --header ignored with command-line input\n")
  let r3 = uu.invoke(s, "numfmt", ["--debug", "--grouping", "--from=si", "4.0K"], vars: {"LC_ALL": "C"})?
  uu.succeeds(r3)
  uu.stdout_is(r3, "4000\n")
  uu.stderr_is(r3, "numfmt: grouping has no effect in this locale\n")
}

# origin: uutils test_numfmt::test_delimiter_hyphen_leading_as_separate_arg
test test_uu_numfmt_delimiter_hyphen_leading_as_separate_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["-d", "-x", "--field=1"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "the delimiter must be a single character")
}

# origin: uutils test_numfmt::test_delimiter_must_not_be_empty
test test_uu_numfmt_delimiter_must_not_be_empty { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["-d"])?
  uu.fails(r0)
}

# origin: uutils test_numfmt::test_delimiter_must_not_be_more_than_one_character
test test_uu_numfmt_delimiter_must_not_be_more_than_one_character { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--delimiter", "sad"])?
  uu.fails(r0)
  uu.stderr_is(r0, "numfmt: the delimiter must be a single character\n")
}

# origin: uutils test_numfmt::test_delimiter_only
test test_uu_numfmt_delimiter_only { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["-d", ","], stdin: bytes.from_text("1234,56"))?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1234,56")
}

# origin: uutils test_numfmt::test_delimiter_overrides_whitespace_separator
test test_uu_numfmt_delimiter_overrides_whitespace_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["-d,"], stdin: bytes.from_text("1 234,56"))?
  uu.fails(r0)
  uu.stderr_is(r0, "numfmt: invalid suffix in input: '1 234'\n")
}

# origin: uutils test_numfmt::test_empty_delimiter_multi_char_unit_separator
test test_uu_numfmt_empty_delimiter_multi_char_unit_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["-d", "", "--from=si", "--unit-separator=  "], stdin: bytes.from_text("1  K\n2  M\n3  G\n"))?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1000\n2000000\n3000000000\n")
}

# origin: uutils test_numfmt::test_empty_delimiter_whitespace_rejection
test test_uu_numfmt_empty_delimiter_whitespace_rejection { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["-d", "", "--from=auto", "2  K"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_contains(r0, "invalid suffix in input")
  let r1 = uu.invoke(s, "numfmt", ["-d", "", "--from=si", "--unit-separator=", "1 K"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_contains(r1, "invalid suffix in input")
}

# origin: uutils test_numfmt::test_field_df_example
test test_uu_numfmt_field_df_example { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "numfmt", "df_input.txt", "df_input.txt")?
  uu.fixture(s, "numfmt", "df_expected.txt", "df_expected.txt")?
  let r0 = uu.invoke(s, "numfmt", ["--header", "--field", "2-4", "--to=si"], stdin: uu.read(s, "df_input.txt")?)?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, uu.read(s, "df_expected.txt")?)
}

# origin: uutils test_numfmt::test_field_with_multibyte_whitespace_separator
test test_uu_numfmt_field_with_multibyte_whitespace_separator { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--field", "2", "1　2"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1　2\n")
  let r1 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "2"], stdin: bytes.from_text("1K　2K\n"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1K　2K\n")
}

# origin: uutils test_numfmt::test_float_precision_greater_than_16bits
test test_uu_numfmt_float_precision_greater_than_16bits { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--to=iec", "--format=%.65536f", "1"])?
  uu.succeeds(r0)
  let zeros = ["0" for index in range(65536)].join("")
  uu.stdout_is(r0, f"1.{zeros}\n")
}

# origin: uutils test_numfmt::test_float_precision_greater_than_16bits_with_suffix
test test_uu_numfmt_float_precision_greater_than_16bits_with_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--to=si", "--format=%.65536f", "1000000"])?
  uu.succeeds(r0)
  let zeros = ["0" for index in range(65536)].join("")
  uu.stdout_is(r0, f"1.{zeros}M\n")
}

# origin: uutils test_numfmt::test_float_precision_greater_than_16bits_without_scaling
test test_uu_numfmt_float_precision_greater_than_16bits_without_scaling { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%.65536f", "1.5"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_only(r0, "numfmt: value/precision too large to be printed: '1.5/65536' (consider using --to)\n")
}

# origin: uutils test_numfmt::test_format
test test_uu_numfmt_format { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=--%f--", "50"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "--50--\n")
}

# origin: uutils test_numfmt::test_format_grouping_conflicts_with_to_option
test test_uu_numfmt_format_grouping_conflicts_with_to_option { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%'f", "--to=si"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "grouping cannot be combined with --to")
}

# origin: uutils test_numfmt::test_format_implied_range_and_field
test test_uu_numfmt_format_implied_range_and_field { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "-2,4", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1000 2000 3K 4000 5K 6K\n")
}

# origin: uutils test_numfmt::test_format_negative_padding_with_prefix_and_suffix
test test_uu_numfmt_format_negative_padding_with_prefix_and_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=--%-6f--", "50"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "--50    --\n")
}

# origin: uutils test_numfmt::test_format_padding_with_prefix_and_suffix
test test_uu_numfmt_format_padding_with_prefix_and_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=--%6f--", "50"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "--    50--\n")
}

# origin: uutils test_numfmt::test_format_precision_too_large_on_zero
test test_uu_numfmt_format_precision_too_large_on_zero { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%.40f", "0"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_only(r0, "numfmt: value/precision too large to be printed: '0/40' (consider using --to)\n")
  let r1 = uu.invoke(s, "numfmt", ["--format=%.18f", "0"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.000000000000000000\n")
}

# origin: uutils test_numfmt::test_format_precision_zero_with_to_scale_issue_11667
test test_uu_numfmt_format_precision_zero_with_to_scale_issue_11667 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--to=iec", "--format=%.0f", "5183776"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "5M\n")
}

# origin: uutils test_numfmt::test_format_selected_field
test test_uu_numfmt_format_selected_field { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "3", "1K 2K 3K"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1K 2K 3000\n")
  let r1 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "2", "1K 2K 3K"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1K 2000 3K\n")
}

# origin: uutils test_numfmt::test_format_selected_field_range
test test_uu_numfmt_format_selected_field_range { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "2-5", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1K 2000 3000 4000 5000 6K\n")
}

# origin: uutils test_numfmt::test_format_selected_fields
test test_uu_numfmt_format_selected_fields { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "1,4,3", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1000 2K 3000 4000 5K 6K\n")
  let r1 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "1,4 3", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1000 2K 3000 4000 5K 6K\n")
}

# origin: uutils test_numfmt::test_format_value_below_large_threshold_ok
test test_uu_numfmt_format_value_below_large_threshold_ok { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%5.1f", "999999999999999999"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "999999999999999999.0\n")
}

# origin: uutils test_numfmt::test_format_with_format_padding_overriding_implicit_padding
test test_uu_numfmt_format_with_format_padding_overriding_implicit_padding { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%6f", "      1234"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "  1234\n")
}

# origin: uutils test_numfmt::test_format_with_format_padding_overriding_padding_option
test test_uu_numfmt_format_with_format_padding_overriding_padding_option { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%6f", "--padding=10", "1234"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "  1234\n")
}

# origin: uutils test_numfmt::test_format_with_negative_format_padding_and_suffix
test test_uu_numfmt_format_with_negative_format_padding_and_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%-6f", "1234 ?"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1234   ?\n")
}

# origin: uutils test_numfmt::test_format_with_precision_and_unitless_to_arg
test test_uu_numfmt_format_with_precision_and_unitless_to_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--to=si", "--format=%.1f", "3.14"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "4.0\n")
  let r1 = uu.invoke(s, "numfmt", ["--to=si", "--format=%.1f", "--round=down", "3.14"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "3.0\n")
}

# origin: uutils test_numfmt::test_format_with_separate_value
test test_uu_numfmt_format_with_separate_value { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format", "--%f--", "50"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "--50--\n")
}

# origin: uutils test_numfmt::test_format_with_zero_padding_and_implicit_padding
test test_uu_numfmt_format_with_zero_padding_and_implicit_padding { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%06f", "    1234"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "  001234\n")
}

# origin: uutils test_numfmt::test_format_with_zero_padding_and_negative_padding_option
test test_uu_numfmt_format_with_zero_padding_and_negative_padding_option { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%06f", "--padding=-8", "1234"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "001234  \n")
}

# origin: uutils test_numfmt::test_format_with_zero_padding_and_padding_option
test test_uu_numfmt_format_with_zero_padding_and_padding_option { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%06f", "--padding=8", "1234"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "  001234\n")
}

# origin: uutils test_numfmt::test_format_with_zero_padding_and_suffix
test test_uu_numfmt_format_with_zero_padding_and_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%06f", "1234 ?"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "001234 ?\n")
}

# origin: uutils test_numfmt::test_from_auto
test test_uu_numfmt_from_auto { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("1K\n1Ki"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1000\n1024")
}

# origin: uutils test_numfmt::test_from_iec
test test_uu_numfmt_from_iec { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=iec"], stdin: bytes.from_text("1024\n1.1M\n0.1G"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1024\n1153434\n107374183")
}

# origin: uutils test_numfmt::test_from_iec_fails_if_i_suffix
test test_uu_numfmt_from_iec_fails_if_i_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=iec", "10Mi"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_is(r0, "numfmt: invalid suffix in input '10Mi': 'i'\n")
}

# origin: uutils test_numfmt::test_from_iec_i
test test_uu_numfmt_from_iec_i { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=iec-i"], stdin: bytes.from_text("1.1Mi\n0.1Gi"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1153434\n107374183")
}

# origin: uutils test_numfmt::test_from_iec_i_requires_suffix
test test_uu_numfmt_from_iec_i_requires_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=iec-i", "10M"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_is(r0, "numfmt: missing 'i' suffix in input: '10M' (e.g Ki/Mi/Gi)\n")
}

# origin: uutils test_numfmt::test_from_iec_i_without_suffix_are_bytes
test test_uu_numfmt_from_iec_i_without_suffix_are_bytes { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=iec-i", "1024"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1024\n")
}

# origin: uutils test_numfmt::test_from_si
test test_uu_numfmt_from_si { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=si"], stdin: bytes.from_text("1000\n1.1M\n0.1G"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1000\n1100000\n100000000")
}

# origin: uutils test_numfmt::test_from_unit
test test_uu_numfmt_from_unit { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from-unit=512", "4"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "2048\n")
}

# origin: uutils test_numfmt::test_from_unit_fractional_precision_issue_11663
test test_uu_numfmt_from_unit_fractional_precision_issue_11663 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=iec", "--from-unit=959", "--", "-615484.454"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "-590249591.386\n")
}

# origin: uutils test_numfmt::test_grouping_conflicts_with_format_option
test test_uu_numfmt_grouping_conflicts_with_format_option { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%f", "--grouping"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "--grouping cannot be combined with --format")
}

# origin: uutils test_numfmt::test_header
test test_uu_numfmt_header { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=si", "--header=2"], stdin: bytes.from_text("header\nheader2\n1K\n1.1M\n0.1G"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "header\nheader2\n1000\n1100000\n100000000")
}

# origin: uutils test_numfmt::test_header_default
test test_uu_numfmt_header_default { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=si", "--header"], stdin: bytes.from_text("header\n1K\n1.1M\n0.1G"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "header\n1000\n1100000\n100000000")
}

# origin: uutils test_numfmt::test_header_detached
test test_uu_numfmt_header_detached { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--header", "1", "2"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1\n2\n")
}

# origin: uutils test_numfmt::test_header_error_if_0
test test_uu_numfmt_header_error_if_0 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--header=0"])?
  uu.fails(r0)
  uu.stderr_is(r0, "numfmt: invalid header value '0'\n")
}

# origin: uutils test_numfmt::test_header_error_if_negative
test test_uu_numfmt_header_error_if_negative { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--header=-3"])?
  uu.fails(r0)
  uu.stderr_is(r0, "numfmt: invalid header value '-3'\n")
}

# origin: uutils test_numfmt::test_header_error_if_non_numeric
test test_uu_numfmt_header_error_if_non_numeric { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--header=two"])?
  uu.fails(r0)
  uu.stderr_is(r0, "numfmt: invalid header value 'two'\n")
}

# origin: uutils test_numfmt::test_ignores_invalid_mode_issue11935
test test_uu_numfmt_ignores_invalid_mode_issue11935 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--invalid=warn", "100", "1e5", "200"])?
  uu.succeeds(r0)
  uu.stderr_is(r0, "numfmt: invalid suffix in input: '1e5'\n")
  uu.stdout_is(r0, "100\n1e5\n200\n")
}

# origin: uutils test_numfmt::test_implied_initial_field_value
test test_uu_numfmt_implied_initial_field_value { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "-2", "1K 2K 3K"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1000 2000 3K\n")
  let r1 = uu.invoke(s, "numfmt", ["--from=auto", "--field=-2", "1K 2K 3K"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1000 2000 3K\n")
}

# origin: uutils test_numfmt::test_input_from_free_arguments
test test_uu_numfmt_input_from_free_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=si", "1K", "1.1M", "0.1G"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1000\n1100000\n100000000\n")
}

# origin: uutils test_numfmt::test_invalid_arg
test test_uu_numfmt_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--definitely-invalid"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_numfmt::test_invalid_arg_number_with_abort_returns_status_2
test test_uu_numfmt_invalid_arg_number_with_abort_returns_status_2 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--invalid=abort", "4Q"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_only(r0, "numfmt: rejecting suffix in input: '4Q' (consider using --from)\n")
}

# origin: uutils test_numfmt::test_invalid_arg_number_with_fail_returns_status_2
test test_uu_numfmt_invalid_arg_number_with_fail_returns_status_2 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--invalid=fail", "4Q"])?
  uu.fails_with_code(r0, 2)
  uu.stdout_is(r0, "4Q\n")
  uu.stderr_is(r0, "numfmt: rejecting suffix in input: '4Q' (consider using --from)\n")
}

# origin: uutils test_numfmt::test_invalid_arg_number_with_ignore_returns_status_0
test test_uu_numfmt_invalid_arg_number_with_ignore_returns_status_0 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--invalid=ignore", "4Q"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "4Q\n")
}

# origin: uutils test_numfmt::test_format_all_fields
test test_uu_numfmt_format_all_fields { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "-", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1000 2000 3000 4000 5000 6000\n")
  let r1 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "-,3", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1000 2000 3000 4000 5000 6000\n")
  let r2 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "3,-", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "1000 2000 3000 4000 5000 6000\n")
  let r3 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "1,-,3", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "1000 2000 3000 4000 5000 6000\n")
  let r4 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "- 3", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "1000 2000 3000 4000 5000 6000\n")
}

# origin: uutils test_numfmt::test_format_with_zero_padding
test test_uu_numfmt_format_with_zero_padding { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%06f", "1234"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "001234\n")
  let r1 = uu.invoke(s, "numfmt", ["--format=%0 6f", "1234"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "001234\n")
}

# origin: uutils test_numfmt::test_format_with_zero_padding_excludes_unit_suffix
test test_uu_numfmt_format_with_zero_padding_excludes_unit_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%05.1f", "--to=si", "1234567"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "001.3M\n")
  let r1 = uu.invoke(s, "numfmt", ["--format=%06.1f", "--to=si", "1234"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0001.3k\n")
  let r2 = uu.invoke(s, "numfmt", ["--format=%08.1f", "--to=iec-i", "1234567"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "000001.2Mi\n")
  let r3 = uu.invoke(s, "numfmt", ["--format=%08.1f", "--to=si", "--", "-1234567"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-00001.3M\n")
  let r4 = uu.invoke(s, "numfmt", ["--format=%03.1f", "--to=si", "1234567"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "1.3M\n")
  let r5 = uu.invoke(s, "numfmt", ["--format=%05.1f", "--to=si", "--suffix=B", "1234567"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "001.3MB\n")
  let r6 = uu.invoke(s, "numfmt", ["--format=%05.1f", "--suffix=B", "1.5"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "001.5B\n")
  let r7 = uu.invoke(s, "numfmt", ["--format=%08.1f", "--to=si", "--padding=12", "1234567"])?
  uu.succeeds(r7)
  uu.stdout_only(r7, "   000001.3M\n")
}

# origin: uutils test_numfmt::test_format_with_precision
test test_uu_numfmt_format_with_precision { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%.1f", "0.99"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1.0\n")
  let r1 = uu.invoke(s, "numfmt", ["--format=%.1f", "1"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.0\n")
  let r2 = uu.invoke(s, "numfmt", ["--format=%.1f", "1.01"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "1.1\n")
  let r3 = uu.invoke(s, "numfmt", ["--format=%.2f", "0.99"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "0.99\n")
  let r4 = uu.invoke(s, "numfmt", ["--format=%.2f", "1"])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "1.00\n")
  let r5 = uu.invoke(s, "numfmt", ["--format=%.2f", "1.01"])?
  uu.succeeds(r5)
  uu.stdout_is(r5, "1.01\n")
}

# origin: uutils test_numfmt::test_format_with_precision_and_down_rounding
test test_uu_numfmt_format_with_precision_and_down_rounding { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%.1f", "0.99", "--round=down"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "0.9\n")
  let r1 = uu.invoke(s, "numfmt", ["--format=%.1f", "1", "--round=down"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.0\n")
  let r2 = uu.invoke(s, "numfmt", ["--format=%.1f", "1.01", "--round=down"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "1.0\n")
}

# origin: uutils test_numfmt::test_format_with_precision_and_to_arg
test test_uu_numfmt_format_with_precision_and_to_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%.1f", "9991239123", "--to=si"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "10.0G\n")
  let r1 = uu.invoke(s, "numfmt", ["--format=%.4f", "9991239123", "--to=si"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "9.9913G\n")
}

# origin: uutils test_numfmt::test_format_preserve_trailing_zeros_if_no_precision_is_specified
test test_uu_numfmt_format_preserve_trailing_zeros_if_no_precision_is_specified { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%f", "10.0"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "10.0\n")
  let r1 = uu.invoke(s, "numfmt", ["--format=%f", "0.0100"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "0.0100\n")
}

# origin: uutils test_numfmt::test_format_without_percentage_directive
test test_uu_numfmt_format_without_percentage_directive { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format="])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "format '' has no % directive")
  let r1 = uu.invoke(s, "numfmt", ["--format=hello"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "format 'hello' has no % directive")
}

# origin: uutils test_numfmt::test_format_with_percentage_directive_at_end
test test_uu_numfmt_format_with_percentage_directive_at_end { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=hello%"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "format 'hello%' ends in %")
}

# origin: uutils test_numfmt::test_format_with_too_many_percentage_directives
test test_uu_numfmt_format_with_too_many_percentage_directives { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%f %f"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "format '%f %f' has too many % directives")
}

# origin: uutils test_numfmt::test_format_with_invalid_format
test test_uu_numfmt_format_with_invalid_format { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%d"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "invalid format '%d', directive must be %[0]['][-][N][.][N]f")
  let r1 = uu.invoke(s, "numfmt", ["--format=% -43 f"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "invalid format '% -43 f', directive must be %[0]['][-][N][.][N]f")
}

# origin: uutils test_numfmt::test_format_with_width_overflow
test test_uu_numfmt_format_with_width_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%18446744073709551616f"])?
  uu.succeeds(r0)
  uu.no_output(r0)
}

# origin: uutils test_numfmt::test_format_with_invalid_precision
test test_uu_numfmt_format_with_invalid_precision { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%.-1f"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "invalid precision in format '%.-1f'")
  let r1 = uu.invoke(s, "numfmt", ["--format=%.+1f"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "invalid precision in format '%.+1f'")
  let r2 = uu.invoke(s, "numfmt", ["--format=%. 1f"])?
  uu.fails_with_code(r2, 1)
  uu.stderr_contains(r2, "invalid precision in format '%. 1f'")
  let r3 = uu.invoke(s, "numfmt", ["--format=%.18446744073709551616f"])?
  uu.fails_with_code(r3, 1)
  uu.stderr_contains(r3, "invalid precision in format '%.18446744073709551616f'")
}

# origin: uutils test_numfmt::test_format_error_escapes_special_characters
test test_uu_numfmt_format_error_escapes_special_characters { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=ab\ncd"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "format 'ab\\ncd' has no % directive")
  let r1 = uu.invoke(s, "numfmt", ["--format=a\nb%"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "format 'a\\nb%' ends in %")
  let r2 = uu.invoke(s, "numfmt", ["--format=a\nb%f%"])?
  uu.fails_with_code(r2, 1)
  uu.stderr_contains(r2, "format 'a\\nb%f%' has too many % directives")
  let r3 = uu.invoke(s, "numfmt", ["--format=a\tb%f%"])?
  uu.fails_with_code(r3, 1)
  uu.stderr_contains(r3, "format 'a\\tb%f%' has too many % directives")
  let r4 = uu.invoke(s, "numfmt", ["--format=a\\b%f%"])?
  uu.fails_with_code(r4, 1)
  uu.stderr_contains(r4, "format 'a\\\\b%f%' has too many % directives")
  let r5 = uu.invoke(s, "numfmt", ["--format=a'b%f%"])?
  uu.fails_with_code(r5, 1)
  uu.stderr_contains(r5, "format 'a\\'b%f%' has too many % directives")
  let r6 = uu.invoke(s, "numfmt", ["--format=a b%f%"])?
  uu.fails_with_code(r6, 1)
  uu.stderr_contains(r6, "format 'a b%f%' has too many % directives")
  let r7 = uu.invoke(s, "numfmt", ["--format=a\nb%q"])?
  uu.fails_with_code(r7, 1)
  uu.stderr_contains(r7, "invalid format 'a\\nb%q', directive must be %[0]['][-][N][.][N]f")
  let r8 = uu.invoke(s, "numfmt", ["--format=a\nb%99999999999999999999f"])?
  uu.succeeds(r8)
  uu.no_output(r8)
  let r9 = uu.invoke(s, "numfmt", ["--format=a\nb%.-f"])?
  uu.fails_with_code(r9, 1)
  uu.stderr_contains(r9, "invalid format 'a\\nb%.-f', directive must be %[0]['][-][N][.][N]f")
}

# origin: uutils test_numfmt::test_empty_delimiter_success
test test_uu_numfmt_empty_delimiter_success { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["-d", "", "--from=si", "4.0 K"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "4000\n")
  let r1 = uu.invoke(s, "numfmt", ["-d", "", "--from=si", "4  "])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "4\n")
  let r2 = uu.invoke(s, "numfmt", ["-d", "", "--from=auto", "2 "])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "2\n")
  let r3 = uu.invoke(s, "numfmt", ["-d", "", "--from=auto", "2  "])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "2\n")
  let r4 = uu.invoke(s, "numfmt", ["-d", "", "--from=auto", "2K "])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "2000\n")
  let r5 = uu.invoke(s, "numfmt", ["-d", "", "--from=si", "--unit-separator= ", "1 K"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "1000\n")
  let r6 = uu.invoke(s, "numfmt", ["-d", "", "--from=iec", "--unit-separator= ", "2 M"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "2097152\n")
}

# origin: uutils test_numfmt::test_format_value_too_large_issue_11936
test test_uu_numfmt_format_value_too_large_issue_11936 { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--format=%5.1f", "1000000000000000000"])?
  uu.fails_with_code(r0, 2)
  uu.stderr_contains(r0, "value/precision too large")
  uu.stderr_contains(r0, "1e+18/1")
  let r1 = uu.invoke(s, "numfmt", ["--format=%.2f", "100000000000000000"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_contains(r1, "value/precision too large")
  uu.stderr_contains(r1, "1e+17/2")
  let r2 = uu.invoke(s, "numfmt", ["--format=%.3f", "10000000000000000"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_contains(r2, "value/precision too large")
  uu.stderr_contains(r2, "1e+16/3")
}

# origin: uutils test_numfmt::test_iec_format_precision_cap
test test_uu_numfmt_iec_format_precision_cap { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "numfmt", ["--to=iec", "--format=%.5f", "1500"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "1.46500K\n")
  let r1 = uu.invoke(s, "numfmt", ["--to=iec", "--format=%.5f", "999999"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "976.56200K\n")
  let r2 = uu.invoke(s, "numfmt", ["--to=iec", "--format=%.5f", "310174"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "302.90500K\n")
}

# origin: uutils test_numfmt::diagnostics::test_input_from_stdin_keeps_its_plain_message
test test_uu_numfmt_diagnostics_input_from_stdin_keeps_its_plain_message { |ctx|
  let s = uu.scene(ctx)?
  let r = tty_stderr(s, [], bytes.from_text("12abc\n"))?
  uu.fails_with_code(r, 2)
  assert trim_end(r.stderr.utf8()?) == "numfmt: invalid suffix in input: '12abc'"
}

# origin: uutils test_numfmt::diagnostics::test_a_line_of_fields_keeps_its_plain_message
test test_uu_numfmt_diagnostics_a_line_of_fields_keeps_its_plain_message { |ctx|
  let s = uu.scene(ctx)?
  let r = tty_stderr(s, ["--field=2", "x 12abc"], bytes.from_text(""))?
  uu.fails_with_code(r, 2)
  assert trim_end(r.stderr.utf8()?) == "numfmt: invalid suffix in input: '12abc'"
}

# origin: uutils test_numfmt::diagnostics::test_other_option_errors_keep_their_plain_message
test test_uu_numfmt_diagnostics_other_option_errors_keep_their_plain_message { |ctx|
  let s = uu.scene(ctx)?
  let r = tty_stderr(s, ["--format=%f", "--delimiter=ab", "1000"], bytes.from_text(""))?
  uu.fails_with_code(r, 1)
  assert trim_end(r.stderr.utf8()?) == "numfmt: the delimiter must be a single character"
}

# origin: uutils test_numfmt::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_numfmt_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = pipe_stderr(s, ["--format=%q", "1000"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "numfmt: invalid format '%q', directive must be %[0]['][-][N][.][N]f\n")
}

# origin: uutils test_numfmt::field_diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_numfmt_field_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = pipe_stderr(s, ["--field=0", "--to=si", "1"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "numfmt: fields are numbered from 1\n")
}
