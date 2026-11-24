var total = 0
for i in range(200000) {
  let value = i * 2 + 1
  if value % 3 != 0 { total += value }
}
print $total
