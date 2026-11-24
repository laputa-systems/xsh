pure identity(value) { value }
var total = 0
for i in range(200000) {
  total += identity(i)
}
print $total
