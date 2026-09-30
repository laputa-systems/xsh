stream rows() [] -> Stream[Int] {
  yield @[1, 2]
  yield 3
}

stream forwarded() [] -> Stream[Int] {
  yield @rows()
  yield @[4, 5]
}

for value in forwarded() { print $value }
