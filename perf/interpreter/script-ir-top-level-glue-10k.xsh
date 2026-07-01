pure adjust(value: Int) -> Int {
  if value % 3 == 0 {
    return value / 3
  }

  if value % 5 == 0 {
    return value * 2
  }

  return value + 7
}

var i = 0
var total = 0
var even = 0
var odd = 0

while i < 10000 {
  let adjusted = adjust(i)

  if adjusted % 2 == 0 {
    even += adjusted
  } else {
    odd += adjusted
  }

  total += even % 97
  total += odd % 89
  i += 1
}

print $total % 256
