pure score(path_value: Path, root: Path) -> Int {
  let normalized = path_value.normalize()
  let relative = normalized.relative_to(root)
  let shown = normalized.display()
  let parent = normalized.parent()
  let name = path_value.name()
  let ext = path_value.ext()
  var total = shown.count_chars() + relative.display().count_chars() + name.count_chars() + ext.count_chars()

  if parent == root {
    total += 7
  } else {
    total += parent.name().count_chars()
  }

  return total
}

let root = /tmp/xsh
var i = 0
var total = 0

while i < 5000 {
  let dir = if i % 2 == 0 { "xsh" } else { "other" }
  let path_value = fp"/tmp/xsh/../${dir}/./file-${i}.txt"
  total += score(path_value, root)
  i += 1
}

print $total % 256
