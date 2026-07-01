pure score(text: Str, blanks: Int, comments: Int) -> Int {
  return text.count_lines() - blanks - comments
}

proc main() {
  let text = """alpha
beta

# note
gamma
"""

  var total = 0
  var i = 0

  while i < 20000 {
    total += score(text, 1, 1)
    i += 1
  }

  print $total % 256
}
