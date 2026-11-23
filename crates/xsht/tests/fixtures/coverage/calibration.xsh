pure choose(a: Bool, b: Bool, c: Bool) -> Bool {
  if a and (b or c) { return true } else { return false }
}
pure arithmetic(n: Int) -> Int {
  let x = n + 1
  return x * 2
}
pure early(n: Int) -> Int {
  if n > 0 { return 1 }
  return 2
}
proc fallible() -> Result[Int] {
  let n = "bad".parse_int()?
  return n + 1
}
stream values() [] -> Stream[Int] {
  yield 1
  yield 2
  yield 3
}
test calibration {
  assert ! choose(false, true, true)
  assert arithmetic(2) == 6
  assert early(1) == 1
  assert fallible() is Err(_)
  let unused = values()
  let _ = unused
  let first = values() |> take(1) |> collect()
  assert first == [1]
}
