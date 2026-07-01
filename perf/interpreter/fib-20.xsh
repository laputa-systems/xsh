pure fib(n: Int) -> Int {
  if n < 2 {
    return n
  }

  return fib(n - 1) + fib(n - 2)
}

let result = fib(20)
print $result % 256
