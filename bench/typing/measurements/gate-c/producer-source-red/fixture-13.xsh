stream quiet() [] -> Stream[Int] { yield 1 }
stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 2 }
let quiet_rows = quiet()
let clock_rows = clocked()
pure identity(value) { value }
pure choose(flag: Bool, left, right) { if flag { (left) } else { (right) } }
pure independent() -> Int { identity(choose(true, 7, 8)) }
proc consume(flag: Bool) [time] -> Unit {
  let original = identity(clock_rows)
  let selected = original
  for item in selected { let _ = item }
}
