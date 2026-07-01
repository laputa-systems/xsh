pure count_text_lines(input_text: Str) -> Int {
  var blanks = 0
  var code = 0
  var comments = 0

  for line in input_text.lines() {
    let trimmed = line.trim()

    if trimmed == "" {
      blanks += 1
    } else if trimmed.starts_with("//") {
      comments += 1
    } else {
      code += 1
    }
  }

  return blanks + code * 3 + comments * 5
}

let sample = """let one = 1

// note
let two = 2
"""

var i = 0
var total = 0

while i < 10000 {
  total += count_text_lines(sample)
  i += 1
}

print $total % 256
