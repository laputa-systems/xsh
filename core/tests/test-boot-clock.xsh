use core.lib.boot_clock
use core.lib.monotonic

test test_boot_nanoseconds_round_down_to_milliseconds_and_microseconds {
  let at = boot_clock.Boot(ns: 7123456789)
  assert boot_clock.millis(at) == 7123
  assert boot_clock.micros(at) == 7123456
  assert boot_clock.millis(boot_clock.Boot(ns: 999999)) == 0
  assert boot_clock.micros(boot_clock.Boot(ns: 999)) == 0
  assert boot_clock.micros(boot_clock.Boot(ns: 9223372036854775807)) == 9223372036854775
}

test test_a_negative_boot_reading_converts_to_zero {
  assert boot_clock.millis(boot_clock.Boot(ns: -5000000)) == 0
  assert boot_clock.micros(boot_clock.Boot(ns: -5000)) == 0
}

test test_boot_elapsed_microseconds_never_go_negative {
  let first = boot_clock.Boot(ns: 5000000000)
  assert boot_clock.elapsed_us(first, boot_clock.Boot(ns: 5000250999)) == 250
  assert boot_clock.elapsed_us(first, first) == 0
  assert boot_clock.elapsed_us(first, boot_clock.Boot(ns: 4000000000)) == 0
  assert boot_clock.elapsed_us(boot_clock.Boot(ns: 0), boot_clock.Boot(ns: 9223372036854775807)) == 9223372036854775
}

test test_the_live_boot_clock_is_ahead_of_the_process_monotonic_origin {
  let stamp = monotonic.stamp()?
  let boot = boot_clock.now()?
  assert boot.ns > stamp.ns
  let later = boot_clock.now()?
  assert boot_clock.elapsed_us(boot, later) < 60000000
}

test test_a_boot_reading_is_neither_a_stamp_nor_wall_time { |ctx|
  test.expect(ctx, r"""use core.lib.boot_clock
use core.lib.monotonic
let boot = boot_clock.now()?
let gap = monotonic.elapsed_ns(boot, monotonic.stamp()?)
""", status: 2, stderr: ["[check.type-mismatch]"])?
  test.expect(ctx, r"""use core.lib.boot_clock
let wall: Int = time.now()
let ms = boot_clock.millis(wall)
""", status: 2, stderr: ["[check.type-mismatch]"])?
}
