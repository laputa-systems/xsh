proc descend(n: Int) -> Result[Int] {
  if n <= 0 {
    return Ok(0)
  }

  if true {
    let next = descend(n - 1)?
    return Ok(next + 1)
  }

  return Ok(-1)
}

print ${descend(12000)?}
