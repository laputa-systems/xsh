var i = 0
var sum = 0

while i < 10000 {
  sum += i
  i += 1
}

print $sum % 256
