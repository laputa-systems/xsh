pure anchor(value: Str) -> Bool { value == "x" }
pure split_before_anchor(text) -> Int {
  var count = 0
  for line in text.split("\n") {
    for word in line.split("\t") { count += 1 }
  }
  let anchored = anchor(text)
  count
}
let count: Int = split_before_anchor("a\tb")
