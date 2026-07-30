pure scan_hash(text: Bytes) -> Int {
  var blanks = 0
  var comments = 0
  for line in text.lines() {
    let trimmed = line.trim()
    if trimmed == b"" {
      blanks += 1
    } else if trimmed.starts_with(b"#") {
      comments += 1
    }
  }

  return text.count_lines() - blanks - comments
}

let text = b"# comment\nvalue = 1\n\n# comment\nvalue = 2\n\nvalue = 3\n# comment\nvalue = 4\n\nvalue = 5\n"
var total = 0
var index = 0

while index < 1000 {
  total += scan_hash(text)
  index += 1
}

print $total
