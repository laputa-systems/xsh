##! Shared calculations.
## Compute a non-trivial value.
export pure calculate(value: Int) -> Int {
  var total = 0
  for i in range(value) { total += i * i }
  total
}
