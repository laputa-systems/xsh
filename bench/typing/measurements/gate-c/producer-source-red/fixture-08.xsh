stream quiet() [] -> Stream[Int] { yield 1 }
stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 2 }
let quiet_rows = quiet()
let clock_rows = clocked()
pure identity(value) { value }
pure choose(flag: Bool, left, right) { if flag { (left) } else { (right) } }
pure independent() -> Int { identity(choose(true, 7, 8)) }
proc consume(flag: Bool) [] -> Unit {
  let boxed = {quiet: identity(quiet_rows), clocked: identity(clock_rows)}
  let sources = [boxed.quiet, boxed.clocked]
  let selected = choose(flag, sources[0], sources[1])
  for item in selected { let _ = item }
}
