pure score(row: Record) -> Result[Int] {
  let name_len = row.name.count_chars()
  let weight = row.get("weight")?

  if row.enabled {
    return Ok(name_len + weight)
  }

  weight
}

var i = 0
var total = 0
var counts: Map[Int] = {}

while i < 2000 {
  let row = {name: "pkg", weight: i % 17, enabled: i % 2 == 0}
  let value = score(row)?
  counts[row.name] = counts.get(row.name, 0) + value
  total += counts.get(row.name, 0)

  if row.name.starts_with("p") and row.name.ends_with("g") {
    total += row.name.count_bytes()
  }

  i += 1
}

print $total % 256
