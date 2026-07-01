error NegativeError = Negative(message: Str)

pure normalize(n: Int) -> Result[Int] {
  if n < 0 {
    return Err(NegativeError.Negative(message: "negative value"))
  }

  if n % 2 == 0 {
    return Ok(n / 2)
  }

  n * 3 + 1
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

let result = accumulate(10000)?
print ${result % 256}
