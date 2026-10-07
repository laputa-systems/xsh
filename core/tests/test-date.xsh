use core.lib.date_parse

test test_date_format { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u +%Y
  assert output.trim().count_chars() == 4
  let offset = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u +%z
  assert offset.trim() == "+0000"
}

test test_date_format_modifiers_and_width_bounds { |ctx|
  let script = fp"{ctx.core_dir}/date.xsh"
  let output = run.text ${ctx.xsh_bin} $script -- -u -d "1999-06-01 05:00:00" "+%10Y|%_5m|%-5d|%^B|%#P|%Om|%02j|%+6Y"
  assert output == "0000001999|    6|1|JUNE|am|06|152|+01999\n"
  let nanos = run.text ${ctx.xsh_bin} $script -- -u -d "@0" "+%_3N|%-N|%-3N|%10N"
  assert nanos == "0  |000000000|0|0000000000\n"
  let modifiers = run.text ${ctx.xsh_bin} $script -- -u -d "@0" "+%EN|%ON"
  assert modifiers == "%EN|000000000\n"

  let oversized = run.capture --text ${ctx.xsh_bin} $script -- -u -d "@0" "+%8888888888888s"
  assert oversized.status.exited_with(1)
  assert oversized.stdout == ""
  assert oversized.stderr == "date: format modifier width '8888888888888' is too large for specifier '%s'\n"

  let overflowing = run.capture --text ${ctx.xsh_bin} $script -- -u -d "@0" "+%999999999999999999999999999999999Y"
  assert overflowing.status.exited_with(1)
  assert overflowing.stdout == ""
  assert overflowing.stderr == "date: date format width too large\n"
}

test test_date_dash_input_means_midnight_today { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -d - +%T
  assert output == "00:00:00\n"
}

test test_date_empty_date_option_means_midnight_today { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -d "" +%T
  assert output == "00:00:00\n"
}

test test_date_negative_relative_offset { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -d "2000-01-02 00:00:00 -1 hour" +%F_%T
  assert output == "2000-01-01_23:00:00\n"
}

test test_date_debug_diagnostics { |ctx|
  let script = fp"{ctx.core_dir}/date.xsh"
  let output = run.capture --text ${ctx.xsh_bin} $script -- -u --debug -d "2005-01-01" +%Y
  assert output.status.exited_with(0)
  assert output.stdout == "2005\n"
  assert "date: input string: 2005-01-01" in output.stderr
  assert "date: parsed date part: (Y-M-D) 2005-01-01" in output.stderr
  assert "date: parsed time part:" in output.stderr
  assert "date: input timezone:" in output.stderr
  assert "date: warning: using midnight" in output.stderr

  let quiet = run.capture --text ${ctx.xsh_bin} $script -- -u --debug +%Y
  assert quiet.status.exited_with(0)
  assert quiet.stderr == ""
}

test test_date_debug_file_inputs { |ctx|
  let target = test.temp_file(ctx, name: "debug-dates.txt", contents: b"2005-01-01\n2006-02-02\n")?
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- --debug -f $target +%Y
  assert result.status.exited_with(0)
  assert result.stdout == "2005\n2006\n"
  assert "date: input string: 2005-01-01" in result.stderr
  assert "date: input string: 2006-02-02" in result.stderr
}

test test_date_unknown_options_use_unexpected_argument { |ctx|
  let script = fp"{ctx.core_dir}/date.xsh"
  for option in ["--fB", "-w"] {
    let result = run.capture --text ${ctx.xsh_bin} $script -- $option
    assert result.status.exited_with(1)
    assert result.stdout == ""
    assert f"unexpected argument '{option}'" in result.stderr
  }
}

test test_date_bare_dash_remains_invalid { |ctx|
  let output = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -
  assert output.status.exited_with(1)
  assert output.stdout == ""
  assert output.stderr.starts_with("date: invalid date")
}

test test_date_empty_operand_remains_invalid { |ctx|
  let output = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- ""
  assert output.status.exited_with(1)
  assert output.stdout == ""
  assert output.stderr.starts_with("date: invalid date")
}

test test_date_second_operand_is_reported_as_extra { |ctx|
  let output = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- test extra
  assert output.status.exited_with(1)
  assert output.stdout == ""
  assert "date: extra operand 'extra'\n" in output.stderr
}

test test_date_native_epoch_and_nanoseconds { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -d "@-0.000000001" "+%Y-%m-%d %H:%M:%S.%N %s"
  assert output == "1969-12-31 23:59:59.999999999 -1\n"
  let plus = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -d "@0" "+literal+%Y"
  assert plus == "literal+1970\n"
  let iso = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -d "2000-02-29 12:34:56" -Iseconds
  assert iso == "2000-02-29T12:34:56+00:00\n"
}

test test_date_batch_invalid_bytes_continue_and_nul_terminates { |ctx|
  let target = test.temp_file(ctx, name: "batch-dates.txt", contents: b"2024-01-15 12:00:00\x00ignored\nHello\xffx\n2024-01-16 13:00:00\n")?
  let output = run.capture --text env LC_ALL=C ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -u -f $target "+%F %T"
  assert output.status.exited_with(1)
  assert output.stdout == "2024-01-15 12:00:00\n2024-01-16 13:00:00\n"
  assert output.stderr == "date: invalid date 'Hello\\377x'\n"
}

test test_date_grammar_epoch_precision_and_boundaries {
  assert date_parse.parse("1970-01-01T00:00:00Z")? == 0
  assert date_parse.parse("1970-01-01 01:30:00+01:30")? == 0
  assert date_parse.parse("1969-12-31 23:59:59.999999999Z")? == -1
  assert date_parse.parse("@-0.000000001")? == -1
  assert date_parse.parse("@9223372036.854775807")? == 9223372036854775807
  assert date_parse.parse("@-9223372036.854775808")? == -9223372036854775807 - 1
  assert date_parse.parse("@9223372036.854775808") is Err(_)
  assert date_parse.parse("@-9223372036.854775809") is Err(_)
  assert date_parse.parse("@99999999999999999999999999999999999999") is Err(_)
  assert date_parse.parse("@1.1234567890") is Err(_)
  assert date_parse.parse("@-+1") is Err(_)
}

test test_date_grammar_relative_baseline_and_civil_overflow {
  assert date_parse.parse("+5 days", utc: true, base_ns: 0)? == 432000000000000
  assert date_parse.parse("2 hours 3 minutes ago", utc: true, base_ns: 0)? == -7380000000000
  assert date_parse.parse("last thu", utc: true, base_ns: 0)? == -604800000000000
  assert date_parse.parse("this thu", utc: true, base_ns: 0)? == 0
  assert date_parse.parse("next thu", utc: true, base_ns: 0)? == 604800000000000
  assert date_parse.parse("yesterday 10:00 GMT", utc: true, base_ns: 0)? == -50400000000000
  assert date_parse.parse("2000-02-29 + 2 years", utc: true)? == date_parse.parse("2002-03-01", utc: true)?
  assert date_parse.parse("1996-01-31 + 1 month", utc: true)? == date_parse.parse("1996-03-02", utc: true)?
  assert date_parse.parse("2003-08-31 12:00:00 +0 7 months ago", utc: true)? == date_parse.parse("2003-01-31 12:00:00", utc: true)?
  assert date_parse.parse("now", base_ns: 123456789)? == 123456789
  assert date_parse.parse("1 second", base_ns: -1)? == 999999999
}

test test_date_grammar_compact_clock_and_named_input {
  assert date_parse.parse("200002290000.05", utc: true)? == 951782405000000000
  assert date_parse.parse("0002290000.05", utc: true)? == 951782405000000000
  assert date_parse.parse("0700", utc: true, base_ns: 0)? == 25200000000000
  assert date_parse.parse("1230j", utc: true, base_ns: 0)? == 45000000000000
  assert date_parse.parse("A", utc: true, base_ns: 0)? == -3600000000000
  assert date_parse.parse("y", utc: true, base_ns: 0)? == 43200000000000
  assert date_parse.parse("m9", utc: true, base_ns: 0)? == -10800000000000
  assert date_parse.parse("2024-06-15 3:00 p.m.", utc: true)? == date_parse.parse("2024-06-15 15:00", utc: true)?
  assert date_parse.parse("2026(comment)-01-05", utc: true)? == date_parse.parse("2026-01-05", utc: true)?
  assert date_parse.parse("((ignored)2026-01-05)", utc: true, base_ns: 0)? == 0
  assert date_parse.parse("2024-01-15 12:00 IST", utc: true)? == date_parse.parse("2024-01-15 06:30", utc: true)?
  assert date_parse.parse("Sat 20 Mar 2021 14:53:01 AWST", utc: true)? == date_parse.parse("2021-03-20 06:53:01", utc: true)?
  assert date_parse.parse("Thu Jan 01 12:34:00 2015", utc: true)? == date_parse.parse("2015-01-01 12:34:00", utc: true)?
  assert date_parse.parse("2024-01-15\t12:00:00", utc: true)? == date_parse.parse("2024-01-15 12:00:00", utc: true)?
  assert date_parse.parse("Jan 23\x0b 2026 1:00AM", utc: true)? == date_parse.parse("2026-01-23 01:00", utc: true)?
  assert date_parse.parse("Jan 23\x0c2026 1:00AM", utc: true)? == date_parse.parse("2026-01-23 01:00", utc: true)?
  assert date_parse.parse("not a date", utc: true) is Err(_)
  assert date_parse.parse("2024-02-30", utc: true) is Err(_)
  assert date_parse.parse("2024-01-01 12:00 UTC EST", utc: true) is Err(_)
}

test test_date_grammar_timezone_and_dst { |ctx|
  let source = "use lib.date_parse\nprint time.format(date_parse.parse(\"2024-07-01 12:00:00\")?, \"%H:%M %z\")?"
  let output = test.run_xsh(ctx, source, env: {TZ: "EST5EDT,M3.2.0,M11.1.0", XSH_MODULE_PATH: ctx.core_dir})?
  assert output.success, output.stderr
  assert output.stdout == "12:00 -0400\n"
  let winter = test.run_xsh(ctx, source.replace("2024-07", with: "2024-01"), env: {TZ: "EST5EDT,M3.2.0,M11.1.0", XSH_MODULE_PATH: ctx.core_dir})?
  assert winter.success, winter.stderr
  assert winter.stdout == "12:00 -0500\n"
  let missing = test.run_xsh(ctx, "use lib.date_parse\nassert date_parse.parse(\"2024-03-10 02:30:00\") is Err(_)", env: {TZ: "EST5EDT,M3.2.0,M11.1.0", XSH_MODULE_PATH: ctx.core_dir})?
  assert missing.success, missing.stderr
}

test test_date_grammar_relative_dst_gap_matches_host_calendar { |ctx|
  let source = "use lib.date_parse\nprint time.format(date_parse.parse(\"2024-03-09 02:30:00 1 day\")?, \"%F %T %z\")?\nprint time.format(date_parse.parse(\"2024-03-11 02:30:00 1 day ago\")?, \"%F %T %z\")?"
  let output = test.run_xsh(ctx, source, env: {TZ: "EST5EDT,M3.2.0,M11.1.0", XSH_MODULE_PATH: ctx.core_dir})?
  assert output.success, output.stderr
  assert output.stdout == "2024-03-10 01:30:00 -0500\n2024-03-10 01:30:00 -0500\n"
}
