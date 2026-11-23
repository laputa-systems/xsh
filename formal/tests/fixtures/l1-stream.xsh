var trace: List[Int] = []
stream count(n: Int) -> Stream[Int] {
  defer { trace += [7] }
  errdefer { trace += [8] }
  var i = 0
  while i < n {
    trace += [1]
    yield i
    i += 1
  }
}
let source = count(8)
trace += [8]
for x in source {
  trace += [x]
  break when x == 0
}
for x in source { trace += [x] }
for value in trace { print $value }
