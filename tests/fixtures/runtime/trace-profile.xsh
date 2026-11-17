pure leaf(n: Int) -> Int {
  var result = n
  for i in range(1000) { result += i }
  result
}

pure recurse(n: Int) -> Int {
  if n == 0 { return leaf(n) }
  recurse(n - 1) + leaf(n)
}

proc root() -> Int {
  recurse(4)
}

let _ = root()
