pure update_counts(seed: Map[Int]) -> Int {
  var counts = seed
  var i = 0

  while i < 10000 {
    let key = if i % 3 == 0 { "alpha" } else if i % 3 == 1 { "beta" } else { "gamma" }
    counts[key] = counts.get(key, 0) + 1
    i += 1
  }

  return counts.get("alpha", 0) * 3 + counts.get("beta", 0) * 5 + counts.get("gamma", 0) * 7
}

var seed: Map[Int] = {}
print (update_counts(seed) % 256)
