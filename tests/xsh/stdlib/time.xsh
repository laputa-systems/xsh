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

test test_time_timestamp_api {
  let now = time.wall_now()
  assert now.nanoseconds >= 0
  assert now.nanoseconds < 1000000000

  let resolution = time.clock_resolution()?
  assert resolution.seconds >= 0
  assert resolution.nanoseconds >= 0
  assert resolution.nanoseconds < 1000000000
  assert resolution.seconds > 0 or resolution.nanoseconds > 0

  assert time.format(0, 0, "%Y-%m-%d %H:%M:%S %N", "UTC", "gregorian")? ==
    "1970-01-01 00:00:00 000000000"
  assert time.format(0, 0, "%a, %d %b %Y %H:%M:%S %z", "UTC", "gregorian", "C")? ==
    "Thu, 01 Jan 1970 00:00:00 +0000"
  assert time.format(-2, 500000000, "%s", "UTC", "gregorian")? == "-2"

  let parsed = time.parse("@-1.5", 0, "UTC")?
  assert parsed.seconds == -2
  assert parsed.nanoseconds == 500000000

  let date = time.parse("2026-01-01 00:00:00", 0, "UTC")?
  assert time.format(date.seconds, date.nanoseconds, "%Y-%m-%d", "UTC", "gregorian")? ==
    "2026-01-01"
}
