use qualified_helper

proc score(row: Record, root: Path, pattern: Str) [error] -> Result[Int] {
  let normalized = row.path.normalize()
  let relative = normalized.relative_to(root)
  let message = qualified_helper.invalid_option(row.name).lower()
  let re = regex.compile(pattern)?

  if ! re.matches(row.name) {
    return Ok(message.count_chars())
  }

  let caps = re.captures(row.name)
  relative.display().count_chars() + caps.len() + row.name.count_chars() + row.weight
}

let root: Path = /tmp/xsh
let path_a: Path = /tmp/xsh/../xsh/pkg.txt
let path_b: Path = /tmp/xsh/../other/pkg.txt
let pattern: Str = "^pkg-(\\d+)$"
var counts: Map[Int] = {}
var i: Int = 0
var total: Int = 0

while i < 2000 {
  let row = {name: f"pkg-${i % 97}", path: if i % 2 == 0 { path_a } else { path_b }, weight: i % 17}
  let value = score(row, root, pattern)?
  counts[row.name] = counts.get(row.name, 0) + value
  total += counts.get(row.name, 0)
  i += 1
}

print $total % 256
