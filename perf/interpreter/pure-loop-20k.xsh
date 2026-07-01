pure checksum(limit: Int) -> Int {
  var i = 0
  var total = 0

  while i < limit {
    total += i % 17 * (i % 5)

    if total > 1000000 {
      total = total % 4096
    }

    i += 1
  }

  return total
}

let result = checksum(20000)
print $result % 256
