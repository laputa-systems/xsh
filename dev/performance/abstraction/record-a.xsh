pure step(value: Int, offset: Int) -> Int { value * 2 + offset }
var total = 0
for i in range(200000) {
  total += step(i, 1)
}
print $total
