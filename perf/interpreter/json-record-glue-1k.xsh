pure score(row: Record) -> Result[Int] {
  let base = row.name.count_chars() + row.get("weight")?

  if row.enabled {
    return Ok(base * 2)
  }

  base
}

var i = 0
var rows: List[Record] = []

while i < 1000 {
  rows = rows.push({name: f"pkg-${i}", weight: i % 17, enabled: i % 2 == 0})
  i += 1
}

var total = 0
var counts: Map[Int] = {}

for row in rows {
  let value = score(row)?
  counts[row.name] = counts.get(row.name, 0) + value
  total += counts.get(row.name, 0)
}

print $total % 256
