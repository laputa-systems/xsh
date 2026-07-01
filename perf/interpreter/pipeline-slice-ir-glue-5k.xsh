pure window(raw: Str, start: Int, width: Int) -> Str {
  let chars = raw.split("")
  let tail = chars |> drop(start)
  let head = tail |> take(width)
  return head.join("")
}

pure score(raw: Str, start: Int, width: Int) -> Int {
  let clipped = window(raw, start, width)

  if clipped.contains("pkg") {
    return clipped.count_chars() + 7
  }

  return clipped.count_chars()
}

var i = 0
var total = 0

while i < 5000 {
  let raw = f"prefix-pkg-${i}-suffix"
  total += score(raw, i % 4, 6)
  i += 1
}

print $total % 256
