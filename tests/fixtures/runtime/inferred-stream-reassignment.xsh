var reassignment_pulled = 0
var reassignment_closed = 0
stream reassignment_ready() -> Stream[Int] {
  defer { reassignment_closed += 1 }
  reassignment_pulled += 1
  yield 7
}
proc reassignment_total() [] -> Int {
  var rows = reassignment_ready()
  rows = reassignment_ready()
  var total = 0
  for item in rows { total += item }
  total
}
proc verify_inferred_stream_reassignment() [error] -> Result[Unit] {
  assert reassignment_total() == 7
  assert reassignment_pulled == 1
  assert reassignment_closed == 1
}
