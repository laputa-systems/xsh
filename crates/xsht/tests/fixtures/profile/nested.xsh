pure leaf() -> Int {
  var total = 0
  for i in range(1000) { total += i }
  total
}
pure middle() -> Int {
  var total = 0
  for _ in range(4) { total += leaf() }
  total
}
pure outer() -> Int {
  var total = 0
  for _ in range(3) { total += middle() }
  total
}
print (outer())
