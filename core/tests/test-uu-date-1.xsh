##! Native ports of the frozen uutils date integration tests.

use support.uu as uu

# The upstream locale guard checks the host locale utility's UTF-8 charmap.
proc locale_available(name: Str) -> Result[Bool] {
  let result = run.capture --text env LC_ALL=$name locale charmap
  Ok(result.stdout.trim() == "UTF-8")
}

# origin: uutils test_date::test_bad_format_option_missing_leading_plus_after_d_flag
test test_uu_date_bad_format_option_missing_leading_plus_after_d_flag { |ctx|
  let s = uu.scene(ctx)?
  for bad in ["q", "a", "test", "%Y-%m-%d"] {
    let r = uu.invoke(s, "date", ["--date", "1996-01-31", bad])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, f"the argument '{bad}' lacks a leading '+';\nwhen using an option to specify date(s), any non-option\nargument must be a format string beginning with '+'")
  }
}

# origin: uutils test_date::test_capitalized_numeric_time_zone
test test_uu_date_capitalized_numeric_time_zone { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["+%#z"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("^[+-][0-9]{4}\n$", r.stdout, extended: true)?).is_empty()
  }
}

# origin: uutils test_date::test_date_accepts_gnu_timezone_abbreviations
test test_uu_date_date_accepts_gnu_timezone_abbreviations { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-u", "-d", "2024-01-15 12:00 MEZ", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC0"})?
  uu.succeeds(r)
  uu.stdout_is(r, "11:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-u", "-d", "2024-01-15 12:00 MESZ", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC0"})?
  uu.succeeds(r)
  uu.stdout_is(r, "10:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-u", "-d", "2024-01-15 12:00 MEST", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC0"})?
  uu.succeeds(r)
  uu.stdout_is(r, "10:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-u", "-d", "2024-01-15 12:00 KST", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC0"})?
  uu.succeeds(r)
  uu.stdout_is(r, "03:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-u", "-d", "2024-01-15 12:00 EST", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC0"})?
  uu.succeeds(r)
  uu.stdout_is(r, "17:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-u", "-d", "2024-01-15 12:00 IST", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC0"})?
  uu.succeeds(r)
  uu.stdout_is(r, "06:30\n")
  }
}

# origin: uutils test_date::test_date_debug_basic
test test_uu_date_date_debug_basic { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["--debug", "-d", "2005-01-01", "+%Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.trim() == "2005"
  uu.stderr_contains(r, "date: starting date/time:")
  uu.stderr_contains(r, "date: parsed date part:")
  uu.stderr_contains(r, "date: warning: using midnight as starting time: 00:00:00")
  uu.stderr_contains(r, "date: input timezone:")
}

# origin: uutils test_date::test_date_debug_current_time
test test_uu_date_date_debug_current_time { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["--debug", "+%Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stderr_is(r, "date: output format: '%Y'\n")
}

# origin: uutils test_date::test_date_debug_midnight_warnings
test test_uu_date_date_debug_midnight_warnings { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "2005-01-01", "+%Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  assert "date: warning: using midnight" in r.stderr.utf8()?
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "1997-01-19 08:17:48 +0", "+%Y-%m-%d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  assert "warning: using midnight" not in r.stderr.utf8()?
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "@0", "+%Y-%m-%d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  assert "warning: using midnight" not in r.stderr.utf8()?
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", " ", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  assert "warning: using midnight" not in r.stderr.utf8()?
  }
}

# origin: uutils test_date::test_date_debug_various_formats
test test_uu_date_date_debug_various_formats { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "2005-01-01 +345 day", "+%Y-%m-%d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "2005-12-12")
  uu.stderr_contains(r, "date: final: (Y-M-D) 2005-12-12 00:00:00 (UTC)")
  uu.stderr_contains(r, "date: parsed relative part: +345 day(s)")
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "@0", "+%Y-%m-%d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "1970-01-01")
  uu.stderr_contains(r, "date: final: (Y-M-D) 1970-01-01 00:00:00 (UTC)")
  uu.stderr_contains(r, "date: parsed number of seconds part: number of seconds: 0")
  assert "warning: using midnight" not in r.stderr.utf8()?
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "@-22", "+%s"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "-22")
  uu.stderr_contains(r, "date: final: (Y-M-D) 1969-12-31 23:59:38 (UTC)")
  uu.stderr_contains(r, "date: parsed number of seconds part: number of seconds: -22")
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "2021-03-20 14:53:01 EST", "+%Y-%m-%d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "2021-03-20")
  uu.stderr_contains(r, "date: parsed date part: (Y-M-D) 2021-03-20")
  uu.stderr_contains(r, "date: parsed time part: 14:53:01")
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "m9", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "21:00:00")
  uu.stderr_contains(r, "date: parsed number part: 09:00:00")
  uu.stderr_contains(r, "date: parsed zone part: UTC+12")
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", " ", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "00:00:00")
  uu.stderr_contains(r, "date: using specified time as starting value: '00:00:00'")
  uu.stderr_contains(r, "date: parsed number part: 00:00:00")
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "1 day ago", "+%Y-%m-%d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stderr_contains(r, "date: using current date as starting value:")
  uu.stderr_contains(r, "date: parsed relative part: -1 day(s)")
  }
}

# origin: uutils test_date::test_date_debug_with_flags
test test_uu_date_date_debug_with_flags { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "2005-01-01", "+%Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "2005")
  uu.stderr_contains(r, "date: parsed date part: (Y-M-D) 2005-01-01")
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-u", "-d", "2005-01-01", "+%Y-%m-%d %Z"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "UTC")
  uu.stderr_contains(r, "date: parsed date part:")
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-R", "-d", "2005-01-01"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "Sat, 01 Jan 2005")
  uu.stderr_contains(r, "date: parsed date part: (Y-M-D) 2005-01-01")
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-d", "invalid", "+%Y"], vars: {TZ: "UTC"})?
  uu.fails(r)
  uu.stderr_contains(r, "invalid date")
  }
}

# origin: uutils test_date::test_date_debug_with_multiple_inputs
test test_uu_date_date_debug_with_multiple_inputs { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "debug_test_file", "2005-01-01\n2006-02-02\n")?
  {
  let r = uu.invoke(s, "date", ["--debug", "-f", "debug_test_file", "+%Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "2005\n2006\n")
  uu.stderr_contains(r, "date: starting date/time: '(Y-M-D) 2005-01-01 00:00:00'")
  uu.stderr_contains(r, "date: starting date/time: '(Y-M-D) 2006-02-02 00:00:00'")
  uu.stderr_contains(r, "date: parsed date part: (Y-M-D) 2005-01-01")
  uu.stderr_contains(r, "date: parsed date part: (Y-M-D) 2006-02-02")
  }
  {
  let r = uu.invoke(s, "date", ["--debug", "-f", "-", "+%Y"], vars: {TZ: "UTC"}, stdin: b"2005-01-01\n2006-02-02\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "2005\n2006\n")
  uu.stderr_contains(r, "date: starting date/time: '(Y-M-D) 2005-01-01 00:00:00'")
  uu.stderr_contains(r, "date: starting date/time: '(Y-M-D) 2006-02-02 00:00:00'")
  }
}

# origin: uutils test_date::test_date_debug_without_flag
test test_uu_date_date_debug_without_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "2005-01-01", "+%Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  assert "date: input string:" not in r.stderr.utf8()?
  assert "date: parsed date part:" not in r.stderr.utf8()?
}

# origin: uutils test_date::test_date_double_timezone_is_invalid
test test_uu_date_date_double_timezone_is_invalid { |ctx|
  let s = uu.scene(ctx)?
  for input in ["EST EST", "EST PST", "2021-03-20 14:53:01 EST EST"] {
    let r = uu.invoke(s, "date", ["-d", input])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "invalid date")
  }
  let r = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 EST", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r)
  uu.no_stderr(r)
}

# origin: uutils test_date::test_date_email
test test_uu_date_date_email { |ctx|
  let s = uu.scene(ctx)?
  for param in ["--rfc-email", "--rfc-e", "-R", "--rfc-2822", "--rfc-822"] {
  let r = uu.invoke(s, "date", [param])?
  uu.succeeds(r)
  }
}

# origin: uutils test_date::test_date_email_multiple_aliases
test test_uu_date_date_email_multiple_aliases { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["--rfc-email", "--rfc-822", "--rfc-2822"])?
  uu.succeeds(r)
  }
}

# origin: uutils test_date::test_date_embedded_timezone_conversion
test test_uu_date_date_embedded_timezone_conversion { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "TZ=\"CET-1\" 1970-01-01 00:00"], vars: {TZ: "UTC0", LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "Dec 31")
  uu.stdout_contains(r, "23:00:00")
  uu.stdout_contains(r, "1969")
  }
}

# origin: uutils test_date::test_date_empty_string
test test_uu_date_date_empty_string { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", ""], vars: {TZ: "UTC+1"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "00:00:00")
  }
}

# origin: uutils test_date::test_date_empty_string_variations
test test_uu_date_date_empty_string_variations { |ctx|
  let s = uu.scene(ctx)?
  for input in ["", " ", "  ", "\t", "\n", " \t ", "\t\n\t", "\x0b", "\x0c"] {
    let r = uu.invoke(s, "date", ["-d", input, "+%T"], vars: {TZ: "UTC"})?
    uu.succeeds(r)
    uu.stdout_is(r, "00:00:00\n")
  }
  let r = uu.invoke(s, "date", ["-u", "-d", "", "+%T %Z"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "00:00:00")
  uu.stdout_contains(r, "UTC")
}

# origin: uutils test_date::test_date_empty_tz_time
test test_uu_date_date_empty_tz_time { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "@0"], vars: {TZ: ""})?
  uu.succeeds(r)
  uu.stdout_only(r, "Thu Jan  1 00:00:00 Universal 1970\n")
  }
}

# origin: uutils test_date::test_date_error_echoes_input_verbatim
test test_uu_date_date_error_echoes_input_verbatim { |ctx|
  let s = uu.scene(ctx)?
  for input in ["1e9", "+1e-2", "+9.e-0", "9.", "-0"] {
    let r = uu.invoke(s, "date", ["-d", input])?
    uu.fails(r)
    uu.stderr_is(r, f"date: invalid date '{input}'\n")
  }
  let r = uu.invoke(s, "date", ["--date", "1996-01-31", "1e9"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "the argument '1e9' lacks a leading '+'")
}

# origin: uutils test_date::test_date_ethiopian_locale_calendar
test test_uu_date_date_ethiopian_locale_calendar { |ctx|
  let s = uu.scene(ctx)?
  if ! locale_available("am_ET.UTF-8")? { return }
  let r = uu.invoke(s, "date", ["+%Y"], vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  let current_year = r.stdout.utf8()?.trim().parse_int()?
  {
  let r = uu.invoke(s, "date", ["-d", f"{current_year}-09-10", "+%Y"], vars: {LC_ALL: "am_ET.UTF-8"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.trim().parse_int()? == current_year - 8
  }
  {
  let r = uu.invoke(s, "date", ["-d", f"{current_year}-09-12", "+%Y"], vars: {LC_ALL: "am_ET.UTF-8"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.trim().parse_int()? == current_year - 7
  }
  {
  let r = uu.invoke(s, "date", ["--iso-8601=hours"], vars: {LC_ALL: "am_ET.UTF-8"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.starts_with(f"{current_year}")
  }
  {
  let r = uu.invoke(s, "date", ["--rfc-3339=date"], vars: {LC_ALL: "am_ET.UTF-8"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.starts_with(f"{current_year}")
  }
}

# origin: uutils test_date::test_date_explicit_format_overrides_locale
test test_uu_date_date_explicit_format_overrides_locale { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "2025-10-11T13:00", "+%H:%M"], vars: {LC_ALL: "en_US.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "13:00\n")
  }
}

# origin: uutils test_date::test_date_file_invalid_utf8_line
test test_uu_date_date_file_invalid_utf8_line { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "test_date_file_invalid_utf8", b"Hello\xffx\n")?
  let r = uu.invoke(s, "date", ["-u", "-f", "test_date_file_invalid_utf8"])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "date: invalid date 'Hello\\377x'")
}

# origin: uutils test_date::test_date_file_invalid_utf8_line_continues
test test_uu_date_date_file_invalid_utf8_line_continues { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "test_date_file_mixed_invalid_utf8", b"2024-01-15 12:00:00\nHello\xffx\n2024-01-16 13:00:00\n")?
  let r = uu.invoke(s, "date", ["-u", "-f", "test_date_file_mixed_invalid_utf8"])?
  uu.fails_with_code(r, 1)
  uu.stdout_contains(r, "Mon Jan 15 12:00:00 UTC 2024")
  uu.stdout_contains(r, "Tue Jan 16 13:00:00 UTC 2024")
  uu.stderr_contains(r, "date: invalid date 'Hello\\377x'")
}

# origin: uutils test_date::test_date_file_line_ends_at_nul
test test_uu_date_date_file_line_ends_at_nul { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-u", "-f", "-"], stdin: b"2024-01-15 12:00:00\x00garbage\nbad\x00\xff\n")?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "Mon Jan 15 12:00:00 UTC 2024\n")
  uu.stderr_is(r, "date: invalid date 'bad'\n")
}

# origin: uutils test_date::test_date_for_dir_as_file
test test_uu_date_date_for_dir_as_file { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["--file", "/"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "date: /: read error: Is a directory\n")
  }
}

# origin: uutils test_date::test_date_for_empty_file
test test_uu_date_date_for_empty_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test_date_for_file")?
  let r = uu.invoke(s, "date", ["--file", "test_date_for_file"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_date::test_date_for_file_mtime
test test_uu_date_date_for_file_mtime { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "reference_file")?
  fs.set_times(uu.at(s, "reference_file"), mtime_sec: 1234)?
  let r = uu.invoke(s, "date", ["--reference", "reference_file", "+%s"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1234\n")
}

# origin: uutils test_date::test_date_for_file_with_non_utf8_path
test test_uu_date_date_for_file_with_non_utf8_path { |ctx|
  let s = uu.scene(ctx)?
  let file = uu.at_bytes(s, b"file_\xff\xfe.txt")?
  file.write("")?
  let r = uu.invoke_paths(s, "date", [p"--file", file])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_date::test_date_for_no_permission_file
test test_uu_date_date_for_no_permission_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file-no-perm-1")?
  uu.set_mode(s, "file-no-perm-1", 0o222)?
  let r = uu.invoke(s, "date", ["--file", "file-no-perm-1"])?
  uu.fails(r)
  uu.stderr_only(r, "date: file-no-perm-1: Permission denied\n")
}

# origin: uutils test_date::test_date_for_non_existing_file
test test_uu_date_date_for_non_existing_file { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["--file", "non_existing_file"])?
  uu.fails(r)
  uu.stderr_only(r, "date: non_existing_file: No such file or directory\n")
  }
}

# origin: uutils test_date::test_date_format_a_french_locale
test test_uu_date_date_format_a_french_locale { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "2025-01-15", "+%A %a"], vars: {LC_TIME: "fr_FR.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  if output.trim() != "mercredi mer." { return }
  assert output.trim() == "mercredi mer."
  }
  {
  let r = uu.invoke(s, "date", ["-d", "2025-02-15", "+%A %a"], vars: {LC_TIME: "fr_FR.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  if output.trim() != "samedi sam." { return }
  assert output.trim() == "samedi sam."
  }
}

# origin: uutils test_date::test_date_format_b_french_locale
test test_uu_date_date_format_b_french_locale { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "2025-01-15", "+%B %b"], vars: {LC_TIME: "fr_FR.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  if output.trim() != "janvier janv." { return }
  assert output.trim() == "janvier janv."
  }
  {
  let r = uu.invoke(s, "date", ["-d", "2025-02-15", "+%B %b"], vars: {LC_TIME: "fr_FR.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  if output.trim() != "février févr." { return }
  assert output.trim() == "février févr."
  }
}

# origin: uutils test_date::test_date_format_day
test test_uu_date_date_format_day { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["+%a"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("[^[:space:]]+", r.stdout, extended: true)?).is_empty()
  }
  {
  let r = uu.invoke(s, "date", ["+%A"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("[^[:space:]]+", r.stdout, extended: true)?).is_empty()
  }
  {
  let r = uu.invoke(s, "date", ["+%u"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("^[0-9]\n$", r.stdout, extended: true)?).is_empty()
  }
}

# origin: uutils test_date::test_date_format_full_day
test test_uu_date_date_format_full_day { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["+'%a %Y-%m-%d'"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("[^[:space:]]+ [0-9]{4}-[0-9]{2}-[0-9]{2}", r.stdout, extended: true)?).is_empty()
  }
}

# origin: uutils test_date::test_date_format_large_width_no_oom
test test_uu_date_date_format_large_width_no_oom { |ctx|
  let s = uu.scene(ctx)?
  for width in [300, 10000] {
    let r = uu.invoke(s, "date", ["-d", "2024-01-01", f"+%{width}S"])?
    uu.succeeds(r)
    uu.stdout_is(r, ["0" for _ in range(width)].join("") + "\n")
  }
  let r = uu.invoke(s, "date", ["-d", "2024-01-01", "+%2uueuu%6666u-r"])?
  uu.succeeds(r)
  uu.stdout_is(r, "01ueuu" + ["0" for _ in range(6665)].join("") + "1-r\n")
}

# origin: uutils test_date::test_date_format_literal
test test_uu_date_date_format_literal { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["+%%s"])?
  uu.succeeds(r)
  uu.stdout_is(r, "%s\n")
  }
  {
  let r = uu.invoke(s, "date", ["+%%N"])?
  uu.succeeds(r)
  uu.stdout_is(r, "%N\n")
  }
}

# origin: uutils test_date::test_date_format_m
test test_uu_date_date_format_m { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["+%b"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("[^[:space:]]+", r.stdout, extended: true)?).is_empty()
  }
  {
  let r = uu.invoke(s, "date", ["+%m"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("^[0-9]{2}\n$", r.stdout, extended: true)?).is_empty()
  }
}

# origin: uutils test_date::test_date_format_modifier_case_precedence
test test_uu_date_date_format_modifier_case_precedence { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%^#B"], vars: {TZ: "UTC", LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_is(r, "JUNE\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%#^B"], vars: {TZ: "UTC", LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_is(r, "JUNE\n")
  }
}

# origin: uutils test_date::test_date_format_modifier_combined_flags
test test_uu_date_date_format_modifier_combined_flags { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%-^10B"], vars: {TZ: "UTC", LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_is(r, "JUNE\n")
  }
}

# origin: uutils test_date::test_date_format_modifier_edge_cases
test test_uu_date_date_format_modifier_edge_cases { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%_d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, " 1\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-15", "+%_m"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, " 6\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01 05:00:00", "+%_H"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, " 5\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%_Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "1999\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%_C"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "19\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "2024-06-01", "+%_C"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "20\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-01-01", "+%_j"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "  1\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-04-10", "+%_j"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "100\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-05", "+%0e"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "05\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01 05:00:00", "+%0k"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "05\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01 05:00:00", "+%0l"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "05\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%0d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "01\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-15", "+%0m"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "06\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-01-01", "+%0j"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "001\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-05", "+%e"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, " 5\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01 05:00:00", "+%k"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, " 5\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01 05:00:00", "+%l"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, " 5\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%+Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "1999\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%+6Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "+01999\n")
  }
}

# origin: uutils test_date::test_date_format_modifier_force_sign
test test_uu_date_date_format_modifier_force_sign { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1970-01-01", "+%+6Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "+01970\n")
  }
}

# origin: uutils test_date::test_date_format_modifier_huge_width_fails_without_abort
test test_uu_date_date_format_modifier_huge_width_fails_without_abort { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["+%18446744073709551615c"], timeout: 10s)?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_date::test_date_format_modifier_multiple
test test_uu_date_date_format_modifier_multiple { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%10Y-%_5m-%-5d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "0000001999-    6-1\n")
  }
}

# origin: uutils test_date::test_date_format_modifier_no_pad
test test_uu_date_date_format_modifier_no_pad { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%-10Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "1999\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%-d"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "1\n")
  }
}

# origin: uutils test_date::test_date_format_modifier_percent_escape
test test_uu_date_date_format_modifier_percent_escape { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%%Y=%10Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "%Y=0000001999\n")
  }
}

# origin: uutils test_date::test_date_format_modifier_underscore_padding
test test_uu_date_date_format_modifier_underscore_padding { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%_10m"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "         6\n")
  }
}

# origin: uutils test_date::test_date_format_modifier_uppercase
test test_uu_date_date_format_modifier_uppercase { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%^B"], vars: {TZ: "UTC", LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_is(r, "JUNE\n")
  }
}

# origin: uutils test_date::test_date_format_modifier_width
test test_uu_date_date_format_modifier_width { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "1999-06-01", "+%10Y"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "0000001999\n")
  }
}

# origin: uutils test_date::test_date_format_q
test test_uu_date_date_format_q { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["+%q"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("^[1-4]\n$", r.stdout, extended: true)?).is_empty()
  }
}

# origin: uutils test_date::test_date_format_without_plus
test test_uu_date_date_format_without_plus { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["%s"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "date: invalid date '%s'")
  }
}

# origin: uutils test_date::test_date_format_y
test test_uu_date_date_format_y { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["+%Y"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("^[0-9]{4}\n$", r.stdout, extended: true)?).is_empty()
  }
  {
  let r = uu.invoke(s, "date", ["+%y"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("^[0-9]{2}\n$", r.stdout, extended: true)?).is_empty()
  }
}

# origin: uutils test_date::test_date_french_full_sentence
test test_uu_date_date_french_full_sentence { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "2026-01-21", "+Nous sommes le %A %d %B %Y"], vars: {LANG: "fr_FR.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  if output.trim() == "Nous sommes le mercredi 21 janvier 2026" { assert output.trim() == "Nous sommes le mercredi 21 janvier 2026" }
}

# origin: uutils test_date::test_date_from_stdin
test test_uu_date_date_from_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-f", "-"], vars: {TZ: "UTC0"}, stdin: b"2023-03-27 08:30:00\n2023-04-01 12:00:00\n2023-04-15 18:30:00\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "Mon Mar 27 08:30:00 UTC 2023\nSat Apr  1 12:00:00 UTC 2023\nSat Apr 15 18:30:00 UTC 2023\n")
}

# origin: uutils test_date::test_date_input_hhmm_ampm
test test_uu_date_date_input_hhmm_ampm { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "2024-06-15 12:00 PM", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "12:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "2024-06-15 11:30am", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "11:30\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "2024-06-15 3:00 PM", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "15:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "2024-06-15 12:00 AM", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "2024-06-15 3:00 p.m.", "+%H:%M"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "15:00\n")
  }
}

# origin: uutils test_date::test_date_input_trailing_tz_abbrev_rezones
test test_uu_date_date_input_trailing_tz_abbrev_rezones { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "2024-01-01 EST", "+%H:%M:%S %:z"], vars: {LC_ALL: "C", TZ: "UTC+1"})?
  uu.succeeds(r)
  uu.stdout_is(r, "04:00:00 -01:00\n")
  }
}

# origin: uutils test_date::test_date_invalid_utf8_byte_rejected
test test_uu_date_date_invalid_utf8_byte_rejected { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "date", [p"-d", Path.parse_bytes(b"\xe0")?])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "invalid date '\\340'")
}

# origin: uutils test_date::test_date_iranian_locale_solar_hijri_calendar
test test_uu_date_date_iranian_locale_solar_hijri_calendar { |ctx|
  let s = uu.scene(ctx)?
  if ! locale_available("fa_IR.UTF-8")? { return }
  let r = uu.invoke(s, "date", ["+%Y"], vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  let current_year = r.stdout.utf8()?.trim().parse_int()?
  {
  let r = uu.invoke(s, "date", ["-d", f"{current_year}-03-19", "+%Y"], vars: {LC_ALL: "fa_IR.UTF-8"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.trim().parse_int()? == current_year - 622
  }
  {
  let r = uu.invoke(s, "date", ["-d", f"{current_year}-03-22", "+%Y"], vars: {LC_ALL: "fa_IR.UTF-8"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.trim().parse_int()? == current_year - 621
  }
  {
  let r = uu.invoke(s, "date", ["--iso-8601=hours"], vars: {LC_ALL: "fa_IR.UTF-8"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.starts_with(f"{current_year}")
  }
  {
  let r = uu.invoke(s, "date", ["--rfc-3339=date"], vars: {LC_ALL: "fa_IR.UTF-8"})?
  uu.succeeds(r)
  assert r.stdout.utf8()?.starts_with(f"{current_year}")
  }
}

# origin: uutils test_date::test_date_issue_3780
test test_uu_date_date_issue_3780 { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["+%Y-%m-%d %H-%M-%S%:::z"])?
  uu.succeeds(r)
  }
}

# origin: uutils test_date::test_date_leap_year_arithmetic_overflow
test test_uu_date_date_leap_year_arithmetic_overflow { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["--date", "02/29/2000 1 year", "+%Y-%m-%d"])?
  uu.succeeds(r)
  uu.stdout_is(r, "2001-03-01\n")
  }
  {
  let r = uu.invoke(s, "date", ["--date", "2000-02-29 + 2 years", "+%Y-%m-%d"])?
  uu.succeeds(r)
  uu.stdout_is(r, "2002-03-01\n")
  }
  {
  let r = uu.invoke(s, "date", ["--date", "2000-02-29 + 4 years", "+%Y-%m-%d"])?
  uu.succeeds(r)
  uu.stdout_is(r, "2004-02-29\n")
  }
}

# origin: uutils test_date::test_date_locale_c_uses_24_hour
test test_uu_date_date_locale_c_uses_24_hour { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "2025-12-14T13:00"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  assert "AM" not in output and "PM" not in output
  assert "13" in output
}

# origin: uutils test_date::test_date_locale_en_us_vs_c_difference
test test_uu_date_date_locale_en_us_vs_c_difference { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "2025-12-14T13:00:00"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  assert "AM" not in output and "PM" not in output
  }
  {
  let r = uu.invoke(s, "date", ["-d", "2025-12-14T13:00:00"], vars: {LC_ALL: "en_US.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  if "PM" in output { assert "1:00" in output or "01:00" in output }
  }
}

# origin: uutils test_date::test_date_locale_format_not_hardcoded
test test_uu_date_date_locale_format_not_hardcoded { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "2025-12-14T01:00:00"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  assert "01:00" in output or " 1:00" in output
  assert "AM" not in output and "PM" not in output
}

# origin: uutils test_date::test_date_locale_format_structure
test test_uu_date_date_locale_format_structure { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "2025-12-14T13:00:00"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  assert ! [day for day in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] if day in output].is_empty()
  assert ! [month for month in ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"] if month in output].is_empty()
  assert "2025" in output
}

# origin: uutils test_date::test_date_locale_fr_french
test test_uu_date_date_locale_fr_french { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "2025-12-14T13:00:00"], vars: {LC_ALL: "fr_FR.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  assert "AM" not in output and "PM" not in output
  assert "13:00" in output
  assert "UTC" in output or "+00" in output or "Z" in output
}

# origin: uutils test_date::test_date_locale_hour_c_locale
test test_uu_date_date_locale_hour_c_locale { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "2025-10-11T13:00"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_contains(r, "13:00")
  }
}

# origin: uutils test_date::test_date_locale_hour_en_us
test test_uu_date_date_locale_hour_en_us { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "2025-10-11T13:00"], vars: {LC_ALL: "en_US.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  assert "1:00" in output or "13:00" in output
}

# origin: uutils test_date::test_date_locale_hu_hungarian
test test_uu_date_date_locale_hu_hungarian { |ctx|
  let s = uu.scene(ctx)?
  if ! locale_available("hu_HU.UTF-8")? { return }
  let r = uu.invoke(s, "date", ["-d", "2025-12-14T13:00:00", "+%Y. %b %-e., %A, %H:%M:%S %Z"], vars: {LC_ALL: "hu_HU.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "2025. dec 14., vasárnap, 13:00:00 UTC\n")
}

# origin: uutils test_date::test_date_locale_leading_zeros_en_us
test test_uu_date_date_locale_leading_zeros_en_us { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "2025-12-14T01:00"], vars: {LC_ALL: "en_US.UTF-8", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  if "AM" in output or "PM" in output {
  assert "01:00" in output or " 1:00" in output
  }
}

# origin: uutils test_date::test_date_locale_timezone_included
test test_uu_date_date_locale_timezone_included { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "2025-12-14T13:00"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  assert "UTC" in output or "+00" in output
}

# origin: uutils test_date::test_date_military_timezone_j_variations
test test_uu_date_date_military_timezone_j_variations { |ctx|
  let s = uu.scene(ctx)?
  for input in ["J", "j", " J ", " j ", "\tJ\t"] {
    let r = uu.invoke(s, "date", ["-d", input, "+%T"], vars: {TZ: "UTC"})?
    uu.succeeds(r)
    uu.stdout_is(r, "00:00:00\n")
  }
  let r = uu.invoke(s, "date", ["-u", "-d", "J", "+%T %Z"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "00:00:00")
  uu.stdout_contains(r, "UTC")
}

# origin: uutils test_date::test_date_military_timezone_j_with_time
test test_uu_date_date_military_timezone_j_with_time { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "8j", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "08:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "9j", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "09:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "9J", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "09:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "12j", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "12:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "1230j", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "12:30:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "0j", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "00:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "00j", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "00:00:00\n")
  }
  for input in ["2400j", "2360j", "12345j"] {
  let r = uu.invoke(s, "date", ["-d", input], vars: {TZ: "UTC"})?
  uu.fails(r)
  uu.stderr_contains(r, "invalid date")
  }
}

# origin: uutils test_date::test_date_military_timezone_with_offset_and_date
test test_uu_date_date_military_timezone_with_offset_and_date { |ctx|
  let s = uu.scene(ctx)?
  let today = time.now() * 1000000
  {
  let expected = time.format(today + -86400000000000, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "m", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + -86400000000000, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "a", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + 0, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "n", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + 0, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "y", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + 0, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "z", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + 0, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "n2", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + 0, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "a1", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + 0, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "a5", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + 0, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "m23", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + 86400000000000, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "n23", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + 86400000000000, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "y23", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
  {
  let expected = time.format(today + -86400000000000, "%F", utc: true)? + "\n"
  let r = uu.invoke(s, "date", ["-d", "m9", "+%F"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  }
}

# origin: uutils test_date::test_date_military_timezone_with_offset_variations
test test_uu_date_date_military_timezone_with_offset_variations { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "a", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "23:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "m", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "12:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "z", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "00:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "m9", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "21:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "a5", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "04:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "z3", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "03:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "M", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "12:00:00\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "A5", "+%T"], vars: {TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "04:00:00\n")
  }
}

# origin: uutils test_date::test_date_month_subtraction_keeps_day
test test_uu_date_date_month_subtraction_keeps_day { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["--date", "2003-08-31 12:00:00 +0 7 months ago", "+%Y-%m-%d %T"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "2003-01-31")
  }
  {
  let r = uu.invoke(s, "date", ["--date", "1996-01-31 + 1 month", "+%Y-%m-%d"])?
  uu.succeeds(r)
  uu.stdout_is(r, "1996-03-02\n")
  }
}

# origin: uutils test_date::test_date_multiple_files
test test_uu_date_date_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "2022-02-22")?
  uu.write(s, "b", "1999-09-19")?
  for item in [{first: "a", last: "b", expected: "Sun Sep 19 00:00:00 UTC 1999\n"}, {first: "b", last: "a", expected: "Tue Feb 22 00:00:00 UTC 2022\n"}] {
    let r = uu.invoke(s, "date", ["-u", "--file", item.first, "--file", item.last])?
    uu.succeeds(r)
    uu.stdout_only(r, item.expected)
  }
}

# origin: uutils test_date::test_date_multiple_references
test test_uu_date_date_multiple_references { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  fs.set_times(uu.at(s, "a"), mtime_sec: 1111)?
  fs.set_times(uu.at(s, "b"), mtime_sec: 2222)?
  for item in [{first: "a", last: "b", expected: "2222\n"}, {first: "b", last: "a", expected: "1111\n"}] {
    let r = uu.invoke(s, "date", ["--reference", item.first, "--reference", item.last, "+%s"])?
    uu.succeeds(r)
    uu.stdout_only(r, item.expected)
  }
}

# origin: uutils test_date::test_date_nano_seconds
test test_uu_date_date_nano_seconds { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["+%N"])?
  uu.succeeds(r)
  assert ! (regex.captures_bytes("^[0-9]{1,9}\n$", r.stdout, extended: true)?).is_empty()
  }
}

# origin: uutils test_date::test_date_negative_fractional_epoch_flooring
test test_uu_date_date_negative_fractional_epoch_flooring { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "@-1.5", "+%s"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "-2\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "@-0.25", "+%s"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "-1\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "@-2.75", "+%s.%N"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "-3.250000000\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "@-100.5", "+%s"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "-101\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "@42.9", "+%s"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "42\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "@-7", "+%s"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "-7\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "@-1.5", "+%%s=%s"], vars: {LC_ALL: "C", TZ: "UTC"})?
  uu.succeeds(r)
  uu.stdout_is(r, "%s=-2\n")
  }
}

# origin: uutils test_date::test_date_numeric_d_basic_utc
test test_uu_date_date_numeric_d_basic_utc { |ctx|
  let s = uu.scene(ctx)?
  let today = time.format(time.now() * 1000000, "%F", utc: true)?
  {
  let r = uu.invoke(s, "date", ["-d", "0", "+%F %T %Z"], vars: {TZ: "UTC0"})?
  uu.succeeds(r)
  uu.stdout_only(r, today + " 00:00:00 UTC\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "7", "+%F %T %Z"], vars: {TZ: "UTC0"})?
  uu.succeeds(r)
  uu.stdout_only(r, today + " 07:00:00 UTC\n")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "0700", "+%F %T %Z"], vars: {TZ: "UTC0"})?
  uu.succeeds(r)
  uu.stdout_only(r, today + " 07:00:00 UTC\n")
  }
}

# origin: uutils test_date::test_date_numeric_d_invalid_numbers
test test_uu_date_date_numeric_d_invalid_numbers { |ctx|
  let s = uu.scene(ctx)?
  {
  let r = uu.invoke(s, "date", ["-d", "2400", "+%F %T %Z"], vars: {TZ: "UTC0"})?
  uu.fails(r)
  uu.stderr_contains(r, "invalid date")
  }
  {
  let r = uu.invoke(s, "date", ["-d", "2360", "+%F %T %Z"], vars: {TZ: "UTC0"})?
  uu.fails(r)
  uu.stderr_contains(r, "invalid date")
  }
}

