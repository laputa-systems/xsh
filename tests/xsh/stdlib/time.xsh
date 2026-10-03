test test_time_module {
  let before = time.now()
  time.sleep(1ms)?
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
