pure parse_or_default(raw: Str, default: Int) -> Int {
  return raw.parse_int() ?? default
}

pure score(raw: Str, index: Int) -> Int {
  let parsed = parse_or_default(raw, index % 13)
  return parsed + raw.count_chars()
}

var i = 0
var total = 0

while i < 5000 {
  let raw = if i % 4 == 0 { "bad" } else { f"${i}" }
  total += score(raw, i)
  i += 1
}

print $total % 256
