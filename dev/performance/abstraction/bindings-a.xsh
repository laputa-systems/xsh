var total: Int = 0
for i in range(200000) {
  let value: Int = i * 2 + 1
  total += value
}
print $total
