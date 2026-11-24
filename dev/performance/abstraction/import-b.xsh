use helper
var total = 0
for i in range(200000) {
  total += helper.step(i)
}
print $total
