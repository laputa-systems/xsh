var i = 0
var total = 0
var counts: Map[Int] = {}

while i < 2000 {
  let row = {name: "pkg", version: i, enabled: i % 2 == 0}
  total += row.version
  let version: Int = row.get("version")?
  total += version

  if row.has("enabled") {
    total += 1
  }

  counts["pkg"] = counts.get("pkg", 0) + 1
  total += counts.get("pkg", 0)
  i += 1
}

print $total % 256
