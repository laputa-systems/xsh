pure changed() -> Int {
  var total = 0
  for i in range(10000) { total += i }
  total
}
pure stable() -> Int {
  42
}
let _ = changed()
for i in range(5000) { let _ = stable() }
