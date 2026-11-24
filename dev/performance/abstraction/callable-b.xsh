pure step(value: Int) -> Int { value * 2 + 1 }
let call: pure(value: Int) -> Int = step
var total = 0
for i in range(200000) {
  total += call(i)
}
print $total
