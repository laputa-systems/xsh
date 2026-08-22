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
  test.ok(time.millis(1000) == 1s)?
  test.ok(time.seconds(2) == 2000ms)?
  test.ok(time.millis(-5) == 0ms)?
  test.ok(time.seconds(-1) == 0ms)?
}
