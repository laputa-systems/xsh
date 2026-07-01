pure score(weights: List[Int], index: Int) -> Int {
  let row = {weight: weights[index], enabled: index % 2 == 0}

  if row.enabled {
    return row.weight * 2
  }

  return row.weight
}

pure bump(counts: Map[Int], key: Str, value: Int) -> Map[Int] {
  return counts.set(key, counts.get(key, 0) + value)
}

let weights = [
  0,
  1,
  2,
  3,
  4,
  5,
  6,
  7,
  8,
  9,
  10,
  11,
  12,
  13,
  14,
  15,
  16,
  0,
  1,
  2,
  3,
  4,
  5,
  6,
  7,
  8,
  9,
  10,
  11,
  12,
  13,
  14,
  15,
  16,
  0,
  1,
  2,
  3,
  4,
  5,
  6,
  7,
  8,
  9,
  10,
  11,
  12,
  13,
  14,
  15,
  16,
  0,
  1,
  2,
  3,
  4,
  5,
  6,
  7,
  8,
  9,
  10,
  11,
  12,
]

var i = 0
var total = 0
var counts: Map[Int] = {}

while i < 5000 {
  let value = score(weights, i % weights.len())
  total += value
  counts = bump(counts, "pkg", value % 17)
  i += 1
}

print $total + counts.get("pkg", 0) ) % 256
