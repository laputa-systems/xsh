pure score(line: Str, pattern: Str, seed: Int) -> Result[Int] {
  let re = regex.compile(pattern)?

  if ! re.matches(line) {
    return Ok(seed % 7)
  }

  let caps = re.captures(line)
  let normalized = re.replace(line, "pkg:$1:$2")
  caps.len() + caps.get(1, "").count_chars() + caps.get(2, "0").parse_int()? + normalized.count_chars()
}

var i = 0
var total = 0

while i < 5000 {
  let line = if i % 5 == 0 { f"skip-${i}" } else { f"pkg-${i % 97}-${i % 13}" }
  total += score(line, "^pkg-(\\d+)-(\\d+)$", i)?
  i += 1
}

print $total % 256
