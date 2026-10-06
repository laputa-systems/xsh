test test_time_module {
  let before = time.now()
  time.sleep(1ms)
  assert time.now() >= before
  let measured = time.measure(process.command_argv("true", ["true"]))?
  assert measured.status.exited_with(0)
  assert measured.duration_ms >= 0
  assert time.duration_compact(69) == "    1:09"
  assert time.duration_compact(-1) == "    0:00"
  assert time.duration_compact(0) == "    0:00"
  assert time.duration_compact(2 * 3600 + 15 * 60) == "   2h15m"
  assert time.duration_compact(25 * 3600 + 4 * 60) == "  1d01h"
  for amount in [1000] {
    assert time.millis(amount) == 1s
  }

  for amount in [2] {
    assert time.seconds(amount) == 2000ms
  }

  assert time.millis(-5) == 0ms
  assert time.seconds(-1) == 0ms
}

test test_time_calendar_round_trip {
  assert time.from_calendar(1970, 1, 1, utc: true)? == 0
  assert time.from_calendar(2000, 2, 29, utc: true)? == 951782400000000000
  let fields = time.to_calendar(-1, utc: true)?
  assert fields.year == 1969 and fields.month == 12 and fields.day == 31
  assert fields.hour == 23 and fields.minute == 59 and fields.second == 59
  assert fields.weekday == 3 and fields.offset_seconds == 0 and fields.nanosecond == 999999999
  let leap = time.to_calendar(951782400000000000, utc: true)?
  assert time.from_calendar(leap.year, leap.month, leap.day, leap.hour, leap.minute, leap.second, utc: true)? == 951782400000000000
  assert time.format(-1, "%Y-%m-%d %H:%M:%S.%N %s", utc: true)? == "1969-12-31 23:59:59.999999999 -1"
  assert time.format(0, "%q %z %:z %::z %:::z", utc: true)? == "1 +0000 +00:00 +00:00:00 +00"
  assert time.format(123456789, "%3N %12N", utc: true)? == "123 123456789000"
  assert time.format(0, "literal+%%", utc: true)? == "literal+%"
}

test test_time_calendar_rejects_invalid_input {
  assert time.from_calendar(2023, 2, 29, utc: true) is Err(_)
  assert time.from_calendar(2024, 2, 30, utc: true) is Err(_)
  assert time.from_calendar(2024, 1, 1, hour: 24, utc: true) is Err(_)
  assert time.from_calendar(2500, 1, 1, utc: true) is Err(_)
  assert time.format(0, "%999999999999999999999Y", utc: true) is Err(_)
}

test test_time_calendar_timezone_and_dst { |ctx|
  let source = "let epoch = time.from_calendar(2024, 7, 1, 12)?\nlet fields = time.to_calendar(epoch)?\nassert fields.hour == 12\nassert fields.offset_seconds == -14400\nprint time.format(epoch, \"%H:%M %z\")?"
  let output = test.run_xsh(ctx, source, env: {TZ: "EST5EDT,M3.2.0,M11.1.0"})?
  assert output.stdout == "12:00 -0400\n"
  let winter_source = source.replace("2024, 7", with: "2024, 1").replace("-14400", with: "-18000")
  let winter = test.run_xsh(ctx, winter_source, env: {TZ: "EST5EDT,M3.2.0,M11.1.0"})?
  assert winter.stdout == "12:00 -0500\n"
  let missing = test.run_xsh(ctx, "assert time.from_calendar(2024, 3, 10, 2, 30) is Err(_)", env: {TZ: "EST5EDT,M3.2.0,M11.1.0"})?
  assert missing.success
}

test test_time_clock_resolution {
  assert time.clock_resolution()? > 0
  assert time.format(0, "%65536Y%65536Y", utc: true) is Err(_)
  assert time.format(0, "%! %_::::z", utc: true)? == "%! %_::::z"
}

test test_time_explicit_calendar_normalization { |ctx|
  let source = "assert time.from_calendar(2024, 3, 10, 2, 30) is Err(_)\nlet epoch = time.from_calendar(2024, 3, 10, 2, 30, normalize: true)?\nprint time.format(epoch, \"%F %T %z\")?"
  let output = test.run_xsh(ctx, source, env: {TZ: "EST5EDT,M3.2.0,M11.1.0"})?
  assert output.success, output.stderr
  assert output.stdout == "2024-03-10 01:30:00 -0500\n"
}
