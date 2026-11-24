pure step(value: Int) -> Int { value * 2 + 1 }
pure caller(count: Int) -> Int {
  var total = 0
  for i in range(count) { total += step(i) }
  total
}
print caller(10000)
