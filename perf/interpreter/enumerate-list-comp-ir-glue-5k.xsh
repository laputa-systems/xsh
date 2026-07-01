pure selected_index(index: Int, seed: Int) -> Bool {
  return index % 3 == seed % 3 or index == 0
}

pure select_parts(raw: Str, delimiter: Str, seed: Int) -> Str {
  let parts = raw.split(delimiter)
  let selected = [item.value for item in parts |> enumerate() if selected_index(item.index, seed)]
  return selected.join(delimiter)
}

var i = 0
var total = 0

while i < 5000 {
  let raw = f"alpha:${i}:beta:${i % 17}:gamma:${i % 5}:delta"
  let selected = select_parts(raw, ":", i)
  total += selected.count_chars()
  i += 1
}

print $total % 256
