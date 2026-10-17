pure anchor(value: Str) -> Bool { value == "x" }
pure count_words(text) -> Int {
  let flat = text.trim()
  let anchored = anchor(flat)
  var count = 0
  for line in flat.split("\n") {
    for word in line.split(" ") { count += 1 }
  }
  count
}
assert count_words(" a b\nc ") == 3
