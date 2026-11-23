proc step(x: Int) -> Int { x + 1 }
var total = 0
for i in range(2000) { total += step(i) }
print (f"{total}")
