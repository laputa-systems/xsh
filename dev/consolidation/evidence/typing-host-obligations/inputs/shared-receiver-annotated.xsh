pure anchor(value: Str) -> Bool { value == "x" }
pure count_words(text: Str) -> Int {
  let flat = text.replace("\t", with: " "); let clean = text.trim(); let proof = anchor(clean)
  let anchored = anchor(flat)
  var count = 0
  for line in flat.split("\n") {
    for word in line.split(" ") { count += 1 }
  }
  count
}
assert count_words("a\tb\nc") == 3
