use support.uu as uu

# Calendar fields carry leading zeroes that the strict integer parser rejects.
pure decimal_field(value: Str) -> Result[Int, Error] {
  var number = 0
  for digit in value { number = number * 10 + digit.parse_int_decimal()? }
  Ok(number)
}

# origin: uutils test_date::test_date_one_digit_date
test test_uu_date_date_one_digit_date { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "date", ["-d", "2000-1-1"], vars: {"TZ": "UTC0"})?
  uu.succeeds(r1)
  uu.stdout_only(r1, "Sat Jan  1 00:00:00 UTC 2000\n")
  let r2 = uu.invoke(s, "date", ["-d", "2000-1-4"], vars: {"TZ": "UTC0"})?
  uu.succeeds(r2)
  uu.stdout_only(r2, "Tue Jan  4 00:00:00 UTC 2000\n")
}

# origin: uutils test_date::test_date_overflow
test test_uu_date_date_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r3 = uu.invoke(s, "date", ["-d68888888888888sms"])?
  uu.fails(r3)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "invalid date")
}

# origin: uutils test_date::test_date_parenthesis_comment
test test_uu_date_date_parenthesis_comment { |ctx|
  let s = uu.scene(ctx)?
  let r4 = uu.invoke(s, "date", ["-d", "(", "-u", "+%H:%M:%S"], vars: {"TZ": "UTC"})?
  uu.succeeds(r4)
  uu.stdout_only(r4, "00:00:00\n")
  let r5 = uu.invoke(s, "date", ["-d", "1(ignore comment to eol", "-u", "+%H:%M:%S"], vars: {"TZ": "UTC"})?
  uu.succeeds(r5)
  uu.stdout_only(r5, "01:00:00\n")
  let r6 = uu.invoke(s, "date", ["-d", "2026-01-05(this is a comment", "-u", "+%Y-%m-%d"], vars: {"TZ": "UTC"})?
  uu.succeeds(r6)
  uu.stdout_only(r6, "2026-01-05\n")
  let r7 = uu.invoke(s, "date", ["-d", "2026(this is a comment)-01-05", "-u", "+%Y-%m-%d"], vars: {"TZ": "UTC"})?
  uu.succeeds(r7)
  uu.stdout_only(r7, "2026-01-05\n")
  let r8 = uu.invoke(s, "date", ["-d", "((foo)2026-01-05)", "-u", "+%H:%M:%S"], vars: {"TZ": "UTC"})?
  uu.succeeds(r8)
  uu.stdout_only(r8, "00:00:00\n")
  let r9 = uu.invoke(s, "date", ["-d", "(2026-01-05(foo))", "-u", "+%H:%M:%S"], vars: {"TZ": "UTC"})?
  uu.succeeds(r9)
  uu.stdout_only(r9, "00:00:00\n")
}

# origin: uutils test_date::test_date_parenthesis_vs_other_special_chars
test test_uu_date_date_parenthesis_vs_other_special_chars { |ctx|
  let s = uu.scene(ctx)?
  let r10 = uu.invoke(s, "date", ["-d", "["])?
  uu.fails(r10)
  uu.stderr_contains(r10, "invalid date")
  let r11 = uu.invoke(s, "date", ["-d", "."])?
  uu.fails(r11)
  uu.stderr_contains(r11, "invalid date")
  let r12 = uu.invoke(s, "date", ["-d", "^"])?
  uu.fails(r12)
  uu.stderr_contains(r12, "invalid date")
}

# origin: uutils test_date::test_date_posix_format_specifiers
test test_uu_date_date_posix_format_specifiers { |ctx|
  let s = uu.scene(ctx)?
  let r13 = uu.invoke(s, "date", ["-d", "1997-01-19 08:17:48", "+%r"], vars: {"TZ": "UTC"})?
  uu.succeeds(r13)
  uu.stdout_is(r13, "08:17:48 AM\n")
  let r14 = uu.invoke(s, "date", ["-d", "1997-01-19 08:17:48", "+%x"], vars: {"TZ": "UTC"})?
  uu.succeeds(r14)
  uu.stdout_is(r14, "01/19/97\n")
  let r15 = uu.invoke(s, "date", ["-d", "1997-01-19 08:17:48", "+%X"], vars: {"TZ": "UTC"})?
  uu.succeeds(r15)
  uu.stdout_is(r15, "08:17:48\n")
  let r16 = uu.invoke(s, "date", ["-d", "1997-01-19 08:17:48", "+%:8z"], vars: {"TZ": "UTC"})?
  uu.succeeds(r16)
  uu.stdout_is(r16, "%:8z\n")
}

# origin: uutils test_date::test_date_rejects_input_that_cannot_take_a_timezone
test test_uu_date_date_rejects_input_that_cannot_take_a_timezone { |ctx|
  let s = uu.scene(ctx)?
  let r17 = uu.invoke(s, "date", ["-d", "Jan 23 6:00PM GMT-1 EST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r17, 1)
  uu.stderr_contains(r17, "invalid date")
  let r18 = uu.invoke(s, "date", ["-d", "023-060 MEST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r18, 1)
  uu.stderr_contains(r18, "invalid date")
  let r19 = uu.invoke(s, "date", ["-d", "2024-01-15 12:00 EST EST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r19, 1)
  uu.stderr_contains(r19, "invalid date")
  let r20 = uu.invoke(s, "date", ["-d", "@0 EST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r20, 1)
  uu.stderr_contains(r20, "invalid date")
  let r21 = uu.invoke(s, "date", ["-d", "2024-01-15 12:00 UTC EST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r21, 1)
  uu.stderr_contains(r21, "invalid date")
  let r22 = uu.invoke(s, "date", ["-d", "2024-01-15 12:00 GMT EST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r22, 1)
  uu.stderr_contains(r22, "invalid date")
  let r23 = uu.invoke(s, "date", ["-d", "2024-01-15 12:00 +0000 EST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r23, 1)
  uu.stderr_contains(r23, "invalid date")
  let r24 = uu.invoke(s, "date", ["-d", "2024-01-15 12:00 -0500 EST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r24, 1)
  uu.stderr_contains(r24, "invalid date")
  let r25 = uu.invoke(s, "date", ["-d", "UTC 2024-01-15 12:00 EST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r25, 1)
  uu.stderr_contains(r25, "invalid date")
}

# origin: uutils test_date::test_date_relative_m9
test test_uu_date_date_relative_m9 { |ctx|
  let s = uu.scene(ctx)?
  let r26 = uu.invoke(s, "date", ["-d", "m9"], vars: {"TZ": "UTC+9"})?
  uu.succeeds(r26)
  uu.stdout_contains(r26, "12:00:00")
}

# origin: uutils test_date::test_date_resolution_no_combine
test test_uu_date_date_resolution_no_combine { |ctx|
  let s = uu.scene(ctx)?
  let r27 = uu.invoke(s, "date", ["--resolution", "-d", "2025-01-01"])?
  uu.fails(r27)
}

# origin: uutils test_date::test_date_rfc_3339_invalid_arg
test test_uu_date_date_rfc_3339_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r28 = uu.invoke(s, "date", ["--iso-3339=foo"])?
  uu.fails(r28)
  let r29 = uu.invoke(s, "date", ["--rfc-3=foo"])?
  uu.fails(r29)
}

# origin: uutils test_date::test_date_rfc_822_uses_english
test test_uu_date_date_rfc_822_uses_english { |ctx|
  let s = uu.scene(ctx)?
  let r30 = uu.invoke(s, "date", ["-R", "-d", "1997-01-19 08:17:48 +0"], vars: {"LC_ALL": "de_DE.UTF-8", "TZ": "UTC"})?
  uu.succeeds(r30)
  uu.stdout_contains(r30, "Sun, 19 Jan 1997")
  let r31 = uu.invoke(s, "date", ["-R", "-d", "1997-01-19 08:17:48 +0"], vars: {"LC_ALL": "fr_FR.UTF-8", "TZ": "UTC"})?
  uu.succeeds(r31)
  uu.stdout_contains(r31, "Sun, 19 Jan 1997")
}

# origin: uutils test_date::test_date_rfc_8601
test test_uu_date_date_rfc_8601 { |ctx|
  let s = uu.scene(ctx)?
  let r32 = uu.invoke(s, "date", ["--iso-8601=ns"])?
  uu.succeeds(r32)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2},\\d{9}[+-]\\d{2}:\\d{2}\\n$")?.matches(r32.stdout.utf8()?)
  let r33 = uu.invoke(s, "date", ["--i=ns"])?
  uu.succeeds(r33)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2},\\d{9}[+-]\\d{2}:\\d{2}\\n$")?.matches(r33.stdout.utf8()?)
}

# origin: uutils test_date::test_date_rfc_8601_date
test test_uu_date_date_rfc_8601_date { |ctx|
  let s = uu.scene(ctx)?
  let r34 = uu.invoke(s, "date", ["--iso-8601=date"])?
  uu.succeeds(r34)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}\\n$")?.matches(r34.stdout.utf8()?)
  let r35 = uu.invoke(s, "date", ["--i=date"])?
  uu.succeeds(r35)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}\\n$")?.matches(r35.stdout.utf8()?)
}

# origin: uutils test_date::test_date_rfc_8601_default
test test_uu_date_date_rfc_8601_default { |ctx|
  let s = uu.scene(ctx)?
  let r36 = uu.invoke(s, "date", ["--iso-8601"])?
  uu.succeeds(r36)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}\\n$")?.matches(r36.stdout.utf8()?)
  let r37 = uu.invoke(s, "date", ["--i"])?
  uu.succeeds(r37)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}\\n$")?.matches(r37.stdout.utf8()?)
}

# origin: uutils test_date::test_date_rfc_8601_hour
test test_uu_date_date_rfc_8601_hour { |ctx|
  let s = uu.scene(ctx)?
  let r38 = uu.invoke(s, "date", ["--iso-8601=hour"])?
  uu.succeeds(r38)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r38.stdout.utf8()?)
  let r39 = uu.invoke(s, "date", ["--iso-8601=hours"])?
  uu.succeeds(r39)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r39.stdout.utf8()?)
  let r40 = uu.invoke(s, "date", ["--i=hour"])?
  uu.succeeds(r40)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r40.stdout.utf8()?)
  let r41 = uu.invoke(s, "date", ["--i=hours"])?
  uu.succeeds(r41)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r41.stdout.utf8()?)
}

# origin: uutils test_date::test_date_rfc_8601_invalid_arg
test test_uu_date_date_rfc_8601_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r42 = uu.invoke(s, "date", ["--iso-8601=@"])?
  uu.fails(r42)
  let r43 = uu.invoke(s, "date", ["--i=@"])?
  uu.fails(r43)
}

# origin: uutils test_date::test_date_rfc_8601_minute
test test_uu_date_date_rfc_8601_minute { |ctx|
  let s = uu.scene(ctx)?
  let r44 = uu.invoke(s, "date", ["--iso-8601=minute"])?
  uu.succeeds(r44)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r44.stdout.utf8()?)
  let r45 = uu.invoke(s, "date", ["--iso-8601=minutes"])?
  uu.succeeds(r45)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r45.stdout.utf8()?)
  let r46 = uu.invoke(s, "date", ["--i=minute"])?
  uu.succeeds(r46)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r46.stdout.utf8()?)
  let r47 = uu.invoke(s, "date", ["--i=minutes"])?
  uu.succeeds(r47)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r47.stdout.utf8()?)
}

# origin: uutils test_date::test_date_rfc_8601_second
test test_uu_date_date_rfc_8601_second { |ctx|
  let s = uu.scene(ctx)?
  let r48 = uu.invoke(s, "date", ["--iso-8601=second"])?
  uu.succeeds(r48)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r48.stdout.utf8()?)
  let r49 = uu.invoke(s, "date", ["--iso-8601=seconds"])?
  uu.succeeds(r49)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r49.stdout.utf8()?)
  let r50 = uu.invoke(s, "date", ["--i=second"])?
  uu.succeeds(r50)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r50.stdout.utf8()?)
  let r51 = uu.invoke(s, "date", ["--i=seconds"])?
  uu.succeeds(r51)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}\\n$")?.matches(r51.stdout.utf8()?)
}

# origin: uutils test_date::test_date_stdin_invalid_utf8_line
test test_uu_date_date_stdin_invalid_utf8_line { |ctx|
  let s = uu.scene(ctx)?
  let r52 = uu.invoke(s, "date", ["-u", "-f", "-"], stdin: b"Hello\xffx\n")?
  uu.fails_with_code(r52, 1)
  uu.stderr_contains(r52, "date: invalid date 'Hello\\377x'")
}

# origin: uutils test_date::test_date_strftime_case_flag_on_alt_ampm
test test_uu_date_date_strftime_case_flag_on_alt_ampm { |ctx|
  let s = uu.scene(ctx)?
  let r53 = uu.invoke(s, "date", ["-d", "2024-06-15 13:45:30", "+%#P"], vars: {"LC_ALL": "C", "TZ": "UTC"})?
  uu.succeeds(r53)
  uu.stdout_is(r53, "pm\n")
}

# origin: uutils test_date::test_date_strftime_narrow_width_on_wide_default
test test_uu_date_date_strftime_narrow_width_on_wide_default { |ctx|
  let s = uu.scene(ctx)?
  let r54 = uu.invoke(s, "date", ["-d", "2024-01-01", "+%02j"], vars: {"LC_ALL": "C", "TZ": "UTC"})?
  uu.succeeds(r54)
  uu.stdout_is(r54, "01\n")
}

# origin: uutils test_date::test_date_strftime_o_modifier
test test_uu_date_date_strftime_o_modifier { |ctx|
  let s = uu.scene(ctx)?
  let r55 = uu.invoke(s, "date", ["-d", "2024-06-15", "+%Om-%Oy-%Od"], vars: {"LC_ALL": "C", "TZ": "UTC"})?
  uu.succeeds(r55)
  uu.stdout_is(r55, "06-24-15\n")
}

# origin: uutils test_date::test_date_string_human
test test_uu_date_date_string_human { |ctx|
  let s = uu.scene(ctx)?
  let r56 = uu.invoke(s, "date", ["-d", "1 year ago", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r56)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r56.stdout.utf8()?)
  let r57 = uu.invoke(s, "date", ["-d", "1 year", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r57)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r57.stdout.utf8()?)
  let r58 = uu.invoke(s, "date", ["-d", "2 months ago", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r58)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r58.stdout.utf8()?)
  let r59 = uu.invoke(s, "date", ["-d", "15 days ago", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r59)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r59.stdout.utf8()?)
  let r60 = uu.invoke(s, "date", ["-d", "1 week ago", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r60)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r60.stdout.utf8()?)
  let r61 = uu.invoke(s, "date", ["-d", "5 hours ago", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r61)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r61.stdout.utf8()?)
  let r62 = uu.invoke(s, "date", ["-d", "30 minutes ago", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r62)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r62.stdout.utf8()?)
  let r63 = uu.invoke(s, "date", ["-d", "10 seconds", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r63)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r63.stdout.utf8()?)
  let r64 = uu.invoke(s, "date", ["-d", "last day", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r64)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r64.stdout.utf8()?)
  let r65 = uu.invoke(s, "date", ["-d", "last monday", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r65)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r65.stdout.utf8()?)
  let r66 = uu.invoke(s, "date", ["-d", "last week", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r66)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r66.stdout.utf8()?)
  let r67 = uu.invoke(s, "date", ["-d", "last month", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r67)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r67.stdout.utf8()?)
  let r68 = uu.invoke(s, "date", ["-d", "last year", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r68)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r68.stdout.utf8()?)
  let r69 = uu.invoke(s, "date", ["-d", "this monday", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r69)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r69.stdout.utf8()?)
  let r70 = uu.invoke(s, "date", ["-d", "next day", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r70)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r70.stdout.utf8()?)
  let r71 = uu.invoke(s, "date", ["-d", "next monday", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r71)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r71.stdout.utf8()?)
  let r72 = uu.invoke(s, "date", ["-d", "next week", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r72)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r72.stdout.utf8()?)
  let r73 = uu.invoke(s, "date", ["-d", "next month", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r73)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r73.stdout.utf8()?)
  let r74 = uu.invoke(s, "date", ["-d", "next year", "+%Y-%m-%d %S:%M"])?
  uu.succeeds(r74)
  assert regex.compile("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}\\n$")?.matches(r74.stdout.utf8()?)
}

# origin: uutils test_date::test_date_tz_abbreviation_australian_timezones
test test_uu_date_date_tz_abbreviation_australian_timezones { |ctx|
  let s = uu.scene(ctx)?
  let awst = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 AWST", "+%Y-%m-%d %H:%M:%S"])?
  uu.fails_with_code(awst, 1)
  uu.stderr_only(awst, "date: invalid date '2021-03-20 14:53:01 AWST'\n")
  let acst = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 ACST", "+%Y-%m-%d %H:%M:%S"])?
  uu.fails_with_code(acst, 1)
  uu.stderr_only(acst, "date: invalid date '2021-03-20 14:53:01 ACST'\n")
  let acdt = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 ACDT", "+%Y-%m-%d %H:%M:%S"])?
  uu.fails_with_code(acdt, 1)
  uu.stderr_only(acdt, "date: invalid date '2021-03-20 14:53:01 ACDT'\n")
  let aest = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 AEST", "+%Y-%m-%d %H:%M:%S"])?
  uu.fails_with_code(aest, 1)
  uu.stderr_only(aest, "date: invalid date '2021-03-20 14:53:01 AEST'\n")
  let aedt = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 AEDT", "+%Y-%m-%d %H:%M:%S"])?
  uu.fails_with_code(aedt, 1)
  uu.stderr_only(aedt, "date: invalid date '2021-03-20 14:53:01 AEDT'\n")
}

# origin: uutils test_date::test_date_tz_abbreviation_dst_handling
test test_uu_date_date_tz_abbreviation_dst_handling { |ctx|
  let s = uu.scene(ctx)?
  let r80 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 PST", "+%z"])?
  uu.succeeds(r80)
  uu.no_stderr(r80)
  let r81 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 PDT", "+%z"])?
  uu.succeeds(r81)
  uu.no_stderr(r81)
}

# origin: uutils test_date::test_date_tz_abbreviation_fixed_offset_outside_season
test test_uu_date_date_tz_abbreviation_fixed_offset_outside_season { |ctx|
  let s = uu.scene(ctx)?
  let r82 = uu.invoke(s, "date", ["-u", "-d", "2026-01-15 10:00 EDT", "+%F %T %Z"], vars: {"TZ": "UTC"})?
  uu.succeeds(r82)
  uu.stdout_is(r82, "2026-01-15 14:00:00 UTC\n")
  let r83 = uu.invoke(s, "date", ["-u", "-d", "2026-06-15 10:00 PST", "+%F %T %Z"], vars: {"TZ": "UTC"})?
  uu.succeeds(r83)
  uu.stdout_is(r83, "2026-06-15 18:00:00 UTC\n")
  let r84 = uu.invoke(s, "date", ["-u", "-d", "2026-01-15 10:00 PDT", "+%F %T %Z"], vars: {"TZ": "UTC"})?
  uu.succeeds(r84)
  uu.stdout_is(r84, "2026-01-15 17:00:00 UTC\n")
  let r85 = uu.invoke(s, "date", ["-u", "-d", "2026-01-15 10:00 CDT", "+%F %T %Z"], vars: {"TZ": "UTC"})?
  uu.succeeds(r85)
  uu.stdout_is(r85, "2026-01-15 15:00:00 UTC\n")
  let r86 = uu.invoke(s, "date", ["-u", "-d", "2026-01-15 10:00 MDT", "+%F %T %Z"], vars: {"TZ": "UTC"})?
  uu.succeeds(r86)
  uu.stdout_is(r86, "2026-01-15 16:00:00 UTC\n")
  let r87 = uu.invoke(s, "date", ["-u", "-d", "2026-01-15 10:00 MEST", "+%F %T %Z"], vars: {"TZ": "UTC"})?
  uu.succeeds(r87)
  uu.stdout_is(r87, "2026-01-15 08:00:00 UTC\n")
}

# origin: uutils test_date::test_date_tz_abbreviation_unknown
test test_uu_date_date_tz_abbreviation_unknown { |ctx|
  let s = uu.scene(ctx)?
  let r88 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 XYZ"])?
  uu.fails(r88)
  uu.stderr_contains(r88, "invalid date")
}

# origin: uutils test_date::test_date_tz_abbreviation_us_timezones
test test_uu_date_date_tz_abbreviation_us_timezones { |ctx|
  let s = uu.scene(ctx)?
  let r89 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 PST", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r89)
  uu.no_stderr(r89)
  let r90 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 PDT", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r90)
  uu.no_stderr(r90)
  let r91 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 MST", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r91)
  uu.no_stderr(r91)
  let r92 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 MDT", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r92)
  uu.no_stderr(r92)
  let r93 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 CST", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r93)
  uu.no_stderr(r93)
  let r94 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 CDT", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r94)
  uu.no_stderr(r94)
  let r95 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 EST", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r95)
  uu.no_stderr(r95)
  let r96 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 EDT", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r96)
  uu.no_stderr(r96)
}

# origin: uutils test_date::test_date_tz_abbreviation_utc_gmt
test test_uu_date_date_tz_abbreviation_utc_gmt { |ctx|
  let s = uu.scene(ctx)?
  let r97 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 UTC", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r97)
  let r98 = uu.invoke(s, "date", ["-d", "2021-03-20 14:53:01 GMT", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r98)
}

# origin: uutils test_date::test_date_tz_abbreviation_with_day_of_week
test test_uu_date_date_tz_abbreviation_with_day_of_week { |ctx|
  let s = uu.scene(ctx)?
  let r99 = uu.invoke(s, "date", ["-d", "Sat 20 Mar 2021 14:53:01 AWST", "+%Y-%m-%d %H:%M:%S"])?
  uu.fails_with_code(r99, 1)
  uu.stderr_contains(r99, "invalid date")
  let r100 = uu.invoke(s, "date", ["-d", "Sat 20 Mar 2021 14:53:01 EST", "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r100)
  uu.no_stderr(r100)
}

# origin: uutils test_date::test_date_tz_with_utc_flag
test test_uu_date_date_tz_with_utc_flag { |ctx|
  let s = uu.scene(ctx)?
  let r101 = uu.invoke(s, "date", ["-u", "+%Z"], vars: {"TZ": "Europe/Berlin"})?
  uu.succeeds(r101)
  uu.stdout_only(r101, "UTC\n")
}

# origin: uutils test_date::test_date_utc
test test_uu_date_date_utc { |ctx|
  let s = uu.scene(ctx)?
  let r102 = uu.invoke(s, "date", ["--universal"])?
  uu.succeeds(r102)
  let r103 = uu.invoke(s, "date", ["--utc"])?
  uu.succeeds(r103)
  let r104 = uu.invoke(s, "date", ["--uct"])?
  uu.succeeds(r104)
  let r105 = uu.invoke(s, "date", ["--uni"])?
  uu.succeeds(r105)
  let r106 = uu.invoke(s, "date", ["--u"])?
  uu.succeeds(r106)
}

# origin: uutils test_date::test_date_utc_issue_6495
test test_uu_date_date_utc_issue_6495 { |ctx|
  let s = uu.scene(ctx)?
  let r107 = uu.invoke(s, "date", ["-u", "-d", "@0"], vars: {"TZ": "UTC0"})?
  uu.succeeds(r107)
  uu.stdout_is(r107, "Thu Jan  1 00:00:00 UTC 1970\n")
}

# origin: uutils test_date::test_date_utc_multiple_aliases
test test_uu_date_date_utc_multiple_aliases { |ctx|
  let s = uu.scene(ctx)?
  let r108 = uu.invoke(s, "date", ["--uct", "--utc", "--universal"])?
  uu.succeeds(r108)
}

# origin: uutils test_date::test_date_utc_output_formats
test test_uu_date_date_utc_output_formats { |ctx|
  let s = uu.scene(ctx)?
  let r109 = uu.invoke(s, "date", ["-u", "-d", "2024-06-15 12:00", "-I"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r109)
  uu.stdout_contains(r109, "2024-06-15")
  let r110 = uu.invoke(s, "date", ["-u", "-d", "2024-06-15 12:00", "--rfc-3339=seconds"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r110)
  uu.stdout_contains(r110, "+00:00")
  let r111 = uu.invoke(s, "date", ["-u", "-d", "2024-06-15 12:00", "-R"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r111)
  uu.stdout_contains(r111, "+0000")
}

# origin: uutils test_date::test_date_utc_stdin
test test_uu_date_date_utc_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r112 = uu.invoke(s, "date", ["-u", "-f", "-", "+%H:%M %Z"], stdin: b"2024-01-01 12:00\n2024-06-15 18:30\n", vars: {"TZ": "America/New_York"})?
  uu.succeeds(r112)
  uu.stdout_is(r112, "12:00 UTC\n18:30 UTC\n")
}

# origin: uutils test_date::test_date_utc_vs_local
test test_uu_date_date_utc_vs_local { |ctx|
  let s = uu.scene(ctx)?
  let r113 = uu.invoke(s, "date", ["-d", "2024-01-01 12:00", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r113)
  uu.stdout_is(r113, "12:00 EST\n")
  let r114 = uu.invoke(s, "date", ["-ud", "2024-01-01 12:00", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r114)
  uu.stdout_is(r114, "12:00 UTC\n")
  let r115 = uu.invoke(s, "date", ["-d", "2024-06-15 12:00", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r115)
  uu.stdout_is(r115, "12:00 EDT\n")
  let r116 = uu.invoke(s, "date", ["-ud", "2024-06-15 12:00", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r116)
  uu.stdout_is(r116, "12:00 UTC\n")
  let r117 = uu.invoke(s, "date", ["-d", "@0", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r117)
  uu.stdout_is(r117, "19:00 EST\n")
  let r118 = uu.invoke(s, "date", ["-ud", "@0", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r118)
  uu.stdout_is(r118, "00:00 UTC\n")
}

# origin: uutils test_date::test_date_utc_with_d_flag
test test_uu_date_date_utc_with_d_flag { |ctx|
  let s = uu.scene(ctx)?
  let r119 = uu.invoke(s, "date", ["-u", "-d", "2024-01-01 12:00", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r119)
  uu.stdout_is(r119, "12:00 UTC\n")
  let r120 = uu.invoke(s, "date", ["-u", "-d", "2024-06-15 10:30", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r120)
  uu.stdout_is(r120, "10:30 UTC\n")
  let r121 = uu.invoke(s, "date", ["-u", "-d", "2024-12-31 23:59:59", "+%H:%M:%S %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r121)
  uu.stdout_is(r121, "23:59:59 UTC\n")
  let r122 = uu.invoke(s, "date", ["-u", "-d", "@0", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r122)
  uu.stdout_is(r122, "1970-01-01 00:00:00 UTC\n")
  let r123 = uu.invoke(s, "date", ["-u", "-d", "@3600", "+%H:%M:%S %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r123)
  uu.stdout_is(r123, "01:00:00 UTC\n")
  let r124 = uu.invoke(s, "date", ["-u", "-d", "@86400", "+%Y-%m-%d %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r124)
  uu.stdout_is(r124, "1970-01-02 UTC\n")
  let r125 = uu.invoke(s, "date", ["-u", "-d", "2024-06-15 10:30 EDT", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r125)
  uu.stdout_is(r125, "14:30 UTC\n")
  let r126 = uu.invoke(s, "date", ["-u", "-d", "2024-01-15 10:30 EST", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r126)
  uu.stdout_is(r126, "15:30 UTC\n")
  let r127 = uu.invoke(s, "date", ["-u", "-d", "2024-06-15 12:00 PDT", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r127)
  uu.stdout_is(r127, "19:00 UTC\n")
  let r128 = uu.invoke(s, "date", ["-u", "-d", "2024-01-15 12:00 PST", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r128)
  uu.stdout_is(r128, "20:00 UTC\n")
  let r129 = uu.invoke(s, "date", ["-u", "-d", "2024-01-01 12:00 +0000", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r129)
  uu.stdout_is(r129, "12:00 UTC\n")
  let r130 = uu.invoke(s, "date", ["-u", "-d", "2024-01-01 12:00 +0530", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r130)
  uu.stdout_is(r130, "06:30 UTC\n")
  let r131 = uu.invoke(s, "date", ["-u", "-d", "2024-01-01 12:00 -0500", "+%H:%M %Z"], vars: {"TZ": "America/New_York"})?
  uu.succeeds(r131)
  uu.stdout_is(r131, "17:00 UTC\n")
}

# origin: uutils test_date::test_date_whitespace_between_items
test test_uu_date_date_whitespace_between_items { |ctx|
  let s = uu.scene(ctx)?
  let r132 = uu.invoke(s, "date", ["-d", "Jan 23\x0B 2026 1:00AM"], vars: {"LANG": "C", "LC_ALL": "C", "TZ": "UTC0"})?
  uu.succeeds(r132)
  uu.stdout_is(r132, "Fri Jan 23 01:00:00 UTC 2026\n")
  let r133 = uu.invoke(s, "date", ["-d", "Jan 23\x0C2026 1:00AM"], vars: {"LANG": "C", "LC_ALL": "C", "TZ": "UTC0"})?
  uu.succeeds(r133)
  uu.stdout_is(r133, "Fri Jan 23 01:00:00 UTC 2026\n")
}

# origin: uutils test_date::test_empty_arguments
test test_uu_date_empty_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r134 = uu.invoke(s, "date", [""])?
  uu.fails_with_code(r134, 1)
  let r135 = uu.invoke(s, "date", ["", ""])?
  uu.fails_with_code(r135, 1)
  let r136 = uu.invoke(s, "date", ["", "", ""])?
  uu.fails_with_code(r136, 1)
}

# origin: uutils test_date::test_extra_operands
test test_uu_date_extra_operands { |ctx|
  let s = uu.scene(ctx)?
  let r137 = uu.invoke(s, "date", ["test", "extra"])?
  uu.fails_with_code(r137, 1)
  uu.stderr_contains(r137, "extra operand 'extra'")
}

# origin: uutils test_date::test_format_option_not_to_capture_other_valid_arguments
test test_uu_date_format_option_not_to_capture_other_valid_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r138 = uu.invoke(s, "date", ["+%Y%m%d%H%M%S", "--date", "@1770996496"])?
  uu.succeeds(r138)
}

# origin: uutils test_date::test_invalid_arg
test test_uu_date_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r139 = uu.invoke(s, "date", ["--definitely-invalid"])?
  uu.fails_with_code(r139, 1)
}

# origin: uutils test_date::test_invalid_date_string
test test_uu_date_invalid_date_string { |ctx|
  let s = uu.scene(ctx)?
  let r140 = uu.invoke(s, "date", ["-d", "foo"])?
  uu.fails(r140)
  uu.no_stdout(r140)
  uu.stderr_contains(r140, "invalid date")
  let r141 = uu.invoke(s, "date", ["-d", "this fooday"])?
  uu.fails(r141)
  uu.no_stdout(r141)
  uu.stderr_contains(r141, "invalid date")
}

# origin: uutils test_date::test_invalid_format_string
test test_uu_date_invalid_format_string { |ctx|
  let s = uu.scene(ctx)?
  let r142 = uu.invoke(s, "date", ["+%!"])?
  uu.succeeds(r142)
  uu.stdout_is(r142, "%!\n")
}

# origin: uutils test_date::test_invalid_format_string_with_too_many_colons
test test_uu_date_invalid_format_string_with_too_many_colons { |ctx|
  let s = uu.scene(ctx)?
  let r143 = uu.invoke(s, "date", ["+%_::::z"])?
  uu.succeeds(r143)
  uu.stdout_is(r143, "%_::::z\n")
}

# origin: uutils test_date::test_invalid_long_option
test test_uu_date_invalid_long_option { |ctx|
  let s = uu.scene(ctx)?
  let r144 = uu.invoke(s, "date", ["--fB"])?
  uu.fails_with_code(r144, 1)
  uu.stderr_contains(r144, "unrecognized option '--fB'")
}

# origin: uutils test_date::test_invalid_short_option
test test_uu_date_invalid_short_option { |ctx|
  let s = uu.scene(ctx)?
  let r145 = uu.invoke(s, "date", ["-w"])?
  uu.fails_with_code(r145, 1)
  uu.stderr_contains(r145, "invalid option -- 'w'")
}

# origin: uutils test_date::test_korean_time_zone
test test_uu_date_korean_time_zone { |ctx|
  let s = uu.scene(ctx)?
  let r146 = uu.invoke(s, "date", ["-u", "-d", "2026-01-15 10:00 KST", "+%F %T %Z"], vars: {"TZ": "UTC"})?
  uu.succeeds(r146)
  uu.stdout_is(r146, "2026-01-15 01:00:00 UTC\n")
}

# origin: uutils test_date::test_large_year_default_output
test test_uu_date_large_year_default_output { |ctx|
  let s = uu.scene(ctx)?
  let r147 = uu.invoke(s, "date", ["-d", "18978-01-01"], vars: {"LANG": "C", "LC_ALL": "C", "TZ": "UTC0"})?
  uu.succeeds(r147)
  uu.stdout_is(r147, "Thu Jan  1 00:00:00 UTC 18978\n")
}

# origin: uutils test_date::test_large_year_default_output_boundary
test test_uu_date_large_year_default_output_boundary { |ctx|
  let s = uu.scene(ctx)?
  let r149 = uu.invoke(s, "date", ["-d", "9999-01-01"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.succeeds(r149)
  uu.stdout_is(r149, "Fri Jan  1 00:00:00 UTC 9999\n")
  let r150 = uu.invoke(s, "date", ["-d", "10000-01-01"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.succeeds(r150)
  uu.stdout_is(r150, "Sat Jan  1 00:00:00 UTC 10000\n")
  let r151 = uu.invoke(s, "date", ["-d", "10000-01-01 00:00 +1400"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.succeeds(r151)
  uu.stdout_is(r151, "Fri Dec 31 10:00:00 UTC 9999\n")
  let r152 = uu.invoke(s, "date", ["-d", "9999-12-31 23:00 -1400"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.succeeds(r152)
  uu.stdout_is(r152, "Sat Jan  1 13:00:00 UTC 10000\n")
  let r148 = uu.invoke(s, "date", ["-d", "10000-02-30"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r148, 1)
  uu.stderr_contains(r148, "invalid date")
}

# origin: uutils test_date::test_locale_abbreviated_month_names
test test_uu_date_locale_abbreviated_month_names { |ctx|
  let s = uu.scene(ctx)?
  let r153 = uu.invoke(s, "date", ["-d", "2026-02-12", "+%b"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r153)
  assert r153.stdout.utf8()?.trim() == "févr."
  let r154 = uu.invoke(s, "date", ["-d", "2026-06-14", "+%b"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r154)
  assert r154.stdout.utf8()?.trim() == "juin"
  let r155 = uu.invoke(s, "date", ["-d", "2026-12-09", "+%b"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r155)
  assert r155.stdout.utf8()?.trim() == "déc."
  let r156 = uu.invoke(s, "date", ["-d", "2026-02-12", "+%b"], vars: {"LC_ALL": "de_DE.UTF-8"})?
  uu.succeeds(r156)
  assert r156.stdout.utf8()?.trim() == "Feb"
  let r157 = uu.invoke(s, "date", ["-d", "2026-06-14", "+%b"], vars: {"LC_ALL": "de_DE.UTF-8"})?
  uu.succeeds(r157)
  assert r157.stdout.utf8()?.trim() == "Jun"
  let r158 = uu.invoke(s, "date", ["-d", "2026-12-09", "+%b"], vars: {"LC_ALL": "de_DE.UTF-8"})?
  uu.succeeds(r158)
  assert r158.stdout.utf8()?.trim() == "Dez"
  let r159 = uu.invoke(s, "date", ["-d", "2026-02-12", "+%b"], vars: {"LC_ALL": "es_ES.UTF-8"})?
  uu.succeeds(r159)
  assert r159.stdout.utf8()?.trim() == "feb"
  let r160 = uu.invoke(s, "date", ["-d", "2026-06-14", "+%b"], vars: {"LC_ALL": "es_ES.UTF-8"})?
  uu.succeeds(r160)
  assert r160.stdout.utf8()?.trim() == "jun"
  let r161 = uu.invoke(s, "date", ["-d", "2026-12-09", "+%b"], vars: {"LC_ALL": "es_ES.UTF-8"})?
  uu.succeeds(r161)
  assert r161.stdout.utf8()?.trim() == "dic"
  let r162 = uu.invoke(s, "date", ["-d", "2026-02-12", "+%b"], vars: {"LC_ALL": "it_IT.UTF-8"})?
  uu.succeeds(r162)
  assert r162.stdout.utf8()?.trim() == "feb"
  let r163 = uu.invoke(s, "date", ["-d", "2026-06-14", "+%b"], vars: {"LC_ALL": "it_IT.UTF-8"})?
  uu.succeeds(r163)
  assert r163.stdout.utf8()?.trim() == "giu"
  let r164 = uu.invoke(s, "date", ["-d", "2026-12-09", "+%b"], vars: {"LC_ALL": "it_IT.UTF-8"})?
  uu.succeeds(r164)
  assert r164.stdout.utf8()?.trim() == "dic"
  let r165 = uu.invoke(s, "date", ["-d", "2026-02-12", "+%b"], vars: {"LC_ALL": "pt_BR.UTF-8"})?
  uu.succeeds(r165)
  assert r165.stdout.utf8()?.trim() == "fev"
  let r166 = uu.invoke(s, "date", ["-d", "2026-06-14", "+%b"], vars: {"LC_ALL": "pt_BR.UTF-8"})?
  uu.succeeds(r166)
  assert r166.stdout.utf8()?.trim() == "jun"
  let r167 = uu.invoke(s, "date", ["-d", "2026-12-09", "+%b"], vars: {"LC_ALL": "pt_BR.UTF-8"})?
  uu.succeeds(r167)
  assert r167.stdout.utf8()?.trim() == "dez"
  let r168 = uu.invoke(s, "date", ["-d", "2026-02-12", "+%b"], vars: {"LC_ALL": "ja_JP.UTF-8"})?
  uu.succeeds(r168)
  assert r168.stdout.utf8()?.trim() == "2月"
  let r169 = uu.invoke(s, "date", ["-d", "2026-06-14", "+%b"], vars: {"LC_ALL": "ja_JP.UTF-8"})?
  uu.succeeds(r169)
  assert r169.stdout.utf8()?.trim() == "6月"
  let r170 = uu.invoke(s, "date", ["-d", "2026-12-09", "+%b"], vars: {"LC_ALL": "ja_JP.UTF-8"})?
  uu.succeeds(r170)
  assert r170.stdout.utf8()?.trim() == "12月"
  let r171 = uu.invoke(s, "date", ["-d", "2026-02-12", "+%b"], vars: {"LC_ALL": "zh_CN.UTF-8"})?
  uu.succeeds(r171)
  assert r171.stdout.utf8()?.trim() == "2月"
  let r172 = uu.invoke(s, "date", ["-d", "2026-06-14", "+%b"], vars: {"LC_ALL": "zh_CN.UTF-8"})?
  uu.succeeds(r172)
  assert r172.stdout.utf8()?.trim() == "6月"
  let r173 = uu.invoke(s, "date", ["-d", "2026-12-09", "+%b"], vars: {"LC_ALL": "zh_CN.UTF-8"})?
  uu.succeeds(r173)
  assert r173.stdout.utf8()?.trim() == "12月"
  let r174 = uu.invoke(s, "date", ["-d", "2026-02-12", "+%b"], vars: {"LC_ALL": "hu_HU.UTF-8"})?
  uu.succeeds(r174)
  assert r174.stdout.utf8()?.trim() == "febr"
  let r175 = uu.invoke(s, "date", ["-d", "2026-06-14", "+%b"], vars: {"LC_ALL": "hu_HU.UTF-8"})?
  uu.succeeds(r175)
  assert r175.stdout.utf8()?.trim() == "jún"
  let r176 = uu.invoke(s, "date", ["-d", "2026-12-09", "+%b"], vars: {"LC_ALL": "hu_HU.UTF-8"})?
  uu.succeeds(r176)
  assert r176.stdout.utf8()?.trim() == "dec"
}

# origin: uutils test_date::test_locale_calendar_conversions
test test_uu_date_locale_calendar_conversions { |ctx|
  let s = uu.scene(ctx)?
  let r194 = uu.invoke(s, "date", ["-d", "2026-01-01", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r194)
  assert r194.stdout.utf8()?.trim() == "1404-10-11"
  let r195 = uu.invoke(s, "date", ["-d", "2026-01-26", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r195)
  assert r195.stdout.utf8()?.trim() == "1404-11-06"
  let r196 = uu.invoke(s, "date", ["-d", "2026-03-20", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r196)
  assert r196.stdout.utf8()?.trim() == "1404-12-29"
  let r197 = uu.invoke(s, "date", ["-d", "2026-03-21", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r197)
  assert r197.stdout.utf8()?.trim() == "1405-01-01"
  let r198 = uu.invoke(s, "date", ["-d", "2026-03-22", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r198)
  assert r198.stdout.utf8()?.trim() == "1405-01-02"
  let r199 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r199)
  assert r199.stdout.utf8()?.trim() == "1405-03-25"
  let r200 = uu.invoke(s, "date", ["-d", "2026-12-31", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r200)
  assert r200.stdout.utf8()?.trim() == "1405-10-10"
  let r201 = uu.invoke(s, "date", ["-d", "2025-03-20", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r201)
  assert r201.stdout.utf8()?.trim() == "1403-12-30"
  let r202 = uu.invoke(s, "date", ["-d", "2025-03-21", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r202)
  assert r202.stdout.utf8()?.trim() == "1404-01-01"
  let r203 = uu.invoke(s, "date", ["-d", "2024-03-19", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r203)
  assert r203.stdout.utf8()?.trim() == "1402-12-29"
  let r204 = uu.invoke(s, "date", ["-d", "2024-03-20", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r204)
  assert r204.stdout.utf8()?.trim() == "1403-01-01"
  let r205 = uu.invoke(s, "date", ["-d", "2000-03-20", "+%Y-%m-%d"], vars: {"LC_ALL": "fa_IR.UTF-8"})?
  uu.succeeds(r205)
  assert r205.stdout.utf8()?.trim() == "1379-01-01"
  let r186 = uu.invoke(s, "date", ["-d", "2026-01-01", "+%Y-%m-%d"], vars: {"LC_ALL": "th_TH.UTF-8"})?
  uu.succeeds(r186)
  assert r186.stdout.utf8()?.trim() == "2569-01-01"
  let r187 = uu.invoke(s, "date", ["-d", "2026-01-26", "+%Y-%m-%d"], vars: {"LC_ALL": "th_TH.UTF-8"})?
  uu.succeeds(r187)
  assert r187.stdout.utf8()?.trim() == "2569-01-26"
  let r188 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%Y-%m-%d"], vars: {"LC_ALL": "th_TH.UTF-8"})?
  uu.succeeds(r188)
  assert r188.stdout.utf8()?.trim() == "2569-06-15"
  let r189 = uu.invoke(s, "date", ["-d", "2026-12-31", "+%Y-%m-%d"], vars: {"LC_ALL": "th_TH.UTF-8"})?
  uu.succeeds(r189)
  assert r189.stdout.utf8()?.trim() == "2569-12-31"
  let r190 = uu.invoke(s, "date", ["-d", "2025-01-01", "+%Y-%m-%d"], vars: {"LC_ALL": "th_TH.UTF-8"})?
  uu.succeeds(r190)
  assert r190.stdout.utf8()?.trim() == "2568-01-01"
  let r191 = uu.invoke(s, "date", ["-d", "2024-02-29", "+%Y-%m-%d"], vars: {"LC_ALL": "th_TH.UTF-8"})?
  uu.succeeds(r191)
  assert r191.stdout.utf8()?.trim() == "2567-02-29"
  let r192 = uu.invoke(s, "date", ["-d", "2000-01-01", "+%Y-%m-%d"], vars: {"LC_ALL": "th_TH.UTF-8"})?
  uu.succeeds(r192)
  assert r192.stdout.utf8()?.trim() == "2543-01-01"
  let r193 = uu.invoke(s, "date", ["-d", "1970-01-01", "+%Y-%m-%d"], vars: {"LC_ALL": "th_TH.UTF-8"})?
  uu.succeeds(r193)
  assert r193.stdout.utf8()?.trim() == "2513-01-01"
  let r177 = uu.invoke(s, "date", ["-d", "2026-01-01", "+%Y-%m-%d"], vars: {"LC_ALL": "am_ET.UTF-8"})?
  uu.succeeds(r177)
  assert r177.stdout.utf8()?.trim() == "2018-04-23"
  let r178 = uu.invoke(s, "date", ["-d", "2026-01-26", "+%Y-%m-%d"], vars: {"LC_ALL": "am_ET.UTF-8"})?
  uu.succeeds(r178)
  assert r178.stdout.utf8()?.trim() == "2018-05-18"
  let r179 = uu.invoke(s, "date", ["-d", "2026-09-10", "+%Y-%m-%d"], vars: {"LC_ALL": "am_ET.UTF-8"})?
  uu.succeeds(r179)
  assert r179.stdout.utf8()?.trim() == "2018-13-05"
  let r180 = uu.invoke(s, "date", ["-d", "2026-09-11", "+%Y-%m-%d"], vars: {"LC_ALL": "am_ET.UTF-8"})?
  uu.succeeds(r180)
  assert r180.stdout.utf8()?.trim() == "2019-01-01"
  let r181 = uu.invoke(s, "date", ["-d", "2026-09-12", "+%Y-%m-%d"], vars: {"LC_ALL": "am_ET.UTF-8"})?
  uu.succeeds(r181)
  assert r181.stdout.utf8()?.trim() == "2019-01-02"
  let r182 = uu.invoke(s, "date", ["-d", "2026-12-31", "+%Y-%m-%d"], vars: {"LC_ALL": "am_ET.UTF-8"})?
  uu.succeeds(r182)
  assert r182.stdout.utf8()?.trim() == "2019-04-22"
  let r183 = uu.invoke(s, "date", ["-d", "2025-09-11", "+%Y-%m-%d"], vars: {"LC_ALL": "am_ET.UTF-8"})?
  uu.succeeds(r183)
  assert r183.stdout.utf8()?.trim() == "2018-01-01"
  let r184 = uu.invoke(s, "date", ["-d", "2025-09-10", "+%Y-%m-%d"], vars: {"LC_ALL": "am_ET.UTF-8"})?
  uu.succeeds(r184)
  assert r184.stdout.utf8()?.trim() == "2017-13-05"
  let r185 = uu.invoke(s, "date", ["-d", "2000-09-11", "+%Y-%m-%d"], vars: {"LC_ALL": "am_ET.UTF-8"})?
  uu.succeeds(r185)
  assert r185.stdout.utf8()?.trim() == "1993-01-01"
}

# origin: uutils test_date::test_locale_day_names
test test_uu_date_locale_day_names { |ctx|
  let s = uu.scene(ctx)?
  let r206 = uu.invoke(s, "date", ["-d", "2026-01-26", "+%A"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r206)
  assert r206.stdout.utf8()?.trim() == "lundi"
  let r207 = uu.invoke(s, "date", ["-d", "2026-01-25", "+%A"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r207)
  assert r207.stdout.utf8()?.trim() == "dimanche"
  let r208 = uu.invoke(s, "date", ["-d", "2026-01-24", "+%A"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r208)
  assert r208.stdout.utf8()?.trim() == "samedi"
  let r209 = uu.invoke(s, "date", ["-d", "2026-01-26", "+%A"], vars: {"LC_ALL": "de_DE.UTF-8"})?
  uu.succeeds(r209)
  assert r209.stdout.utf8()?.trim() == "Montag"
  let r210 = uu.invoke(s, "date", ["-d", "2026-01-25", "+%A"], vars: {"LC_ALL": "de_DE.UTF-8"})?
  uu.succeeds(r210)
  assert r210.stdout.utf8()?.trim() == "Sonntag"
  let r211 = uu.invoke(s, "date", ["-d", "2026-01-24", "+%A"], vars: {"LC_ALL": "de_DE.UTF-8"})?
  uu.succeeds(r211)
  assert r211.stdout.utf8()?.trim() == "Samstag"
  let r212 = uu.invoke(s, "date", ["-d", "2026-01-26", "+%A"], vars: {"LC_ALL": "es_ES.UTF-8"})?
  uu.succeeds(r212)
  assert r212.stdout.utf8()?.trim() == "lunes"
  let r213 = uu.invoke(s, "date", ["-d", "2026-01-25", "+%A"], vars: {"LC_ALL": "es_ES.UTF-8"})?
  uu.succeeds(r213)
  assert r213.stdout.utf8()?.trim() == "domingo"
  let r214 = uu.invoke(s, "date", ["-d", "2026-01-24", "+%A"], vars: {"LC_ALL": "es_ES.UTF-8"})?
  uu.succeeds(r214)
  assert r214.stdout.utf8()?.trim() == "sábado"
  let r215 = uu.invoke(s, "date", ["-d", "2026-01-26", "+%A"], vars: {"LC_ALL": "ja_JP.UTF-8"})?
  uu.succeeds(r215)
  assert r215.stdout.utf8()?.trim() == "月曜日"
  let r216 = uu.invoke(s, "date", ["-d", "2026-01-25", "+%A"], vars: {"LC_ALL": "ja_JP.UTF-8"})?
  uu.succeeds(r216)
  assert r216.stdout.utf8()?.trim() == "日曜日"
  let r217 = uu.invoke(s, "date", ["-d", "2026-01-24", "+%A"], vars: {"LC_ALL": "ja_JP.UTF-8"})?
  uu.succeeds(r217)
  assert r217.stdout.utf8()?.trim() == "土曜日"
  let r218 = uu.invoke(s, "date", ["-d", "2026-01-26", "+%A"], vars: {"LC_ALL": "zh_CN.UTF-8"})?
  uu.succeeds(r218)
  assert r218.stdout.utf8()?.trim() == "星期一"
  let r219 = uu.invoke(s, "date", ["-d", "2026-01-25", "+%A"], vars: {"LC_ALL": "zh_CN.UTF-8"})?
  uu.succeeds(r219)
  assert r219.stdout.utf8()?.trim() == "星期日"
  let r220 = uu.invoke(s, "date", ["-d", "2026-01-24", "+%A"], vars: {"LC_ALL": "zh_CN.UTF-8"})?
  uu.succeeds(r220)
  assert r220.stdout.utf8()?.trim() == "星期六"
}

# origin: uutils test_date::test_locale_month_names
test test_uu_date_locale_month_names { |ctx|
  let s = uu.scene(ctx)?
  let r221 = uu.invoke(s, "date", ["-d", "2026-01-15", "+%B"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r221)
  assert r221.stdout.utf8()?.trim() == "janvier"
  let r222 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%B"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r222)
  assert r222.stdout.utf8()?.trim() == "juin"
  let r223 = uu.invoke(s, "date", ["-d", "2026-12-15", "+%B"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r223)
  assert r223.stdout.utf8()?.trim() == "décembre"
  let r224 = uu.invoke(s, "date", ["-d", "2026-01-15", "+%B"], vars: {"LC_ALL": "de_DE.UTF-8"})?
  uu.succeeds(r224)
  assert r224.stdout.utf8()?.trim() == "Januar"
  let r225 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%B"], vars: {"LC_ALL": "de_DE.UTF-8"})?
  uu.succeeds(r225)
  assert r225.stdout.utf8()?.trim() == "Juni"
  let r226 = uu.invoke(s, "date", ["-d", "2026-12-15", "+%B"], vars: {"LC_ALL": "de_DE.UTF-8"})?
  uu.succeeds(r226)
  assert r226.stdout.utf8()?.trim() == "Dezember"
  let r227 = uu.invoke(s, "date", ["-d", "2026-01-15", "+%B"], vars: {"LC_ALL": "es_ES.UTF-8"})?
  uu.succeeds(r227)
  assert r227.stdout.utf8()?.trim() == "enero"
  let r228 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%B"], vars: {"LC_ALL": "es_ES.UTF-8"})?
  uu.succeeds(r228)
  assert r228.stdout.utf8()?.trim() == "junio"
  let r229 = uu.invoke(s, "date", ["-d", "2026-12-15", "+%B"], vars: {"LC_ALL": "es_ES.UTF-8"})?
  uu.succeeds(r229)
  assert r229.stdout.utf8()?.trim() == "diciembre"
  let r230 = uu.invoke(s, "date", ["-d", "2026-01-15", "+%B"], vars: {"LC_ALL": "it_IT.UTF-8"})?
  uu.succeeds(r230)
  assert r230.stdout.utf8()?.trim() == "gennaio"
  let r231 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%B"], vars: {"LC_ALL": "it_IT.UTF-8"})?
  uu.succeeds(r231)
  assert r231.stdout.utf8()?.trim() == "giugno"
  let r232 = uu.invoke(s, "date", ["-d", "2026-12-15", "+%B"], vars: {"LC_ALL": "it_IT.UTF-8"})?
  uu.succeeds(r232)
  assert r232.stdout.utf8()?.trim() == "dicembre"
  let r233 = uu.invoke(s, "date", ["-d", "2026-01-15", "+%B"], vars: {"LC_ALL": "pt_BR.UTF-8"})?
  uu.succeeds(r233)
  assert r233.stdout.utf8()?.trim() == "janeiro"
  let r234 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%B"], vars: {"LC_ALL": "pt_BR.UTF-8"})?
  uu.succeeds(r234)
  assert r234.stdout.utf8()?.trim() == "junho"
  let r235 = uu.invoke(s, "date", ["-d", "2026-12-15", "+%B"], vars: {"LC_ALL": "pt_BR.UTF-8"})?
  uu.succeeds(r235)
  assert r235.stdout.utf8()?.trim() == "dezembro"
  let r236 = uu.invoke(s, "date", ["-d", "2026-01-15", "+%B"], vars: {"LC_ALL": "hu_HU.UTF-8"})?
  uu.succeeds(r236)
  assert r236.stdout.utf8()?.trim() == "január"
  let r237 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%B"], vars: {"LC_ALL": "hu_HU.UTF-8"})?
  uu.succeeds(r237)
  assert r237.stdout.utf8()?.trim() == "június"
  let r238 = uu.invoke(s, "date", ["-d", "2026-12-15", "+%B"], vars: {"LC_ALL": "hu_HU.UTF-8"})?
  uu.succeeds(r238)
  assert r238.stdout.utf8()?.trim() == "december"
  let r239 = uu.invoke(s, "date", ["-d", "2026-01-15", "+%B"], vars: {"LC_ALL": "ja_JP.UTF-8"})?
  uu.succeeds(r239)
  assert r239.stdout.utf8()?.trim() == "1月"
  let r240 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%B"], vars: {"LC_ALL": "ja_JP.UTF-8"})?
  uu.succeeds(r240)
  assert r240.stdout.utf8()?.trim() == "6月"
  let r241 = uu.invoke(s, "date", ["-d", "2026-12-15", "+%B"], vars: {"LC_ALL": "ja_JP.UTF-8"})?
  uu.succeeds(r241)
  assert r241.stdout.utf8()?.trim() == "12月"
  let r242 = uu.invoke(s, "date", ["-d", "2026-01-15", "+%B"], vars: {"LC_ALL": "zh_CN.UTF-8"})?
  uu.succeeds(r242)
  assert r242.stdout.utf8()?.trim() == "一月"
  let r243 = uu.invoke(s, "date", ["-d", "2026-06-15", "+%B"], vars: {"LC_ALL": "zh_CN.UTF-8"})?
  uu.succeeds(r243)
  assert r243.stdout.utf8()?.trim() == "六月"
  let r244 = uu.invoke(s, "date", ["-d", "2026-12-15", "+%B"], vars: {"LC_ALL": "zh_CN.UTF-8"})?
  uu.succeeds(r244)
  assert r244.stdout.utf8()?.trim() == "十二月"
}

# origin: uutils test_date::test_multiple_dates
test test_uu_date_multiple_dates { |ctx|
  let s = uu.scene(ctx)?
  let r245 = uu.invoke(s, "date", ["-d", "invalid", "-d", "2000-02-02", "+%Y"])?
  uu.succeeds(r245)
  uu.stdout_is(r245, "2000\n")
  uu.no_stderr(r245)
}

# origin: uutils test_date::test_date_set_echo_honors_format_and_utc
test test_uu_date_date_set_echo_honors_format_and_utc { |ctx|
  let s = uu.scene(ctx)?
  let r246 = uu.invoke(s, "date", ["--set", "2020-03-12 13:30:00+08:00", "+%F %T %Z"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails(r246)
  uu.stdout_is(r246, "2020-03-12 05:30:00 UTC\n")
  let r247 = uu.invoke(s, "date", ["-u", "--set", "2020-03-12 13:30:00+08:00"], vars: {"LC_ALL": "C", "TZ": "Europe/Helsinki"})?
  uu.fails(r247)
  uu.stdout_is(r247, "Thu Mar 12 05:30:00 UTC 2020\n")
}

# origin: uutils test_date::test_date_set_permissions_error
test test_uu_date_date_set_permissions_error { |ctx|
  let s = uu.scene(ctx)?
  let r248 = uu.invoke(s, "date", ["--set", "2020-03-11 21:45:00+08:00"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails(r248)
  uu.stdout_is(r248, "Wed Mar 11 13:45:00 UTC 2020\n")
  assert r248.stderr.utf8()?.starts_with("date: cannot set date: ")
}

# origin: uutils test_date::test_date_set_invalid
test test_uu_date_date_set_invalid { |ctx|
  let s = uu.scene(ctx)?
  let r249 = uu.invoke(s, "date", ["--set", "123abcd"], vars: {})?
  uu.fails(r249)
  uu.no_stdout(r249)
  assert r249.stderr.utf8()?.starts_with("date: invalid date ")
}

# origin: uutils test_date::test_date_set_hyphen_prefixed_values
test test_uu_date_date_set_hyphen_prefixed_values { |ctx|
  let s = uu.scene(ctx)?
  let r250 = uu.invoke(s, "date", ["--set", "-1 hour"], vars: {"LC_ALL":"C", "TZ":"UTC0"})?
  uu.fails(r250)
  assert regex.compile("^\\w{3} \\w{3} {1,2}\\d{1,2} \\d{2}:\\d{2}:\\d{2} UTC \\d{4}\n$")?.matches(r250.stdout.utf8()?)
  assert r250.stderr.utf8()?.starts_with("date: cannot set date: ")
  let r251 = uu.invoke(s, "date", ["--set", "-2 days"], vars: {"LC_ALL":"C", "TZ":"UTC0"})?
  uu.fails(r251)
  assert regex.compile("^\\w{3} \\w{3} {1,2}\\d{1,2} \\d{2}:\\d{2}:\\d{2} UTC \\d{4}\n$")?.matches(r251.stdout.utf8()?)
  assert r251.stderr.utf8()?.starts_with("date: cannot set date: ")
  let r252 = uu.invoke(s, "date", ["--set", "-3 weeks"], vars: {"LC_ALL":"C", "TZ":"UTC0"})?
  uu.fails(r252)
  assert regex.compile("^\\w{3} \\w{3} {1,2}\\d{1,2} \\d{2}:\\d{2}:\\d{2} UTC \\d{4}\n$")?.matches(r252.stdout.utf8()?)
  assert r252.stderr.utf8()?.starts_with("date: cannot set date: ")
  let r253 = uu.invoke(s, "date", ["--set", "-1 month"], vars: {"LC_ALL":"C", "TZ":"UTC0"})?
  uu.fails(r253)
  assert regex.compile("^\\w{3} \\w{3} {1,2}\\d{1,2} \\d{2}:\\d{2}:\\d{2} UTC \\d{4}\n$")?.matches(r253.stdout.utf8()?)
  assert r253.stderr.utf8()?.starts_with("date: cannot set date: ")
}

# origin: uutils test_date::test_format_percent_before_non_utf8_byte
test test_uu_date_format_percent_before_non_utf8_byte { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "date", [p"-u", p"-d", p"2031-07-23T04:05:06", Path.parse_bytes(b"+w%\xd0z")?])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"w%\xd0z\n")
}

# origin: uutils test_date::test_format_with_non_utf8_bytes
test test_uu_date_format_with_non_utf8_bytes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "date", [p"-u", p"-d", p"2031-07-23T04:05:06", Path.parse_bytes(b"+\xc5[%Y]\xa7%%\xe4")?])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"\xc5[2031]\xa7%\xe4\n")
}

# origin: uutils test_date::test_format_with_gb18030_bytes
test test_uu_date_format_with_gb18030_bytes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "date", [p"-u", p"-d", p"2031-07-23T04:05:06", Path.parse_bytes(b"+%Y\xc4\xea%-m\xd4\xc2%-d\xc8\xd5")?])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"2031\xc4\xea7\xd4\xc223\xc8\xd5\n")
}

# origin: uutils test_date::test_date_parse_from_format
test test_uu_date_date_parse_from_format { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file-with-dates", "2023-03-27 08:30:00\n2023-04-01 12:00:00\n2023-04-15 18:30:00")?
  let r = uu.invoke(s, "date", ["-f", uu.at(s, "file-with-dates").display(), "+%Y-%m-%d %H:%M:%S"])?
  uu.succeeds(r)
}

# origin: uutils test_date::test_date_reference_is_non_utf8_path
test test_uu_date_date_reference_is_non_utf8_path { |ctx|
  let s = uu.scene(ctx)?
  let file = uu.at_bytes(s, b"reference_\xff\xfe.txt")?
  file.write("")?
  fs.set_times(file, mtime_sec: 1234)?
  let r = uu.invoke_paths(s, "date", [p"--reference", Path.parse_bytes(b"reference_\xff\xfe.txt")?, p"+%s"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1234\n")
}

# origin: uutils test_date::test_date_resolution
test test_uu_date_date_resolution { |ctx|
  let s = uu.scene(ctx)?
  for args in [["--resolution"], ["--resolution", "--resolution"]] {
    let r = uu.invoke(s, "date", args)?
    uu.succeeds(r)
    assert r.stdout.utf8()?.trim().parse_float() is Ok(_)
  }
  let r = uu.invoke(s, "date", ["--resolution", "-Iseconds"])?
  uu.succeeds(r)
  uu.stdout_only(r, "1970-01-01T00:00:00+00:00\n")
}

# origin: uutils test_date::test_date_rfc_3339
test test_uu_date_date_rfc_3339 { |ctx|
  let s = uu.scene(ctx)?
  for param in ["--rfc-3339", "--rfc-3"] {
    for precision in ["ns", "seconds"] {
      let r = uu.invoke(s, "date", [f"{param}={precision}"])?
      uu.succeeds(r)
      assert rx"(\d+)-(0[1-9]|1[012])-(0[1-9]|[12]\d|3[01])\s([01]\d|2[0-3]):([0-5]\d):([0-5]\d|60)(\.\d+)?(([Zz])|([\+|\-]([01]\d|2[0-3])))".matches(r.stdout.utf8()?)
    }
  }
}

# origin: uutils test_date::test_date_tz
test test_uu_date_date_tz { |ctx|
  let s = uu.scene(ctx)?
  let r254 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": ""})?
  uu.succeeds(r254)
  uu.stdout_only(r254, "2024-01-02 12:00:00 Universal\n")
  let r255 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "UTC0"})?
  uu.succeeds(r255)
  uu.stdout_only(r255, "2024-01-02 12:00:00 UTC\n")
  let r256 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "America/Vancouver"})?
  uu.succeeds(r256)
  uu.stdout_only(r256, "2024-01-02 04:00:00 PST\n")
  let r257 = uu.invoke(s, "date", ["-d", "2024-07-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "America/Vancouver"})?
  uu.succeeds(r257)
  uu.stdout_only(r257, "2024-07-02 05:00:00 PDT\n")
  let r258 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Europe/Berlin"})?
  uu.succeeds(r258)
  uu.stdout_only(r258, "2024-01-02 13:00:00 CET\n")
  let r259 = uu.invoke(s, "date", ["-d", "2024-07-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Europe/Berlin"})?
  uu.succeeds(r259)
  uu.stdout_only(r259, "2024-07-02 14:00:00 CEST\n")
  let r260 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Africa/Cairo"})?
  uu.succeeds(r260)
  uu.stdout_only(r260, "2024-01-02 14:00:00 EET\n")
  let r261 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Asia/Tokyo"})?
  uu.succeeds(r261)
  uu.stdout_only(r261, "2024-01-02 21:00:00 JST\n")
  let r262 = uu.invoke(s, "date", ["-d", "2024-07-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Asia/Tokyo"})?
  uu.succeeds(r262)
  uu.stdout_only(r262, "2024-07-02 21:00:00 JST\n")
  let r263 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Australia/Sydney"})?
  uu.succeeds(r263)
  uu.stdout_only(r263, "2024-01-02 23:00:00 AEDT\n")
  let r264 = uu.invoke(s, "date", ["-d", "2024-07-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Australia/Sydney"})?
  uu.succeeds(r264)
  uu.stdout_only(r264, "2024-07-02 22:00:00 AEST\n")
  let r265 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Pacific/Tahiti"})?
  uu.succeeds(r265)
  uu.stdout_only(r265, "2024-01-02 02:00:00 -10\n")
  let r266 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Pacific/Auckland"})?
  uu.succeeds(r266)
  uu.stdout_only(r266, "2024-01-03 01:00:00 NZDT\n")
  let r267 = uu.invoke(s, "date", ["-d", "2024-07-02 12:00:00 +0000", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "Pacific/Auckland"})?
  uu.succeeds(r267)
  uu.stdout_only(r267, "2024-07-03 00:00:00 NZST\n")
}

# origin: uutils test_date::test_date_tz_various_formats
test test_uu_date_date_tz_various_formats { |ctx|
  let s = uu.scene(ctx)?
  let r268 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%z %:z %::z %:::z %Z"], vars: {"TZ": "America/Vancouver"})?
  uu.succeeds(r268)
  uu.stdout_only(r268, "-0800 -08:00 -08:00:00 -08 PST\n")
  let r269 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%z %:z %::z %:::z %Z"], vars: {"TZ": "Asia/Kolkata"})?
  uu.succeeds(r269)
  uu.stdout_only(r269, "+0530 +05:30 +05:30:00 +05:30 IST\n")
  let r270 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%z %:z %::z %:::z %Z"], vars: {"TZ": "Europe/Berlin"})?
  uu.succeeds(r270)
  uu.stdout_only(r270, "+0100 +01:00 +01:00:00 +01 CET\n")
  let r271 = uu.invoke(s, "date", ["-d", "2024-01-02 12:00:00 +0000", "+%z %:z %::z %:::z %Z"], vars: {"TZ": "Australia/Sydney"})?
  uu.succeeds(r271)
  uu.stdout_only(r271, "+1100 +11:00 +11:00:00 +11 AEDT\n")
}

# origin: uutils test_date::test_date_tz_with_relative_time
test test_uu_date_date_tz_with_relative_time { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["-d", "1 hour ago", "+%Y-%m-%d %H:%M:%S %Z"], vars: {"TZ": "America/Vancouver"})?
  uu.succeeds(r)
  assert rx"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} P[DS]T\n$".matches(r.stdout.utf8()?)
}

# origin: uutils test_date::test_date_tz_abbreviation_with_relative_date
test test_uu_date_date_tz_abbreviation_with_relative_date { |ctx|
  let s = uu.scene(ctx)?
  let expected = uu.invoke(s, "date", ["-u", "-d", "yesterday 10:00 GMT", "+%F %T %Z"], vars: {"TZ": "UTC"})?
  uu.succeeds(expected)
  let r = uu.invoke(s, "date", ["-u", "-d", "yesterday 10:00 GMT", "+%F %T %Z"], vars: {"TZ": "Australia/Sydney"})?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected.stdout)
}

# origin: uutils test_date::test_date_utc_time
test test_uu_date_date_utc_time { |ctx|
  let s = uu.scene(ctx)?
  let utc1 = uu.invoke(s, "date", ["-u", "+%-H"], vars: {"TZ":"Asia/Taipei"})?
  uu.succeeds(utc1)
  let local = uu.invoke(s, "date", ["+%-H"], vars: {"TZ":"Asia/Taipei"})?
  uu.succeeds(local)
  let utc2 = uu.invoke(s, "date", ["-u", "+%-H"], vars: {"TZ":"Asia/Taipei"})?
  uu.succeeds(utc2)
  let hour1 = utc1.stdout.utf8()?.trim().parse_int_decimal()?
  let hour2 = utc2.stdout.utf8()?.trim().parse_int_decimal()?
  let taipei = local.stdout.utf8()?.trim().parse_int_decimal()?
  assert (taipei - hour1 + 24) % 24 == 8 or (taipei - hour2 + 24) % 24 == 8
  let zone = uu.invoke(s, "date", ["-u", "+%Z"])?
  uu.succeeds(zone)
  uu.stdout_only(zone, "UTC\n")
  let epoch = uu.invoke(s, "date", ["-u", "-d", "@0"])?
  uu.succeeds(epoch)
  uu.stdout_only(epoch, "Thu Jan  1 00:00:00 UTC 1970\n")
}

# origin: uutils test_date::test_date_write_error_dev_full
test test_uu_date_date_write_error_dev_full { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["+%s"], stdout: p"/dev/full")?
  uu.fails(r)
  uu.stderr_contains(r, "write error")
}

# origin: uutils test_date::test_nanoseconds_width_prefix_ignored_issue12001
test test_uu_date_nanoseconds_width_prefix_ignored_issue12001 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["+%3N"])?
  uu.succeeds(r)
  assert r.stdout.len() == 4
}

# origin: uutils test_date::test_negative_offset
test test_uu_date_negative_offset { |ctx|
  let s = uu.scene(ctx)?
  for pair in [["-1 hour", "3600"], ["-1 hours", "3600"], ["-1 day", "86400"], ["-2 weeks", "1209600"]] {
    let r = uu.invoke(s, "date", ["-d", pair[0], "--rfc-3339=seconds"])?
    uu.succeeds(r)
    let fields = rx"^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})\+00:00$".captures(r.stdout.utf8()?.trim())
    assert fields.len() == 7
    let actual = time.from_calendar(decimal_field(fields[1])?, decimal_field(fields[2])?, decimal_field(fields[3])?, decimal_field(fields[4])?, decimal_field(fields[5])?, decimal_field(fields[6])?, utc: true)? / 1000000000
    let expected = time.now() / 1000 - pair[1].parse_int_decimal()?
    assert actual - expected > -600 and actual - expected < 600
  }
}

# origin: uutils test_date::test_locale_names_for_several_dates_in_one_run
test test_uu_date_locale_names_for_several_dates_in_one_run { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "dates", "2026-01-26\n2026-06-14\n2026-12-12\n")?
  let r = uu.invoke(s, "date", ["-f", "dates", "+%A %a %B %b"], vars: {"LC_ALL": "fr_FR.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is(r, "lundi lun. janvier janv.\ndimanche dim. juin juin\nsamedi sam. décembre déc.\n")
}

# origin: uutils test_date::test_date_thai_locale_solar_calendar
test test_uu_date_date_thai_locale_solar_calendar { |ctx|
  let s = uu.scene(ctx)?
  let current = uu.invoke(s, "date", ["+%Y"], vars: {"LC_ALL": "C"})?
  uu.succeeds(current)
  let year = current.stdout.utf8()?.trim().parse_int_decimal()?
  let thai = uu.invoke(s, "date", ["+%Y"], vars: {"LC_ALL": "th_TH.UTF-8"})?
  uu.succeeds(thai)
  assert thai.stdout.utf8()?.trim().parse_int_decimal()? == year + 543
  for month in ["01", "03", "05", "07", "08", "10", "12"] {
    let r = uu.invoke(s, "date", ["--date", f"{year}-{month}-01", "+%B"], vars: {"LC_ALL":"th_TH.UTF-8"})?
    uu.succeeds(r)
    assert r.stdout.utf8()?.trim().ends_with("คม")
  }
  for format in ["--iso-8601=hours", "--rfc-3339=date"] {
    let r = uu.invoke(s, "date", [format], vars: {"LC_ALL":"th_TH.UTF-8"})?
    uu.succeeds(r)
    assert r.stdout.utf8()?.starts_with(f"{year}")
  }
}

# origin: uutils test_date::test_date_set_valid_2
test test_uu_date_date_set_valid_2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "date", ["--set", "Sat 20 Mar 2021 14:53:01 AWST"], vars: {"LC_ALL": "C", "TZ": "UTC0"})?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "date: invalid date 'Sat 20 Mar 2021 14:53:01 AWST'\n")
}
