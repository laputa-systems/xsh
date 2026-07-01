pure score(name: Str, weight: Int, enabled: Bool) -> Result[Int] {
  let row = {name: name, weight: weight, enabled: enabled}
  let base = row.name.count_chars() + row.get("weight")?

  if row.enabled {
    return Ok(base * 2)
  }

  base
}

var i = 0
var total = 0

while i < 5000 {
  let name = if i % 2 == 0 { "alpha" } else { "beta" }
  total += score(name, i % 17, i % 3 == 0)?
  i += 1
}

print $total % 256
