pure descend(n: Int) -> Int {
  if n <= 0 {
    return 0
  }

  return 1 + descend(n - 1)
}

print ${descend(20000)}
