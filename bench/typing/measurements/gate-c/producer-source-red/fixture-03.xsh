stream quiet() [] -> Stream[Int] { yield 1 }
stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 2 }
let quiet_rows = quiet()
let clock_rows = clocked()
pure identity(value) { value }
pure choose(flag: Bool, left, right) { if flag { (left) } else { (right) } }
pure independent() -> Int { identity(choose(true, 7, 8)) }
proc consume(flag: Bool) [] -> Unit {
  let sources = [clock_rows, quiet_rows]
  let wrapped = sources |> map { |value| identity(value) } |> first()
  let selected = wrapped ?? quiet_rows
  for item in selected { let _ = item }
}
