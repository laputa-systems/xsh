error RowsError = Late(row: Int)

proc fail() [] -> Result[Unit, RowsError] {
  return Err(RowsError.Late(row: 4))
}

stream child() [error] -> Stream[Int] {
  yield 3
  fail()?
  yield 88
}

stream parent() [error] -> Stream[Int] {
  yield @[1, 2]
  yield @child()
  yield 99
}
