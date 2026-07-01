type Row = {root: Path, name: Str, weight: Int, enabled: Bool}

pure make_row(root: Path, name: Str, weight: Int, enabled: Bool) -> Row {
  return {root: root, name: name, weight: weight, enabled: enabled}
}

pure row_score(row: Row) -> Int {
  let base = row.name.count_chars() + row.root.name().count_chars() + row.weight

  if row.enabled {
    return base * 2
  }

  return base
}

pure score(root: Path, name: Str, weight: Int, enabled: Bool) -> Int {
  return row_score(make_row(root, name, weight, enabled))
}

let root = /tmp/xsh
var i = 0
var total = 0

while i < 5000 {
  let name = if i % 2 == 0 { "alpha" } else { "beta" }
  total += score(root, name, i % 17, i % 3 == 0)
  i += 1
}

print $total % 256
