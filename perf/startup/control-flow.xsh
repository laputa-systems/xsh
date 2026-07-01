type Row = {name: Str, value: Int, enabled: Bool}

pure score(row: Row, offset: Int) -> Int {
  if row.enabled {
    return row.name.count_chars() + row.value + offset
  }

  return offset
}

let rows = [
  {name: "alpha", value: 3, enabled: true},
  {name: "beta", value: 5, enabled: false},
  {name: "gamma", value: 8, enabled: true},
]

var total = 0

for row in rows {
  total += score(row, 2)
}

print $total
