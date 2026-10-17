pure anchor(value: Str) -> Bool { value == "x" }
pure split_indirect(text) -> Int {
  let flat = text.trim()
  let anchored = anchor(flat)
  var count = 0
  for line in flat.split("\n") {
    for word in line.split("\t") { count += 1 }
  }
  count
}
let count: Int = split_indirect("a\tb")
