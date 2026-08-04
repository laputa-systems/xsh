let counts = ["a", "b", "a"]
  |> reduce(map.empty()) { |acc, item|
    acc.set(item, acc.get(item, 0) + 1)
  }
