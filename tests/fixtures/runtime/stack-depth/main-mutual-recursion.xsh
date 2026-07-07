pure left(n: Int) -> Int {
  if n <= 0 {
    return 0
  }

  return 1 + right(n - 1)
}

pure right(n: Int) -> Int {
  if n <= 0 {
    return 0
  }

  return 1 + left(n - 1)
}

print ${left(20000)}
