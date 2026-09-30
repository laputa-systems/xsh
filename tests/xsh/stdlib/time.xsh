test test_time_module [process, time, error] {
  let before = time.now()
  time.sleep(1ms)?
  (time.now() >= before)
  let measured = time.measure(process.command_argv("true", ["true"]))?
  measured.status.exited_with(0)
  (measured.duration_ms >= 0)
  time.duration_compact(69) == "    1:09"
  time.duration_compact(-1) == "    0:00"
  time.duration_compact(0) == "    0:00"
  time.duration_compact(2 * 3600 + 15 * 60) == "   2h15m"
  time.duration_compact(25 * 3600 + 4 * 60) == "  1d01h"
  (time.millis(1000) == 1s)
  (time.seconds(2) == 2000ms)
  (time.millis(-5) == 0ms)
  (time.seconds(-1) == 0ms)
}
