proc test_time_module() [process, time, error] {
  let before = time.now()
  time.sleep(1ms)?
  test.ok(time.now() >= before)?
  let measured = time.measure(process.command_argv("true", ["true"]))?
  test.ok(measured.status.exited_with(0))?
  test.ok(measured.duration_ms >= 0)?
  test.eq(time.duration_compact(69), "    1:09")?
  test.eq(time.format(0, "%Y", utc: true)?, "1970")?
  test.eq(time.format(0, "%:z", utc: true)?, "+00:00")?
  test.ok(time.millis(1000) == 1s)?
  test.ok(time.seconds(2) == 2000ms)?
  test.ok(time.millis(-5) == 0ms)?
  test.ok(time.seconds(-1) == 0ms)?
}
