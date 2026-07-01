pure score(left: List[Str], right: List[Str], counts: Map[Int], row: Record) -> Int {
  let names = left.extend(right)
  let map_keys = counts.keys()
  let map_values = counts.values()
  let row_keys = row.keys()
  var total = names.len() + map_keys.len() + map_values.len() + row_keys.len()

  for name in names {
    total += name.reverse().count_chars()
  }

  for key in map_keys {
    total += counts.get(key, 0)
  }

  for value in map_values {
    total += value
  }

  for key in row_keys {
    total += key.count_chars()
  }

  return total
}

var i = 0
var total = 0
var counts: Map[Int] = {}

while i < 5000 {
  counts["alpha"] = counts.get("alpha", 0) + i % 7
  counts["beta"] = counts.get("beta", 0) + i % 5
  let left = ["pkg", "lib"]
  let right = [f"item-${i}", "tool"]
  let row = {name: "pkg", weight: i % 17, enabled: i % 2 == 0}
  total += score(left, right, counts, row)
  i += 1
}

print $total % 256
