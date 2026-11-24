pure step(value: Int) -> Int { value * 2 + 1 }
var total = 0
for i in range(200000) {
  total += step(i)
}
print $total
