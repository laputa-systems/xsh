pure step(value: Int) -> Int { value * 2 + 1 }
var total = 0
for i in range(1000) { total += step(i) }
print $total
