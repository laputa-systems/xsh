pure compute() -> Int {
  var total = 0
  for i in range(200000) { total += i }
  total
}
print (compute())
