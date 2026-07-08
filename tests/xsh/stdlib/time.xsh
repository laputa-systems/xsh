proc test_time_module() [process, time, error] {
  let before = time.now()
  time.sleep(1ms)?
  test.ok(time.now() >= before)?
  let measured = time.measure(process.command_argv("true", ["true"]))?
  test.ok(measured.status.exited_with(0))?
  test.ok(measured.duration_ms >= 0)?
  test.eq(time.duration_compact(69), "    1:09")?
  test.eq(time.duration_compact(-1), "    0:00")?
  test.eq(time.duration_compact(0), "    0:00")?
  test.eq(time.duration_compact(2 * 3600 + 15 * 60), "   2h15m")?
  test.eq(time.duration_compact(25 * 3600 + 4 * 60), "  1d01h")?
  test.eq(time.format(0, "%Y", utc: true)?, "1970")?
  test.eq(time.format(0, "%:z", utc: true)?, "+00:00")?
  test.eq(time.format(0, "%Y-%m-%dT%H:%M:%S%:z", utc: true)?, "1970-01-01T00:00:00+00:00")?
  test.ok(time.millis(1000) == 1s)?
  test.ok(time.seconds(2) == 2000ms)?
  test.ok(time.millis(-5) == 0ms)?
  test.ok(time.seconds(-1) == 0ms)?
  test.error_kind(time.format(0, "%", utc: true), "time-format")?
  test.error_kind(time.format(0, "%#", utc: true), "time-format")?
  test.error_kind(time.format(999999999999999999, "%Y", utc: true), "time-format")?
}

proc test_time_module_formats_local_time_under_tz(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    r"""print ${time.format(0, "%Y-%m-%d %H:%M %Z %z", utc: false)?}
""",
    [],
    {TZ: "America/New_York"},
  )?

  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "1969-12-31 19:00 EST -0500\n")?
  test.eq(output.stderr, "")?
}
