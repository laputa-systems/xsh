pure parse_count(raw: Str, label: Str) -> Result[Int] {
  return raw.parse_int().context("usage", f"unsupported ${label} '${raw}'")
}

pure score(raw: Str, label: Str, fallback: Int) -> Int {
  return parse_count(raw, label) ?? fallback
}

var i = 0
var total = 0

while i < 5000 {
  let raw = if i % 9 == 0 { "bad" } else { f"${i % 97}" }
  total += score(raw, "count", 3)
  i += 1
}

print $total % 256
