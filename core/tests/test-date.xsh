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

test test_date_embedded_posix_timezone { |ctx|
  for item in [{zone: "CET-1", expected: "1969-12-31_23:00:00"}, {zone: "EST5", expected: "1970-01-01_05:00:00"}, {zone: "CET1", expected: "1970-01-01_01:00:00"}, {zone: "UTC0", expected: "1970-01-01_00:00:00"}] {
    let input = f"TZ=\"{item.zone}\" 1970-01-01 00:00"
    let output = run.text env LC_ALL=C TZ=UTC0 ${ctx.xsh_bin} fp"{ctx.core_dir}/date.xsh" -- -d $input +%F_%T
    assert output == f"{item.expected}\n"
  }
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

type RawRan = {status: Int, stdout: Bytes, stderr: Str}

# Passes each argument as given: a Path carries operand bytes that are not UTF-8,
# which a Str argument cannot represent.
proc date_raw_run(ctx: TestContext, args: List[Union[Str, Path]]) [fs, process, error] -> Result[RawRan] {
  let root = test.temp_dir(ctx, name: "date-raw")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/date.xsh"
  var argv: List[Union[Str, Path]] = [ctx.xsh_bin.display(), script.display()]
  for arg in args { argv += [arg] }
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_date_non_utf8_operands_are_octal_escaped { |ctx|
  let invalid_date: List[Union[Str, Path]] = ["-d", Path.parse_bytes(b"gr\xf6n")?]
  let bad_date = date_raw_run(ctx, invalid_date)?
  assert bad_date.status == 1
  assert bad_date.stdout == b""
  assert bad_date.stderr == "date: invalid date 'gr\\366n'\n"

  let extra: List[Union[Str, Path]] = ["+%Y", Path.parse_bytes(b"\xf1ao")?]
  let extra_result = date_raw_run(ctx, extra)?
  assert extra_result.status == 1
  assert extra_result.stderr.starts_with("date: extra operand '\\361ao'\n"), extra_result.stderr

  let lacking: List[Union[Str, Path]] = ["-d", "2031-07-23", Path.parse_bytes(b"%Y\xd8")?]
  let lacking_result = date_raw_run(ctx, lacking)?
  assert lacking_result.status == 1
  assert lacking_result.stderr.starts_with("date: the argument %Y\\330 lacks a leading '+';\n"), lacking_result.stderr
}

test test_date_format_passes_non_utf8_bytes_through { |ctx|
  let gb18030: List[Union[Str, Path]] = ["-u", "-d", "2031-07-23T04:05:06", Path.parse_bytes(b"+%Y\xc4\xea%-m\xd4\xc2%-d\xc8\xd5")?]
  let legacy = date_raw_run(ctx, gb18030)?
  assert legacy.status == 0, legacy.stderr
  assert legacy.stdout == b"2031\xc4\xea7\xd4\xc223\xc8\xd5\n"

  let percent: List[Union[Str, Path]] = ["-u", "-d", "2031-07-23T04:05:06", Path.parse_bytes(b"+w%\xd0z")?]
  let percent_result = date_raw_run(ctx, percent)?
  assert percent_result.status == 0, percent_result.stderr
  assert percent_result.stdout == b"w%\xd0z\n"

  let literal: List[Union[Str, Path]] = ["-u", "-d", "2031-07-23T04:05:06", Path.parse_bytes(b"+\xc5[%Y]\xa7%%\xe4")?]
  let literal_result = date_raw_run(ctx, literal)?
  assert literal_result.status == 0, literal_result.stderr
  assert literal_result.stdout == b"\xc5[2031]\xa7%\xe4\n"
}

test test_date_file_and_reference_accept_non_utf8_paths { |ctx|
  let root = test.temp_dir(ctx, name: "date-raw-paths")?
  let dates = Path.parse_bytes(bytes.concat([root.bytes(), b"/dates\xff"]))?
  dates.write("2005-01-01\n")
  let from_file: List[Union[Str, Path]] = ["-u", "--file", dates, "+%Y"]
  let file_result = date_raw_run(ctx, from_file)?
  assert file_result.status == 0, file_result.stderr
  assert file_result.stdout == b"2005\n"

  let reference = Path.parse_bytes(bytes.concat([root.bytes(), b"/reference\xfe"]))?
  reference.write("x")
  let from_reference: List[Union[Str, Path]] = ["-u", "--reference", reference, "+%s"]
  let reference_result = date_raw_run(ctx, from_reference)?
  assert reference_result.status == 0, reference_result.stderr
  assert reference_result.stdout == bytes.from_text(f"{fs.stat(reference)?.mtime_ns / 1000000000}\n")
}

test test_date_parse_instant_carries_years_beyond_nanoseconds {
  let instant = date_parse.parse_instant("18978-01-01", utc: true)?
  assert instant.seconds == date_parse.days_from_civil(18978, 1, 1) * 86400
  assert instant.nanoseconds == 0
  assert date_parse.instant_ns(instant) is Err(_)
  assert date_parse.parse("18978-01-01", utc: true) is Err(_)
  assert date_parse.parse("2000-01-01 00:00:00.5", utc: true)? == 946684800500000000
}

test test_date_large_years_default_output { |ctx|
  let script = fp"{ctx.core_dir}/date.xsh"
  let output = run.text env LC_ALL=C TZ=UTC0 ${ctx.xsh_bin} $script -- -d 18978-01-01
  assert output == "Thu Jan  1 00:00:00 UTC 18978\n"
  let offset = run.text env LC_ALL=C TZ=UTC0 ${ctx.xsh_bin} $script -- -d "10000-01-01 00:00 +1400"
  assert offset == "Fri Dec 31 10:00:00 UTC 9999\n"
  let invalid = run.capture --text ${ctx.xsh_bin} $script -- -d 10000-02-30
  assert invalid.status.exited_with(1)
  assert invalid.stdout == ""
  assert invalid.stderr == "date: invalid date '10000-02-30'\n"
}

test test_date_shifted_years_format_each_field { |ctx|
  let script = fp"{ctx.core_dir}/date.xsh"
  let seconds = date_parse.days_from_civil(18978, 1, 1) * 86400
  let output = run.text env LC_ALL=C TZ=UTC0 ${ctx.xsh_bin} $script -- -d 18978-01-01 "+%C %y %F %D %s %c"
  assert output == f"189 78 18978-01-01 01/01/78 {seconds} Thu Jan  1 00:00:00 18978\n"
  let unsupported = run.capture --text env LC_ALL=C TZ=UTC0 ${ctx.xsh_bin} $script -- -d 18978-01-01 "+%+"
  assert unsupported.status.exited_with(1)
  assert unsupported.stderr == "date: format directive %+ is not supported for this year or locale\n"
}

test test_date_locale_names_follow_locale_table { |ctx|
  let script = fp"{ctx.core_dir}/date.xsh"
  let names = run.text env LC_ALL=fr_FR.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-01-26 "+%A %a %B %b"
  assert names == "lundi lun. janvier janv\n"
  let german = run.text env LC_ALL=de_DE.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-06-15 +%B
  assert german == "Juni\n"
  let japanese = run.text env LC_ALL=ja_JP.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-01-24 +%A
  assert japanese == "土曜日\n"
  let english = run.text env LC_ALL=en_US.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-01-24 +%A
  assert english == "Saturday\n"
  let modified = run.capture --text env LC_ALL=fr_FR.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-01-26 "+%^A"
  assert modified.status.exited_with(1)
  assert modified.stderr == "date: format directive %^A is not supported for this year or locale\n"
}

test test_date_locale_calendars_convert_year_month_and_day { |ctx|
  let script = fp"{ctx.core_dir}/date.xsh"
  let calendar_1 = run.text env LC_ALL=fa_IR.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-03-21 "+%Y-%m-%d"
  assert calendar_1 == "1405-01-01\n"
  let calendar_2 = run.text env LC_ALL=fa_IR.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-03-20 "+%Y-%m-%d"
  assert calendar_2 == "1404-12-29\n"
  let calendar_3 = run.text env LC_ALL=th_TH.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-01-01 "+%Y-%m-%d"
  assert calendar_3 == "2569-01-01\n"
  let calendar_4 = run.text env LC_ALL=am_ET.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-09-11 "+%Y-%m-%d"
  assert calendar_4 == "2019-01-01\n"
  let calendar_5 = run.text env LC_ALL=am_ET.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -d 2026-09-10 "+%Y-%m-%d"
  assert calendar_5 == "2018-13-05\n"
}

test test_date_rfc_and_iso_formats_ignore_calendar_locale { |ctx|
  let script = fp"{ctx.core_dir}/date.xsh"
  let rfc = run.text env LC_ALL=fr_FR.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -R -d "1997-01-19 08:17:48 +0"
  assert rfc == "Sun, 19 Jan 1997 08:17:48 +0000\n"
  let iso = run.text env LC_ALL=fa_IR.UTF-8 TZ=UTC0 ${ctx.xsh_bin} $script -- -I -d 2026-03-21
  assert iso == "2026-03-21\n"
}

test test_date_clock_with_zone_offset_and_signed_day_counts { |ctx|
  let script = fp"{ctx.core_dir}/date.xsh"
  let zoned = run.text ${ctx.xsh_bin} $script -- -u -d "2020-01-02 21:04 +0100" "+%F %R"
  assert zoned == "2020-01-02 20:04\n", zoned
  let bare_clock = run.text ${ctx.xsh_bin} $script -- -u -d "21:04 +0100" "+%R"
  assert bare_clock == "20:04\n", bare_clock
  let day_before = run.text ${ctx.xsh_bin} $script -- -u -d "12:00 today -2 days" "+%F %R"
  let two_days_ago = run.text ${ctx.xsh_bin} $script -- -u -d "2 days ago" "+%F"
  assert day_before == f"{two_days_ago.trim()} 12:00\n", day_before
  let earlier = run.text ${ctx.xsh_bin} $script -- -u -d "12:00 today" "+%F %R"
  assert earlier.ends_with(" 12:00\n"), earlier
}
