pure tag(name: Str, index: Int) -> Str {
  return match index % 2 { 0 => f"${name}:${index}:even", _ => f"${name}:odd" }
}

pure score(label: Str) -> Result[Int] {
  let cleaned = label.lower().replace(":", ",")
  let parts = cleaned.split(",")
  let _joined = parts.join(":")
  var width = 0

  for part in parts {
    width += part.count_chars()
  }

  let scale = match "alpha" in cleaned or cleaned.ends_with("odd") { true => 2, _ => 1 }
  let number = if parts.len() > 2 { parts.get(1, "0").parse_int()? } else { 0 }
  var base = 0

  if "alpha" in cleaned or cleaned.ends_with("odd") {
    base = width * scale
  } else {
    base = width
  }

  let extra = if ":" in label and cleaned.ends_with("odd") { cleaned.count_bytes() + number } else { number }
  base + extra
}

var i = 0
var total = 0

while i < 5000 {
  let base = if i % 3 == 0 { "alpha" } else { "beta" }
  let label = tag(base, i)
  total += score(label)?
  i += 1
}

print $total % 256
