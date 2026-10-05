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
  assert time.parse("1970-01-01T00:00:00Z")? == 0
  assert time.parse("1970-01-01 01:30:00+01:30")? == 0
  assert time.parse("1969-12-31 23:59:59.999999999Z")? == -1
  assert time.parse("@-0.000000001")? == -1
  assert time.parse("200002290000.05", utc: true)? == 951782405000000000
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
  assert time.parse("not a date", utc: true) is Err(_)
  assert time.parse("2024-02-30", utc: true) is Err(_)
  assert time.parse("@99999999999999999999999999999999999999") is Err(_)
  assert time.format(0, "%999999999999999999999Y", utc: true) is Err(_)
}

test test_time_calendar_timezone_and_dst { |ctx|
  let output = test.run_xsh(ctx, "print time.format(time.parse(\"2024-07-01 12:00:00\")?, \"%H:%M %z\")?", env: {TZ: "EST5EDT,M3.2.0,M11.1.0"})?
  assert output.stdout == "12:00 -0400\n"
  let winter = test.run_xsh(ctx, "print time.format(time.parse(\"2024-01-01 12:00:00\")?, \"%H:%M %z\")?", env: {TZ: "EST5EDT,M3.2.0,M11.1.0"})?
  assert winter.stdout == "12:00 -0500\n"
  let missing = test.run_xsh(ctx, "assert time.parse(\"2024-03-10 02:30:00\") is Err(_)", env: {TZ: "EST5EDT,M3.2.0,M11.1.0"})?
  assert missing.success
}

test test_time_clock_resolution {
  assert time.clock_resolution()? > 0
  assert time.format(0, "%65536Y%65536Y", utc: true) is Err(_)
  assert time.format(0, "%! %_::::z", utc: true)? == "%! %_::::z"
  assert time.parse("A", utc: true, base_ns: 0)? == -3600000000000
  assert time.parse("y", utc: true, base_ns: 0)? == 43200000000000
}
