pure keep(n: Int) -> Bool {
  return n % 3 != 0
}

pure score(n: Int) -> Int {
  return n * 2 + n % 5
}

let values = range(0, 5000)

let total = values
  |> where keep(.)
  |> map { |n|
    score(n)
  }
  |> fold(0) { |acc|
    acc + .
  }

print $total % 256
