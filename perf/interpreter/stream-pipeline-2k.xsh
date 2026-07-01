let values = range(0, 2000)

let total = values
  |> where . % 3 != 0
  |> map { |n|
    n * 2
  }
  |> fold(0) { |acc|
    acc + .
  }

print $total % 256
