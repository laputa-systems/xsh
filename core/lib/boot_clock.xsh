##! Reads the host boot clock and converts its nanoseconds to coarser units.
##!
##! A Boot reading counts nanoseconds from a host origin. On Linux it includes
##! time spent suspended, so it is not the process monotonic clock of
##! [monotonic](monotonic.xsh) and not wall time; the nominal type keeps the
##! three apart.
## Holds nanoseconds on the host boot clock.
export nominal type Boot = {ns: Int}

## Reads the boot clock in nanoseconds.
export proc now() [time, error] -> Result[Boot, Error] {
  Ok(Boot(ns: time.boottime()?))
}

## Rounds a boot reading down to whole milliseconds; a negative reading is zero.
export pure millis(at: Boot) -> Int {
  if at.ns <= 0 { 0 } else { at.ns / 1000000 }
}

## Rounds a boot reading down to whole microseconds; a negative reading is zero.
export pure micros(at: Boot) -> Int {
  if at.ns <= 0 { 0 } else { at.ns / 1000 }
}

## Reports the microseconds from one boot reading to a later one, never below zero.
export pure elapsed_us(start: Boot, end: Boot) -> Int {
  if end.ns <= start.ns { 0 } else { (end.ns - start.ns) / 1000 }
}
