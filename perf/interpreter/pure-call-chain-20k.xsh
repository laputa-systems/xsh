pure bias(n: Int) -> Int {
  return n % 7 + 3
}

pure weight(n: Int) -> Int {
  return n % 11 * bias(n)
}

pure score(n: Int) -> Int {
  if n % 5 == 0 {
    return weight(n) - bias(n)
  }

  return weight(n) + bias(n + 1)
}

var i = 0
var total = 0

while i < 20000 {
  total += score(i)
  i += 1
}

print $total % 256
