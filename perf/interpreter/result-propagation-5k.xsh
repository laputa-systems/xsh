pure normalize(n: Int) -> Result[Int] {
  if n % 2 == 0 {
    return Ok(n)
  }

  n + 1
}

pure accumulate(limit: Int) -> Result[Int] {
  var i = 0
  var total = 0

  while i < limit {
    total += normalize(i)?
    i += 1
  }

  total
}

let result = accumulate(5000)?
print $result % 256
