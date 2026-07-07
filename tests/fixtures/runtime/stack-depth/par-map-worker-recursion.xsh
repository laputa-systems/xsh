pure leaf(n: Int) -> Int {
  if n <= 0 {
    return 1
  }

  return leaf(n - 1) + 1
}

pure package_step(seed: Int, depth: Int) -> Int {
  let normalized = if seed % 2 == 0 { depth } else { depth - 1 }
  return leaf(normalized) + seed
}

let rows = [0] |> range(0, 64)
let values = rows |> par-map --jobs=4 { |seed|
  package_step(seed, 12000)
}

print ${values.len()}
